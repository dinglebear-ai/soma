# Reusable Self-Update Crate Implementation Plan

> Second-review hardening: the implemented transaction uses explicit durable
> `prepared`/`installed`/`rolling_back`/`rolled_back` phases; canonical state
> locks; exact, owner-checked, digest-verified rollback artifacts; dead-PID-only
> orphan reclamation; repeated staged identity checks; authoritative-first
> failed-install cleanup; process-group/Windows-Job validator termination;
> explicit executable-leaf symlink rejection; updater-scoped crash failpoints;
> and deterministic lock-protected marker-temporary recovery.
> Final security hardening also preflights the largest reachable marker before mutation, rejects
> generated backup collisions, kills validator trees on future cancellation,
> requires redirect/final-response URL validation by transport adapters,
> verifies installed bytes before recovery/confirmation state changes, opens
> markers no-follow and nonblocking, and offloads synchronous transactions from
> async executor workers. Marker temporaries and lock descriptors are secured
> to mode 0600 independently of umask, authoritative markers require exactly
> mode 0600 without special bits, and owned legacy lock permissions are repaired.
> Relative layouts bind to their construction-time current directory, partial
> downloads and rollback copies are private before byte writes, marker schema 3
> records the actual backup owner, intended executable modes survive validator
> changes, and successful validators terminate and drain their configured
> process tree before their captured result is accepted. Staging cleanup guards
> arm only after exclusive file creation, and post-marker pre-swap validation
> errors clean authoritative state before backups while preserving combined
> operation and cleanup failures. Copy-backup failures also durably remove
> their owned partial destination and preserve both errors at cleanup failures;
> outer verification applies that contract to hard links too. Prepared-marker
> write failures clean authoritative state before backups and retain the
> recovery pair if state cleanup fails.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract Cortex's agent binary update transaction into a portable `soma-self-update` shared crate that has no dependencies on any Soma, Cortex, or other workspace crate, while closing the URL-resolution, unbounded-download, validation-timeout, and recovery-test gaps found in the Cortex implementation.

**Architecture:** Put a transport-neutral state machine in `crates/shared/self-update`: callers authenticate and obtain an update directive, resolve its artifact URL under an explicit transport policy, and hand the crate an `AsyncRead` body. The crate incrementally stages and hashes a bounded artifact, validates its exact reported version under a timeout, atomically replaces a Unix executable while retaining a rollback copy and durable marker, then returns an explicit restart action. Startup recovery and successful-health confirmation are separate calls so each host decides what "healthy" means. HTTP clients, bearer tokens, heartbeat DTOs, Axum routes, application configuration, and service restart policy remain outside the crate.

**Tech Stack:** Rust 2024 (MSRV 1.96), Tokio async I/O/processes, `serde`/`serde_json`, `sha2`, `thiserror`, `url`, `fs2`, public-API integration tests with `tempfile`, Cargo workspace/xtask architecture guards.

## Global Constraints

- Work only in `/home/jmagar/workspace/soma/.worktrees/cortex-auto-update-crate-review` on `codex/cortex-auto-update-crate-review`; preserve every other checkout and the protected `marketplace-no-mcp` worktree.
- Track implementation under Bead `rmcp-template-xdlz` and keep it `in_progress` until all verification, review, CI, and documentation are complete.
- Use test-driven development: add the named failing test first, run it and record the expected failure, implement the smallest public behavior that passes it, then refactor.
- The crate may use published crates.io dependencies only. It must have no `path` or `workspace = true` dependencies, and its manifest must say so directly.
- Do not add HTTP, bearer-token, Cortex heartbeat, Soma configuration, Axum, tracing-subscriber, or product lifecycle policy to this crate.
- SHA-256 proves artifact integrity, not publisher authenticity. Default URL policy is HTTPS; loopback HTTP requires an explicit opt-in. The README and rustdoc must state that callers must authenticate the directive independently (or verify a detached signature) before invoking the transaction.
- Never derive filenames from the server-supplied version or artifact URL. Stage, lock, marker, and backup paths are derived only from the caller-supplied executable/state paths plus process/time/counter suffixes.
- Do not silently swallow corrupt marker files, cleanup failures, or missing rollback backups. Return typed errors with the affected path and leave enough state for operator diagnosis.
- Unix gets the provided atomic executable replacement/re-exec implementation. Non-Unix builds keep the transport-neutral directive/staging/validation API but return `UnsupportedPlatform` for the provided installer/re-exec helper; callers may supply a platform-specific deployment strategy later.
- Keep tests in `crates/shared/self-update/tests/` so they exercise only the public API. Classify the crate explicitly in `xtask/src/test_siblings.rs` as an integration-test-only shared contract.

