# Python Persistent Supervised Runner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the production one-shot Python sidecar with an opt-in, supervised persistent runner while retaining an explicit one-shot fallback.

**Architecture:** Preserve `soma-provider-core` and the existing versioned `python_protocol` types. A matching installed `soma-provider` wheel makes the isolated Python worker discoverable in both prepared and ambient interpreters. A cross-platform private control transport and process-tree owner isolate the worker; a Rust supervisor owns framing, negotiated initialization, one active invocation, limits, timeout/restart policy, and canonical public error conversion. An async registry refresh coordinator validates workers outside registry locks, atomically publishes only ready candidates, and retains its last valid snapshot on failure.

**Tech Stack:** Rust 2021, Tokio process/I/O/sync, serde JSON, Python 3.11+, existing `soma_provider` SDK, cargo tests, Python `unittest`.

## Global Constraints

- Python metadata continues to normalize through `soma-provider-core`; no second manifest model.
- `_soma_native` stays a thin validation binding and never owns Tokio, registry, policy, or public surfaces.
- Use a private cross-platform control transport for four-byte big-endian,
  bounded UTF-8 JSON frames; never send protocol frames over stdout/stderr. The
  delivered implementation uses an ephemeral loopback TCP listener with a
  per-launch authentication token on every platform.
- The host owns a new Unix process session/process group or Windows Job Object with kill-on-close. Close host control handles before reaping; make the worker's control handle non-inheritable before provider import.
- Persistent mode requires the matching installed SDK wheel. Prepared environments already receive it; ambient persistent mode validates it with `python -I -c 'import soma_provider.runner'` and fails closed if absent.
- Equal protocol major versions are mandatory. Worker `Hello` is followed by host `Initialize { minor, features }`, then worker `Ready`; no request is accepted before this exchange.
- Rust remains responsible for validation, authorization, redaction, policy, and public errors; the protocol is defensive transport, not a trust boundary.
- Default to `SOMA_PYTHON_RUNNER_MODE=one-shot`; accept only `one-shot` and opt-in `persistent`.
- Persistent Phase 6 supports exactly one active invocation per provider worker. It returns `python_provider_busy` immediately while occupied; worker pools and queued parallelism are deferred.
- Persistent mode rejects any provider/tool runtime environment requirement with stable `python_persistent_env_unsupported`; it never merges per-tool secrets into a long-lived process and never falls back silently to one-shot.
- Send no actor, scopes, or trace values to the Phase-6 worker. Context contains only request, provider, action, surface, snapshot, deadline, and cancellation identity.
- Bound frames, per-tool input/output, aggregate pending bytes, stderr retained bytes, in-flight calls, global active workers/candidate starts, startup/request timeouts, restart attempts, restart backoff, and restart window.
- A synchronous timeout makes the worker indeterminate: kill/reap it, fail that call with `python_provider_timeout`, and replace it before another call.
- Preserve plain, decorated, async, LangChain, LlamaIndex, prepared-interpreter, and legacy authoring compatibility.
- Do not implement Phase 7 generation-atomic reload, capability brokering/containment, WIT/WASI, graduation, `componentize-py`, or release/soak work here.

## Engineering Review Decisions Incorporated

- The persistent runner's minimum portability target is the existing wheel CI matrix: Linux x86_64/aarch64, Windows x86_64, and macOS x86_64/arm64. Real subprocess tests run on each platform.
- The wire format becomes tagged `PythonRunnerHostMessage::{Initialize, Request, HostReply}` and `PythonRunnerWorkerMessage::{Hello, Ready, Reply, HostCall}`. `Accepted` changes request state but is not terminal; only terminal `Ok` or `Error` resolves a call.
- Every supervisor has immutable identity: normalized source digest, canonical provider path, selected interpreter/environment fingerprint, catalog fingerprint, runner configuration fingerprint, and registry generation. Persistent `PythonProvider` owns a prestarted `Arc<PythonWorkerSupervisor>`; concurrent calls cannot create additional workers.
- The state machine is `Queued -> Written -> Accepted -> Terminal | TimedOut | WorkerLost`. An accepted call is never replayed. A later call may start a replacement worker after kill/reap, subject to one serialized restart transition and budget.
- Worker-originated messages are untrusted. Rust maps errors to canonical public code/message, applies `redact_public` to diagnostics, discards raw payloads from public output, and continuously drains stderr into a bounded ring buffer while discarding overflow.
- Candidate startup re-hashes the regular, non-symlink provider source immediately before launch and again after `describe`; mismatch rejects the candidate. Provider roots remain trusted-writer-only, but source substitution cannot activate a mismatched catalog.
- Refresh is made async end-to-end, coalesces concurrent requests, performs startup/health outside the registry write lock, publishes only after all candidates are ready, and retires old workers in bounded background work.

