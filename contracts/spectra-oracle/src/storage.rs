// SPDX-License-Identifier: BUSL-1.1
// ---------------------------------------------------------------------------
// Storage layout & lifecycle
// ---------------------------------------------------------------------------
//
// This module defines the storage layout for the Spectra Oracle contract.
// All data lives in **instance storage**, which shares a single TTL and
// can be restored after archival via `RestoreFootprintOp`.
//
//   Instance storage:
//     - `owner`              — managed by `stellar_access::ownable`; admin role.
//     - `upgrader`           — address authorized to upgrade the contract WASM.
//                               (`is_initialized()` checks this key's existence).
//     - `future_pt_value`    — needed for post-maturity oracle queries.
//     - `pt`                 — the PT token address.
//     - `maturity`           — maturity timestamp.
//     - `start_time`         — deployment timestamp.
//     - `initial_implied_apy`— APY set at initialization (18 decimals).
//     - `decimals`           — output price precision.
//
// ## TTL strategy
//
// The network max TTL is ~6 months. For oracles with maturities beyond
// that, periodic `bump()` calls are needed to keep the contract alive.
//
// - **Automatic bumps** (on write paths): extend to
//   `min(maturity + 30 days, max_ttl)` — as close to maturity as possible.
// - **Manual bumps** (`bump(ledgers)`): caller chooses the extension,
//   clamped to `max_ttl`. Useful for long-maturity oracles.
// - **Read paths** (`price`, `lastprice`): do NOT extend any TTL.
// ---------------------------------------------------------------------------

use soroban_sdk::{symbol_short, unwrap::UnwrapOptimized, Address, Env, Symbol};

// ---------------------------------------------------------------------------
// TTL constants
// ---------------------------------------------------------------------------

const LEDGERS_PER_DAY: u32 = 17_280; // 86 400 s / 5 s per ledger

/// Grace period added beyond `ledgers_until_maturity` for automatic bumps.
const GRACE_PERIOD_DAYS: u32 = 30;

/// Threshold below which automatic bumps trigger (7 days).
const BUMP_THRESHOLD: u32 = 7 * LEDGERS_PER_DAY;

// ---------------------------------------------------------------------------
// Storage keys (all instance storage)
// ---------------------------------------------------------------------------

const MATURITY: Symbol = symbol_short!("maturity");
const START_TIME: Symbol = symbol_short!("start_t");
const IMPLIED_APY: Symbol = symbol_short!("apy");
const DECIMALS: Symbol = symbol_short!("decimals");
const UPGRADER: Symbol = symbol_short!("upgrader");
const FUTURE_PT: Symbol = symbol_short!("future_pt");
const PT: Symbol = symbol_short!("pt");

// ---------------------------------------------------------------------------
// TTL helpers
// ---------------------------------------------------------------------------

/// Compute the ideal TTL in ledgers: `min(maturity + 30d - now, max_ttl)`.
///
/// When maturity is in the past, the grace period alone applies.
/// The result is always clamped to `max_ttl` so `extend_ttl` never reverts.
fn compute_ttl(env: &Env, maturity: u64) -> u32 {
    let now = env.ledger().timestamp();
    let target_end = maturity.saturating_add((GRACE_PERIOD_DAYS as u64) * 86_400);
    let secs_left = target_end.saturating_sub(now);
    let max = env.storage().max_ttl();
    let desired = (secs_left / 5).min(max as u64) as u32;
    desired.min(max)
}

/// Automatic TTL bump — extends instance to `min(maturity + 30d, max_ttl)`.
///
/// Only extends if current TTL is below the threshold (7 days).
/// Called in `initialize()` and on write paths.
pub fn bump_instance(env: &Env, maturity: u64) {
    let ttl = compute_ttl(env, maturity);
    if ttl > 0 {
        env.storage().instance().extend_ttl(BUMP_THRESHOLD, ttl);
    }
}

/// Manual TTL bump — caller specifies the desired extension in ledgers.
///
/// Clamped to `max_ttl` to prevent reverts.
pub fn bump_instance_manual(env: &Env, ledgers: u32) {
    let max = env.storage().max_ttl();
    let ttl = ledgers.min(max);
    if ttl > 0 {
        env.storage().instance().extend_ttl(ttl, ttl);
    }
}

// ---------------------------------------------------------------------------
// Oracle config (instance storage)
// ---------------------------------------------------------------------------

// --- Maturity ---

pub fn get_maturity(env: &Env) -> u64 {
    env.storage().instance().get(&MATURITY).unwrap_optimized()
}

pub fn set_maturity(env: &Env, maturity: u64) {
    env.storage().instance().set(&MATURITY, &maturity);
}

// --- Start Time ---

pub fn get_start_time(env: &Env) -> u64 {
    env.storage().instance().get(&START_TIME).unwrap_optimized()
}

pub fn set_start_time(env: &Env, start_time: u64) {
    env.storage().instance().set(&START_TIME, &start_time);
}

// --- Initial Implied APY ---

pub fn get_initial_implied_apy(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&IMPLIED_APY)
        .unwrap_optimized()
}

pub fn set_initial_implied_apy(env: &Env, apy: i128) {
    env.storage().instance().set(&IMPLIED_APY, &apy);
}

// --- Decimals ---

pub fn get_decimals(env: &Env) -> u32 {
    env.storage().instance().get(&DECIMALS).unwrap_optimized()
}

pub fn set_decimals(env: &Env, decimals: u32) {
    env.storage().instance().set(&DECIMALS, &decimals);
}

// --- Initialization check ---

/// Returns `true` if the contract has been initialized.
///
/// Internally checks whether the upgrader key exists in instance storage.
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&UPGRADER)
}

// --- Upgrader (instance storage) ---

pub fn get_upgrader(env: &Env) -> Option<Address> {
    env.storage().instance().get(&UPGRADER)
}

pub fn set_upgrader(env: &Env, upgrader: &Address) {
    env.storage().instance().set(&UPGRADER, upgrader);
}

// --- Future PT Value (instance storage) ---

pub fn get_future_pt_value(env: &Env) -> i128 {
    env.storage().instance().get(&FUTURE_PT).unwrap_optimized()
}

pub fn set_future_pt_value(env: &Env, value: i128) {
    env.storage().instance().set(&FUTURE_PT, &value);
}

// --- PT address (instance storage) ---

pub fn get_pt(env: &Env) -> Address {
    env.storage().instance().get(&PT).unwrap_optimized()
}

pub fn set_pt(env: &Env, pt: &Address) {
    env.storage().instance().set(&PT, pt);
}
