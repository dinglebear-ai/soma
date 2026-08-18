---
title: "Cortex Extraction Review Log"
created: 2026-08-17
updated: 2026-08-18
doc_type: "report"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-08-18"
---

# Cortex Extraction Review Log

This file records review findings and resolutions for the extraction foundation
branch. Command results are filled in after verification runs; do not treat a
planned check as passed evidence.

## Review 1: architecture and API inventory

### Finding A1: Cortex application models are not a clean domain boundary

**Severity:** P1 if copied wholesale.

Many public `app/models/**` types currently convert directly from `db::*` or
contain inventory, scanner, file-tail, notification DB, or runtime types. Moving
that directory into `cortex-domain` unchanged would make the shared domain
depend on implementations and either violate architecture direction or force DB
types into the domain package.

**Resolution:** The spec makes domain extraction a dedicated seam wave before
storage/application extraction. Conversion ownership moves to a legal adapter
layer; raw DB/runtime types are removed from public semantic contracts. No
`cortex-domain` crate is created in wave 0.

### Finding A2: enrichment output is coupled to DB batch rows

**Severity:** P1 if placed in `cortex-ingest-core`.

Parser logic is close to reusable, but dispatch/output code accepts
`db::LogBatchEntry` directly. Moving it into the pure ingest-core crate would
reverse the desired dependency direction.

**Resolution:** Wave 0 extracts only normalization and metadata safety. The
larger ingest wave must define a storage-neutral event/sink boundary before
moving dispatch/output.

### Finding A3: Cortex auth is pinned to external Labby auth

**Severity:** P1 for fleet reuse.

Runtime, MCP, OTLP, heartbeat, agent/transcript/shell ingestion, test helpers,
and OAuth tests reference `lab-auth` from the Labby repository. That prevents
Cortex extraction from converging on Soma's shared auth layer.

**Resolution:** ADR 0014 and C7 prohibit new direct `lab-auth` dependencies and
make migration to `soma-auth`/a shared adapter part of the surface/runtime
cutover, with auth parity tests required.

### Finding A4: a single full-Cortex extraction PR conflicts with Soma's own ADR 0009

**Severity:** P1 process/merge risk.

The requested end state spans workspace manifests, SQLite, ingest sources,
application/service code, REST/MCP, auth, runtime, deployment, CLI, and the final
binary. Moving all of it in one branch would create an unreviewable diff and
violate the repository's accepted isolated-lane extraction model.

**Resolution:** This branch is the integration contract/foundation plus a small
proof crate. The progress tracker defines later isolated lanes and an explicit
integration owner while still targeting a Cortex binary composed from the same
shared crates.

## Review 2: wave 0 behavior and security

Status: complete for the proof crate.

The donor diff confirms the normalization algorithm is unchanged; its only source
diffs are the public visibility required for reuse. The metadata algorithm is
likewise unchanged apart from module/public rustdoc, public visibility, and the
test-sidecar filename. Donor test files have no content diff. The external
consumer test imports only public APIs and exercises normalization, hashing,
redaction, bounds, and lossless bounded encoding.

### Finding B1: sha2 dependency drift broke donor source compatibility

**Severity:** P1 for behavior-preserving extraction.

The first manifest draft used `sha2 = "0.11"`, matching another current Soma
shared crate. Cortex's donor uses `sha2 = "0.10"`; 0.11 no longer implements
`LowerHex` for `finalize()` output, so the untouched donor
`format!("{:x}", hasher.finalize())` failed to compile.

**Resolution:** Pin `cortex-ingest-core` to donor-compatible `sha2 = "0.10"`
rather than rewriting the hashing implementation during extraction. Re-run
clippy/tests/rustdoc after the correction.

### Finding B2: toolchain lint rename conflicts with the fleet contract

**Severity:** P2 compatibility noise.

The pinned toolchain reports that `missing_crate_level_docs` moved from the Rust
lint namespace to `rustdoc::missing_crate_level_docs`. Moving the workspace key
to the new namespace removes that warning locally, but the shared repository and
fleet contract explicitly requires `rust.missing_crate_level_docs = deny`. The
first PR CI run correctly rejected the namespace move in both contract jobs.

**Resolution:** Preserve the fleet-required `[workspace.lints.rust]` key exactly
as it exists on main. The renamed-lint diagnostic is a known compatibility
warning and is exempt from `-D warnings`; changing the fleet contract belongs in
a coordinated workflows/repository-policy update, not this Cortex extraction.

### Finding B3: the Python import performance gate was scheduler-sensitive

**Severity:** P1 because it made the required documentation gate nondeterministic.