## File Structure

- Create: `packages/python/python/soma_provider/runner.py` — persistent worker, codec, handshake, catalog, and invocation dispatch.
- Create: `packages/python/tests/test_runner.py` — subprocess protocol and worker-lifecycle coverage.
- Create: `crates/shared/provider-adapters/src/python/supervisor.rs` — supervisor, child lifecycle, timeout/restart/quarantine policy.
- Create: `crates/shared/provider-adapters/src/python/supervisor_tests.rs` — deterministic fake-worker tests.
- Modify: `crates/shared/provider-adapters/src/python.rs`, `python_protocol.rs`, `lib.rs`, `Cargo.toml` — mode selection and protocol integration.
- Modify: `crates/soma/config/src/config.rs`, `config_tests.rs` — typed runner configuration.
- Modify: `crates/soma/application/src/providers/filesystem.rs`, `filesystem_python.rs`, `provider_registry.rs` and tests — prepared interpreter selection, candidate health checks, and worker retirement.
- Modify: `apps/soma/tests/python_provider.rs`, `docs/PROVIDERS.md`, `docs/specs/python-provider-platform.md`, `CHANGELOG.md` — production parity proof and operations documentation.

---

### Task 1: Define the executable runner configuration and reply contract

**Files:**
- Modify: `crates/soma/config/src/config.rs`
- Modify: `crates/soma/config/src/config_tests.rs`
- Modify: `crates/shared/provider-adapters/src/python_protocol.rs`
- Modify: `crates/shared/provider-adapters/src/python_protocol_tests.rs`

**Interfaces:**
- Produces `PythonRunnerMode::{OneShot, Persistent}`.
- Produces `PythonRunnerConfig { max_in_flight: NonZeroUsize, startup_timeout: Duration, request_timeout: Duration, max_restarts: u32, restart_window: Duration, max_log_bytes: usize }`.
- Produces private `PythonRunnerHostReply::{Ok, Error, Unsupported}` correlated by `request_id`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn persistent_runner_rejects_zero_concurrency_and_unknown_mode() {
    assert!(PythonRunnerConfig::from_env_map(&[("SOMA_PYTHON_RUNNER_MODE", "persistent"), ("SOMA_PYTHON_RUNNER_MAX_IN_FLIGHT", "0")]).is_err());
    assert!(PythonRunnerConfig::from_env_map(&[("SOMA_PYTHON_RUNNER_MODE", "pool")]).is_err());
}

#[test]
fn host_reply_round_trips_with_request_id() {
    let frame = encode_runner_frame(&PythonRunnerHostReply::unsupported(9, "host.http"))?;
    assert_eq!(decode_runner_frame::<PythonRunnerHostReply>(&frame)?.request_id(), 9);
}
```

- [ ] **Step 2: Confirm the tests fail**

Run: `cargo test -p soma-config python_runner && cargo test -p soma-provider-adapters python_protocol`

Expected: FAIL because the new types do not exist.

- [ ] **Step 3: Implement the typed parser and reply envelope**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonRunnerMode { OneShot, Persistent }
```

Parse only the two literal modes. Reject zero limits and non-positive durations with a configuration error naming the environment variable. Keep host replies protocol-private and return explicit unsupported errors for broker calls until Phase 8.

- [ ] **Step 4: Verify and commit**

Run: `cargo test -p soma-config python_runner && cargo test -p soma-provider-adapters python_protocol`

Expected: PASS.

```bash
git add crates/soma/config/src/config.rs crates/soma/config/src/config_tests.rs crates/shared/provider-adapters/src/python_protocol.rs crates/shared/provider-adapters/src/python_protocol_tests.rs
git commit -m "feat(python): configure persistent runner limits"
```

### Task 2: Add the dependency-free Python persistent worker

**Files:**
- Create: `packages/python/python/soma_provider/runner.py`
- Create: `packages/python/tests/test_runner.py`
- Modify: `packages/python/tests/runner_protocol_v1.json`

**Interfaces:**
- Consumes `SOMA_PYTHON_RUNNER_FD` and `PythonRunnerHostRequest` frames.
- Produces `PythonRunnerHello` then `PythonRunnerReply` frames through that descriptor.
- Exposes `python -I -m soma_provider.runner`.