---

### Task 1: Scaffold the standalone crate and lock its dependency boundary

**Files:**
- Create: `crates/shared/self-update/Cargo.toml`
- Create: `crates/shared/self-update/src/lib.rs`
- Create: `crates/shared/self-update/src/error.rs`
- Create: `crates/shared/self-update/README.md`
- Create: `crates/shared/self-update/LICENSE`
- Create: `crates/shared/self-update/tests/public_api.rs`
- Modify: `Cargo.toml`
- Modify: `apps/soma/tests/architecture_boundaries.rs`
- Modify: `xtask/src/test_siblings.rs`

- [x] **Step 1: Write the failing workspace-boundary and public API tests**

  Add `soma-self-update` to the workspace members, but do not create its manifest yet. In `apps/soma/tests/architecture_boundaries.rs`, add a focused test that locates the `soma-self-update` package in Cargo metadata and fails if any dependency has a non-null `path` or if `cargo tree -p soma-self-update --edges normal` contains a package whose manifest is under this workspace. In `crates/shared/self-update/tests/public_api.rs`, import the intended root types:

  ```rust
  use soma_self_update::{
      ArtifactTransportPolicy, RecoveryAction, UpdateDirective, UpdateLayout,
      UpdatePolicy, Updater,
  };

  #[test]
  fn public_contract_is_constructible_without_product_types() {
      let layout = UpdateLayout::new("/opt/example/bin/example", "/opt/example/state/update.json");
      let updater = Updater::new(layout, UpdatePolicy::default());
      let directive = UpdateDirective::new(
          "1.2.3",
          "/v1/agent/binary",
          "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      )
      .unwrap();
      assert_eq!(directive.version(), "1.2.3");
      assert_eq!(updater.policy().transport(), ArtifactTransportPolicy::HttpsOnly);
      assert!(matches!(RecoveryAction::NoPendingUpdate, RecoveryAction::NoPendingUpdate));
  }
  ```

- [x] **Step 2: Run the tests to verify they fail for the missing crate**

  Run: `cargo test -p soma --test architecture_boundaries self_update_crate_has_no_workspace_dependencies -- --exact`

  Expected: FAIL because Cargo metadata cannot resolve `crates/shared/self-update/Cargo.toml` or cannot find `soma-self-update`.

  Run: `cargo test -p soma-self-update --test public_api`

  Expected: FAIL because package `soma-self-update` does not exist yet.

- [x] **Step 3: Add the minimal standalone manifest, root types, and errors**

  Create a package named `soma-self-update`, version `0.1.0`, edition `2024`, `rust-version = "1.96"`, `publish = false`, MIT license, `readme = "README.md"`, and `[package.metadata.soma-architecture] layer = "shared"`. Declare only exact crates.io dependencies (no workspace inheritance):

  ```toml
  [dependencies]
  fs2 = "0.4"
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  sha2 = "0.11"
  thiserror = "2"
  tokio = { version = "1", features = ["fs", "io-util", "process", "time"] }
  url = "2"

  [dev-dependencies]
  tempfile = "3"
  tokio = { version = "1", features = ["full"] }
  ```

  Define `UpdateError`/`Result<T>`, `ArtifactTransportPolicy`, validated `UpdateDirective`, `UpdatePolicy`, `UpdateLayout`, `Updater`, and the complete `RecoveryAction` enum in the named modules, re-exported from `lib.rs`. `UpdatePolicy::default()` must be HTTPS-only, limit artifacts to 128 MiB, time validation out after 10 seconds, and allow three unconfirmed restarts. Reject empty versions, non-64-character/non-hex hashes, zero size limits, zero timeouts, and zero restart limits with typed configuration/directive errors.