`cargo xtask check-docs` first passed with a 485.662 ms SDK import sample, then
failed on unchanged Python code at 997.089 ms. An isolated retry measured
1,190.363 ms. Five diagnostic subprocesses then showed the same import varying
from 163.531 ms to 872.346 ms wall-clock while child CPU time remained between
117.268 ms and 418.437 ms. The gate was therefore capable of rejecting host
scheduler contention as if it were SDK import work.

**Resolution:** Keep the existing 500 ms import budget, add the policy-controlled
`performance.sdk_import_trials = 5`, launch five fresh isolated Python
interpreters, enforce the best sample as the intrinsic cold-process import cost,
and emit every sample in the result for diagnosis. The exact `cargo xtask
check-docs` gate subsequently passed with a 286.721 ms best sample while the
other samples ranged as high as 905.138 ms. The Python platform specification
documents the sampling contract.

### Finding B4: the new crate was outside Soma's test-sibling coverage classifier

**Severity:** P1 because a new workspace member could otherwise bypass a
repository-wide source/test-layout invariant.

The first all-features Nextest run reached 2,983 of 2,994 passing tests and then
`xtask::test_siblings::every_workspace_member_src_root_is_classified` rejected
`crates/shared/cortex/ingest-core/src`: every workspace member must be either a
checked sibling-test tree or an explicitly documented exemption.

**Resolution:** Register `crates/shared/cortex/ingest-core/src` in
`CHECKED_SRC_ROOTS`. The crate already follows the required `foo.rs` +
`foo_tests.rs` convention. The focused `xtask` sibling tests and
`cargo xtask check-test-siblings` then passed with 23 source trees checked.

### Finding B5: full-workspace live tests require local tool prerequisites

**Severity:** P2 verification-environment issue, not a Cortex code defect.

The same Nextest pass exposed nine Python tests that fail closed when the
repository's frozen Python SDK environment has not been installed, plus a live
Codex app-server smoke test whose `codex` executable is a Mise shim. That smoke
test intentionally replaces `HOME`; on DOOKIE this caused Mise to treat the real
user config as untrusted inside the child process.

**Resolution:** Run the repository-prescribed
`uv sync --project packages/python --frozen` before the parity/supervisor tests.
For the DOOKIE live Codex smoke, pass the nonpersistent
`MISE_TRUSTED_CONFIG_PATHS=/home/jmagar/.config/mise/config.toml` and
`MISE_OFFLINE=1` environment to the test process. This keeps the user's global
Mise trust database untouched while allowing the already-installed Codex CLI to
run inside the isolated `HOME`. The targeted Codex smoke passed, the Python
authoring parity test passed, and the Python supervisor family passed 14 tests
with one intentionally ignored cgroup-only test. The final all-features Nextest
run then passed the entire runnable workspace suite.

### Finding B6: PR contract replay exposed two pre-existing soma-ops violations

**Severity:** P1 because the shared Rust fleet contract is a required PR gate.

After restoring the fleet-required lint namespace, replaying the exact contract
used by PR CI surfaced two violations already present on `origin/main`: the
optional `schemars` dependency in `soma-ops` used the semver range `1.2.1`
instead of the fleet-required exact pin, and the standalone external-consumer
fixture did not declare workspace lint inheritance.

**Resolution:** Change the optional dependency to `schemars = "=1.2.1"`, add an
empty local `[workspace.lints]` table plus `[lints] workspace = true` to the
standalone fixture, and refresh that fixture's lockfile. The fixture compiles,
`soma-ops` passes its all-features test suite, and the exact workflow-library
contract revision used by PR CI (`ac57c3208cf92d71c5971bb936df51c400cb1ccf`)
now reports `fleet contract valid`. These fixes are contract-only cleanup; the
root workspace was already locked to the same `schemars` version.

### Finding B7: PR coupled-file contract required script documentation

**Severity:** P1 because `Soma Contracts` is a required PR gate.

After the ASCII fixture fix, the next hosted run reached the coupled-file gate
and reported that the Python platform scripts changed without the required
`scripts/README.md` update. The implementation and provider specification were
already current, but the script catalog did not describe the new
`sdk_import_trials` policy or scheduler-resilient cold-import sampling.

**Resolution:** document both `check-python-platform-policy.py` and
`python-platform-gates.py` in the script quick index and reference section,
including the positive trial/budget policy, isolated-process sampling, best-sample
selection, emitted samples, and full-vs-developer soak behavior. This satisfies
the repository coupled-file ownership contract instead of weakening the gate.

### Finding B8: advisory database newly rejected locked `h2`

