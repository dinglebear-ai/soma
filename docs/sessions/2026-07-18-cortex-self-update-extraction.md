---
date: 2026-07-18 16:08:03 EDT
repo: git@github.com:jmagar/soma.git
branch: codex/cortex-auto-update-crate-review
head: 6c7e83e11f02ddd58a6edbeb727a900646ca2155
plan: docs/superpowers/plans/2026-07-18-reusable-self-update-crate.md
working directory: /home/jmagar/workspace/soma/.worktrees/cortex-auto-update-crate-review
worktree: /home/jmagar/workspace/soma/.worktrees/cortex-auto-update-crate-review
pr: "#170 feat: add reusable self-update crate (https://github.com/jmagar/soma/pull/170)"
beads: rmcp-template-xdlz, rmcp-template-xdlz.1 through rmcp-template-xdlz.24
---

# Cortex self-update extraction

## User Request

Create a worktree from `main`, inspect Cortex's automatic update mechanism, determine whether it can become a reusable crate with no Soma dependencies, write a plan, and execute it with the work-it workflow.

## Session Overview

Created an owned feature worktree and draft PR, distinguished Cortex's heartbeat-driven agent updater from its operator deployment updater, and extracted the reusable logic into `soma-self-update`. The new crate has only crates.io dependencies, a transport-neutral API, bounded download staging, exact-version validation, process-tree containment, crash-consistent install/confirm/rollback, and Linux plus native Windows coverage. Repeated independent and ready-for-review passes produced 24 tracked findings; all were fixed, verified, and closed.

## Sequence of Events

1. Created `/home/jmagar/workspace/soma/.worktrees/cortex-auto-update-crate-review` on `codex/cortex-auto-update-crate-review`, based on current `origin/main`.
2. Inspected `/home/jmagar/workspace/cortex/src/agent/self_update.rs`, its heartbeat caller, and server directive creation; excluded Cortex's separate operator SSH/Compose updater from the extraction target.
3. Wrote the implementation plan and opened draft PR #170 before implementation.
4. Added the standalone crate using test-driven batches for directive validation, bounded staging, candidate execution, transaction recovery, documentation, examples, and workspace enforcement.
5. Ran repeated independent reviews. Each actionable issue was recorded as a child Bead, fixed, retested, and reviewed again.
6. Added native Windows CI execution after cross-compilation alone proved insufficient; the Windows job caught and verified platform-specific path and Job Object behavior.
7. Split transaction concerns into focused sibling modules to satisfy Soma's 350-line new-file target without adding an exception.
8. Closed the first 23 child Beads and parent `rmcp-template-xdlz` after local and remote gates passed.
9. Marked PR #170 ready; the Codex connector found a same-version executable replacement gap in confirmation. Added Bead `.24`, reproduced it, rehashed installed bytes before cleanup, passed 53 tests and full gates, resolved the review thread, and reclosed the parent.

## Key Findings

- Cortex's automatic mechanism is the heartbeat agent flow in `/home/jmagar/workspace/cortex/src/agent/self_update.rs`; `/home/jmagar/workspace/cortex/src/update.rs` is a different operator deployment coordinator.
- The reusable boundary is directive policy, streamed staging, candidate validation, and filesystem transaction/recovery. Heartbeat transport, release publishing, and service restart orchestration remain adopter concerns.
- Cortex's original flow used whole-body buffering, substring version matching, no complete transaction state machine, and insufficient process-tree/cross-platform containment.
- Artifact URLs require HTTPS and same-origin validation for the initial URL, every redirect hop, and the final response URL. Loopback HTTP is an explicit policy opt-in.
- The final crate has no path/workspace dependencies; `apps/soma/tests/architecture_boundaries.rs` enforces this mechanically.

## Technical Decisions

- Kept HTTP fetching outside the crate so adopters can use any transport while still applying the crate's redirect/final-URL validator.
- Used type states (`StagedArtifact` to `ValidatedArtifact`) and rechecked digest plus file identity immediately before install.
- Rejected executable leaf symlinks; canonicalized state identity and locks; required exact, owned, non-symlink artifact grammar.
- Used durable marker phases and parent-directory synchronization for crash-idempotent prepared/install/rollback transitions.
- Used `process-wrap` process groups on Unix and Job Objects on Windows; an armed drop guard contains timeout and caller-cancellation paths.
- Kept same-origin as a fixed security policy instead of adding a caller-controlled cross-origin allowlist.

## Files Changed

