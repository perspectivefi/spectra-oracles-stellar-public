// SPDX-License-Identifier: BUSL-1.1
use soroban_sdk::{
    testutils::Address as _, testutils::Ledger, vec, Address, BytesN, Env, Symbol, Vec,
};

use crate::{SpectraOracleFactory, SpectraOracleFactoryClient};

const UNIT: i128 = 1_000_000_000_000_000_000; // 1e18
const SECONDS_PER_YEAR: u64 = 31_536_000;

// WASM bytes for the oracle contract
const ORACLE_WASM: &[u8] =
    include_bytes!("../../../target/wasm32v1-none/release/spectra_oracle.wasm");

fn create_test_env() -> (
    Env,
    Address,
    Address,
    BytesN<32>,
    SpectraOracleFactoryClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    // Upload oracle WASM and get hash
    let oracle_wasm_hash = env.deployer().upload_contract_wasm(ORACLE_WASM);

    // Register factory
    let factory_id = env.register(SpectraOracleFactory, ());
    let factory_client = SpectraOracleFactoryClient::new(&env, &factory_id);

    let owner = Address::generate(&env);
    let upgrader = Address::generate(&env);

    (env, owner, upgrader, oracle_wasm_hash, factory_client)
}

/// Helper to call oracle view functions via invoke_contract
fn get_oracle_owner(env: &Env, oracle_address: &Address) -> Option<Address> {
    let empty: Vec<soroban_sdk::Val> = vec![env];
    env.invoke_contract(oracle_address, &Symbol::new(env, "get_owner"), empty)
}

fn get_oracle_maturity(env: &Env, oracle_address: &Address) -> u64 {
    let empty: Vec<soroban_sdk::Val> = vec![env];
    env.invoke_contract(oracle_address, &Symbol::new(env, "get_maturity"), empty)
}

fn get_oracle_initial_implied_apy(env: &Env, oracle_address: &Address) -> i128 {
    let empty: Vec<soroban_sdk::Val> = vec![env];
    env.invoke_contract(
        oracle_address,
        &Symbol::new(env, "get_initial_implied_apy"),
        empty,
    )
}

fn get_oracle_future_pt_value(env: &Env, oracle_address: &Address) -> i128 {
    let empty: Vec<soroban_sdk::Val> = vec![env];
    env.invoke_contract(
        oracle_address,
        &Symbol::new(env, "get_future_pt_value"),
        empty,
    )
}

fn get_oracle_decimals(env: &Env, oracle_address: &Address) -> u32 {
    let empty: Vec<soroban_sdk::Val> = vec![env];
    env.invoke_contract(oracle_address, &Symbol::new(env, "decimals"), empty)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct PriceData {
    price: i128,
    timestamp: u64,
}

/// Asset type matching the oracle contract's definition.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum Asset {
    Stellar(Address),
    Other(Symbol),
}

fn get_oracle_lastprice(env: &Env, oracle_address: &Address, pt: &Address) -> Option<PriceData> {
    use soroban_sdk::IntoVal;
    let asset = Asset::Stellar(pt.clone());
    let args: Vec<soroban_sdk::Val> = vec![env, asset.into_val(env)];
    env.invoke_contract(oracle_address, &Symbol::new(env, "lastprice"), args)
}

use soroban_sdk::contracttype;

