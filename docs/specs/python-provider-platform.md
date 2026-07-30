---
title: "Python Provider Platform Plan"
doc_type: "spec"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: true
upstream_refs:
  - "docs/adr/0013-python-provider-authoring-boundary.md"
  - "docs/specs/dynamic-provider-runtime.md"
  - "docs/PROVIDERS.md"
  - "crates/shared/provider-adapters/src/python.rs"
  - "crates/shared/provider-adapters/src/python_protocol.rs"
  - "crates/shared/provider-adapters/src/python/"
  - "packages/python/"
last_reviewed: "2026-07-30"
---

# Python Provider Platform Plan

## Purpose

This is the canonical delivery plan and status ledger for Soma's Python provider
platform. It replaces the session-local roadmap used for the initial
implementation.

It is authoritative for phase status, remaining scope, implementation order, and
completion gates. Code, tests, contracts, and accepted ADRs remain authoritative
for runtime behavior.

## Status Rules

| Status | Meaning |
|---|---|
| Complete | Merged, verified, and used by the intended runtime path. |
| Foundation complete | The seam is merged and verified, but production has not switched to it. |
| Partial | Useful implementation exists, but exit gates remain. |
| Planned | Agreed direction with no complete implementation. |
| Experimental | Optional work that must not block the core platform. |

A phase is not complete merely because types or tests exist. The intended runtime
path must use it and its failure behavior must be verified.

## Verified Snapshot

Last reconciled on **2026-07-30** against the Phase 5–7 implementation branch
stacked on the production-environment work in PR #249.

The immutable-environment stack was merged by `749bc026` and ends at
`4993e57e`; the supervised persistent runner was merged by `47bb3131`:

```text
1d4b3e1b  Python authoring contract
237d1382  versioned runner protocol
2d91e8ef  installable Python SDK
c79c1555  SDK Context and validation
4ed50f3e  PyO3 native bindings
eacebcda  PEP 723 / uv environment planner
3df74c74  atomic uv materializer
a8e78876  prepared interpreter selection
a463cf74  candidate preparation before activation
99330511  prepared-environment proof tests
11df06b7  runtime compatibility validation
dbc03a68  readiness verification
cb6c957b  cache inventory
3c105d87  cache pruning
4a55996d  cache repair
305b00dd  immutable environment updates
4993e57e  immutable candidate activation
47bb3131  supervised persistent runner activation
```

Post-environment-merge CI repairs `f9860585` and `12fc929b` are also on
`main`.

## Current Runtime Truth

| Area | Current state |
|---|---|
| Authoring | Plain Python, decorated Python, LangChain, and LlamaIndex providers work. |
| Contract | Rust `soma-provider-core` is canonical. |
| SDK | `soma-provider` 0.2.0 provides `provider`, `tool`, `Context`, typing, examples, and pure-Python fallback. |
| Native binding | Thin PyO3 abi3 module delegates manifest validation to Rust. |
| Dependencies | PEP 723 is parsed without executing provider code. |
| Environments | The adapter library can plan, materialize, verify, inventory, prune, repair, update, and activate content-addressed `uv` environments. Production startup installs that lifecycle when the complete, disabled-by-default `[python.environment]` configuration is enabled. |
| Execution | One-shot remains the default; `SOMA_PYTHON_RUNNER_MODE=persistent` activates supervised workers. |
| Persistent protocol | Negotiated framing, describe/invoke/health/drain/shutdown, supervision, async candidate preflight, and prepared-interpreter authoring-path parity are active in persistent mode. Active cancel frames and brokered `host.*` calls remain protocol-only. |
| Context capabilities | Request identity is injected; HTTP, secrets, state, logging, metrics, progress, and cancellation are not live broker services. |
| Isolation | Out of process with filtered environment and bounded I/O, but not OS-sandboxed. |
| Wasm graduation | Contract boundary exists; WIT components and graduation tooling do not. |

## Product Outcome

An author drops one `.py` file into Soma. Dependencies and interpreter
requirements live in PEP 723 metadata inside that file. Soma owns dependency
locking, immutable environments, SDK/native wheel injection, caching,
activation, supervision, policy, rollback, and status.

