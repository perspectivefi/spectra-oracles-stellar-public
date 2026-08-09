// SPDX-License-Identifier: BUSL-1.1
use soroban_sdk::{
    symbol_short, testutils::Address as _, testutils::Events, testutils::Ledger, Address, Env,
    IntoVal, String,
};

use crate::{Asset, SpectraOracle, SpectraOracleClient};

// WASM bytes for upgrade testing (loaded at compile time)
const WASM: &[u8] = include_bytes!("../../../target/wasm32v1-none/release/spectra_oracle.wasm");

const UNIT: i128 = 1_000_000_000_000_000_000; // 1e18
const SECONDS_PER_YEAR: u64 = 31_536_000;

/// Network max TTL in ledgers (~6 months at 5s/ledger).
const MAX_ENTRY_TTL: u32 = 3_110_400;

fn create_test_env() -> (Env, Address, Address, SpectraOracleClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    // Set a realistic max_entry_ttl so TTL tests behave like mainnet.
    env.ledger().with_mut(|li| {
        li.max_entry_ttl = MAX_ENTRY_TTL;
    });

    let contract_id = env.register(SpectraOracle, ());
    let client = SpectraOracleClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let upgrader = Address::generate(&env);

    (env, owner, upgrader, client)
}

/// Helper that initializes with default params and returns the PT as an Asset.
fn initialize_default(
    env: &Env,
    client: &SpectraOracleClient,
    owner: &Address,
    upgrader: &Address,
    maturity: u64,
    implied_apy: i128,
    future_pt_value: i128,
    decimals: u32,
) -> Asset {
    let pt = Address::generate(env);
    client.initialize(
        owner,
        upgrader,
        &pt,
        &maturity,
        &implied_apy,
        &future_pt_value,
        &decimals,
    );
    Asset::Stellar(pt)
}

// =========================================================================
// Initialization tests
// =========================================================================

#[test]
fn test_initialization() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10; // 10% APY
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let pt = Address::generate(&env);
    client.initialize(
        &owner,
        &upgrader,
        &pt,
        &maturity,
        &implied_apy,
        &future_pt_value,
        &decimals,
    );

    assert_eq!(client.get_owner(), Some(owner));
    assert_eq!(client.get_maturity(), maturity);
    assert_eq!(client.get_start_time(), current_time);
    assert_eq!(client.get_initial_implied_apy(), implied_apy);
    assert_eq!(client.get_future_pt_value(), future_pt_value);
    assert_eq!(client.decimals(), decimals);
    assert_eq!(client.get_pt(), pt);
}

#[test]
fn test_price_at_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Move time to maturity
    env.ledger().with_mut(|li| {
        li.timestamp = maturity;
    });

    let pd = client.lastprice(&asset).unwrap();
    assert_eq!(pd.price, future_pt_value);
}

#[test]
fn test_price_at_start() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10; // 10% APY
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    let pd = client.lastprice(&asset).unwrap();
    let price = pd.price;

    // ZCB formula: price = FV / (1 + r)^t
    // With 10% APY and 1 year to maturity:
    // price = 1 / (1.1)^1 = 0.909090909...
    let expected = 909_090_909_090_909_090;

    let diff = if price > expected {
        price - expected
    } else {
        expected - price
    };
    let tolerance = UNIT / 1000; // 0.1%
    assert!(
        diff < tolerance,
        "Price {} not within tolerance of expected {}",
        price,
        expected
    );
}

#[test]
fn test_price_halfway_to_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Move time to 6 months
    env.ledger().with_mut(|li| {
        li.timestamp = current_time + SECONDS_PER_YEAR / 2;
    });

    let pd = client.lastprice(&asset).unwrap();
    let price = pd.price;

    // ZCB formula: price = 1 / (1.1)^0.5 ~ 0.953462589...
    let expected = 953_462_589_245_592_230;

    let diff = if price > expected {
        price - expected
    } else {
        expected - price
    };
    let tolerance = UNIT / 1000;
    assert!(
        diff < tolerance,
        "Price {} not within tolerance of expected {}",
        price,
        expected
    );
}