#[test]
fn test_factory_initialization() {
    let (_env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    assert_eq!(factory.get_owner(), Some(owner));
    assert_eq!(factory.get_upgrader(), Some(upgrader));
    assert_eq!(factory.get_oracle_wasm_hash(), oracle_wasm_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_factory_double_initialization() {
    let (_, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);
    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);
}

#[test]
fn test_deploy_oracle() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    // Deploy an oracle
    let oracle_owner = Address::generate(&env);
    let pt = Address::generate(&env);
    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10; // 10%
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let oracle_address = factory.deploy_oracle(
        &oracle_owner,
        &pt,
        &maturity,
        &implied_apy,
        &future_pt_value,
        &decimals,
    );

    // Verify the oracle was deployed and initialized correctly
    assert_eq!(get_oracle_owner(&env, &oracle_address), Some(oracle_owner));
    assert_eq!(get_oracle_maturity(&env, &oracle_address), maturity);
    assert_eq!(
        get_oracle_initial_implied_apy(&env, &oracle_address),
        implied_apy
    );
    assert_eq!(
        get_oracle_future_pt_value(&env, &oracle_address),
        future_pt_value
    );
    assert_eq!(get_oracle_decimals(&env, &oracle_address), decimals);

    // Verify the oracle can calculate prices
    let pd = get_oracle_lastprice(&env, &oracle_address, &pt);
    assert!(pd.is_some());
    let price = pd.unwrap().price;
    assert!(price > 0);
    assert!(price < future_pt_value); // Price should be discounted
}

#[test]
fn test_deploy_multiple_oracles() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    let oracle_owner = Address::generate(&env);
    let pt = Address::generate(&env);

    // Deploy first oracle (1 year maturity)
    let maturity1 = current_time + SECONDS_PER_YEAR;
    let oracle1 =
        factory.deploy_oracle(&oracle_owner, &pt, &maturity1, &(UNIT / 10), &UNIT, &18u32);

    // Deploy second oracle (2 year maturity, different PT)
    let maturity2 = current_time + 2 * SECONDS_PER_YEAR;
    let pt2 = Address::generate(&env);
    let oracle2 = factory.deploy_oracle(
        &oracle_owner,
        &pt2,
        &maturity2,
        &(UNIT / 5), // 20% APY
        &UNIT,
        &18u32,
    );

    // Verify different addresses
    assert_ne!(oracle1, oracle2);

    // Verify each oracle has correct maturity
    assert_eq!(get_oracle_maturity(&env, &oracle1), maturity1);
    assert_eq!(get_oracle_maturity(&env, &oracle2), maturity2);
}

#[test]
fn test_update_oracle_wasm_hash() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    // Create a new WASM hash (in real scenario this would be a new version)
    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);

    factory.set_oracle_wasm_hash(&new_wasm_hash);

    assert_eq!(factory.get_oracle_wasm_hash(), new_wasm_hash);
}

#[test]
fn test_upgrader_can_change_upgrader() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    let new_upgrader = Address::generate(&env);
    factory.set_upgrader(&new_upgrader);

    assert_eq!(factory.get_upgrader(), Some(new_upgrader));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_deploy_oracle_not_initialized() {
    let (env, _, _, _, factory) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let oracle_owner = Address::generate(&env);
    let pt = Address::generate(&env);

    factory.deploy_oracle(
        &oracle_owner,
        &pt,
        &(current_time + SECONDS_PER_YEAR),
        &(UNIT / 10),
        &UNIT,
        &18u32,
    );
}

#[test]
fn test_deterministic_oracle_address() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    let oracle_owner = Address::generate(&env);
    let pt = Address::generate(&env);
    let maturity = current_time + SECONDS_PER_YEAR;

    let oracle1 = factory.deploy_oracle(&oracle_owner, &pt, &maturity, &(UNIT / 10), &UNIT, &18u32);

    // Different PT must produce a different address (salt is derived
    // from oracle parameters, so any parameter change distinguishes
    // addresses).
    let pt2 = Address::generate(&env);
    let oracle2 =
        factory.deploy_oracle(&oracle_owner, &pt2, &maturity, &(UNIT / 10), &UNIT, &18u32);

    assert_ne!(oracle1, oracle2);
}

// =========================================================================
// Front-running self-defeat — issue #35 (auditor finding L-04)
//
// These tests verify the claim behind parameter-derived salt: an attacker
// who watches the mempool cannot cause a DoS by submitting the same
// deployment first, because:
//   (a) mirroring all parameters yields the team's intended oracle,
//       owned by the team (attacker just paid the gas), and
//   (b) changing any parameter yields a different address — no
//       collision with the team's deployment.
// =========================================================================