No generated `.venv`, project file, lockfile, wheel, or readiness marker belongs
beside provider source.

The intended graduation path is:

```text
single-file Python
  -> soma_provider authoring API
  -> reusable Rust through PyO3
  -> the same Rust core exposed as native Rust and WIT/WASI component providers
```

The portable unit is the provider contract and reusable business logic. Soma
does not claim automatic translation of arbitrary Python into Rust or Wasm.

## Architectural Boundaries

- Python metadata normalizes into the canonical Rust provider model.
- `_soma_native` stays thin and must not own the application, registry, Tokio
  runtime, supervisor, capability policy, or public surfaces.
- Legacy `PROVIDER`, public-function inference, explicit `TOOLS`, LangChain, and
  LlamaIndex remain compatible until separately deprecated.
- The runner protocol is defensive transport, not a trust boundary.
- Rust owns validation, authorization, policy, redaction, public errors, and
  registry activation.

## Completed Foundation

### SDK and authoring

- `provider(...)` and `@tool(...)`
- validated metadata and JSON Schema helpers
- immutable request `Context`
- Context exclusion from public input schemas
- sync and async tools
- legacy and framework compatibility
- typed package with `py.typed`
- pure-Python fallback
- native SDK/version matching
- minimal, decorated, async, Pydantic, LangChain, and LlamaIndex examples

### Persistent protocol seam

- independent protocol versioning
- major/minor negotiation and feature intersection
- four-byte big-endian length-prefixed UTF-8 JSON
- bounded frames and finite JSON enforcement
- typed `describe`, `invoke`, `cancel`, `health`, `drain`, and `shutdown`
- typed brokered `host.*` envelopes
- IDs, deadlines, trace, actor/scopes, cancellation token, and generation
- stable states and errors
- Rust/Python codecs and shared fixtures

Persistent mode negotiates and implements `describe`, `invoke`, `health`,
`drain`, and `shutdown`. `cancel` and brokered `host.*` messages remain
contract-only and are not advertised by the active worker. The one-shot bridge
remains the default migration and rollback path.

### Immutable environment lifecycle

Implementation under `crates/shared/provider-adapters/src/python/` includes:

- bounded PEP 723 parsing
- normalized Python/dependency/`tool.uv` requirements
- versioned content-addressed keys
- runtime, SDK wheel, `uv`, and policy fingerprints
- uniquely owned staging directories
- exact SDK wheel SHA-256 verification
- `uv lock` and locked synchronization
- offline SDK/native wheel installation
- atomic publication and frozen cache reopening
- incomplete/corrupt cache detection
- readiness and runtime compatibility checks
- inventory, prune, repair, update, and activation

## Phase Roadmap

| Phase | Status | Exit summary |
|---|---|---|
| 1. Authoring boundary | Complete | Python normalizes into the Rust provider contract. |
| 2. Runner protocol | Foundation complete | Cross-language protocol is versioned and tested. |
| 3. Python SDK | Complete baseline | Facade, Context identity, schemas, examples, and compatibility paths exist. |
| 4. PyO3/maturin | Complete baseline | Thin abi3 validation binding and wheel CI exist. |
| 5. PEP 723 and `uv` lifecycle | Complete | Planning, production immutable activation, and authorized operator status/prune/repair/update surfaces are implemented. |
| 6. Persistent supervised runner | Complete, opt-in | Installed-wheel supervised workers cover all authoring paths with cancellation, bounded redacted logs, status, quarantine, and reset controls. |
| 7. Generation-aware reload | Complete | Candidate preflight, debounced/coalesced atomic activation, bounded retained history, retirement, status, and rollback are implemented. |
| 8. Capability broker and containment | Planned | Context services are live under explicit execution profiles. |
| 9. WIT/WASI components | Planned | Versioned component ABI and reference provider exist. |
| 10. Graduation tooling | Planned | Scaffold, compare, promote, and rollback manual rewrites. |
| 11. `componentize-py` | Experimental | Narrow compatibility-scanned Python component path. |
| 12. Release and operations | Partial | Wheel build matrix exists; publication and hardening remain. |