- [ ] **Step 1: Write subprocess tests**

```python
def test_hello_precedes_describe_and_stdout_has_no_frames():
    worker = start_runner(FIXTURES / "decorated.py")
    assert worker.read_frame()["protocol"] == {"major": 1, "minor": 0}
    worker.write_frame({"method": "describe", "request_id": 1, "path": str(FIXTURES / "decorated.py"), "generation_id": "g1"})
    assert worker.read_frame()["status"] == "ok"
    assert worker.stdout.read1(1) == b""

def test_drain_rejects_new_invocations_and_shutdown_exits_cleanly():
    worker = start_runner(FIXTURES / "decorated.py")
    worker.complete_handshake()
    worker.write_frame({"method": "drain", "request_id": 2})
    assert worker.read_frame()["status"] == "ok"
    worker.write_frame(invoke_request(3, "echo", {"message": "later"}))
    assert worker.read_frame()["error"]["code"] == "python_worker_draining"
    worker.write_frame({"method": "shutdown", "request_id": 4})
    assert worker.wait().returncode == 0
```

Cover malformed/oversize frames, major mismatch, import error, async invocation, and deadline expiry for a synchronous handler.

- [ ] **Step 2: Confirm failure**

Run: `PYTHONPATH=packages/python/python python3 -m unittest packages.python.tests.test_runner -v`

Expected: FAIL because `soma_provider.runner` is absent.

- [ ] **Step 3: Implement the worker**

```python
def main() -> int:
    channel = FramedChannel.from_env_fd("SOMA_PYTHON_RUNNER_FD", MAX_FRAME_BYTES)
    channel.write(hello())
    return ProviderWorker(channel).serve_forever()
```

Open only the inherited descriptor; do not accept a provider-controlled socket/path. Import each provider once after `describe`, cache the catalog/callables, inject `Context` identity per request, and encode output with `allow_nan=False`. Return a redacted structured error; write bounded diagnostics only to stderr.

- [ ] **Step 4: Verify and commit**

Run: `PYTHONPATH=packages/python/python python3 -m unittest discover -s packages/python/tests -p 'test_*.py' && cargo test -p soma-provider-adapters python_protocol::tests::shared_python_golden_fixtures_decode_as_rust_protocol_types`

Expected: PASS.

```bash
git add packages/python/python/soma_provider/runner.py packages/python/tests/test_runner.py packages/python/tests/runner_protocol_v1.json
git commit -m "feat(python): add persistent runner module"
```

### Task 3: Implement the Rust supervisor and mode switch

**Files:**
- Create: `crates/shared/provider-adapters/src/python/supervisor.rs`
- Create: `crates/shared/provider-adapters/src/python/supervisor_tests.rs`
- Modify: `crates/shared/provider-adapters/src/python.rs`
- Modify: `crates/shared/provider-adapters/Cargo.toml`

**Interfaces:**
- Produces `PythonWorkerSupervisor::start(spec, limits) -> Result<Arc<Self>, PythonSupervisorError>`.
- Produces `describe`, `invoke`, `health`, `drain`, and `shutdown` async methods.
- Guarantees no automatic replay after `Accepted` or indeterminate worker loss.

- [ ] **Step 1: Write lifecycle tests**

```rust
#[tokio::test]
async fn timeout_kills_indeterminate_worker_before_next_call() {
    let supervisor = test_supervisor([Script::accept_then_hang(), Script::healthy_reply()]).await;
    assert_eq!(supervisor.invoke(call("first")).await.unwrap_err().code(), "python_provider_timeout");
    assert_eq!(supervisor.invoke(call("second")).await.unwrap().json(), json!({"ok": true}));
    assert_eq!(supervisor.started_workers(), 2);
}

#[tokio::test]
async fn restart_budget_exhaustion_quarantines_the_provider() {
    let supervisor = test_supervisor([Script::crash_before_ready(), Script::crash_before_ready()]).await;
    assert_eq!(supervisor.ensure_ready().await.unwrap_err().code(), "python_provider_quarantined");
}
```

Cover startup timeout, major mismatch, malformed/oversize frame, crash before/after acceptance, in-flight saturation, drain, shutdown, and restart-window exhaustion.

- [ ] **Step 2: Confirm failure**

Run: `cargo test -p soma-provider-adapters python::supervisor -- --nocapture`

