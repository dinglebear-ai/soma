---
title: "Python Provider Platform Plan"
created: 2026-07-29
updated: 2026-07-30
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

Last reconciled on **2026-07-30** against the Phase 8–10 implementation in
PR #253, stacked on the merged Phase 5–7 runtime and production-environment
work.

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
| Persistent protocol | Negotiated framing, describe/invoke/cancel/health/drain/shutdown, supervision, candidate preflight, and invocation-bound host calls are active in persistent mode. |
| Context capabilities | Request and actor identity, trace context, HTTP, secrets, namespaced state, logging, metrics, progress, and cancellation are live broker services subject to intersected authority. |
| Isolation | Trusted mode is out of process with filtered environment and bounded I/O. Brokered Linux mode adds namespaces, cgroups, seccomp, rlimits, a read-only filesystem view, and no ambient network; unavailable enforcement fails closed. |
| Wasm graduation | A versioned component ABI, constrained WASI host, reusable guest SDK, shared conformance fixtures, and honest scaffold/build/verify/compare/activate/rollback tooling are active. |

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

Persistent mode negotiates and implements `describe`, `invoke`, `cancel`,
`health`, `drain`, `shutdown`, and invocation-bound broker host calls. The
one-shot bridge remains the default migration and rollback path.

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
| 8. Capability broker and containment | Complete | Invocation-bound broker services and fail-closed Linux containment merged in PR #253. |
| 9. WIT/WASI components | Complete | Versioned component runtime, guest SDK, host limits, and conformance fixtures merged in PR #253. |
| 10. Graduation tooling | Complete | Scaffold/build/verify/compare/activate/rollback workflow merged in PR #253. |
| 11. `componentize-py` | Foundation started | Non-executing source and wheel compatibility scanner reports unsupported authority and native-extension assumptions. |
| 12. Release and operations | Foundation started | Wheel CI exists and `soma-provider` now has independent tag and cross-file version policy. |
| 13. SDK completeness | Foundation started | `Annotated`, `TypedDict`, dataclass, literal, tuple, and typed-map schema inference are implemented. |

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
- Cancellation first exposes a cooperative flag over the broker and also
  terminates the contained worker process tree, so uninterruptible synchronous
  code cannot survive the deadline.
- Provider/tool runtime environment declarations remain rejected in persistent
  mode. Actor scopes and trace context are invocation-bound and propagated to
  the broker without becoming ambient process environment.
- Trusted Unix workers own a process group. Brokered Linux workers use the full
  containment boundary described in Phase 8; Windows uses kill-on-close Job
  Objects and fails closed when the requested profile cannot be enforced.

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

Status: implemented; pending merge and CI verification.

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

Delivered in Phase 8:

- live `Context` services over invocation-bound host calls;
- deployment allowlists intersected with declarations and actor scopes;
- `disabled`, `trusted`, and fail-closed `brokered` profiles;
- Linux namespaces, cgroup v2, seccomp, rlimits, read-only filesystem view,
  private network namespace, and authenticated Unix control channel;
- kill-on-close Windows Job Objects, with brokered mode unavailable when the
  platform cannot enforce the complete boundary;
- bounded redacted audit events and the dedicated
  [threat model](../security/python-provider-threat-model.md).

## Phase 9: WIT/WASI Component Runtime

Status: implemented; pending merge and CI verification.

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

The versioned WIT package is `wit/soma-provider/world.wit`. Wasmtime detects
components before falling back to the legacy core ABI, caches compiled
artifacts, applies memory/table/instance/fuel limits, and uses a live epoch
ticker so deadlines interrupt execution rather than only timing out the
waiting task. `soma-provider-guest` supplies the reusable Rust core and
explicit HTTP, secret, state, log, metric, and progress capability helpers.
The reference Python and Rust component implementations share
`examples/providers/components/conformance-v1.json`.

## Phase 10: Graduation Tooling

**Complete.** Merged in PR #253 after the full local and GitHub verification
matrix passed.

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