#[test]
fn test_price_past_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Move time past maturity
    env.ledger().with_mut(|li| {
        li.timestamp = maturity + 1000;
    });

    let pd = client.lastprice(&asset).unwrap();
    assert_eq!(pd.price, future_pt_value);
}

#[test]
fn test_set_future_pt_value() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Update future PT value (owner only)
    let new_value = UNIT + UNIT / 10; // 1.1
    client.set_future_pt_value(&new_value);

    assert_eq!(client.get_future_pt_value(), new_value);
}

#[test]
fn test_owner_is_set() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    assert_eq!(client.get_owner(), Some(owner));
}

#[test]
fn test_higher_apy() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 4; // 25% APY
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    let pd = client.lastprice(&asset).unwrap();
    let price = pd.price;

    // ZCB formula: price = 1 / (1.25)^1 = 0.8
    let expected = 800_000_000_000_000_000;

    let diff = if price > expected {
        price - expected
    } else {
        expected - price
    };
    let tolerance = UNIT / 1000;
    assert!(
        diff < tolerance,
        "Price {} not within tolerance of expected {}",
        price,
        expected
    );
}

#[test]
fn test_description() {
    let (_env, _, _upgrader, client) = create_test_env();

    let description = client.get_description();
    let expected = String::from_str(
        &_env,
        "Spectra Deterministic Oracle - Zero Coupon Bond Model",
    );
    assert_eq!(description, expected);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialization() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;
    let pt = Address::generate(&env);

    client.initialize(
        &owner,
        &upgrader,
        &pt,
        &maturity,
        &implied_apy,
        &future_pt_value,
        &decimals,
    );
    client.initialize(
        &owner,
        &upgrader,
        &pt,
        &maturity,
        &implied_apy,
        &future_pt_value,
        &decimals,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_invalid_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time - 1; // In the past
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_invalid_apy_zero() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = 0;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_invalid_apy_over_100() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT + 1; // Over 100%
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );
}

#[test]
fn test_upgrade_by_owner() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SpectraOracle, ());
    let client = SpectraOracleClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let upgrader = Address::generate(&env);

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Upload the WASM and get the hash
    let new_wasm_hash = env.deployer().upload_contract_wasm(WASM);

    // Upgrader upgrades the contract
    client.upgrade(&new_wasm_hash, &upgrader);

    // Verify the contract still works after upgrade
    let pd = client.lastprice(&asset);
    assert!(pd.is_some());
    assert!(pd.unwrap().price > 0);

    // Verify owner is still set
    assert_eq!(client.get_owner(), Some(owner));
}

#[test]
fn test_upgrade_preserves_state() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SpectraOracle, ());
    let client = SpectraOracleClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let upgrader = Address::generate(&env);

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Get price before upgrade
    let price_before = client.lastprice(&asset).unwrap().price;

    // Upload and upgrade (upgrader is the upgrade authority)
    let new_wasm_hash = env.deployer().upload_contract_wasm(WASM);
    client.upgrade(&new_wasm_hash, &upgrader);

    // Verify state is preserved after upgrade
    assert_eq!(client.get_owner(), Some(owner));
    assert_eq!(client.get_maturity(), maturity);
    assert_eq!(client.get_initial_implied_apy(), implied_apy);
    assert_eq!(client.get_future_pt_value(), future_pt_value);
    assert_eq!(client.decimals(), decimals);

    // Price should be the same
    let price_after = client.lastprice(&asset).unwrap().price;
    assert_eq!(price_before, price_after);
}

// --- Access Control Tests ---

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_non_upgrader_cannot_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SpectraOracle, ());
    let client = SpectraOracleClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let upgrader = Address::generate(&env);
    let attacker = Address::generate(&env);

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Upload WASM
    let new_wasm_hash = env.deployer().upload_contract_wasm(WASM);

    // Attacker tries to upgrade - should fail because only upgrader can upgrade
    client.upgrade(&new_wasm_hash, &attacker);
}