The earlier session-local implementation plan numbered the `uv` lifecycle as
Phase 4 and the persistent runner as Phase 5. This canonical plan separates
PyO3/maturin into its own Phase 4, so those same slices appear here as Phases 5
and 6. Their production exit gates are now recorded in the completed phase
sections below.

## Phase 5: PEP 723 and Immutable `uv` Environments

**Complete.**

The planner, materializer, readiness checks, cache operations, update flow, and
candidate validation are implemented and tested. Application tests prove that a
`PythonEnvironmentLifecycle` installed with
`FileProviderSource::with_python_environment_preparer(...)` selects prepared
interpreters before candidate activation.

Production startup constructs and installs that preparer when
`[python.environment] enabled = true` (or the equivalent
`SOMA_PYTHON_ENVIRONMENT_*` variables) supplies the cache root, `uv` and Python
runtime identity, release SDK wheel and digest, and offline policy. The feature
remains disabled by default; enabled but incomplete configuration fails closed.
Startup probes the selected Python and (outside offline mode) `uv`, verifies the
wheel digest, requires a private cache root, and rejects a mismatched policy
version before provider activation. `update = true` resolves an immutable
candidate during preparation, while `offline = true` reopens only complete
caches and does not require `uv` to remain installed. Both one-shot and
persistent runners receive the resulting prepared interpreter.

The shared application operator surface inventories ready, incomplete, corrupt,
and staging entries without importing provider code; creates bounded
conservative prune plans; applies race-safe prune plans after confirmation;
repairs an exact managed provider environment; and prepares, validates, then
atomically activates explicit immutable updates. CLI, MCP, and REST use the
same action catalog, authorization, confirmation, path-containment, and
response-size checks.

## Phase 6: Persistent Supervised Runner

**Complete as an explicit opt-in with one-shot rollback.**

Scope:

- add `python -I -m soma_provider.runner`;
- use an authenticated loopback TCP connection as the cross-platform duplex
  control channel;
- keep protocol traffic separate from stdout/stderr;
- negotiate the protocol before accepting work;
- support persistent `describe` and `invoke`;
- implement health, drain, and graceful shutdown;
- add bounded concurrency and worker generation IDs;
- capture bounded structured logs and expose them to operators;
- define restart budgets and crash-loop behavior;
- recycle workers after uninterruptible synchronous timeouts;
- retain the one-shot bridge as the default migration and rollback mode.

Delivered gates:

- persistent-mode catalog/invoke paths use persistent workers;
- worker crashes cannot corrupt the active registry;
- unhealthy candidates cannot activate;
- timeouts cannot poison later calls;
- restart loops are bounded and visible;
- with an explicitly prepared interpreter, plain, decorated, async,
  LangChain-compatible, and LlamaIndex-compatible fixtures produce equal
  catalogs and results in both modes.

The focused supervisor/application suites verify serial busy rejection,
timeout followed by a non-replayed restart, persistent-environment eligibility,
candidate preflight, and retention of the previous snapshot when a Python
candidate fails.

The remaining gates are delivered:

- active invocations are cancelled deterministically at the process-tree
  boundary, and later work starts a clean worker without replay;
- bounded stderr is split into structured sequenced entries, redacted before
  retention, and exposed through shared operator status;
- production bootstrap/config selection uses the prepared interpreter and is
  covered by the same authoring-path parity proof;
- missing runner/wheel, malformed protocol frames, source substitution,
  quarantine exhaustion/reset, and secret-bearing diagnostics have focused
  fail-closed evidence.

Implementation notes:

- Workers require the matching installed `soma-provider` wheel and start with
  `python -I -m soma_provider.runner`.
- The worker connects to an ephemeral loopback listener and authenticates with
  a per-launch token before protocol negotiation.
- Provider stdout is redirected to stderr. The host continuously drains it into
  a bounded structured line ring, applies public diagnostic redaction before
  retention, and exposes it through the authorized worker-status action.
- One invocation is active per worker. Concurrent work receives
  `python_provider_busy` without queueing.