- [x] **Step 4: Document portability and classify the test layout**

  The README must open with: "`soma-self-update` is a standalone binary self-update transaction for Rust services. It has zero path-dependencies on the Soma workspace and can be copied into another repository wholesale." Add concise Scope, Non-goals, Safety boundary, Platform support, and API lifecycle sections. Copy the repository MIT license into the crate. Add `crates/shared/self-update/src` to `UNCHECKED_SRC_ROOTS` with the reason that all behavior is tested through the public API in `tests/`.

- [x] **Step 5: Run the focused tests and commit**

  Run: `cargo test -p soma-self-update --test public_api`

  Expected: PASS.

  Run: `cargo test -p soma --test architecture_boundaries self_update_crate_has_no_workspace_dependencies -- --exact`

  Expected: PASS and the cargo tree contains only crates.io packages.

  Run: `cargo xtask check-test-siblings`

  Expected: PASS and the output lists `crates/shared/self-update/src` as intentionally integration-tested.

  Commit: `feat(self-update): scaffold standalone update crate`

### Task 2: Resolve artifact URLs safely and stream bounded verified artifacts

**Files:**
- Create: `crates/shared/self-update/src/directive.rs`
- Create: `crates/shared/self-update/src/staging.rs`
- Create: `crates/shared/self-update/tests/directive.rs`
- Create: `crates/shared/self-update/tests/staging.rs`
- Modify: `crates/shared/self-update/src/lib.rs`
- Modify: `crates/shared/self-update/src/error.rs`

- [x] **Step 1: Write failing directive and URL-policy tests**

  Cover all of these cases through public APIs:

  ```rust
  let directive = UpdateDirective::new("2.0.0", "/v1/agent/binary?os=linux", EMPTY_SHA256)?;
  assert_eq!(
      directive.resolve_artifact_url(
          &url::Url::parse("https://example.test/v1/heartbeats")?,
          ArtifactTransportPolicy::HttpsOnly,
      )?.as_str(),
      "https://example.test/v1/agent/binary?os=linux",
  );
  ```

  Also assert that a relative `agent/binary` reference resolves as a URL sibling rather than under `/v1/heartbeats/`, cross-origin absolute URLs are rejected, plain remote HTTP is rejected by both policies, and `http://127.0.0.1`, `http://[::1]`, and `http://localhost` are accepted only by `HttpsOrLoopbackHttp`.

- [x] **Step 2: Run the directive tests to verify the endpoint-base regression fails**

  Run: `cargo test -p soma-self-update --test directive`

  Expected: FAIL because `resolve_artifact_url` is not implemented.

- [x] **Step 3: Implement URL resolution and policy enforcement**

  Parse the caller's endpoint as `url::Url`, normalize it to an origin-root base before resolving server paths, use `Url::join`, require the resolved URL to retain the base scheme/host/effective port, then enforce `ArtifactTransportPolicy`. Do not concatenate strings. Return typed `InvalidBaseUrl`, `InvalidArtifactUrl`, `CrossOriginArtifact`, and `InsecureTransport` errors.

- [x] **Step 4: Write failing bounded-stream staging tests**

  Use `tokio::io::duplex` or a cursor-backed reader to test:

  - chunks are written incrementally and the final SHA-256 is verified;
  - uppercase advertised hashes are accepted but the stored digest is lowercase;
  - a body one byte over `max_artifact_bytes` returns `ArtifactTooLarge` and removes the partial file;
  - a wrong digest returns `DigestMismatch` and removes the partial file;
  - two staging attempts use distinct `create_new` paths that contain no directive version or URL data;
  - staging happens in the executable directory so the later rename remains same-filesystem.

- [x] **Step 5: Run staging tests to verify they fail**

  Run: `cargo test -p soma-self-update --test staging`

  Expected: FAIL because `Updater::stage` and `StagedArtifact` do not exist.

- [x] **Step 6: Implement incremental bounded staging**

  Implement:

  ```rust
  impl Updater {
      pub async fn stage<R>(
          &self,
          reader: R,
          directive: &UpdateDirective,
      ) -> Result<StagedArtifact>
      where
          R: tokio::io::AsyncRead + Unpin;
  }
  ```

  Read fixed-size chunks, check the running byte count before each write, update `Sha256` incrementally, `flush` and `sync_all` before returning, and preserve the installed executable mode on Unix (falling back to restrictive `0o700` when no target exists). Explicit error paths close and remove partial artifacts while an RAII cleanup guard covers cancellation. `StagedArtifact` exposes read-only `path()`, `target_version()`, `sha256()`, and `bytes_written()` accessors and is consumed by installation.