// --- Edge Case Tests ---

#[test]
fn test_boundary_apy_minimum() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10000; // 0.01% APY (minimum practical)
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    let pd = client.lastprice(&asset).unwrap();
    let price = pd.price;

    // With 0.01% APY: price = 1 / (1.0001)^1 ~ 0.99990001
    // Price should be very close to future_pt_value
    assert!(price < future_pt_value);
    assert!(price > future_pt_value - UNIT / 100); // Within 1% of face value
}

#[test]
fn test_boundary_apy_maximum() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT; // Exactly 100% APY
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    let pd = client.lastprice(&asset).unwrap();
    let price = pd.price;

    // With 100% APY: price = 1 / (2)^1 = 0.5
    let expected = UNIT / 2;
    let diff = if price > expected {
        price - expected
    } else {
        expected - price
    };
    let tolerance = UNIT / 1000;
    assert!(
        diff < tolerance,
        "Price {} not within tolerance of expected {}",
        price,
        expected
    );
}

#[test]
fn test_very_short_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + 86400; // 1 day maturity
    let implied_apy = UNIT / 10; // 10% APY
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    let pd = client.lastprice(&asset).unwrap();
    let price = pd.price;

    // With 1 day to maturity, price should be very close to future value
    assert!(price < future_pt_value);
    assert!(price > future_pt_value - UNIT / 100); // Within 1%
}

#[test]
fn test_very_long_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + 10 * SECONDS_PER_YEAR; // 10 years
    let implied_apy = UNIT / 10; // 10% APY
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    let pd = client.lastprice(&asset).unwrap();
    let price = pd.price;

    // With 10% APY and 10 years: price = 1 / (1.1)^10 ~ 0.3855
    let expected = 385_543_289_429_533_660i128;
    let diff = if price > expected {
        price - expected
    } else {
        expected - price
    };
    let tolerance = UNIT / 100; // 1% tolerance for long duration
    assert!(
        diff < tolerance,
        "Price {} not within tolerance of expected {}",
        price,
        expected
    );
}

#[test]
fn test_different_future_pt_value() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10; // 10% APY
    let future_pt_value = 2 * UNIT; // 2.0 face value
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    let pd = client.lastprice(&asset).unwrap();
    let price = pd.price;

    // With 10% APY and FV=2: price = 2 / (1.1)^1 ~ 1.818
    let expected = 2 * 909_090_909_090_909_090i128;
    let diff = if price > expected {
        price - expected
    } else {
        expected - price
    };
    let tolerance = UNIT / 100;
    assert!(
        diff < tolerance,
        "Price {} not within tolerance of expected {}",
        price,
        expected
    );
}

#[test]
fn test_small_future_pt_value() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT / 1000; // 0.001 face value
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    let pd = client.lastprice(&asset).unwrap();
    let price = pd.price;

    // Price should be proportionally discounted
    assert!(price > 0);
    assert!(price < future_pt_value);
}

#[test]
fn test_lastprice_not_initialized() {
    let (env, _, _upgrader, client) = create_test_env();

    // lastprice on an uninitialized contract should return None
    let asset = Asset::Stellar(Address::generate(&env));
    let result = client.lastprice(&asset);
    assert!(result.is_none());
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_maturity_not_initialized() {
    let (_, _, _upgrader, client) = create_test_env();

    client.get_maturity();
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_set_future_pt_value_zero() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Try to set future PT value to zero
    client.set_future_pt_value(&0);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_set_future_pt_value_negative() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Try to set future PT value to negative
    client.set_future_pt_value(&-1);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_initialize_zero_future_pt_value() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = 0; // Invalid
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_negative_apy() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = -1; // Negative APY
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_maturity_at_current_time() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time; // Exactly at current time (not in future)
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );
}