| Status | Path | Previous path | Purpose | Evidence |
|---|---|---|---|---|
| modified | `.github/workflows/ci.yml` | — | Run native Windows self-update tests | Build Windows CI passed |
| modified | `CHANGELOG.md` | — | Document reusable updater capability | PR diff |
| modified | `Cargo.toml`, `Cargo.lock` | — | Register crate and lock public dependencies | locked builds passed |
| modified | `apps/soma/tests/architecture_boundaries.rs` | — | Reject internal/path/workspace dependencies | boundary test passed |
| created | `crates/shared/self-update/Cargo.toml`, `LICENSE`, `README.md` | — | Standalone package metadata and adoption contract | package check passed |
| created | `crates/shared/self-update/examples/heartbeat_agent.rs` | — | Compile-checked safe adapter lifecycle | example check passed |
| created | `crates/shared/self-update/src/directive.rs`, `error.rs`, `lib.rs` | — | Policy, errors, and public API | tests and Clippy passed |
| created | `crates/shared/self-update/src/staging.rs`, `validation.rs`, `unix.rs` | — | Bounded staging and cross-platform candidate containment | Linux and Windows tests passed |
| created | `crates/shared/self-update/src/transaction.rs`, `transaction_artifacts.rs`, `transaction_io.rs`, `transaction_layout.rs`, `transaction_marker.rs`, `transaction_non_unix.rs` | — | Transaction orchestration and focused filesystem/recovery helpers | crash suite and patterns passed |
| created | `crates/shared/self-update/src/transaction_tests.rs` | — | Private failpoint and lifecycle-bound tests | parallel suite passed |
| created | `crates/shared/self-update/tests/crash_recovery.rs`, `directive.rs`, `public_api.rs`, `staging.rs`, `transaction.rs`, `unix.rs`, `validation.rs`, `validation_windows.rs` | — | Integration and platform regression coverage | 52 Linux tests and native Windows suite passed |
| created | `docs/superpowers/plans/2026-07-18-reusable-self-update-crate.md` | — | Executed implementation plan | all tasks complete |
| modified | `xtask/src/test_siblings.rs` | — | Classify the private test sibling intentionally | test-sibling check passed |

## Beads Activity

All entries were created or updated, claimed, verified, closed, and synchronized with `bd dolt push`.

| ID | Title | Final status | Why it mattered |
|---|---|---|---|
| `rmcp-template-xdlz` | Extract a reusable self-update crate | closed | Parent delivery tracker |
| `.1` | Make production-only self-update library builds compile | closed | Prevented dev-feature masking |
| `.2` | Bound validator lifetime and terminate descendants | closed | Contained hostile candidates |
| `.3` | Reject installs while an update is pending | closed | Preserved last-known-good rollback |
| `.4` | Reject destructive update path collisions | closed | Prevented executable/state overwrite |
| `.5` | Make health confirmation crash-consistent | closed | Preserved authoritative recovery order |
| `.6` | Retain rollback state on running-version mismatch | closed | Failed closed under uncertain identity |
| `.7` | Preserve executable permissions across update and rollback | closed | Kept launchability and access mode |
| `.8` | Bound and clean crash-leftover update artifacts | closed | Prevented disk growth without deleting live stages |
| `.9` | Reverify validated artifact at install time | closed | Closed mutation/path replacement gaps |
| `.10` | Durably persist rollback directory entries | closed | Synced backup data and directory entry |
| `.11` | Bound marker reads and make cleanup diagnostics explicit | closed | Bounded corrupted state and surfaced cleanup failure |
| `.12` | Keep the non-Unix crate surface warning-clean | closed | Added Windows Job Object/runtime proof |
| `.13` | Make update marker phases crash-idempotent | closed | Recovered at every durable transition |
| `.14` | Canonicalize locks and strictly validate rollback artifacts | closed | Prevented alias races and rollback substitution |
| `.15` | Make failed-install cleanup directory-durable | closed | Prevented resurrected incomplete state |
| `.16` | Scope update failpoints so parallel tests cannot interfere | closed | Removed test-order races |
| `.17` | Use one canonical executable identity across the update lifecycle | closed | Rejected unsafe leaf symlink layouts |
| `.18` | Reclaim durable marker temp files safely | closed | Bounded marker temp accumulation |
| `.19` | Validate marker temporary ownership against the updater identity | closed | Supported trusted root-owned service directories |
| `.20` | Reject oversized update markers before filesystem mutation | closed | Preflighted worst-case durable marker size |
| `.21` | Reject generated rollback path collisions before backup creation | closed | Protected dynamic transaction identities |
| `.22` | Kill validator process trees when validation futures are cancelled | closed | Contained caller cancellation on Unix/Windows |
| `.23` | Require artifact URL validation on every redirect and final response | closed | Prevented redirect trust-policy bypass |
| `.24` | Verify installed digest before confirming update | closed | Preserved rollback when same-version installed bytes changed |

## Repository Maintenance

- Plans: `docs/plans/` contained no completed plan eligible to move. The active plan is under `docs/superpowers/plans/` and remains with the feature branch as implementation evidence.
- Beads: parent and all 24 children were read, verified, closed, and synchronized; `.24` followed the ready-for-review Codex finding and focused/full local proof.
- Worktrees/branches: inspected every registered worktree and local/remote branch. No cleanup was safe or in scope because several worktrees are active or of unknown ownership; `marketplace-no-mcp` is protected and was not touched.
- Stale docs: the new crate README, example, changelog, plan, and CI workflow were updated to match the final redirect, ownership, cancellation, and recovery contracts.
- No live configuration changes are required because the crate is not yet integrated into a Soma or Cortex runtime.

## Tools and Skills Used

