---
date: 2026-07-19 00:37:59 EST
repo: git@github.com:jmagar/soma.git
branch: claude/incus-api-crate-d65a18
head: a8b7eae0783270136a297f1ad9f37dbce9f1f9b9
plan: docs/superpowers/plans/2026-07-17-incus-client-crate.md
working directory: /home/jmagar/workspace/soma/.claude/worktrees/incus-api-crate-d65a18
worktree: /home/jmagar/workspace/soma/.claude/worktrees/incus-api-crate-d65a18
pr: #165 feat(incus-client): add Incus REST API client crate (unix-socket only) — https://github.com/jmagar/soma/pull/165 (MERGED)
beads: rmcp-template-hwu2 (epic, closed), rmcp-template-hwu2.1–.34 (34 child beads, all closed), rmcp-template-21b7 (follow-up epic, left open)
---

## User Request

Search the web for the Incus REST API documentation, then use `/lavra-plan` to create a new Rust crate wrapping that API. Follow-up direction placed the crate at `crates/shared/incus-client` as a pure API client library (no MCP/CLI wiring). Later in the session: "I said ALL issues should be addressed - not P1/P2 only - please resume working until you've fixed ALL of the issues from the review pass," and finally "merge it."

## Session Overview

Built `crates/shared/incus-client` from scratch — a hand-rolled, Unix-socket-only async Rust client for the Incus container/VM manager REST API, with no `reqwest`/TLS dependency. Went through research → plan → implementation (8 feature commits) → two rounds of multi-agent review (24 findings, then a regression-catching second pass) → a final all-issues sweep driven by explicit user instruction, which included fetching and reading the real `lxc/incus` Go daemon source to correct a previously-assumed-wrong sync/async return-type contract across every mutating resource endpoint. Wrote a comprehensive README as the crate's definitive reference doc. Fixed two rounds of CI-only failures (Windows build gating, a gitleaks false positive). Merged the finished PR #165 into `main`, resolving three separate merge conflicts against a very fast-moving `main` branch (auto-generated merge commits, ~150 commits landed on `main` across the session's lifetime from concurrent unrelated work) along the way — including one real regression the last merge exposed (a new `xtask` src-root classification gate failing for the newly-merged crate) which was diagnosed and fixed before every required check went green.

## Sequence of Events