#[test]
fn test_price_progression_over_time() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Get price at different time points and verify monotonic increase
    let price_at_start = client.lastprice(&asset).unwrap().price;

    env.ledger().with_mut(|li| {
        li.timestamp = current_time + SECONDS_PER_YEAR / 4; // 3 months
    });
    let price_at_quarter = client.lastprice(&asset).unwrap().price;

    env.ledger().with_mut(|li| {
        li.timestamp = current_time + SECONDS_PER_YEAR / 2; // 6 months
    });
    let price_at_half = client.lastprice(&asset).unwrap().price;

    env.ledger().with_mut(|li| {
        li.timestamp = current_time + 3 * SECONDS_PER_YEAR / 4; // 9 months
    });
    let price_at_three_quarters = client.lastprice(&asset).unwrap().price;

    env.ledger().with_mut(|li| {
        li.timestamp = maturity;
    });
    let price_at_maturity = client.lastprice(&asset).unwrap().price;

    // Verify prices increase monotonically towards maturity
    assert!(
        price_at_start < price_at_quarter,
        "Price should increase from start to quarter"
    );
    assert!(
        price_at_quarter < price_at_half,
        "Price should increase from quarter to half"
    );
    assert!(
        price_at_half < price_at_three_quarters,
        "Price should increase from half to three quarters"
    );
    assert!(
        price_at_three_quarters < price_at_maturity,
        "Price should increase from three quarters to maturity"
    );
    assert_eq!(price_at_maturity, future_pt_value);
}

// --- Resource Usage Tests ---

#[test]
fn test_resource_usage_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SpectraOracle, ());
    let client = SpectraOracleClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let upgrader = Address::generate(&env);

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let implied_apy = UNIT / 10;
    let future_pt_value = UNIT;
    let decimals = 18u32;

    // Reset budget tracking
    env.cost_estimate().budget().reset_default();

    // Call initialize
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        implied_apy,
        future_pt_value,
        decimals,
    );

    // Get resource usage
    let cpu_insns = env.cost_estimate().budget().cpu_instruction_cost();
    let mem_bytes = env.cost_estimate().budget().memory_bytes_cost();

    // Assert reasonable limits
    assert!(
        cpu_insns < 10_000_000,
        "initialize CPU usage too high: {}",
        cpu_insns
    );
    assert!(
        mem_bytes < 1_000_000,
        "initialize memory usage too high: {}",
        mem_bytes
    );
}

// =========================================================================
// Price query tests
// =========================================================================

#[test]
fn test_price_at_timestamp() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Request price at a specific timestamp
    let query_ts = current_time + 600; // 10 minutes after start
    let result = client.price(&query_ts);
    assert!(result.is_some());

    let pd = result.unwrap();
    assert_eq!(pd.timestamp, query_ts);
    assert!(pd.price > 0);
}

#[test]
fn test_price_before_start() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Query before oracle start time should return None
    let result = client.price(&(current_time - 1000));
    assert!(result.is_none());
}

#[test]
fn test_lastprice_returns_current_timestamp() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    let result = client.lastprice(&asset);
    assert!(result.is_some());

    let pd = result.unwrap();
    assert_eq!(pd.timestamp, current_time);
    assert!(pd.price > 0);
}

#[test]
fn test_decimals() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    assert_eq!(client.decimals(), 18);
}

// --- Resource usage for price methods ---

#[test]
fn test_resource_usage_lastprice() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    env.cost_estimate().budget().reset_default();
    let _result = client.lastprice(&asset);

    let cpu_insns = env.cost_estimate().budget().cpu_instruction_cost();
    let mem_bytes = env.cost_estimate().budget().memory_bytes_cost();

    assert!(
        cpu_insns < 10_000_000,
        "lastprice CPU usage too high: {}",
        cpu_insns
    );
    assert!(
        mem_bytes < 1_000_000,
        "lastprice memory usage too high: {}",
        mem_bytes
    );
}

// =========================================================================
// Permissionless bump
// =========================================================================