- Skills: `superpowers:writing-plans` created the executable TDD plan; `vibin:work-it` drove implementation, reviews, Beads, CI, and closeout; worktree, executing-plan, review, verification, save-session, quick-push, and merge-status skill guidance supported their respective phases.
- Shell and GitHub CLI: inspected both repositories, managed the worktree/branch/PR, ran Cargo and xtask gates, queried CI jobs, and synchronized Beads.
- File tools: `apply_patch` created the plan/session artifact and implementation agents made scoped code changes.
- Dedicated agents: implementation, architecture, quality, and security agents performed independent repeated passes. Reviews were intentionally rerun after each stable remediation commit.
- No browser, MCP gateway, or external web research was needed; all evidence came from local source, GitHub CI, and the repository tracker.

## Commands Executed

| Command | Result |
|---|---|
| `git worktree add ... origin/main` | Created owned worktree and feature branch |
| `cargo test -p soma-self-update --all-features` | 53 Linux tests passed after the final confirmation-integrity regression |
| `cargo clippy -p soma-self-update --all-targets --all-features -- -D warnings` | Passed |
| Windows GNU `cargo check`, `clippy`, and `test --no-run` | Passed |
| `cargo test --workspace --locked -- --test-threads=1` | Passed; one unrelated codemode parallel flake reran green |
| `cargo xtask patterns` and `cargo xtask check-test-siblings` | Passed; no self-update size warning |
| architecture/version/release/feature checks | Passed; features 6/6 |
| `gh pr checks 170` | CI Gate, MSRV Gate, Linux, Windows, conformance, and security checks passed |
| `bd dolt push` | Synchronized closed parent and 23 child Beads |

## Errors Encountered

- Initial Windows CI exposed a raw-versus-canonical staging path assertion. The assertion was canonicalized and native reruns passed.
- Soma Contracts rejected an oversized transaction module. Artifact, marker, I/O, layout, and test concerns were split into cohesive sibling modules; `transaction.rs` finished at 348 physical lines.
- Independent reviews reproduced crash-boundary wedges, path collisions, parallel failpoint races, process-tree leaks, redirect bypass, ownership mismatches, and a worst-case marker-size boundary. Each received a Bead, regression test, fix, and clean rereview.
- The ready-for-review Codex connector found that confirmation did not rehash installed bytes. Bead `.24` added the failing same-version replacement regression; `confirm_success()` now preserves marker and rollback state on digest mismatch, and the inline thread was resolved after proof.
- A full workspace run observed one unrelated `soma-codemode` parallel global-state flake; the focused test and serialized full suite reran green, so scope was not expanded.

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Reuse | Cortex-specific heartbeat updater | Standalone `soma-self-update` crate with no internal dependencies |
| Download | Whole-body buffering | Bounded streamed staging with SHA-256 verification |
| Validation | Loose version output and direct-child timeout | Exact version, absolute deadline, process group/Job Object, cancellation guard |
| Install | Ad hoc replacement/backup | Durable phased transaction with confirmation and rollback |
| Trust | Initial URL only | Initial, redirect-hop, and final-response URL validation contract |
| Portability | Unix-oriented | Transport-neutral surface plus native Windows validation coverage |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| crate tests | Lifecycle and regressions pass | 53 Linux tests passed after the final review fix | pass |
| native Windows self-update CI | Job Object timeout/cancellation cleanup works | Build Windows step passed | pass |
| workspace test/Clippy/build | No regression | Passed locally and in CI | pass |
| architecture boundary | No Soma/path/workspace dependency | Passed; cargo tree contains crates.io packages only | pass |
| CI/MSRV gates | Current HEAD accepted | CI Gate and MSRV Gate passed | pass |
| independent final reviews | No actionable findings | Security/quality clean at `6c7e83e`; focused confirmation review clean at `842ca2d` | pass |

## Risks and Rollback

- The crate is new and not yet wired into Cortex or Soma runtime behavior, so reverting the feature commits removes it without runtime migration.
- Adoption requires trusted writable directories, an adapter that validates every redirect/final URL, and caller-controlled restart orchestration as documented in the README.
- Unix executable replacement remains Unix-only; the portable directive/staging/validation surface is supported on Windows.

## Decisions Not Taken

- Did not extract Cortex's operator SSH/Compose updater; it is a different deployment concern.
- Did not add cross-origin allowlisting; same-origin remains the locked security policy.
- Did not add internal Soma dependencies or integrate the crate into a product runtime in this PR.
- Did not delete or modify unrelated worktrees/branches, especially protected `marketplace-no-mcp`.

## References

- PR #170: https://github.com/jmagar/soma/pull/170
- Plan: `docs/superpowers/plans/2026-07-18-reusable-self-update-crate.md`
- Cortex source reviewed: `/home/jmagar/workspace/cortex/src/agent/self_update.rs`

## Next Steps

- Mark PR #170 ready and obtain the non-draft CodeRabbit review.
- Run the merge-status collector after the session artifact lands and all PR checks settle.
- Integrate `soma-self-update` into Cortex in a separate, explicitly scoped change if desired.