**Severity:** P1 because Cargo Deny is a required security gate and the advisory
is actionable.

The next hosted run refreshed the RustSec advisory database and rejected
`h2 0.4.15` for `GHSA-q83h-524g-xf6h`. The advisory reports `0.4.16` as the
patched release. This dependency state predates the Cortex extraction, but the
branch cannot remain knowingly vulnerable simply because the finding surfaced
while unrelated work was in review.

**Resolution:** update only the locked `h2` package from `0.4.15` to `0.4.16`
and its registry checksum. Cargo 1.97 initially rewrote unrelated Windows lock
edges during `cargo update`; those opportunistic changes were discarded so the
security patch remains minimal. `cargo metadata --locked` accepts the resulting
lockfile and `cargo deny check advisories` reports `advisories ok`.

### Result

No unexplained semantic donor diff remains in the proof crate. Public visibility
does not expose an unsanitized metadata path, normalizer versioning is public and
documented, and the crate has no database, network, runtime, auth, or Cortex
product dependency.

## Verification evidence

Wave 0 completed the repository verification contract on DOOKIE. The extraction
foundation is tracked in [PR #363](https://github.com/dinglebear-ai/soma/pull/363).

- `cargo fmt --all --check` passed.
- `cargo clippy -p cortex-ingest-core --all-targets --all-features -- -D warnings` passed.
- `cargo test -p cortex-ingest-core --all-features` passed 14 donor/unit tests and 3 external-consumer integration tests.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p cortex-ingest-core --no-deps --all-features` passed.
- Donor source diffs showed no unexplained normalization or metadata behavior change. Donor test behavior is unchanged; the non-ASCII normalization fixture is spelled with Rust Unicode escapes so the extracted source also satisfies Soma ASCII hygiene.
- `cargo xtask check-architecture` passed.
- `cargo xtask check-test-siblings` passed with 23 source trees checked after registering the new crate.
- `cargo xtask check-docs` passed after the import-budget gate was made scheduler-resilient; the passing run reported a 286.721 ms best import sample under the unchanged 500 ms budget.
- The PR-pinned shared fleet contract passed after restoring the fleet-required lint namespace and resolving the two pre-existing `soma-ops` contract violations.
- The standalone `soma-ops` external-consumer fixture compiled and `cargo test -p soma-ops --all-features` passed.
- `python3 -m py_compile scripts/python-platform-gates.py scripts/check-python-platform-policy.py`, `python3 scripts/check-python-platform-policy.py`, and the Python platform performance gate passed.
- `cargo check --workspace --all-features` passed.
- `MISE_TRUSTED_CONFIG_PATHS=/home/jmagar/.config/mise/config.toml MISE_OFFLINE=1 cargo nextest run --workspace --all-features` passed after the final contract cleanup: 2,994 tests run, 2,994 passed, 3 skipped; Nextest classified 3 passing tests as leaky.
- `git diff --check` passed throughout the review cycle.

The full workspace still emits three pre-existing, non-failing warnings unrelated
to this extraction: the toolchain reports the fleet-required Rust namespace for
`missing_crate_level_docs` as renamed, the vendored Codex app-server schema was
generated against `codex-cli 0.144.3` while DOOKIE currently reports `0.147.0`,
and Cargo warns that `incus-client` and `codex-app-server-client` both have an
example output named `basic`. None requires a Cortex behavior change, so this
branch records them without folding unrelated fleet-policy migration, schema
regeneration, or example renaming into the extraction.

## Review 3: Wave 1 domain seam

### Finding C1: the donor model module has four different owners

**Severity:** P1 if copied wholesale.

The donor exposes 255 public model declarations from one application module, but
the declarations do not share an architectural owner. The complete classification
records 65 semantic contracts, 165 transport DTO/policy types, 23 storage/query
projections, and 2 runtime/collector state types.

**Resolution:** [MODEL-CLASSIFICATION.md](MODEL-CLASSIFICATION.md) classifies all
255 declarations exactly once. All 65 semantic donor declarations are represented
in `cortex-domain`; no type classified as semantic remains unowned.

### Finding C2: storage types leaked through otherwise semantic contracts

**Severity:** P1 for a reusable domain crate.

The donor keeps 53 `impl From<db::...>` mappings beside public models and also
exposes raw heartbeat, MCP-event, and skill-event database types from semantic
aggregates.

**Resolution:** `cortex-domain` owns no database-row conversion. It introduces
domain-owned heartbeat contracts, uses `McpEventEntry` / `SkillEventEntry` in
evidence bundles, and assigns row-to-domain mapping to the Wave 2 SQLite adapter.
The donor remains unchanged until cutover, so extraction does not alter the live
Cortex product while dependency direction is being repaired.

### Finding C3: ServiceError mixes domain meaning with adapter failures

**Severity:** P1 if moved unchanged.

`ServiceError` combines invalid/not-found semantic outcomes with SQLite busy,
timeout, constraint, row, pool, and opaque runtime errors.

**Resolution:** the domain crate exposes only `DomainError::InvalidInput` and
`DomainError::NotFound`. Storage/application adapters retain operational error
classification and translate those failures at their surface boundaries.

### Finding C4: deterministic finding engines are domain behavior

**Severity:** P2 if left coupled to the monolithic application module.

The incident, hook, MCP, and skill finding engines are pure deterministic rule
evaluation. They query no database and invoke no model, but donor location under
`app/` obscured that property.

**Resolution:** all four engines move with their donor parity tests. Their only
adaptations are crate-local imports and replacing raw database event arguments
with domain event contracts. Existing evidence-id, conservative-confidence,
determinism, and unknown/open-question behavior remains covered.

### Finding C5: copied comments violated Soma ASCII source hygiene

**Severity:** P2 CI failure if left unresolved.

Donor comments used typographic punctuation and box-drawing characters.

**Resolution:** Rust source comments are normalized to ASCII spellings while code
and runtime strings remain unchanged. The domain source tree is ASCII-clean.

### Finding C6: transport envelopes are not domain contracts

**Severity:** P2 architecture drift.

Request/response envelopes, surface limit policy, graph response-navigation
metadata, maintenance/query result projections, and collector implementation
state were tempting to move because many are serde-only. Their semantics are
still surface, storage, or runtime-specific.

**Resolution:** these types remain explicitly assigned to later API/MCP/CLI,
application/query, SQLite, inventory, or runtime lanes in the model inventory.
The domain manifest contains only `serde`, `serde_json`, and `thiserror`.

### Finding C7: fanout timeout fixture raced two short timers under load

**Severity:** P1 for a trustworthy all-features gate.

The first final workspace Nextest run reached 3,037 passing tests but exposed a
pre-existing flake in `soma-fleet::fanout_classifies_failures_timeouts_and_partial_success`.
The fixture raced a 10 ms timeout against a 30 ms Tokio sleep. Under heavy
parallel test/compile load both timers can become ready before the runtime polls
them again, allowing the inner sleep result to win and incorrectly making the
fixture report three successes instead of two.

**Resolution:** replace the intentionally late branch with a permanently pending
future. That leaves the scheduler timeout as its only possible terminal path and
tests the behavior the fixture actually claims to test without wall-clock
racing. The targeted case and all 40 `soma-fleet` tests pass, the corrected case
passed 500 consecutive stress executions, and the subsequent full workspace
Nextest run passed 3,038/3,038. Production fanout logic is unchanged.

## Wave 1 final verification

- Cargo metadata registers `cortex-domain` as workspace member 42.
- All 255 donor public model declarations are classified exactly once; all 65
  semantic donor declarations are represented in the domain crate, and normalized
  shape comparison reports 65/65 matches after the documented adapter substitutions.
- `cargo check -p cortex-domain --all-features` and the final
  `cargo check --workspace --all-features` passed.
- `cargo clippy -p cortex-domain --all-targets --all-features -- -D warnings` passed.
- `cargo test -p cortex-domain --all-features` passed 42 unit/parity tests and 2
  independent-consumer integration tests.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p cortex-domain --no-deps --all-features`
  passed with only the known fleet-required renamed-lint warning, which rustdoc
  explicitly exempts from `-D warnings`.
- `cargo nextest run --workspace --all-features` passed 3,038/3,038 runnable tests
  with 3 skipped after resolving the surfaced fanout fixture race.
- `cargo xtask check-architecture` passed with 42 workspace packages and 92
  internal edges; `check-test-siblings` passed with 24 checked source trees.
- ASCII hygiene, coupled-file ownership, generated/docs checks, and the Python
  platform gates pass.
- The exact fleet contract implementation pinned by Soma CI at
  `ac57c3208cf92d71c5971bb936df51c400cb1ccf` reports `fleet contract valid`.
- Full `cargo deny check` reports advisories, bans, licenses, and sources all ok;
  the stacked lockfile contains patched `h2 0.4.16`.
- The crate source and manifest contain no database/pool, HTTP/MCP, auth, scanner,
  receiver, file-tail, config, or product-runtime dependency.
- The Rust source tree is ASCII-clean after comment-only normalization.