#[test]
fn test_bump_succeeds() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Advance time — TTLs decay but oracle is still within grace period
    env.ledger().with_mut(|li| {
        li.timestamp = current_time + SECONDS_PER_YEAR / 2;
    });

    // Anyone can call bump — no auth required
    client.bump(&env.storage().max_ttl());
}

#[test]
fn test_bump_not_initialized() {
    let (env, _owner, _upgrader, client) = create_test_env();

    // bump() should return NotInitialized error
    let result = client.try_bump(&env.storage().max_ttl());
    assert!(result.is_err());
}

#[test]
fn test_bump_by_random_address() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Advance time
    env.ledger().with_mut(|li| {
        li.timestamp = current_time + 86_400 * 30; // 30 days later
    });

    // A completely random address (not owner) can bump
    client.bump(&env.storage().max_ttl());

    // Oracle still works after bump
    let price = client.lastprice(&asset);
    assert!(price.is_some());
}

// =========================================================================
// Overflow boundary tests
// =========================================================================

#[test]
fn test_overflow_max_apy_long_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    // 100% APY, 10 year maturity — maximum realistic exponent
    let maturity = current_time + SECONDS_PER_YEAR * 10;
    let asset = initialize_default(&env, &client, &owner, &upgrader, maturity, UNIT, UNIT, 18);

    let price = client.lastprice(&asset);
    assert!(price.is_some());
    assert!(price.unwrap().price > 0);
}

#[test]
fn test_overflow_max_apy_large_future_pt_value() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    // 100% APY, FV = 100.0 — stresses the fv * UNIT intermediate
    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT,
        100 * UNIT,
        18,
    );

    let price = client.lastprice(&asset);
    assert!(price.is_some());
    assert!(price.unwrap().price > 0);
}

#[test]
fn test_overflow_min_apy_short_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    // APY = 1 (smallest valid), maturity = +1 second
    let maturity = current_time + 1;
    let asset = initialize_default(&env, &client, &owner, &upgrader, maturity, 1, UNIT, 18);

    let price = client.lastprice(&asset);
    assert!(price.is_some());
    assert!(price.unwrap().price > 0);
}

#[test]
fn test_overflow_set_future_pt_value_then_price() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Update to a large-but-realistic face value
    client.set_future_pt_value(&(50 * UNIT));

    let price = client.lastprice(&asset);
    assert!(price.is_some());
    assert!(price.unwrap().price > 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_overflow_extreme_maturity_rejected_at_init() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    // 100% APY, 200 year maturity — exponent exceeds exp()'s 130e18 domain.
    // initialize() validates that the initial price is non-zero, so this
    // combination is rejected at deployment time.
    let maturity = current_time + SECONDS_PER_YEAR * 200;
    initialize_default(&env, &client, &owner, &upgrader, maturity, UNIT, UNIT, 18);
}

// =========================================================================
// Guard tests
// =========================================================================

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_decimals_not_initialized() {
    let (_env, _, _upgrader, client) = create_test_env();

    // decimals() on an uninitialized contract should return NotInitialized error
    client.decimals();
}

#[test]
fn test_set_upgrader_extends_ttl() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Advance time significantly
    env.ledger().with_mut(|li| {
        li.timestamp = current_time + SECONDS_PER_YEAR / 2;
    });

    // set_upgrader should succeed and extend TTLs (no panic)
    let new_upgrader = Address::generate(&env);
    client.set_upgrader(&new_upgrader);

    // Oracle still works after set_upgrader (TTLs were refreshed)
    let price = client.lastprice(&asset);
    assert!(price.is_some());
    assert!(price.unwrap().price > 0);
}

// =========================================================================
// price()
// =========================================================================

#[test]
fn test_price_not_initialized() {
    let (_env, _, _, client) = create_test_env();

    // price() on an uninitialized contract should return None
    let result = client.price(&1_700_000_000u64);
    assert!(result.is_none());
}

