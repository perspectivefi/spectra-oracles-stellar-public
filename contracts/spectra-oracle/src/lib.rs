// SPDX-License-Identifier: BUSL-1.1
#![no_std]

// ---------------------------------------------------------------------------
// Spectra Deterministic Oracle
// ---------------------------------------------------------------------------
//
// A single-asset oracle that prices a zero-coupon bond (PT) against an
// underlying asset using a deterministic formula:
//   price = FV / (1 + APY)^t
//
// All data lives in instance storage, which shares a single TTL and can
// be restored after archival via `RestoreFootprintOp`. See `storage.rs`
// for the full storage layout.
// ---------------------------------------------------------------------------

mod math;
mod storage;

pub use math::log_exp_math;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, Address,
    Env, String, Symbol, I256,
};
use stellar_access::ownable;
use stellar_contract_utils::upgradeable::UpgradeableInternal;
use stellar_macros::{only_owner, Upgradeable};

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

/// Error codes for the Spectra Oracle contract
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OracleError {
    /// Contract has already been initialized
    AlreadyInitialized = 1,
    /// Contract has not been initialized
    NotInitialized = 2,
    /// Maturity timestamp must be in the future
    InvalidMaturity = 3,
    /// APY must be between 0 (exclusive) and 100% (inclusive)
    InvalidApy = 4,
    /// Future PT value must be positive
    InvalidFuturePtValue = 5,
    /// Caller is not authorized to perform this action
    Unauthorized = 6,
    /// No upgrader has been set
    NoUpgraderSet = 7,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Asset type for oracle queries.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

/// Price data returned by `price()` and `lastprice()`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Event emitted when the future PT value is updated
#[contractevent(topics = ["pt_value"])]
pub struct FuturePtValueUpdated {
    pub old_value: i128,
    pub new_value: i128,
}

/// Event emitted when the upgrader is changed
#[contractevent(topics = ["upgrader"])]
pub struct UpgraderChanged {
    pub old_upgrader: Option<Address>,
    pub new_upgrader: Address,
}

use storage::{
    bump_instance, bump_instance_manual, get_decimals, get_future_pt_value,
    get_initial_implied_apy, get_maturity, get_pt, get_start_time, get_upgrader, is_initialized,
    set_decimals, set_future_pt_value, set_initial_implied_apy, set_maturity, set_pt,
    set_start_time, set_upgrader,
};

/// Internal math precision — 18 decimals.
/// The log/exp library is designed for this precision.
/// Output prices are scaled to the oracle's configured `decimals`.
const UNIT: i128 = 1_000_000_000_000_000_000; // 1e18
const SECONDS_PER_YEAR: i128 = 31_536_000; // 365 days in seconds

#[derive(Upgradeable)]
#[contract]
pub struct SpectraOracle;

impl UpgradeableInternal for SpectraOracle {
    fn _require_auth(e: &Env, operator: &Address) {
        // Only upgrader can upgrade the contract
        let upgrader = match get_upgrader(e) {
            Some(u) => u,
            None => panic_with_error!(e, OracleError::NoUpgraderSet),
        };
        if *operator != upgrader {
            panic_with_error!(e, OracleError::Unauthorized);
        }
        operator.require_auth();
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Compute the ZCB price at a given timestamp.
///
/// Returns `None` if the price overflows i128.
///
/// The formula is: price = futurePTValue / (1 + APY)^(timeLeft / SECONDS_PER_YEAR)
fn compute_price_at_time(env: &Env, timestamp: u64) -> Option<i128> {
    let maturity = get_maturity(env);
    let initial_implied_apy = get_initial_implied_apy(env);
    let future_pt_value = get_future_pt_value(env);

    // Clamp to maturity
    let effective_time = if timestamp >= maturity {
        maturity
    } else {
        timestamp
    };

    let time_left = maturity - effective_time;

    // If no time left, price equals future PT value
    if time_left == 0 {
        return Some(future_pt_value);
    }

    // Calculate t = timeLeft / SECONDS_PER_YEAR (in 18 decimals)
    let t: i128 = (time_left as i128 * UNIT) / SECONDS_PER_YEAR;

    // base = 1 + APY (in 18 decimals)
    let base = I256::from_i128(env, UNIT + initial_implied_apy);

    // Calculate ln(base)
    let rate_per_second = log_exp_math::ln(env, &base);

    // Calculate exp(ln(base) * t / UNIT) = base^t
    let t_val = I256::from_i128(env, t);
    let unit_val = I256::from_i128(env, UNIT);
    let exponent = rate_per_second.mul(&t_val).div(&unit_val);

    // exp() panics if the exponent exceeds its domain [-41e18, 130e18].
    // This can happen with extreme APY + maturity combinations (e.g.
    // 100% APY with 188+ year maturity). Return None instead of trapping.
    let max_exp = I256::from_i128(env, 130_000_000_000_000_000_000i128); // 130e18
    if exponent > max_exp {
        return None;
    }

    let denominator = log_exp_math::exp(env, &exponent);

    // Calculate present value = futurePTValue * UNIT / denominator
    let fv = I256::from_i128(env, future_pt_value);
    let present_value = fv.mul(&unit_val).div(&denominator);

    present_value.to_i128()
}

// ---------------------------------------------------------------------------
// Contract implementation
// ---------------------------------------------------------------------------

#[contractimpl]
impl SpectraOracle {
    /// Initialize the oracle with the required parameters.
    ///
    /// # Arguments
    /// * `owner` - The owner address (can update config and transfer ownership)
    /// * `upgrader` - The upgrader address (can upgrade the contract WASM)
    /// * `pt` - The PT token address being priced
    /// * `maturity` - The maturity timestamp (Unix timestamp)
    /// * `initial_implied_apy` - The initial implied APY (18 decimals, e.g., 0.1e18 = 10%)
    /// * `future_pt_value` - The future PT value at maturity (in `decimals` precision)
    /// * `decimals` - The number of decimals for the price output
    ///
    /// # Errors
    /// * `AlreadyInitialized` - Contract has already been initialized
    /// * `InvalidMaturity` - Maturity timestamp is not in the future
    /// * `InvalidApy` - APY is not between 0 and 100%
    /// * `InvalidFuturePtValue` - Future PT value is not positive
    pub fn initialize(
        env: Env,
        owner: Address,
        upgrader: Address,
        pt: Address,
        maturity: u64,
        initial_implied_apy: i128,
        future_pt_value: i128,
        decimals: u32,
    ) -> Result<(), OracleError> {
        if is_initialized(&env) {
            return Err(OracleError::AlreadyInitialized);
        }

        let current_time = env.ledger().timestamp();
        if maturity <= current_time {
            return Err(OracleError::InvalidMaturity);
        }

        if initial_implied_apy <= 0 || initial_implied_apy > UNIT {
            return Err(OracleError::InvalidApy);
        }

        if future_pt_value <= 0 {
            return Err(OracleError::InvalidFuturePtValue);
        }

        // Set owner — serves as access control for admin functions
        ownable::set_owner(&env, &owner);

        // Set upgrader — separate role for contract upgrades
        set_upgrader(&env, &upgrader);

        set_pt(&env, &pt);
        set_maturity(&env, maturity);
        set_start_time(&env, current_time);
        set_initial_implied_apy(&env, initial_implied_apy);
        set_future_pt_value(&env, future_pt_value);
        set_decimals(&env, decimals);

        // Validate that the parameters produce a non-zero price at deployment
        match compute_price_at_time(&env, current_time) {
            Some(p) if p > 0 => {}
            _ => return Err(OracleError::InvalidFuturePtValue),
        }

        // Set instance TTL: min(maturity + 30 days, max_ttl).
        bump_instance(&env, maturity);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Price queries
    // -----------------------------------------------------------------------

    /// Return the number of decimals for the price output.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract has not been initialized
    pub fn decimals(env: Env) -> Result<u32, OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        Ok(get_decimals(&env))
    }

    /// Get price at a specific timestamp.
    ///
    /// Returns `None` if:
    /// - The oracle is not initialized
    /// - The timestamp is before the oracle start time
    /// - The price computation overflows
    pub fn price(env: Env, timestamp: u64) -> Option<PriceData> {
        if !is_initialized(&env) {
            return None;
        }

        let start_time = get_start_time(&env);

        // Cannot provide price before oracle existed
        if timestamp < start_time {
            return None;
        }

        let price = compute_price_at_time(&env, timestamp)?;

        Some(PriceData { price, timestamp })
    }

    /// Get the most recent price.
    ///
    /// # Warning
    ///
    /// The `asset` parameter is **ignored**. It exists solely for interface
    /// compatibility. Each oracle instance is bound to a single PT at
    /// initialization and always prices that PT regardless of what is
    /// passed here.
    ///
    /// Returns `None` if the oracle is not initialized.
    pub fn lastprice(env: Env, _asset: Asset) -> Option<PriceData> {
        if !is_initialized(&env) {
            return None;
        }

        let current_time = env.ledger().timestamp();

        // If past maturity, return future_pt_value directly
        if current_time >= get_maturity(&env) {
            let ftv = get_future_pt_value(&env);
            return Some(PriceData {
                price: ftv,
                timestamp: current_time,
            });
        }

        // Otherwise compute price normally
        let price = compute_price_at_time(&env, current_time)?;

        Some(PriceData {
            price,
            timestamp: current_time,
        })
    }

    // -----------------------------------------------------------------------
    // Admin methods
    // -----------------------------------------------------------------------

    /// Update the future PT value. Only callable by the owner.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract has not been initialized
    /// * `InvalidFuturePtValue` - New value is not positive
    #[only_owner]
    pub fn set_future_pt_value(env: Env, new_value: i128) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }

        if new_value <= 0 {
            return Err(OracleError::InvalidFuturePtValue);
        }

        let old_value = get_future_pt_value(&env);
        set_future_pt_value(&env, new_value);

        // Validate that the new value produces a non-zero price
        match compute_price_at_time(&env, env.ledger().timestamp()) {
            Some(p) if p > 0 => {}
            _ => return Err(OracleError::InvalidFuturePtValue),
        }

        // Emit event
        FuturePtValueUpdated {
            old_value,
            new_value,
        }
        .publish(&env);

        // Re-extend TTL toward maturity + 30 days.
        bump_instance(&env, get_maturity(&env));
        Ok(())
    }

    /// Set a new upgrader address. Only callable by the current upgrader.
    ///
    /// The upgrader role is independent from ownership — the owner cannot
    /// change the upgrader, and the upgrader cannot perform owner actions.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract has not been initialized
    /// * `NoUpgraderSet` - No upgrader has been set
    /// * `Unauthorized` - Caller is not the current upgrader
    pub fn set_upgrader(env: Env, new_upgrader: Address) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }

        let old_upgrader = match get_upgrader(&env) {
            Some(u) => u,
            None => return Err(OracleError::NoUpgraderSet),
        };
        old_upgrader.require_auth();
        set_upgrader(&env, &new_upgrader);

        // Emit event
        UpgraderChanged {
            old_upgrader: Some(old_upgrader),
            new_upgrader,
        }
        .publish(&env);

        // Re-extend TTL toward maturity + 30 days.
        bump_instance(&env, get_maturity(&env));
        Ok(())
    }

    /// Transfer ownership to a new address. Only callable by the current owner.
    /// Uses OpenZeppelin's two-step transfer process.
    #[only_owner]
    pub fn transfer_ownership(env: Env, new_owner: Address, live_until_ledger: u32) {
        ownable::transfer_ownership(&env, &new_owner, live_until_ledger);
    }

    /// Accept ownership transfer. Must be called by the pending owner.
    pub fn accept_ownership(env: Env) {
        ownable::accept_ownership(&env);
    }

    /// Renounce ownership permanently. Only callable by the owner.
    /// WARNING: This is irreversible and will lock all owner-only functions.
    /// The upgrader role is unaffected — the upgrader can still upgrade the contract.
    #[only_owner]
    pub fn renounce_ownership(env: Env) {
        ownable::renounce_ownership(&env);
    }

    // -----------------------------------------------------------------------
    // Permissionless maintenance
    // -----------------------------------------------------------------------

    /// Re-extend the TTL of the oracle instance storage.
    ///
    /// This is a **permissionless** function — anyone can call it and the
    /// caller pays the transaction fees.
    ///
    /// The `ledgers` parameter specifies the desired TTL extension in
    /// ledgers (clamped to the network's `max_ttl`). For long-maturity
    /// oracles where the network max TTL (~6 months) is shorter than
    /// the time to maturity, callers should invoke `bump()` periodically.
    ///
    /// Soroban's `extend_ttl` is monotonic (it never shrinks a TTL), so
    /// calling this when TTLs are already healthy is a safe no-op.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract has not been initialized
    pub fn bump(env: Env, ledgers: u32) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        bump_instance_manual(&env, ledgers);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // View functions
    // -----------------------------------------------------------------------

    /// Get the owner address. Returns None if ownership has been renounced.
    pub fn get_owner(env: Env) -> Option<Address> {
        ownable::get_owner(&env)
    }

    /// Get the upgrader address. Returns None if not initialized.
    pub fn get_upgrader(env: Env) -> Option<Address> {
        get_upgrader(&env)
    }

    /// Get the PT token address.
    pub fn get_pt(env: Env) -> Address {
        get_pt(&env)
    }

    /// Get the maturity timestamp.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract has not been initialized
    pub fn get_maturity(env: Env) -> Result<u64, OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        Ok(get_maturity(&env))
    }

    /// Get the start time timestamp.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract has not been initialized
    pub fn get_start_time(env: Env) -> Result<u64, OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        Ok(get_start_time(&env))
    }

    /// Get the initial implied APY.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract has not been initialized
    pub fn get_initial_implied_apy(env: Env) -> Result<i128, OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        Ok(get_initial_implied_apy(&env))
    }

    /// Get the future PT value.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract has not been initialized
    pub fn get_future_pt_value(env: Env) -> Result<i128, OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        Ok(get_future_pt_value(&env))
    }

    /// Get a description of this oracle.
    pub fn get_description(env: Env) -> String {
        String::from_str(
            &env,
            "Spectra Deterministic Oracle - Zero Coupon Bond Model",
        )
    }
}

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_cross_validation;
