// SPDX-License-Identifier: BUSL-1.1
#![no_std]

mod storage;

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, panic_with_error, vec, xdr::ToXdr,
    Address, Bytes, BytesN, Env, IntoVal, Symbol,
};
use stellar_access::ownable;
use stellar_contract_utils::upgradeable::UpgradeableInternal;
use stellar_macros::{only_owner, Upgradeable};

use storage::{
    bump_instance, get_oracle_wasm_hash, get_upgrader, is_initialized, set_oracle_wasm_hash,
    set_upgrader,
};

/// Error codes for the Spectra Oracle Factory contract
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FactoryError {
    /// Contract has already been initialized
    AlreadyInitialized = 1,
    /// Contract has not been initialized
    NotInitialized = 2,
    /// Caller is not authorized to perform this action
    Unauthorized = 3,
    /// No upgrader has been set
    NoUpgraderSet = 4,
}

/// Event emitted when the oracle WASM hash is updated
#[contractevent(topics = ["wasm"])]
pub struct WasmHashUpdated {
    pub old_hash: BytesN<32>,
    pub new_hash: BytesN<32>,
}

/// Event emitted when the upgrader is changed
#[contractevent(topics = ["upgrader"])]
pub struct UpgraderChanged {
    pub old_upgrader: Option<Address>,
    pub new_upgrader: Address,
}

/// Event emitted when a new oracle is deployed
#[contractevent(topics = ["deployed"])]
pub struct OracleDeployed {
    pub oracle_address: Address,
    pub owner: Address,
    pub pt: Address,
    pub maturity: u64,
    pub initial_implied_apy: i128,
    pub future_pt_value: i128,
    pub decimals: u32,
}

#[derive(Upgradeable)]
#[contract]
pub struct SpectraOracleFactory;

impl UpgradeableInternal for SpectraOracleFactory {
    fn _require_auth(e: &Env, operator: &Address) {
        // Only upgrader can upgrade the factory
        let upgrader = match get_upgrader(e) {
            Some(u) => u,
            None => panic_with_error!(e, FactoryError::NoUpgraderSet),
        };
        if *operator != upgrader {
            panic_with_error!(e, FactoryError::Unauthorized);
        }
        operator.require_auth();
    }
}

#[contractimpl]
impl SpectraOracleFactory {
    /// Initialize the factory with the oracle WASM hash.
    ///
    /// # Arguments
    /// * `owner` - The owner address that can update the WASM hash
    /// * `upgrader` - The upgrader address that can upgrade the factory
    /// * `oracle_wasm_hash` - The WASM hash of the oracle contract to deploy
    ///
    /// # Errors
    /// * `AlreadyInitialized` - Factory has already been initialized
    pub fn initialize(
        env: Env,
        owner: Address,
        upgrader: Address,
        oracle_wasm_hash: BytesN<32>,
    ) -> Result<(), FactoryError> {
        if is_initialized(&env) {
            return Err(FactoryError::AlreadyInitialized);
        }

        ownable::set_owner(&env, &owner);
        set_upgrader(&env, &upgrader);
        set_oracle_wasm_hash(&env, &oracle_wasm_hash);

        bump_instance(&env);
        Ok(())
    }