- [x] **Step 7: Run focused tests and commit**

  Run: `cargo test -p soma-self-update --test directive --test staging`

  Expected: PASS.

  Commit: `feat(self-update): stream and verify bounded artifacts`

### Task 3: Validate staged executables with an exact version and hard timeout

**Files:**
- Create: `crates/shared/self-update/src/validation.rs`
- Create: `crates/shared/self-update/tests/validation.rs`
- Modify: `crates/shared/self-update/src/lib.rs`
- Modify: `crates/shared/self-update/src/error.rs`

- [x] **Step 1: Write failing Unix validation tests**

  Under `#[cfg(unix)]`, generate executable shell fixtures and assert:

  - `example 1.2.3` accepts expected version `1.2.3`;
  - `example 11.2.30` rejects expected version `1.2.3` (the Cortex substring-match regression);
  - a non-zero exit returns `ValidationFailed` with exit status and bounded stderr;
  - invalid UTF-8 output returns `InvalidVersionOutput`;
  - a script that sleeps longer than policy returns `ValidationTimedOut` within a two-second outer test timeout and is killed rather than orphaned;
  - output larger than 16 KiB is rejected or truncated without unbounded allocation.

- [x] **Step 2: Run validation tests to verify they fail**

  Run: `cargo test -p soma-self-update --test validation`

  Expected: FAIL because `Updater::validate` is not implemented.

- [x] **Step 3: Implement the validator**

  Add:

  ```rust
  impl Updater {
      pub async fn validate(&self, staged: StagedArtifact) -> Result<ValidatedArtifact>;
  }
  ```

  Consume the staged artifact, spawn its path with `--version`, pipe stdout/stderr, set `kill_on_drop(true)`, enforce `policy.validation_timeout()` with `tokio::time::timeout`, and explicitly kill/wait on timeout. Drain each output stream while retaining at most 16 KiB. Treat the advertised version as a whole ASCII-whitespace-delimited token after trimming surrounding ASCII punctuation, never as a substring. `ValidatedArtifact` privately retains the staged artifact so installation cannot accept unvalidated bytes.

- [x] **Step 4: Run focused tests and commit**

  Run: `cargo test -p soma-self-update --test validation`

  Expected: PASS with no process left alive after the timeout test.

  Commit: `feat(self-update): validate staged binaries with timeout`

### Task 4: Implement atomic install, durable markers, confirmation, and rollback

**Files:**
- Create: `crates/shared/self-update/src/transaction.rs`
- Create: `crates/shared/self-update/tests/transaction.rs`
- Modify: `crates/shared/self-update/src/lib.rs`
- Modify: `crates/shared/self-update/src/error.rs`

- [x] **Step 1: Write failing install and recovery transaction tests**

  Exercise only public APIs with temporary paths. Under `#[cfg(unix)]`, cover:

  - `install(validated, "1.0.0")` takes an exclusive `fs2` lock, creates a unique backup, durably writes a marker before replacement, atomically renames the staged file over the executable, and returns `InstallOutcome::RestartRequired`;
  - an install failure leaves original executable bytes untouched and removes any marker/backup created by that failed attempt;
  - a second concurrent updater gets `UpdateInProgress` rather than racing;
  - `recover_on_startup("2.0.0")` increments attempts and returns `RecoveryAction::PendingUpdate` until the configured limit;
  - the next startup restores the backup and returns `RecoveryAction::RollbackInstalled { executable }`;
  - a marker targeting a different running version is reported as `RecoveryAction::StaleMarkerRemoved` and cannot replace the executable;
  - malformed marker JSON is an error and remains on disk for diagnosis;
  - a missing rollback backup is an error and does not clear the marker;
  - `confirm_success("2.0.0")` removes both marker and backup and returns `Confirmed`, while a mismatched running version returns an error without deleting recovery state.

- [x] **Step 2: Run transaction tests to verify they fail**

  Run: `cargo test -p soma-self-update --test transaction`

  Expected: FAIL because installation/recovery APIs do not exist.

