// SPDX-License-Identifier: BUSL-1.1
use soroban_sdk::{symbol_short, unwrap::UnwrapOptimized, Address, BytesN, Env, Symbol};

// Storage TTL constants (in ledgers)
// ~7 days at 5 seconds per ledger - bump when TTL falls below this
const INSTANCE_LIFETIME_THRESHOLD: u32 = 120_960;
// ~30 days - extend TTL to this value when bumping
const INSTANCE_BUMP_AMOUNT: u32 = 518_400;

/// Extend the instance storage TTL to prevent expiration.
/// Should be called on any state-changing operation.
pub fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

// Storage keys
const ORACLE_WASM: Symbol = symbol_short!("orc_wasm");
const UPGRADER: Symbol = symbol_short!("upgrader");

// --- Oracle WASM Hash ---

pub fn get_oracle_wasm_hash(env: &Env) -> BytesN<32> {
    env.storage()
        .instance()
        .get(&ORACLE_WASM)
        .unwrap_optimized()
}

pub fn set_oracle_wasm_hash(env: &Env, wasm_hash: &BytesN<32>) {
    env.storage().instance().set(&ORACLE_WASM, wasm_hash);
}

// --- Initialization check ---

/// Returns `true` if the factory has been initialized.
///
/// Internally checks whether the upgrader key exists in instance storage.
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&UPGRADER)
}

// --- Upgrader ---

pub fn get_upgrader(env: &Env) -> Option<Address> {
    env.storage().instance().get(&UPGRADER)
}

pub fn set_upgrader(env: &Env, upgrader: &Address) {
    env.storage().instance().set(&UPGRADER, upgrader);
}
