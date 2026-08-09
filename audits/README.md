# Security Assessment

Certora assessed the Spectra Bridge system from **1 May 2026 through 18 May
2026**. The assessment included these oracles as a separately scoped component.

- Standalone oracle revision: `ce34890af40625da8a82c9a6fc10191a47cf4d24`
- Final audit monorepo revision: `729e91414f33275df56b35f0e98111c73b8469ed`
- Report: `2026-05-Certora-Spectra-Bridge.pdf`
- Report SHA-256: `42efc46a93ecbf4ad6295aba9cb8cf665dd48681729a7e554afc7f6acb62f4b8`
- Report Git blob: `5eb7c7c2f9bac9793dfe86611d1279f81c1d3256`

The exact five scoped production files were:

1. `contracts/spectra-oracle/src/lib.rs`
2. `contracts/spectra-oracle/src/math.rs`
3. `contracts/spectra-oracle/src/storage.rs`
4. `contracts/spectra-oracle-factory/src/lib.rs`
5. `contracts/spectra-oracle-factory/src/storage.rs`

L-04, deterministic oracle deployment front-running causing denial of service,
was fixed at the standalone audited revision and Certora confirmed the fix.
I-02, that oracle decimals are metadata and values are not normalized against
them, was acknowledged. Integrators must provide `future_pt_value` at the
declared decimal precision and validate oracle selection independently. The
factory intentionally permits oracle creation; consumers must track and trust
the intended oracle rather than treating any deployment for a PT as canonical.

The production code on private `master` at release preparation was byte-identical
to standalone revision `ce34890af40625da8a82c9a6fc10191a47cf4d24`. This public
tree adds a leading SPDX comment and applies Rust `1.96.0` rustfmt to the five
production files. Those release-only textual changes alter file bytes but not
Rust source semantics or executable behavior. Formatting the audited files with
the pinned formatter and disregarding the added first SPDX line reproduces the
public production sources exactly. Tests, manifests, CI, and documentation are
release scaffolding and are not claimed as audit scope.