Expected: FAIL because the supervisor is absent.

- [ ] **Step 3: Implement a one-reader, bounded supervisor**

```rust
pub struct PythonWorkerSupervisor {
    state: Mutex<WorkerState>,
    permits: Semaphore,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<PythonRunnerReply, PythonSupervisorError>>>>,
    restart_budget: Mutex<RestartBudget>,
}
```

Launch `-I -m soma_provider.runner` with a cleared allow-listed environment, private descriptor, and piped bounded stderr. Use one control reader to route monotonically increasing IDs. Treat unknown/duplicate IDs, EOF, timeout, and protocol violations as worker failure; kill/reap. Restart only before acceptance and quarantine after the configured budget.

- [ ] **Step 4: Select the runner from `PythonProvider`**

```rust
match self.runner_mode {
    PythonRunnerMode::OneShot => self.call_one_shot(call).await,
    PythonRunnerMode::Persistent => self.supervisor()?.invoke(self.invocation(call)?).await,
}
```

Reuse existing catalog lookup, environment collection, output size limits, error codes, redaction, and snapshot fields. Do not duplicate manifest validation or surface a raw worker error.

- [ ] **Step 5: Verify and commit**

Run: `cargo test -p soma-provider-adapters --features python --no-fail-fast && cargo clippy -p soma-provider-adapters --features python --all-targets -- -D warnings`

Expected: PASS.

```bash
git add crates/shared/provider-adapters/src/python.rs crates/shared/provider-adapters/src/python/supervisor.rs crates/shared/provider-adapters/src/python/supervisor_tests.rs crates/shared/provider-adapters/Cargo.toml
git commit -m "feat(python): supervise persistent workers"
```

### Task 4: Validate workers during registry refresh

**Files:**
- Modify: `crates/soma/application/src/providers/filesystem.rs`
- Modify: `crates/soma/application/src/providers/filesystem_python.rs`
- Modify: `crates/soma/application/src/provider_registry.rs`
- Modify: `crates/soma/application/src/provider_registry/refresh_tests.rs`
- Modify: `crates/soma/application/src/providers/filesystem_python_tests.rs`

**Interfaces:**
- Consumes typed runner config, immutable prepared interpreter, and registry fingerprint.
- Produces an active persistent Python candidate only after handshake, describe, and `health=ready`.
- Retires replaced workers by drain, completion of in-flight calls, then shutdown.

- [ ] **Step 1: Write refresh tests**

```rust
#[tokio::test]
async fn unhealthy_python_candidate_keeps_previous_snapshot() {
    let registry = registry_with_healthy_persistent_python("g1").await;
    replace_provider_source_with("imports_then_crashes.py");
    assert_eq!(registry.refresh_file_providers().await?.id(), "g1");
}

#[tokio::test]
async fn retired_worker_drains_existing_call_and_rejects_new_work() {
    let worker = ready_worker().await?;
    let existing = worker.invoke(blocking_call());
    worker.drain().await?;
    assert_eq!(worker.invoke(call("new")).await.unwrap_err().code(), "python_worker_draining");
    existing.await?;
}
```

Cover prepared-interpreter use, exactly one candidate startup per changed fingerprint, and no change to active environment/worker after startup failure.

- [ ] **Step 2: Confirm failure**

Run: `cargo test -p soma-application python -- --nocapture`

Expected: FAIL because registry candidates do not own/health-check persistent workers.

- [ ] **Step 3: Implement candidate validation**

Prepare/validate all changed Python candidates before replacing the provider map, retaining the existing last-valid-snapshot fallback. A prepared environment always supplies the child interpreter; never silently use ambient Python. Drain/shutdown old workers after pointer swap. Do not add debounce, multi-generation routing, reset, or rollback APIs; those belong to Phase 7.

- [ ] **Step 4: Verify and commit**

Run: `cargo test -p soma-application python -- --nocapture && cargo test -p soma-application provider_registry::refresh -- --nocapture`

Expected: PASS.

```bash
git add crates/soma/application/src/providers/filesystem.rs crates/soma/application/src/providers/filesystem_python.rs crates/soma/application/src/provider_registry.rs crates/soma/application/src/provider_registry/refresh_tests.rs crates/soma/application/src/providers/filesystem_python_tests.rs
git commit -m "feat(python): validate persistent workers on refresh"
```

### Task 5: Prove production parity and record operations behavior