/// (a): if the attacker mirrors every parameter (including
/// `oracle_owner = team`), the deployed oracle is exactly the one the
/// team intended, owned by the team. Front-running is self-defeating.
#[test]
fn test_front_running_mirrored_params_yields_team_oracle() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });
    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    let team = Address::generate(&env);
    let pt = Address::generate(&env);
    let maturity = current_time + SECONDS_PER_YEAR;
    let apy = UNIT / 10;
    let fpv = UNIT;
    let decimals = 18u32;

    // Simulate the attacker mirroring the team's exact parameters and
    // landing the tx first.
    let oracle = factory.deploy_oracle(&team, &pt, &maturity, &apy, &fpv, &decimals);

    // Regardless of who submitted the tx, the oracle's state is what
    // the team intended, including team-as-owner.
    assert_eq!(get_oracle_owner(&env, &oracle), Some(team));
    assert_eq!(get_oracle_maturity(&env, &oracle), maturity);
    assert_eq!(get_oracle_initial_implied_apy(&env, &oracle), apy);
    assert_eq!(get_oracle_future_pt_value(&env, &oracle), fpv);
    assert_eq!(get_oracle_decimals(&env, &oracle), decimals);
}

/// (b): if the attacker deviates on `oracle_owner` (or any other
/// parameter), they end up at a different address. The team's
/// deployment then succeeds normally — no collision, no DoS.
#[test]
fn test_front_running_different_owner_yields_distinct_addresses() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });
    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    let team_owner = Address::generate(&env);
    let attacker_owner = Address::generate(&env);
    let pt = Address::generate(&env);
    let maturity = current_time + SECONDS_PER_YEAR;

    // Attacker deploys first with their own oracle_owner.
    let attacker_oracle =
        factory.deploy_oracle(&attacker_owner, &pt, &maturity, &(UNIT / 10), &UNIT, &18u32);

    // Team's deployment (same pt/maturity/apy/etc., different owner)
    // succeeds at a different address.
    let team_oracle =
        factory.deploy_oracle(&team_owner, &pt, &maturity, &(UNIT / 10), &UNIT, &18u32);

    assert_ne!(team_oracle, attacker_oracle);
    assert_eq!(get_oracle_owner(&env, &team_oracle), Some(team_owner));
    assert_eq!(
        get_oracle_owner(&env, &attacker_oracle),
        Some(attacker_owner)
    );
}

/// Sanity check: two calls with truly identical parameters collide
/// (Soroban host panics on duplicate address). This is the "first
/// caller wins" property — combined with (a), it means the team always
/// ends up with their intended oracle.
#[test]
#[should_panic]
fn test_identical_params_second_deploy_collides() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });
    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    let oracle_owner = Address::generate(&env);
    let pt = Address::generate(&env);
    let maturity = current_time + SECONDS_PER_YEAR;

    factory.deploy_oracle(&oracle_owner, &pt, &maturity, &(UNIT / 10), &UNIT, &18u32);

    // Identical parameters → identical salt → identical address →
    // deploy_v2 panics with "ExistingValue".
    factory.deploy_oracle(&oracle_owner, &pt, &maturity, &(UNIT / 10), &UNIT, &18u32);
}

// --- Access Control Tests ---

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_non_upgrader_cannot_upgrade_factory() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    // Upload new WASM (we'll use the same one as a mock)
    let new_wasm_hash = env.deployer().upload_contract_wasm(ORACLE_WASM);

    // Attacker tries to upgrade the factory
    let attacker = Address::generate(&env);
    factory.upgrade(&new_wasm_hash, &attacker);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_owner_cannot_upgrade_factory() {
    let (env, owner, upgrader, oracle_wasm_hash, factory) = create_test_env();

    factory.initialize(&owner, &upgrader, &oracle_wasm_hash);

    // Upload new WASM
    let new_wasm_hash = env.deployer().upload_contract_wasm(ORACLE_WASM);

    // Owner tries to upgrade - should fail because only upgrader can upgrade
    factory.upgrade(&new_wasm_hash, &owner);
}
