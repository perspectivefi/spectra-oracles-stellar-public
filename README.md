# Spectra Stellar Oracles

Soroban contracts providing deterministic Principal Token (PT) prices through
the zero-coupon-bond model and the SEP-40 oracle interface.

## Purpose

For a configured implied APY, maturity, and future PT value, the oracle returns:

```text
price = future_pt_value / (1 + implied_apy)^(time_remaining / year)
```

The price converges to `future_pt_value` at maturity and remains fixed after
maturity. `decimals` is metadata; callers must supply and interpret
`future_pt_value` using that precision. See [Security](#security).

## Architecture

- `spectra-oracle` implements SEP-40 pricing, owner-managed future PT value,
  role-separated upgrades, ownership transfer, and permissionless TTL bumps.
- `spectra-oracle-factory` stores the oracle WASM hash and deploys initialized
  oracle instances. Deployment entropy is derived on-chain from oracle
  parameters, addressing audit finding L-04.
- `contracts/spectra-oracle/src/math.rs` ports Balancer LogExpMath under MIT.

The factory is not a registry of canonical oracles. Integrators must identify
and trust the intended deployment independently.

## Prerequisites

- Rust `1.96.0` via rustup; `rust-toolchain.toml` installs it automatically
- `wasm32v1-none` Rust target
- Stellar CLI for `make build` / `make optimize`
- Binaryen `wasm-opt` only when using `scripts/build.sh`

## Build

```bash
cargo build --workspace --release --target wasm32v1-none
```

Or use Stellar CLI:

```bash
make build
make optimize
```

`scripts/build.sh` builds release WASM and additionally applies Binaryen size
optimization when `wasm-opt` is installed. Generated WASM remains under
`target/` and is not committed.

## Tests

Build contract WASM before running the complete test suite:

```bash
cargo build --workspace --release --target wasm32v1-none
cargo test --workspace
make lint
cargo fmt --all -- --check
```

Tests include unit, access-control, lifecycle, resource-budget, and JSON-vector
cross-validation coverage.

## Deployment

Deployment scripts are intentionally omitted from this release. Build reviewed
WASM, install the oracle WASM, deploy and initialize the factory with `owner`,
`upgrader`, and `oracle_wasm_hash`, then call `deploy_oracle` using the current
contract ABI. Use a Stellar CLI identity alias via `--source-account` and an
official network alias or an explicitly reviewed configurable public RPC.
Separate owner and upgrader roles and verify every parameter before signing.

## Audit

The Certora final report and precise scope/revision relationship are in
[`audits/`](audits/README.md). The audit ran 1-18 May 2026. L-04 was fixed and
confirmed; I-02 was acknowledged.

An audit is not a guarantee of correctness. Review the report, code, trust
assumptions, and deployment configuration before use.

## Security

- `decimals` does not normalize or validate returned values. The owner controls
  `future_pt_value`; consumers trust its units and updates.
- Anyone may create an oracle through the factory. A deployment is not canonical
  merely because its PT address matches.
- Owner and upgrader are privileged roles and should be separated and secured.
- Long-lived contracts require TTL management through the permissionless bump
  operation.

See [`SECURITY.md`](SECURITY.md) for vulnerability reporting.

## License

The project is licensed under [BUSL-1.1](LICENSE), except the Balancer-derived
math file, which is MIT-licensed as documented in
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