All six commands are available under `soma providers`. `graduate` copies the
source and optional recorded fixtures into an isolated workspace and emits a
manual-rewrite core plus buildable thin PyO3 and WIT adapters.
`build-component` builds or imports, verifies, and content-addresses a candidate;
`verify-component` enforces the component ABI. `compare` accepts only
side-effect-free providers and non-destructive actions owned by the graduated
provider; filesystem, network, environment, terminal, browser, GitHub, secret,
and state-write authority all fail closed before dual-run. It snapshots the
fixture corpus once, executes live Python, feeds one retained prepared component
the exact same host-owned execution envelope under one absolute deadline,
checks the recorded result against the live result, and persists a source,
catalog, fixture, and artifact digest-bound attestation only after the complete
surface response passes the caller's byte limit.
`activate`/`rollback` revalidate the live provider identity, update durable
state atomically, and retain the previous artifact.

## Phase 11: Experimental `componentize-py`

**Foundation started.** The dependency-free SDK now exposes a non-executing
compatibility scanner that parses provider source with the Python AST, inventories
imports, accepts explicit dependency-wheel evidence, rejects native extensions and
non-pure wheels, and reports process, thread, socket, dynamic-import, native-FFI,
and filesystem assumptions as structured findings. The scanner is embedded in the
one-file authoring bridge as `soma_provider._componentize`, so installed and
source-only SDK modes expose the same report contract.

The scanner deliberately does not transpile, build, or claim compatibility. A
report without hard errors only marks the provider eligible for later isolated
build validation. Remaining work is to:

- map imports to verified wheel distributions;
- generate Python WIT bindings;
- build in isolation;
- validate under Soma's exact Wasmtime host;
- preserve actionable incompatibility reports through CLI, MCP, REST, and web.

This phase must not block or weaken the stable Rust/component graduation path.

## Phase 12: Release and Operations

Already delivered:

- maturin mixed package;
- Python 3.11+ metadata;
- abi3 extension and pure-Python fallback;
- cross-platform wheel CI for Linux, Windows, and macOS x86_64;
- isolated wheel installation and contract verification;
- an independent `soma-provider-v*` release component;
- version parity across `pyproject.toml`, the native Cargo package,
  `Cargo.lock`, and the SDK's `__version__` assignment.

Remaining:

- PyPI publication credentials, trusted publishing, and release execution;
- matching wheelhouse integration with Soma releases;
- checksums, signatures, attestations, SBOM, and provenance;
- dependency and Wasmtime security-update policy;
- upgrade/rollback and cache backup/restore tests;
- CLI/API/MCP/web status for environments, workers, generations, and quarantine;
- cold/warm/reload performance budgets;
- cache-churn, crash-loop, high-volume log, and mixed-provider soak tests.

## Phase 13: SDK Completeness

**Foundation started.** Dependency-free schema inference now covers dataclasses,
`TypedDict` required and optional keys, `Annotated` descriptions and an
allowlisted constraint vocabulary, `Literal`, required/not-required wrappers,
fixed and variadic tuples, typed mapping values, and the existing union and
nullable forms. Unsupported `Annotated` keys fail closed instead of being copied
into public schemas.

Remaining work can land independently without delaying the runner:

- refine union/nullable canonicalization and discriminator support;
- strengthen return-annotation and output-schema inference;
- generate Python catalog models from canonical Rust schemas;
- publish richer type stubs and editor metadata;
- add typed conveniences over the delivered broker-backed Context services.

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

## Delivered Order and Next Work

Phases 8–10 delivered the capability broker, enforced containment, WIT
component runtime, shared conformance path, isolated offline component builds,
and source/catalog/fixture/artifact-bound graduation comparison, activation,
and rollback workflow. Phases 11–13 now have bounded foundations: a static
compatibility scanner, independent Python-package version policy, and richer SDK
schema inference. The recommended next sequence is:

1. Connect the Phase 11 report to operator surfaces and an isolated experimental
   build/Wasmtime validation path.
2. Complete Phase 12 signing, provenance, SBOM, trusted publication, and
   dependency-policy gates.
3. Establish cold/warm performance budgets and mixed-provider soak coverage.
4. Continue Phase 13 output-schema, generated-model, stub, and typed-convenience
   work without weakening the verified graduation path.

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