- [x] **Step 3: Implement the durable transaction**

  Use these public result shapes:

  ```rust
  pub enum InstallOutcome {
      RestartRequired { executable: PathBuf, from: String, to: String },
  }

  pub enum RecoveryAction {
      NoPendingUpdate,
      PendingUpdate { target: String, attempts: u32, max_attempts: u32 },
      RollbackInstalled { executable: PathBuf, restored_version: String },
  }

  pub enum ConfirmationOutcome {
      NoPendingUpdate,
      Confirmed { version: String },
  }
  ```

  Marker JSON contains `schema_version: 3`, target/previous versions, absolute executable/backup paths, the recorded backup owner, attempts, and verified current/previous digests. Write it to a same-directory temporary file, `sync_all`, rename it over the state file, and sync the parent directory on Unix. Acquire an advisory exclusive lock on `<state_file>.lock` for every install/recover/confirm operation. Back up by hard link with copy fallback, never overwrite an existing backup, and sync copied bytes. On any pre-swap failure, clean up only artifacts created by that call; after a successful swap, preserve marker and backup until explicit confirmation.

- [x] **Step 4: Add end-to-end state-machine tests**

  In the same integration test, run both complete paths:

  1. reader → stage → validate → install → confirm; assert new bytes are live and marker/backup are gone;
  2. reader → stage → validate → install → repeated unconfirmed startups → rollback; assert old bytes are live and the action requests restart.

  These are the transaction tests missing from Cortex; do not replace them with helper-only unit tests.

- [x] **Step 5: Run focused tests and commit**

  Run: `cargo test -p soma-self-update --test transaction`

  Expected: PASS.

  Run: `cargo test -p soma-self-update`

  Expected: PASS for every public-API integration test.

  Commit: `feat(self-update): add atomic install and rollback transaction`

### Task 5: Add the explicit Unix restart adapter and adopter documentation

**Files:**
- Create: `crates/shared/self-update/src/unix.rs`
- Create: `crates/shared/self-update/tests/unix.rs`
- Create: `crates/shared/self-update/examples/heartbeat_agent.rs`
- Modify: `crates/shared/self-update/src/lib.rs`
- Modify: `crates/shared/self-update/README.md`

- [x] **Step 1: Write failing platform-contract tests**

  On Unix, assert `restart_command(executable, args)` preserves all caller arguments and targets the installed executable. On non-Unix, assert the provided installer/restart entry points return `UnsupportedPlatform` rather than pretending replacement is atomic.

- [x] **Step 2: Run the tests to verify they fail**

  Run: `cargo test -p soma-self-update --test unix`

  Expected: FAIL because the platform adapter does not exist.

- [x] **Step 3: Implement restart construction and Unix re-exec**

  Export a testable `restart_command(&Path, impl IntoIterator<Item = OsString>) -> Command` and a Unix-only `reexec(&Path, impl IntoIterator<Item = OsString>) -> Result<Infallible>` using `std::os::unix::process::CommandExt::exec`. Do not read global process arguments inside the library; the caller must pass the arguments it wants preserved. Keep process supervision/systemd restart as an explicit adopter alternative in rustdoc.

- [x] **Step 4: Add a compile-checked adopter example and finish the README**

  `examples/heartbeat_agent.rs` must demonstrate the real lifecycle without making an HTTP choice for the library: deserialize/authenticate a directive in caller code, resolve its URL, obtain an `AsyncRead` from a compile-checked in-memory caller function, call `stage` → `validate` → `install`, re-exec on `RestartRequired`, call `recover_on_startup` before entering the service loop, and call `confirm_success` only after the first successful health report. Gate the runnable body so `cargo test --doc` and `cargo check --example heartbeat_agent` require no network or live binary replacement.

  README sections must include:

  - a Cortex extraction map (`AgentUpdateDirective` → `UpdateDirective`; reqwest/bearer auth stays in Cortex; `maybe_update` splits into stage/validate/install/restart; heartbeat success calls confirmation);
  - the fixed endpoint example `https://host/v1/heartbeats` + `/v1/agent/binary` → `https://host/v1/agent/binary`;
  - security guidance that same-channel SHA-256 does not authenticate a hostile server and that detached signature verification belongs before `stage`;
  - 128 MiB default streaming limit and 10-second validation timeout;
  - crash windows, marker semantics, rollback threshold, lock behavior, operator recovery, and backup cleanup;
  - Unix support and the explicit non-Unix limitation;
  - non-goals: release discovery, HTTP/auth, signature format/key management, service orchestration, server artifact hosting, and automatic Cortex/Soma integration.