#[test]
fn test_price_at_exact_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let future_pt_value = UNIT;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        future_pt_value,
        18,
    );

    // price(maturity) should return future_pt_value (time_left = 0)
    let result = client.price(&maturity);
    assert!(result.is_some());
    let pd = result.unwrap();
    assert_eq!(pd.price, future_pt_value);
    assert_eq!(pd.timestamp, maturity);
}

#[test]
fn test_price_past_maturity_via_price_fn() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let future_pt_value = UNIT;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        future_pt_value,
        18,
    );

    // price(maturity + 1000) should clamp to maturity and return FPV
    let result = client.price(&(maturity + 1000));
    assert!(result.is_some());
    let pd = result.unwrap();
    assert_eq!(pd.price, future_pt_value);
    assert_eq!(pd.timestamp, maturity + 1000);
}

#[test]
fn test_price_at_exact_start_time() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // price(start_time) should work (boundary: not before start)
    let result = client.price(&current_time);
    assert!(result.is_some());
    let pd = result.unwrap();
    assert!(pd.price > 0);
    assert!(pd.price < UNIT); // Should be discounted
    assert_eq!(pd.timestamp, current_time);
}

#[test]
fn test_price_one_second_before_start() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // price(start_time - 1) should return None
    let result = client.price(&(current_time - 1));
    assert!(result.is_none());
}

#[test]
fn test_price_consistent_with_lastprice() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // price(current_time) should match lastprice()
    let price_result = client.price(&current_time).unwrap();
    let lastprice_result = client.lastprice(&asset).unwrap();

    assert_eq!(price_result.price, lastprice_result.price);
}

#[test]
fn test_price_monotonic_increase() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Query price() at increasing timestamps — should monotonically increase
    let t0 = current_time;
    let t1 = current_time + SECONDS_PER_YEAR / 4;
    let t2 = current_time + SECONDS_PER_YEAR / 2;
    let t3 = current_time + 3 * SECONDS_PER_YEAR / 4;
    let t4 = maturity;

    let p0 = client.price(&t0).unwrap().price;
    let p1 = client.price(&t1).unwrap().price;
    let p2 = client.price(&t2).unwrap().price;
    let p3 = client.price(&t3).unwrap().price;
    let p4 = client.price(&t4).unwrap().price;

    assert!(p0 < p1, "price should increase: {} < {}", p0, p1);
    assert!(p1 < p2, "price should increase: {} < {}", p1, p2);
    assert!(p2 < p3, "price should increase: {} < {}", p2, p3);
    assert!(p3 < p4, "price should increase: {} < {}", p3, p4);
    assert_eq!(p4, UNIT); // At maturity = FPV
}

// =========================================================================
// set_future_pt_value — extended tests
// =========================================================================

#[test]
fn test_set_future_pt_value_changes_price() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    let price_before = client.lastprice(&asset).unwrap().price;

    // Double the future PT value
    client.set_future_pt_value(&(2 * UNIT));

    let price_after = client.lastprice(&asset).unwrap().price;

    // Price should roughly double (same discount factor, double face value)
    assert!(
        price_after > price_before,
        "price should increase after raising FPV"
    );
    let ratio = (price_after * UNIT) / price_before;
    // ratio should be ~2e18 (allow 1% tolerance)
    assert!(
        ratio > 198 * UNIT / 100 && ratio < 202 * UNIT / 100,
        "price ratio should be ~2x, got {}",
        ratio
    );
}

#[test]
fn test_set_future_pt_value_after_time_progression() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Advance to halfway through maturity
    let mid_time = current_time + SECONDS_PER_YEAR / 2;
    env.ledger().with_mut(|li| {
        li.timestamp = mid_time;
    });

    let price_before = client.lastprice(&asset).unwrap().price;

    // Update FPV to 1.5
    client.set_future_pt_value(&(3 * UNIT / 2));

    let price_after = client.lastprice(&asset).unwrap().price;

    // Price should increase by 1.5x (same discount, 1.5x face value)
    assert!(price_after > price_before);
    let ratio = (price_after * UNIT) / price_before;
    assert!(
        ratio > 148 * UNIT / 100 && ratio < 152 * UNIT / 100,
        "price ratio should be ~1.5x, got {}",
        ratio
    );
}