- The active feature set does not require cooperative `cancel`: cancellation
  terminates the Unix worker process group or uses Windows `taskkill /T /F`,
  then the supervisor restarts cleanly on later work.
- Provider/tool runtime environment declarations are rejected in persistent
  mode. Phase-8 broker capabilities, actor scopes, and trace propagation are
  intentionally unavailable.
- Unix workers own a process group. Windows uses best-effort process-tree
  termination; stronger Job Object containment remains part of Phase 8.

## Phase 7: Generation-Aware Reload

Scope:

- start candidate workers in prepared immutable environments;
- health-check the complete candidate set;
- atomically swap registry and worker-generation pointers;
- keep in-flight calls on their original generation;
- drain and retire old workers;
- debounce filesystem changes and serialize refreshes;
- prevent duplicate preparation;
- quarantine crash loops;
- provide operator reset and rollback.

Exit gates:

- a failed candidate never replaces a healthy generation;
- activation is all-or-nothing across catalog, environment, and workers;
- old generations drain without receiving new work;
- repeated file events cannot create rebuild storms.

All exit gates are delivered. Async refreshes share a single serialized
preparation lane, recheck settled inputs after acquiring it, and wait through
a short filesystem debounce window before fingerprinting settled inputs.
Candidate environments and persistent workers
are fully prepared and health-checked before publication. The registry swaps
provider routing, catalog snapshot, environment selections, and worker set
together; failed candidates retain the prior generation. A bounded three-entry
history keeps recent generations eligible for explicit rollback while newer
requests route only to the active generation. Each retained Python generation
owns a content-addressed, non-symlink snapshot of the complete provider tree,
including adjacent data files, subject to 4,096-file and 64 MiB bounds.
Dispatch leases let calls already routed to an old generation drain before its
worker is parked; reference-counted snapshots are reclaimed after the last
active, retained, or in-flight provider releases them. Evicted generations
drain and retire outside registry locks. Authorized status and confirmed
rollback actions are shared across CLI, MCP, and REST.

## Phase 8: Capability Broker and Containment

Make the existing protocol and unavailable Context handles real:

- `ctx.http`
- secrets
- state
- structured logging
- metrics
- progress
- cancellation

Authority must be the intersection of provider declarations, deployment policy,
actor scopes, and host availability.

Execution profiles:

1. `disabled`
2. `trusted`
3. `brokered`

Brokered mode must restrict ambient authority. Linux should use appropriate
namespaces, cgroups, seccomp, resource limits, filesystem views, and direct
network restrictions. Windows should use Job Objects and the strongest
supportable restrictions. Unsupported enforcement must fail closed.

Security gates include secret redaction, HTTP redirect/DNS escape protection,
confused-deputy protection, state isolation, audit events, and a threat model for
source, dependencies, protocol, cache, broker, and Wasm.

## Phase 9: WIT/WASI Component Runtime

Scope:

- versioned WIT provider package;
- component-model host bindings;
- compatibility with the existing core-Wasm ABI;
- explicit capability imports;
- reusable Rust guest SDK;
- component cache and resource limits;
- interruption that actually stops timed-out work;
- shared Python/component conformance fixtures;
- at least one reference component provider.

## Phase 10: Graduation Tooling

Planned commands:

```text
graduate
build-component
verify-component
compare
activate
rollback
```

Tooling should scaffold a reusable Rust core, PyO3 and WIT adapters, capture
fixtures, compare old/new behavior, and atomically promote or roll back. It must
clearly mark manual business-logic work and never claim arbitrary Python was
translated automatically.

## Phase 11: Experimental `componentize-py`

Only after the stable component host exists:

- scan imports and dependency wheels;
- reject unsupported native extensions;
- check process/thread/socket/filesystem assumptions;
- generate Python WIT bindings;
- build in isolation;
- validate under Soma's exact Wasmtime host;
- return actionable incompatibility reports.

This phase must not block the stable Rust/component graduation path.

## Phase 12: Release and Operations

Already delivered:

- maturin mixed package;
- Python 3.11+ metadata;
- abi3 extension and pure-Python fallback;
- cross-platform wheel CI for Linux x86_64/aarch64, Windows x86_64, and macOS
  x86_64/arm64;
- isolated wheel installation and contract verification.

Remaining:

- `soma-provider` publish/version policy;
- matching wheelhouse integration with Soma releases;
- checksums, signatures, attestations, SBOM, and provenance;
- dependency and Wasmtime security-update policy;
- upgrade/rollback and cache backup/restore tests;
- CLI/API/MCP/web status for environments, workers, generations, and quarantine;
- cold/warm/reload performance budgets;
- cache-churn, crash-loop, high-volume log, and mixed-provider soak tests.

## Cross-Cutting SDK Backlog

These can land independently without delaying the runner:

- complete dataclass and `TypedDict` schema handling;
- `Annotated` descriptions and constraints;
- complete union/nullable handling;
- stronger output-schema inference;
- generated Python catalog models from canonical Rust schemas;
- richer stubs;
- broker-backed Context implementations.

## Open Policy Decisions

- permitted package indexes
- interpreter download policy
- source distribution policy
- Git, URL, and local-path dependency policy
- license, hash, and provenance requirements
- cache quotas and retention
- offline update behavior
- SDK wheel selection by platform
- policy-version migrations

Defaults should favor reproducibility, offline restart, and fail-closed handling
of unsupported dependency sources.

## Recommended Delivery Order

1. Configure and install the immutable environment lifecycle in production,
   including release-wheel/runtime policy and operator surfaces.
2. Add active cancellation and structured, operator-visible worker logs.
3. Finish generation debounce, status, operator rollback, and environment /
   worker activation as one production generation.
4. Add logging/progress/cancellation broker calls.
5. Add HTTP, secrets, and state.
6. Add explicit execution profiles and containment.
7. Implement the WIT component ABI.
8. Add conformance and graduation tooling.
9. Evaluate `componentize-py`.
10. Finish release provenance, status, performance, and soak gates.

The persistent runner and generation boundary are load-bearing. Do not bypass
them to begin component or graduation work.

## Verification Baseline

```bash
cargo test -p soma-provider-adapters --features python
PYTHONPATH=packages/python/python python3 -m unittest discover -s packages/python/tests -p 'test_*.py'
uv sync --project packages/python --frozen
cargo test -p soma --test python_provider
cargo fmt --all -- --check
cargo clippy -p soma-provider-adapters --features python --all-targets -- -D warnings
git diff --check
cargo xtask check-docs
```

For wheel changes:

```bash
uv run --project packages/python --frozen maturin build \
  --manifest-path packages/python/Cargo.toml \
  --profile dev \
  --out wheelhouse

python -I packages/python/tests/verify_installed.py
```

Use the repository's normal compiler wrapper and queue. Do not bypass it or use a
release build merely to evade development-build contention.

## Overall Definition of Done

- PEP 723 providers prepare reproducibly and restart offline.
- Production uses supervised persistent workers.
- Catalog, environment, and worker activation is atomic by generation.
- cancellation, timeout, drain, crash recovery, and quarantine are deterministic.
- Context capabilities are brokered under explicit authority.
- brokered containment is enforced or fails closed.
- operators can inspect every environment/worker/generation state.
- a WIT component passes shared conformance fixtures.
- graduation tooling compares, promotes, and rolls back a manual rewrite.
- releases include provenance, compatibility, performance, and soak gates.

## Maintenance Rules

Update this document in the same pull request whenever phase status, order,
completion gates, active runtime path, compatibility, or policy changes.

For every phase transition:

1. record the commit or pull request;
2. name the active runtime path;
3. record verification evidence;
4. leave incomplete work visible;
5. update `last_reviewed`.

## Related Documents

- [ADR 0013: Python authoring boundary](../adr/0013-python-provider-authoring-boundary.md)
- [Dynamic Provider Runtime](dynamic-provider-runtime.md)
- [Provider Guide](../PROVIDERS.md)
- [Python Package README](../../packages/python/README.md)
- [Drop-in Provider Layout](drop-in-provider-layout.md)