    /// Deploy a new oracle contract.
    ///
    /// The salt is derived on-chain from the oracle parameters, so
    /// each parameter tuple has a canonical address.
    ///
    /// # Arguments
    /// * `oracle_owner` - The owner of the new oracle
    /// * `pt` - The PT token address being priced
    /// * `maturity` - The maturity timestamp
    /// * `initial_implied_apy` - The initial implied APY (18 decimals)
    /// * `future_pt_value` - The future PT value at maturity
    /// * `decimals` - The decimals for the price output
    ///
    /// Returns the address of the newly deployed oracle.
    ///
    /// # Errors
    /// * `NotInitialized` - Factory has not been initialized
    pub fn deploy_oracle(
        env: Env,
        oracle_owner: Address,
        pt: Address,
        maturity: u64,
        initial_implied_apy: i128,
        future_pt_value: i128,
        decimals: u32,
    ) -> Result<Address, FactoryError> {
        if !is_initialized(&env) {
            return Err(FactoryError::NotInitialized);
        }

        // Derive the salt deterministically from all oracle parameters.
        // Front-running becomes self-defeating: mirroring all params
        // yields the team's intended oracle at the same address;
        // changing any param yields a different address.
        let mut salt_data: Bytes = oracle_owner.clone().to_xdr(&env);
        salt_data.append(&pt.clone().to_xdr(&env));
        salt_data.append(&maturity.to_xdr(&env));
        salt_data.append(&initial_implied_apy.to_xdr(&env));
        salt_data.append(&future_pt_value.to_xdr(&env));
        salt_data.append(&decimals.to_xdr(&env));
        let salt: BytesN<32> = env.crypto().sha256(&salt_data).into();

        let wasm_hash = get_oracle_wasm_hash(&env);

        // Deploy the oracle contract
        let oracle_address = env
            .deployer()
            .with_current_contract(salt)
            .deploy_v2(wasm_hash, ());

        // Initialize the oracle
        let init_fn = Symbol::new(&env, "initialize");
        let init_args = vec![
            &env,
            oracle_owner.into_val(&env),
            oracle_owner.into_val(&env), // upgrader = owner for deployed oracle
            pt.into_val(&env),
            maturity.into_val(&env),
            initial_implied_apy.into_val(&env),
            future_pt_value.into_val(&env),
            decimals.into_val(&env),
        ];
        let _: () = env.invoke_contract(&oracle_address, &init_fn, init_args);

        // Emit event
        OracleDeployed {
            oracle_address: oracle_address.clone(),
            owner: oracle_owner,
            pt,
            maturity,
            initial_implied_apy,
            future_pt_value,
            decimals,
        }
        .publish(&env);

        bump_instance(&env);

        Ok(oracle_address)
    }

    /// Update the oracle WASM hash. Only callable by the owner.
    ///
    /// # Errors
    /// * `NotInitialized` - Factory has not been initialized
    #[only_owner]
    pub fn set_oracle_wasm_hash(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), FactoryError> {
        if !is_initialized(&env) {
            return Err(FactoryError::NotInitialized);
        }
        let old_hash = get_oracle_wasm_hash(&env);
        set_oracle_wasm_hash(&env, &new_wasm_hash);

        WasmHashUpdated {
            old_hash,
            new_hash: new_wasm_hash,
        }
        .publish(&env);

        bump_instance(&env);
        Ok(())
    }

    /// Set a new upgrader address. Only callable by the current upgrader.
    ///
    /// The upgrader role is independent from ownership — the owner cannot
    /// change the upgrader, and the upgrader cannot perform owner actions.
    ///
    /// # Errors
    /// * `NotInitialized` - Factory has not been initialized
    /// * `NoUpgraderSet` - No upgrader has been set
    /// * `Unauthorized` - Caller is not the current upgrader
    pub fn set_upgrader(env: Env, new_upgrader: Address) -> Result<(), FactoryError> {
        if !is_initialized(&env) {
            return Err(FactoryError::NotInitialized);
        }
        let old_upgrader = match get_upgrader(&env) {
            Some(u) => u,
            None => return Err(FactoryError::NoUpgraderSet),
        };
        old_upgrader.require_auth();
        set_upgrader(&env, &new_upgrader);

        UpgraderChanged {
            old_upgrader: Some(old_upgrader),
            new_upgrader,
        }
        .publish(&env);

        bump_instance(&env);
        Ok(())
    }

    /// Transfer ownership to a new address. Only callable by the current owner.
    #[only_owner]
    pub fn transfer_ownership(env: Env, new_owner: Address, live_until_ledger: u32) {
        ownable::transfer_ownership(&env, &new_owner, live_until_ledger);
        bump_instance(&env);
    }

    /// Accept ownership transfer. Must be called by the pending owner.
    pub fn accept_ownership(env: Env) {
        ownable::accept_ownership(&env);
        bump_instance(&env);
    }

    /// Renounce ownership permanently. Only callable by the owner.
    #[only_owner]
    pub fn renounce_ownership(env: Env) {
        ownable::renounce_ownership(&env);
        bump_instance(&env);
    }

    // --- View functions ---

    /// Get the owner address.
    pub fn get_owner(env: Env) -> Option<Address> {
        ownable::get_owner(&env)
    }

    /// Get the upgrader address.
    pub fn get_upgrader(env: Env) -> Option<Address> {
        get_upgrader(&env)
    }

    /// Get the oracle WASM hash.
    ///
    /// # Errors
    /// * `NotInitialized` - Factory has not been initialized
    pub fn get_oracle_wasm_hash(env: Env) -> Result<BytesN<32>, FactoryError> {
        if !is_initialized(&env) {
            return Err(FactoryError::NotInitialized);
        }
        Ok(get_oracle_wasm_hash(&env))
    }
}

#[cfg(test)]
mod test;