- [x] **Step 5: Run example/docs tests and commit**

  Run: `cargo test -p soma-self-update --test unix && cargo test -p soma-self-update --doc`

  Expected: PASS.

  Run: `cargo check -p soma-self-update --example heartbeat_agent`

  Expected: PASS without network access.

  Commit: `docs(self-update): document adoption and Unix lifecycle`

### Task 6: Integrate repository metadata and run all quality gates

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `Cargo.lock`
- Modify: `docs/superpowers/plans/2026-07-18-reusable-self-update-crate.md`
- Modify if required by generated checks: repository-owned generated metadata only

- [x] **Step 1: Add the changelog entry**

  Under `[Unreleased] / Added`, describe `crates/shared/self-update` as a standalone, transport-neutral update transaction with bounded streaming SHA-256 verification, timed exact-version validation, Unix atomic replacement, durable health confirmation/rollback, and no internal crate dependencies. State that no Soma runtime or Cortex integration is enabled by this PR.

- [x] **Step 2: Format and stage new files before repository guards**

  Run: `cargo fmt --all -- --check`

  If it fails, run `cargo fmt --all`, then rerun the check.

  Stage the new crate files before `cargo xtask check-test-siblings`, because that guard uses tracked workspace state. Do not stage unrelated files.

- [x] **Step 3: Run crate and architecture gates**

  Run each command separately and require PASS:

  ```bash
  cargo test -p soma-self-update
  cargo clippy -p soma-self-update --all-targets --all-features -- -D warnings
  cargo tree -p soma-self-update --edges normal
  cargo test -p soma --test architecture_boundaries self_update_crate_has_no_workspace_dependencies -- --exact
  cargo xtask check-architecture
  cargo xtask check-test-siblings
  cargo xtask check-version-sync
  cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
  ```

  Inspect the cargo tree output and verify every normal dependency is registry-sourced; a green command alone is insufficient.

- [x] **Step 4: Run the full workspace gates**

  Run:

  ```bash
  cargo test --workspace --all-features -- --test-threads=1
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo fmt --all -- --check
  cargo build --workspace --all-features
  ```

  Expected: all PASS. The serialized test invocation is intentional because the baseline showed `soma-codemode` environment/budget tests can interfere under parallel workspace execution. If the normal parallel command fails, reproduce the named test alone and under its package; fix any failure that reproduces in this worktree.

- [x] **Step 5: Self-review plan completion and code scope**

  Check every box only after its evidence exists. Search the implementation and README for unfinished-work markers, panic-only macro bodies, Cortex-specific env keys, `reqwest`, and internal path dependencies. Expected: no unfinished code or product coupling; mentions of Cortex/reqwest are allowed only in the extraction/non-goal documentation.

  Review `git diff $(git merge-base origin/main HEAD) HEAD` for accidental changes, generated artifacts, secrets, absolute developer paths, version-manifest edits, and plugin manifest `version` fields. Expected: only the crate, workspace registration/lockfile, architecture/test-layout guards, changelog, and this plan.

- [x] **Step 6: Commit the integration, push, and update the PR**

  Commit: `chore(self-update): integrate crate with workspace checks`

  Push `codex/cortex-auto-update-crate-review`, update the draft PR body with the design boundary and verification table, and leave Bead `rmcp-template-xdlz` open until independent review, GitHub comments, and CI are green.

## Plan Self-Review Checklist

- [x] Every requested capability is mapped to a task: standalone crate, no internal dependencies, safe URL resolution, bounded streaming, digest verification, timed exact-version validation, atomic Unix replacement, durable confirmation, rollback, explicit platform/lifecycle boundary, adopter docs, and end-to-end transaction tests.
- [x] Every task names exact files, a failing test, the command and expected failure, the implementation contract, the passing command, and a scoped commit.
- [x] Public types remain consistent across tasks: `UpdateDirective` → `StagedArtifact` → `ValidatedArtifact` → `InstallOutcome`, with `RecoveryAction` and `ConfirmationOutcome` for post-restart lifecycle.
- [x] No task requires a Soma/Cortex crate, live service, network credential, release bump, plugin manifest change, or modification of `../cortex`.
- [x] No unfinished-work marker, omitted branch, or pseudocode-only implementation step remains.