#[test]
fn test_set_future_pt_value_at_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Advance to maturity
    env.ledger().with_mut(|li| {
        li.timestamp = maturity;
    });

    // At maturity, lastprice returns FPV directly
    assert_eq!(client.lastprice(&asset).unwrap().price, UNIT);

    // Update FPV
    client.set_future_pt_value(&(2 * UNIT));

    // Now lastprice should return the new FPV
    assert_eq!(client.lastprice(&asset).unwrap().price, 2 * UNIT);
}

#[test]
fn test_set_future_pt_value_past_maturity() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Advance past maturity
    env.ledger().with_mut(|li| {
        li.timestamp = maturity + 86_400;
    });

    // Update FPV post-maturity — should still work
    client.set_future_pt_value(&(5 * UNIT));
    assert_eq!(client.get_future_pt_value(), 5 * UNIT);
    assert_eq!(client.lastprice(&asset).unwrap().price, 5 * UNIT);
}

#[test]
fn test_set_future_pt_value_emits_event() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    let new_value = 2 * UNIT;
    client.set_future_pt_value(&new_value);

    // Verify event was emitted — at least one event should exist after the update
    let events = env.events().all();
    assert!(!events.is_empty(), "should have emitted at least one event");
}

#[test]
fn test_set_future_pt_value_extends_ttl() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Advance time significantly
    env.ledger().with_mut(|li| {
        li.timestamp = current_time + SECONDS_PER_YEAR / 2;
    });

    // set_future_pt_value should succeed and extend TTLs (no panic)
    client.set_future_pt_value(&(2 * UNIT));

    // Oracle still works after update (TTLs were refreshed)
    let price = client.lastprice(&asset);
    assert!(price.is_some());
    assert!(price.unwrap().price > 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2100)")]
fn test_set_future_pt_value_not_initialized() {
    let (_env, _, _, client) = create_test_env();

    // set_future_pt_value on uninitialized contract should fail
    // (#2100 is from the #[only_owner] macro — no owner is set)
    client.set_future_pt_value(&UNIT);
}

#[test]
fn test_set_future_pt_value_multiple_updates() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Perform several updates and verify the price tracks each change
    let values = [UNIT / 2, 2 * UNIT, UNIT, 3 * UNIT];
    for &val in &values {
        client.set_future_pt_value(&val);
        assert_eq!(client.get_future_pt_value(), val);

        let price = client.lastprice(&asset).unwrap().price;
        assert!(price > 0, "price should be positive for FPV={}", val);
        // Pre-maturity price should be less than FPV (discounted)
        assert!(price < val, "price {} should be < FPV {}", price, val);
    }
}

#[test]
fn test_set_future_pt_value_price_via_price_fn() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    let asset = initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // Update FPV
    client.set_future_pt_value(&(3 * UNIT));

    // price(timestamp) and lastprice() should be consistent with new FPV
    let lp = client.lastprice(&asset).unwrap();
    let p = client.price(&current_time).unwrap();
    assert_eq!(lp.price, p.price);

    // price(maturity) should return the new FPV
    let p_mat = client.price(&maturity).unwrap();
    assert_eq!(p_mat.price, 3 * UNIT);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_set_future_pt_value_tiny_value_rejected() {
    let (env, owner, upgrader, client) = create_test_env();

    let current_time = 1_700_000_000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = current_time;
    });

    let maturity = current_time + SECONDS_PER_YEAR;
    initialize_default(
        &env,
        &client,
        &owner,
        &upgrader,
        maturity,
        UNIT / 10,
        UNIT,
        18,
    );

    // 1 wei is too small to produce a non-zero discounted price — sanity check rejects it
    client.set_future_pt_value(&1i128);
}