**Files:**
- Modify: `apps/soma/tests/python_provider.rs`
- Modify: `docs/PROVIDERS.md`
- Modify: `docs/specs/python-provider-platform.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes one-shot/persistent configuration and runner limits through normal app bootstrap.
- Proves identical catalogs and outputs in both modes plus deterministic timeout/crash behavior.

- [ ] **Step 1: Write end-to-end parity tests**

```rust
#[tokio::test]
async fn python_fixtures_dispatch_through_each_runner_mode() {
    for mode in [PythonRunnerMode::OneShot, PythonRunnerMode::Persistent] {
        let app = app_with_python_fixture(mode, "decorated_async.py").await?;
        assert_eq!(app.execute(tool_call("echo", json!({"message": "hi"}))).await?, json!({"message": "hi"}));
    }
}
```

Cover plain, decorated, async, LangChain, LlamaIndex, timeout, crash, and invalid protocol fixtures. Assert stable public codes and that provider secrets are absent from public errors.

- [ ] **Step 2: Wire bootstrap and document the behavior**

Pass typed configuration through application setup. Document the default, opt-in, limits, readiness, restart/quarantine signal, and `one-shot` rollback. Update the platform spec to mark Phase 6 Complete only after every exit gate and exact verification evidence exists; otherwise mark it Partial and name the failed gate.

- [ ] **Step 3: Run the complete verification baseline**

```bash
cargo test -p soma-provider-adapters --features python
PYTHONPATH=packages/python/python python3 -m unittest discover -s packages/python/tests -p 'test_*.py'
cargo test -p soma --test python_provider
cargo fmt --all -- --check
cargo clippy -p soma-provider-adapters --features python --all-targets -- -D warnings
git diff --check
cargo xtask check-docs
```

Expected: every command exits 0.

- [ ] **Step 4: Commit the milestone**

```bash
git add apps/soma/tests/python_provider.rs docs/PROVIDERS.md docs/specs/python-provider-platform.md CHANGELOG.md
git commit -m "feat(python): activate supervised persistent runner"
```

## Plan Self-Review

- Phase 6's private channel, handshake, catalog/invoke, health/drain/shutdown, bounded concurrency/logging, restart/crash-loop handling, timeout recycle, explicit fallback, and compatibility proof map to Tasks 1–5.
- Phase 7's generation-aware reload and all Phase 8–12 work are explicitly deferred; they are independent milestones, not incomplete implementation details.
- `PythonRunnerConfig`, `PythonRunnerHostReply`, and `PythonWorkerSupervisor` are defined before their consumers; application code does not touch raw framing.
+

## Revised Task Sequence (Authoritative)

The engineering review supersedes Tasks 1-5 above where they conflict. Execute these six tasks in order.

### Task A: Runner bootstrap and child ownership

Files: create `packages/python/python/soma_provider/runner.py` and `crates/shared/provider-adapters/src/python/transport.rs`; modify `packages/python/pyproject.toml` and maturin packaging.

- [ ] Add tests that install the wheel, run `python -I -m soma_provider.runner` under both ambient and prepared interpreters, and read a Hello control frame.
- [ ] Make persistent mode require that installed module; startup returns a stable error instead of falling back to one-shot.
- [x] Implement authenticated loopback TCP control on every platform, redirect
  provider stdout to stderr, and own Unix workers through process groups and
  Windows workers through kill-on-close Job Objects.
- [ ] Test a descendant that holds or writes the inherited handle; timeout, protocol failure, and shutdown must close, kill, and reap the entire tree before restart.
- [ ] Verify with the wheel build, installed-wheel test, and platform transport unit tests; commit `feat(python): establish isolated persistent runner transport`.

### Task B: Negotiated bidirectional protocol

Files: modify `python_protocol.rs`, its tests, `runner.py`, `test_runner.py`, and `runner_protocol_v1.json`.

- [ ] Add failing Rust/Python fixture tests for `Hello -> Initialize -> Ready`, major mismatch, feature mismatch, partial frames, and describe-before-initialize rejection.
- [ ] Replace reply-only framing with tagged host messages `Initialize|Request|HostReply` and worker messages `Hello|Ready|Reply|HostCall`.
- [ ] Implement request state `Queued -> Written -> Accepted -> Terminal|TimedOut|WorkerLost`; Accepted is non-terminal and late/duplicate terminal replies are ignored after one logged protocol event.
- [ ] Enforce exact frame, finite-JSON, request-id, and negotiated-feature limits; Phase-6 host calls receive explicit unsupported replies.
- [ ] Run Rust codec plus Python subprocess fixture tests; commit `feat(python): negotiate persistent runner protocol`.

### Task C: Typed configuration and fail-closed eligibility

Files: modify `crates/soma/config/src/config.rs`, config tests, and `crates/shared/provider-adapters/src/python.rs`.

- [ ] Add failing tests for only `one-shot|persistent`, positive limits, omitted actor/scopes/trace, and `python_persistent_env_unsupported` for every provider/tool runtime environment requirement.
- [ ] Define `PythonRunnerConfig` with startup/request timeout, restart budget/window/backoff, stderr/pending byte caps, global worker cap, and candidate-start cap.
- [ ] Inject the typed configuration through the `FileProviderSource` construction path; do not read process environment from provider adapters after parsing.
- [ ] Build persistent request Context with only request identity, deadline, snapshot, and cancellation identity.
- [ ] Run focused config/adapter tests; commit `feat(python): fail closed for persistent runtime eligibility`.

### Task D: Serial supervisor, limits, and redaction

Files: create `crates/shared/provider-adapters/src/python/supervisor.rs` and tests; modify adapter `Cargo.toml`, `lib.rs`, and `python.rs`.

- [ ] Add failing lifecycle tests for busy rejection, accepted-then-completed, accepted-then-crash, timeout-then-late reply, stderr flood, input flood, restart backoff, quarantine, and concurrent ensure-worker callers.
- [ ] Give each prestarted `Arc<PythonWorkerSupervisor>` immutable source/path, interpreter/environment, catalog, config, and generation fingerprints.
- [ ] Permit one active invocation and return `python_provider_busy` before allocating pending bytes. Enforce each tool input/output limit before framing plus aggregate pending-byte cap.
- [ ] Run one reader and one continuous stderr drainer. Retain only a fixed ring, discard overflow while counting it, map all worker-originated errors to canonical public messages, and redact all diagnostics.
- [ ] Kill/reap after timeout/EOF/violation, never replay accepted work, and allow exactly one serialized later restart subject to global budget/backoff/quarantine.
- [ ] Run adapter tests and clippy; commit `feat(python): supervise serial persistent workers`.

### Task E: Async registry preflight and bounded retirement

Files: modify `filesystem.rs`, `filesystem_python.rs`, `provider_registry.rs`, application tests, `application/src/lib.rs`, and `apps/soma/src/bootstrap.rs`.

- [ ] Add failing tests for source substitution after first digest, unhealthy candidate retaining the old snapshot, concurrent refresh coalescing to one start, prepared interpreter use, and old-worker drain/reap.
- [ ] Convert provider refresh/activation to an async serialized coordinator. Start/describe/health candidate workers outside registry locks and under global process/candidate budgets.
- [ ] Hash regular non-symlink provider source immediately before launch and after describe; reject drift. Publish only after every candidate is ready.
- [ ] Atomically swap ready map, drain old workers immediately, and retire in bounded background work; a failed candidate leaves the prior snapshot and workers intact.
- [ ] Run application and production persistent tests; commit `feat(python): activate ready persistent runner candidates`.

### Task F: Production parity, documentation, and gates

Files: modify `apps/soma/tests/python_provider.rs`, `docs/PROVIDERS.md`, `docs/specs/python-provider-platform.md`, and `CHANGELOG.md`.

- [ ] Test plain, decorated, async, LangChain, and LlamaIndex fixtures in both modes with equal catalogs/results.
- [ ] Test missing wheel, environment requirement, busy, timeout, crash, invalid frame, source substitution, and secret-containing worker stderr/error/result; assert stable codes and no secret exposure.
- [ ] Document default/opt-in/rollback, installed-wheel precondition, environment restriction, serial concurrency, limits, quarantine, source integrity, and unavailable Phase-8 capabilities.
- [ ] Advance Phase 6 only if every original exit gate and the full command evidence pass.
- [ ] Run `cargo test -p soma-provider-adapters --features python`, Python tests, `cargo test -p soma --test python_provider`, formatting, clippy, diff check, and docs check; commit `feat(python): activate supervised persistent runner`.

## Revised Self-Review

- The revised sequence includes every critical and important engineering-review recommendation.
- Deferred work is limited to true Phase 7+ concerns: multi-worker pools, generation routing/debounce/reset/rollback, live broker capabilities, containment, components, and release/soak work.
- The original detailed task text remains implementation reference only; this sequence controls scope and ordering.