1. Researched the Incus REST API (sync/async envelope model, operations/wait long-poll, ETag/If-Match, recursion query semantics) via web search, then ran `/lavra-plan` to produce `docs/superpowers/plans/2026-07-17-incus-client-crate.md`.
2. Clarified crate placement (`crates/shared/incus-client`) and scope (pure library, no MCP/CLI) via `AskUserQuestion`.
3. Implemented the crate across 8 feature commits: scaffolding/error types/config, Unix-socket transport with envelope parsing, operation wait/cancel lifecycle, events subscription (WebSocket, `events` feature), instances resource (CRUD/lifecycle/snapshots), and images/networks/storage/projects resources.
4. Ran review round 1 (8 agents, 24 findings) covering security (CRLF injection, integer overflow in chunked-body cap, header count/byte caps), correctness (`list_storage_volumes(recursion=false)` wire-format bug, missing clippy `--all-targets` gate), API completeness (missing `update_storage_pool`/`update_storage_volume`/`get_storage_volume`, incomplete ETag coverage), and hardening/simplification — fixed all P1/P2.
5. Ran review round 2 (6 `pr-review-toolkit` agents) against the round-1 fix commit; caught a critical regression (the new default per-request timeout silently applying to `wait_for_operation`'s intentionally-unbounded long-poll) independently in 5 of 6 agents, plus error-diagnostics and WebSocket-close-handling fixes.
6. Fixed two Windows-CI-only failures (`#![cfg(unix)]` crate gate, then the same gate on the new `examples/basic.rs`) and one gitleaks false positive (`.gitleaksignore` with the exact pinned-version fingerprint), each reproduced locally against the exact CI toolchain before pushing.
7. User requested a fully comprehensive README covering every part of the crate; rewrote it as the crate's definitive reference doc (method tables, ETag section, error taxonomy, testing counts, known limitations).
8. User gave the explicit instruction to address every remaining P3/P4 finding, not just P1/P2. This surfaced a much larger issue via research: fetched and read the real `lxc/incus` daemon Go source (`cmd/incusd/*.go`) via `gh api` and confirmed the crate's prior "always async" assumption for network/project/storage-pool mutations was wrong — those are synchronous, and storage-volume creation is conditionally sync-or-async. Fixed all affected method signatures, added `WithEtag<T>` encapsulation with guarded update methods, `Error::NotFound`/`Error::WebSocketProtocol`, and full storage ETag parity; closed all 34 child beads plus the epic.
9. Checked PR #165 CI repeatedly across the session as it went from red (Windows build, secret scan) to green.
10. On explicit "merge it" instruction: discovered `main` had moved out from under the branch three separate times during the session (up to 75 commits between checks) and resolved three merge conflicts in sequence — `CHANGELOG.md` (twice, competing `[Unreleased]` entries from parallel PRs) and `xtask/src/test_siblings.rs` (twice, competing new-crate classification entries from a concurrently-merged `unifi`/`gotify` integrations effort).
11. The second `xtask/src/test_siblings.rs` merge exposed a real, previously-passing-then-newly-failing CI test (`every_workspace_member_src_root_is_classified`) because the merge introduced `crates/integrations/unifi` alongside the new `crates/shared/incus-client`, neither yet registered in xtask's src-root allow/deny lists — fixed by classifying both crates correctly (`incus-client` into `CHECKED_SRC_ROOTS`, `unifi` into `UNCHECKED_SRC_ROOTS` with its inline-test-convention rationale), verified locally, and pushed.
12. Also diagnosed (without needing to fix) a `Build Windows` CI failure whose root cause turned out to be stale-relative-to-main test code (`doctor_cli.rs`) that had already been fixed on `main` in the interim — resolved automatically once that merge picked up main's current state.
13. Confirmed CI fully green (`CI Gate`, `MSRV Gate`, `Build Windows`, `Format`, and every other required check) on the final merge commit, then the user (or GitHub, outside this session's direct `gh pr merge` call — no such call was made in this session) merged PR #165 into `main` at merge commit `b8e199e`.
14. Verified the branch HEAD is a confirmed ancestor of `main` post-merge.

## Key Findings

- Incus's REST API envelope model (`{"type":"sync"/"async",...}`) required per-endpoint verification against the real daemon source rather than the plan's original "when unsure, treat as async" heuristic — the heuristic was itself found to produce a *specific, provably wrong* answer for three resource types once checked against `cmd/incusd/networks.go`, `cmd/incusd/api_project.go`, and `cmd/incusd/storage_pools.go`/`storage_volumes.go`.
- `crates/shared/incus-client/src/resources/storage.rs:145-165` — `create_storage_volume` is genuinely conditional (sync when `params` has no `source.name`, async otherwise), verified against `doVolumeCreateOrCopy`; the only resource-mutation endpoint in the crate with this shape.
- `xtask/src/test_siblings.rs`'s `every_workspace_member_src_root_is_classified` test is a real safety net, not boilerplate — it caught a genuine coverage gap (two newly-merged crates unclassified) that would otherwise have let `check-test-siblings` silently skip them while still reporting a clean pass.
- Self-hosted CI runners on this repo were under heavy, unrelated concurrent load throughout the session (dozens of parallel branches/PRs), which manifested as a spuriously cancelled MSRV job (killed mid–cache-save step, not a real MSRV violation) and long `queued` delays — worth knowing before assuming a red/stalled check reflects this PR's own code.

## Technical Decisions

- No `reqwest`/TLS dependency: transport is hand-rolled HTTP/1.1-over-`UnixStream`, matching the epic's unix-socket-only v1 scope and keeping the crate's dependency graph free of `reqwest`/`rustls` (verified via `cargo tree`).
- `WithEtag<T>` fields changed from `pub` to `pub(crate)` with accessor methods plus new `*_guarded` update methods (e.g. `update_storage_pool_guarded`) — prevents callers from fabricating an `If-Match` value that was never actually fetched, while keeping the common "fetch then update" path ergonomic.
- Remote mTLS/TOFU transport and certificates CRUD were split into a separate backlog epic (`rmcp-template-21b7`, left open/unstarted) rather than built speculatively — the original engineering review found this to be the riskiest, least-precedented, and currently consumer-less part of the plan.
- Chose to resolve each `main`-drift merge conflict directly (favoring "land the PR" per explicit user instruction) rather than proposing a rebase strategy each time, since every conflict was a mechanical two-way content collision (competing changelog entries, competing new-crate classification lines) with an obvious non-destructive resolution.

## Files Changed

PR #165's full merged diff (28 files, +5393/-0 net in `crates/shared/incus-client/` alone; totals below include the cross-cutting files):

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| created | `docs/superpowers/plans/2026-07-17-incus-client-crate.md` | — | Implementation plan from `/lavra-plan` | commit `87dc92b` |
| created | `crates/shared/incus-client/Cargo.toml` | — | Crate manifest, `events` feature | commit `0589ad7` |
| created | `crates/shared/incus-client/src/lib.rs` | — | `#![cfg(unix)]` crate gate + module wiring | commit `0589ad7`, `d0dbae5` |
| created | `crates/shared/incus-client/src/error.rs` + `error_tests.rs` | — | `Error` enum, `NotFound`, `WebSocketProtocol`, `Timeout{after,request_fully_sent}` | commits `0589ad7`, `8ca3f89` |
| created | `crates/shared/incus-client/src/config.rs` + `config_tests.rs` | — | `ClientConfig` | commit `0589ad7` |
| created | `crates/shared/incus-client/src/transport.rs` + `transport_tests.rs` | — | envelope parsing, `WithEtag<T>`, `resource_error_or` | commits `d06e361`, `394f899`, `8ca3f89` |
| created | `crates/shared/incus-client/src/transport/unix.rs` + `unix_tests.rs` | — | HTTP/1.1-over-Unix-socket transport, CRLF/path-char rejection, chunked/Content-Length handling, timeouts | commits `d06e361`, `5462446`, `e4d0963`, `77b430e` |
| created | `crates/shared/incus-client/src/operations.rs` + `operations_tests.rs` | — | operation wait/cancel, `OperationClass::Other` fallback, repoll backoff | commits `883620c`, `77b430e` |
| created | `crates/shared/incus-client/src/events.rs` + `events_tests.rs` | — | `/1.0/events` WebSocket subscription, `EventFilter`, `EventStream` | commit `998cbfd`, `77b430e` |
| created | `crates/shared/incus-client/src/resources.rs` + `resources_tests.rs` | — | resource module re-exports (+ structural test sibling) | commits `998cbfd`, `acc381b` |
| created | `crates/shared/incus-client/src/resources/instances.rs` + `instances_tests.rs` | — | instance CRUD/lifecycle/snapshots | commit `3f6c70e` |
| created | `crates/shared/incus-client/src/resources/{images,networks,projects,storage}.rs` + `_tests.rs` | — | remaining resource CRUD, sync/async corrections, ETag parity | commits `0138275`, `32ffc9f`, `1d3b5ac`, `8ca3f89` |
| created | `crates/shared/incus-client/README.md` | — | comprehensive crate reference doc | commit `e9b4ffe`, revised `a5ec780` |
| created | `crates/shared/incus-client/examples/basic.rs` | — | compiled quick-start example, `#![cfg(unix)]`-gated | commits `e9b4ffe`, `d0dbae5` |
| created | `.gitleaksignore` | — | fingerprint suppression for a plan-doc false positive | commit `913b9ee` |
| modified | `CHANGELOG.md` | — | `[Unreleased]` entry for `incus-client`; resolved 2 merge conflicts against parallel PRs | commits throughout, merges `13950b0`/`a8b7eae` |
| modified | `Cargo.toml`, `Cargo.lock` | — | workspace member registration, dependency lock | commit `0589ad7` |
| modified | `xtask/src/test_siblings.rs` | — | classified `incus-client` (`CHECKED_SRC_ROOTS`) and `unifi` (`UNCHECKED_SRC_ROOTS`); resolved 2 merge conflicts against a concurrent `unifi`/`gotify` integrations effort | commits `acc381b`, merges `13950b0`/`a8b7eae` |

## Beads Activity

- `rmcp-template-hwu2` (epic) — created at plan time, closed this session. Close reason: "All 30 child beads closed. Crate fully implemented, researched against real Incus daemon source for sync/async correctness, 2 full rounds of multi-agent review plus a targeted P3/P4 sweep with every finding addressed (fixed or a documented evaluate-and-decide), documented (comprehensive README + compiled example), and CI-green on PR #165."
- `rmcp-template-hwu2.1` through `.34` (34 child beads total, confirmed via `bd list --parent rmcp-template-hwu2 --status closed`) — created across scaffolding and two review rounds, all closed this session. Priorities spanned P1 (3 beads: clippy gate, CRLF injection, storage recursion wire format) through P4 (13 beads, the final all-issues sweep). Representative closures: `.8` CRLF/HTTP-request-splitting fix, `.30` sync-vs-async daemon-source verification (the largest-scope finding, expanded well beyond its original "verify 3 create endpoints" framing), `.31` `WithEtag` fabrication-prevention, `.34` path-char/wait-interval minor hardening.
- `rmcp-template-21b7` (follow-up epic) — created during planning, **left open** intentionally. Scope: remote mTLS/TOFU/trust-token transport + certificates CRUD, deferred pending a real remote consumer. Not touched this session beyond initial creation (predates this session's tracked window but is directly referenced by the closed epic's close reason and the crate's README "Known Limitations" section).

## Repository Maintenance

- **Plans**: `docs/superpowers/plans/2026-07-17-incus-client-crate.md` is now fully implemented and shipped (PR #165 merged). Left in place, not moved — this repo's `docs/superpowers/plans/` directory has no `complete/` subdirectory or move-on-completion convention (checked: `docs/plans/complete` does not exist, and other clearly-shipped plans like `2026-07-15-rmcp-traces.md` remain in the same flat directory). Flagged in Open Questions rather than inventing a new convention unilaterally.
- **Beads**: epic `rmcp-template-hwu2` and all 34 child beads confirmed closed (see Beads Activity). Follow-up epic `rmcp-template-21b7` confirmed still open and correctly left that way (explicitly "not started, not scheduled" by design). No further bead action needed.
- **Worktrees/branches**: `git worktree list` shows this session's worktree (`/home/jmagar/workspace/soma/.claude/worktrees/incus-api-crate-d65a18`, branch `claude/incus-api-crate-d65a18`) at `a8b7eae`, confirmed via `git merge-base --is-ancestor a8b7eae origin/main` to be a full ancestor of `main` post-merge — safe to remove. **Not removed this session**: this session is actively running inside that worktree, so self-deletion was not attempted; flagged for the user or a follow-up session to run `git worktree remove` and `git branch -d claude/incus-api-crate-d65a18` (locally) once this session ends. No other worktree or branch was touched or evaluated as a cleanup candidate — `oauth-provider-support-f427c9`, `traces-crate-followups-01d8e6`, and `cortex-auto-update-crate-review` are unrelated active work, left alone.
- **Stale docs**: `CHANGELOG.md` and `crates/shared/incus-client/README.md` were kept current throughout the session as the implementation evolved (most notably the sync/async correction in the final sweep). No other doc was found stale as a result of this session's changes.
- No maintenance action was skipped or blocked without a documented reason above.

## Tools and Skills Used

- **Shell commands (`Bash`)**: `cargo build`/`test`/`clippy`/`fmt` (workspace and per-crate, including cross-target checks for `x86_64-pc-windows-gnu`/`-msvc`), `git` (merge/conflict-resolution/log/reflog/worktree/rev-parse), `gh` (`pr view`, `pr checks`, `pr edit`, `run list`, `run view`, `api` for raw check-run/job/log inspection), `bd` (`show`, `list`, `search`) for beads verification, `mise install gitleaks@8.24.3` to reproduce a CI-pinned secret-scan failure exactly. No issues beyond routine build-cache warmup delays and one instance of a stale glob picking an outdated test binary (self-corrected by rebuilding).
- **File tools (`Read`/`Edit`/`Write`)**: used throughout for crate source, README, CHANGELOG, and `xtask/src/test_siblings.rs` conflict resolution.
- **Web/GitHub research**: `gh api repos/lxc/incus/contents/...` and `gh api repos/lxc/incus/git/trees/main?recursive=true` to fetch and grep the real upstream Go daemon source for authoritative sync/async behavior — a materially stronger evidence source than the original plan's "verify against a live daemon" ask (no live Incus daemon was available).
- **Subagents**: 8-agent review round 1, 6-agent `pr-review-toolkit` review round 2 (both via the `Agent` tool). One implementation subagent hit an "organization has disabled Claude subscription access" API error mid-task and was not retried; the remaining work was completed directly in the main session instead.
- **`AskUserQuestion`**: used twice — once for crate placement/scope at planning time, once mid-session to resolve whether to keep chasing a fast-moving `main` for mergeability (user chose "wait and check later," later superseded by explicit "merge it").
- **`ScheduleWakeup`**: one attempt failed (missing required `prompt` field outside `/loop` context) and was abandoned in favor of `Bash`-with-`run_in_background` polling loops, which worked correctly for watching asynchronous CI runs.
- No MCP servers, browser tools, or external CLIs beyond `gh`/`bd`/`cargo`/`git`/`mise` were used this session.

## Commands Executed

| command | result |
|---|---|
| `cargo test -p incus-client --all-features` (final run) | 92 passed, 0 failed |
| `cargo test -p incus-client` (default features) | 86 passed, 0 failed |
| `cargo clippy -p incus-client --all-targets --all-features -- -D warnings` | clean |
| `cargo clippy --workspace -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean (post-merge) |
| `./target/debug/xtask check-test-siblings` | `all source files have a _tests.rs sibling (17 tree(s) checked)` |
| `./target/debug/xtask check-version-sync` | `OK: soma version-bearing files are in sync at 0.4.7.` |
| `cargo check`/`clippy` vs `x86_64-pc-windows-gnu`/`-msvc` | clean, both feature variants |
| `cargo tree -p incus-client --all-features -e normal` | confirmed no `reqwest`/`rustls` in dependency tree |
| `gh pr checks 165` (repeated across session) | red → green over the session |
| `git merge-base --is-ancestor a8b7eae origin/main` | `YES` (branch fully merged) |

## Errors Encountered

- Implementation subagent hit "organization has disabled Claude subscription access" mid-task; not retryable, so remaining work was completed directly in the main session.
- `RequestSpec` arity mismatch after a subagent crash left a test call site on an old 7-arg `execute_capped` signature after a `timeout` param addition made it 8, tripping clippy's `too_many_arguments` — fixed by introducing a `RequestSpec<'a>` struct.
- Crate failed to build on Windows CI (unconditional `tokio::net::UnixStream`/`FileTypeExt` usage) — fixed via `#![cfg(unix)]` on `lib.rs`; the same gap resurfaced in the new `examples/basic.rs` and was fixed the same way.
- Gitleaks flagged an unrelated pre-existing path string in the plan doc as a `generic-api-key` under the CI-pinned gitleaks 8.24.3, where the repo's `.gitleaks.toml` allowlist doesn't suppress extended-ruleset findings — fixed via a fingerprint-scoped `.gitleaksignore`, verified locally against the exact pinned binary.
- Three separate merge conflicts against `main` (`CHANGELOG.md` ×2, `xtask/src/test_siblings.rs` ×2) as `main` moved out from under the branch repeatedly (concurrent unrelated work); all resolved by keeping both sides' independent additions.
- The second `test_siblings.rs` merge caused a real, previously-passing CI test to fail (`every_workspace_member_src_root_is_classified`, both `incus-client` and `unifi` unclassified) — fixed by adding both to the appropriate classification list and verifying locally before push.
- One `Build Windows` CI failure (`doctor_cli.rs` test) traced to code already fixed on `main` in the interim; resolved automatically once that merge was picked up, no crate-side fix needed.
- One MSRV CI job showed `cancelled` (not a real MSRV violation) — diagnosed as a self-hosted-runner interruption during the "Post Cache Cargo" step, not a code issue; re-run via `gh run rerun --failed`.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| `incus-client` crate | did not exist | new standalone workspace crate, unix-socket-only, `events` feature optional |
| `create_network`/`create_project`/`create_storage_pool`/`*_update`/`*_delete` return type | assumed always-async (`Result<Operation>`) | corrected to `Result<()>` (synchronous), verified against real daemon source |
| `create_storage_volume` return type | assumed always-async | `Result<Option<Operation>>` — sync when no `source.name`, async otherwise |
| ETag/`If-Match` support | instances only | all resource types (instances, images, networks, storage pools/volumes, projects), including `*_guarded` convenience methods |
| `Error` enum | no 404 distinction, no WebSocket-protocol distinction | `Error::NotFound`, `Error::WebSocketProtocol`, `Error::Timeout{after, request_fully_sent}` |
| `xtask check-test-siblings` | `incus-client`/`unifi` unclassified (would silently skip them) | both explicitly classified, gate passing |
| PR #165 | open, iterating through review | merged into `main` at `b8e199e` |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test -p incus-client --all-features` | all pass | 92 passed, 0 failed | pass |
| `cargo clippy -p incus-client --all-targets --all-features -- -D warnings` | no warnings | clean | pass |
| `cargo fmt --all -- --check` (post-merge) | no diff | clean | pass |
| `./target/debug/xtask check-test-siblings` | all trees classified, no missing siblings | 17 checked, 16 unchecked-by-design, 0 missing | pass |
| `./target/debug/xtask check-version-sync` | versions in sync | `OK: soma version-bearing files are in sync at 0.4.7.` | pass |
| `gh pr view 165 --json statusCheckRollup` (final) | all required checks green | `CI Gate`, `MSRV Gate`, `Build Windows`, `Build Linux`, `Format`, `Test`, `Clippy`, `Secret Scan`, etc. all `SUCCESS` | pass |
| `git merge-base --is-ancestor a8b7eae origin/main` | branch merged into main | `YES` | pass |

## Risks and Rollback

- The crate is contract-tested (synthetic `wiremock`/in-process fake Unix-socket listeners) but not integration-tested against a real Incus daemon — documented explicitly in the epic close reason and the crate's README "Known Limitations." Any real-daemon behavioral surprise (timing, concurrency, permission-model edge cases) would surface only at first real use.
- Sync/async correctness for the mutating endpoints was verified by reading `lxc/incus`'s `main`-branch Go source via `gh api`, not by exercising a live daemon — a strong but still static-analysis-based evidence source; the source could in principle have already diverged from the deployed daemon version a downstream consumer targets.
- Rollback path: PR #165 is a single self-contained merge commit (`b8e199e`) adding one new workspace crate plus small, additive-only changes to `CHANGELOG.md`, `Cargo.toml/.lock`, and `xtask/src/test_siblings.rs`. A revert of that merge commit on `main` would cleanly remove the feature with no other crate depending on `incus-client` yet.

## Decisions Not Taken

- Did not implement remote mTLS/TOFU transport or certificates CRUD in this epic — deliberately deferred to backlog epic `rmcp-template-21b7` per the original engineering review's risk assessment (2 critical security gaps in a hand-rolled TLS trust model, no concrete consumer).
- Did not implement OData-style `?filter=` query support — named in early research but explicitly out of scope for this epic; not silently dropped, tracked as a known gap.
- Did not attempt to rebase this branch onto `main` instead of merging — three sequential merges were simpler to reason about and lower-risk than repeated rebases given how frequently `main` was moving during the session.
- Did not delete the now-merged `claude/incus-api-crate-d65a18` worktree/branch in this session — see Repository Maintenance.

## References

- PR #165: https://github.com/jmagar/soma/pull/165
- Merge commit on `main`: `b8e199eec87f4ef113e9ee02f7aaf15ae744b1df`
- Plan: `docs/superpowers/plans/2026-07-17-incus-client-crate.md`
- Follow-up epic: `rmcp-template-21b7`
- Upstream reference source read during the final review sweep: `lxc/incus` `main` branch, `cmd/incusd/networks.go`, `cmd/incusd/api_project.go`, `cmd/incusd/storage_pools.go`, `cmd/incusd/storage_volumes.go`

## Open Questions

- Should `docs/superpowers/plans/` adopt a `complete/` subdirectory convention for shipped plans? None currently exists despite several plans in that directory (including this one) being fully implemented and merged. Left as-is rather than introducing a new convention unilaterally.
- Should the merged `claude/incus-api-crate-d65a18` worktree and branch be cleaned up now that PR #165 is merged? Deferred to the user/a follow-up session since this session was running inside the worktree.

## Next Steps

- Optional cleanup (not started): remove the merged worktree (`git worktree remove /home/jmagar/workspace/soma/.claude/worktrees/incus-api-crate-d65a18` from the main checkout) and delete the local/remote `claude/incus-api-crate-d65a18` branch once no longer needed.
- No blocked or unfinished implementation work remains for `rmcp-template-hwu2` — it is fully closed and shipped.
- When a real remote/mTLS Incus consumer emerges, pick up `rmcp-template-21b7` rather than re-deriving its security requirements from scratch — it already carries the full hardening checklist from the original review (TLS 1.3 enforcement, `verify_tls12/13_signature`, gated `AcceptAny`, secret redaction, key-strength minimums).
