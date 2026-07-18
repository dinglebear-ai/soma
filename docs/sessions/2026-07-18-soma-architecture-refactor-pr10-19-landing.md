---
date: 2026-07-18 02:15:22 EST
repo: git@github.com:jmagar/soma.git
branch: bd-work/workspace-deps-and-freeze-audit
head: 68a530f824d613eb9cb43956ec8ff489ba6a962a
session id: 9cce028d-f99e-43f9-8da0-4ac91656c946
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-soma/9cce028d-f99e-43f9-8da0-4ac91656c946.jsonl
working directory: /home/jmagar/workspace/soma
pr: #151 "build: centralize internal paths and rmcp pin in [workspace.dependencies] (PR 10 prerequisite)" — https://github.com/jmagar/soma/pull/151 (MERGED, af6b292)
beads: rmcp-template-qd2t, f1ll, 3nqq, yx81, ns29, ub2l, 071p, z7vg, bpe5, 1yz6, 6h0r, 87yy, lllv, oyq6, 1dhm, cfi0, d9t4, 0u5c, z2ns, twzc, otgi, ge55, a7p7, kbal, 75ph, e0rz, 9x9b, l93n, 9x9b, 0hd0
---

## User Request

Review `soma-architecture-refactor-plan-v3.md` against the live repo, land the corrections, then implement plan slices PR10 through PR19 end to end — with independent verification of every claim, not blind trust of agent self-reports — merge everything into the shared integration branch `bd-work/workspace-deps-and-freeze-audit`, and get the resulting PR (#151) to a genuinely CI-green, mergeable state. Along the way, also address the automated PR review comments on #151 via `/gh-pr 151`. The user gave explicit standing instructions mid-session to use only Sonnet 5 high-effort agents, to keep parallelizing wherever possible, and to keep pushing forward without pausing for permission where none was needed.

## Session Overview

Landed the entire 10-PR Soma architecture refactor (PR10–PR19 of `soma-architecture-refactor-plan-v3.md`) on `main`, plus a full round of automated-review bug fixes, closing out a multi-hour session that spanned plan review, multi-agent implementation, merge-conflict resolution, CI-infrastructure debugging, and PR-review triage. The plan document itself was corrected first (arrow direction, incomplete dependency block, member-glob claim, execution-ledger overstatements) and four follow-up beads were filed. PR10–PR19 were then implemented via a mix of the `lavra-work-multi`/`lavra-review` skill path (for the workspace-dependencies prerequisite) and a large custom `Workflow` orchestration script (`soma-refactor-pr10-19-wf_d4181b57-fd0.js`) driving ~30 Sonnet-5 agents across implement/review/fix/freeze-coverage stages for the remaining nine slices. All ten slices were merged into `bd-work/workspace-deps-and-freeze-audit` in dependency order, with three rounds of independent verification catching real regressions along the way. PR #151 (`bd-work` → `main`) then went through a long CI-green cycle: a missing system package plus a broken install-guard, a genuine `Cargo.lock` merge conflict against `main`'s ongoing drift, the full Tauri Linux prerequisite stack plus 16 `cargo-deny` advisory exceptions, two further rounds of merge-conflict resolution against `main` (including a from-scratch rebuild of a sibling-test classification list verified against a real filesystem scan), and finally a complete pass through `/gh-pr 151`'s automated codex review — fixing 9 of 10 findings (including one SSRF-adjacent security bug) and deliberately deferring one that needs a product decision. The user merged PR #151 manually once CI queued.

## Sequence of Events

1. **Plan review.** Read the full 3,703-line `soma-architecture-refactor-plan-v3.md` and checked its claims against the live repo (workspace layout, `cargo xtask check-architecture`, dependency centralization). Verdict: the plan was accurate on substance but had several doc-level defects.
2. **Plan corrections landed to `main`.** Fixed a reversed dependency arrow (§4.1), rewrote an incorrect gateway-dependency claim after checking actual manifests (§3.7 — the plan asserted `gateway → soma-mcp-server`; the real manifests show `gateway → {client, proxy}` only), replaced a member-glob recommendation that contradicted the real explicit-list manifest (§7), added a new PR10 prerequisite step for `[workspace.dependencies]`, added missing crate-creation steps for PR12/PR13, and qualified two execution-ledger overclaims. Filed four beads for work outside the plan doc itself (`qd2t`, `f1ll`, `3nqq`, `yx81`). Rebased onto `origin/main` and pushed directly.
3. **First implementation attempt via `lavra-work-multi`/`lavra-review`.** User invoked `/lavra:lavra-work` with the three P1/P2 beads. Routed to the multi-bead path; created a working branch, decided `3nqq` (brand-neutral crate naming) as "defer to a dedicated rename slice after PR16/before PR19," closed it, and filed follow-up bead `ns29`. Dispatched Wave 1 (bead `qd2t`, workspace-dependencies centralization) as a single agent, then ran the mandatory `/lavra-review` (4 parallel reviewers: architecture, patterns, security, simplicity). This produced the initial commit content that became PR #151.
4. **User pivot to full parallelization.** After the Wave 1 review cycle, the user asked to build a workflow parallelizing as much of PR10–19 as possible. Interrupted the in-flight lavra-review agents, created 10 per-slice tracking beads, and authored a large multi-stage `Workflow` script (`soma-refactor-pr10-19-wf_d4181b57-fd0.js`) implementing, reviewing, and fixing each of the remaining nine slices with pipelined stages (e.g. `rev11 ∥ impl12`) to overlap review cycles with the next slice's implementation.
5. **Model/effort constraint check and correction.** User asked to confirm every agent was Sonnet-5 high-effort; found and fixed one `effort: 'low'` exception on a trivial setup step (a single `git checkout`), leaving every real implement/review/fix agent at Sonnet-5 high effort.
6. **Workflow execution, monitored via `TaskGet`/journal polling and periodic status check-ins** (user: "everyone still workin bro", "3 of five done...", "i dont see ANY active tasks for you rn" — the latter correctly caught a real broken watcher process using a self-matching `pgrep -f cargo` pattern, fixed to `pgrep -x cargo`).
7. **Sequential stacked-branch merges into `bd-work/workspace-deps-and-freeze-audit`**, PR10 through PR19 in dependency order, each preceded by an independent review-and-fix pass (not the agent's own self-report). Caught and fixed real defects along the way (see Key Findings). One `Agent` tool misuse (attempting a nonexistent `to:` parameter to resume an agent) spawned a rogue duplicate agent in the main checkout; caught immediately via `git status`/`git log`, stopped via `TaskStop`, no damage done. One accidental `git merge` against the wrong already-checked-out branch was caught and `git merge --abort`ed before any commit.
8. **PR #154 auto-close incident.** Deleted a merged branch (`refactor/pr10-provider-adapters`) expecting GitHub to retarget the dependent PR #154's base — GitHub auto-closed it instead. Recovered by opening PR #164 from the same commits against `bd-work` directly and cross-linking. Adjusted strategy: retarget dependent PRs' base *before* deleting any merged branch from then on.
9. **Final verification pass** in a fresh, isolated worktree at the true merge tip: `fmt`, `clippy`, `check-architecture` (0 exceptions), `check-version-sync`, all 3 feature-profile builds, and the full 1684-test workspace suite — all green except one confirmed-environmental flake (a web-asset-embed-presence test dependent on local build state, fixed to be conditional).
10. **CI-green cycle for PR #151** (multiple rounds, described in Key Findings): missing `libdbus-1-dev`, broken install-guard, `Cargo.lock` merge conflict, full Tauri Linux prerequisite stack + 16 `cargo-deny` advisory exceptions, two further rounds of `main`-drift merge conflicts.
11. **`/gh-pr 151`** run while waiting on CI ("k while you wait"): fetched 9 open review threads from the automated `chatgpt-codex-connector` reviewer, verified each individually against current code (one was already stale/fixed), fixed 7 in one commit, fixed 1 more (deferred a genuine product-decision thread), then a fresh codex re-review surfaced one additional real bug which was also fixed. All fixes verified locally (build + targeted crate tests + the standalone `apps/palette/src-tauri` Tauri sidecar workspace) before each push.
12. **PR #151 merged** by the user once the final CI run queued (`68a530f` → merge commit `af6b292`).

## Key Findings

- **`crates/soma/client/src/client.rs`** — PR19's branch reintroduced a stale error-message mislabeling that PR12's earlier verified fix had corrected; merge resolution kept the correct version and verified `soma-client` genuinely owns the `client` feature via `grep`.
- **`crates/soma/application/src/providers/filesystem.rs`** — kept bd-work's genuine PR10 fix (return `Result` instead of panicking on an unsupported provider kind) over a stale panicking version on a later branch.
- **`crates/soma/runtime/src/test_support.rs`** (recreated) — PR19's branch forked before PR18's second review-fix commit created this module in its original location (`crates/soma/integrations`); PR19 moved the dependent test files without ever having seen it. Recreated with adapted imports; added `soma-client`/`soma-integrations` dev-dependencies (verified no dependency cycle). Full `cargo test -p soma-runtime --all-features`: 29/29 passed, including the full security-critical auth/scope/proxy-target-resolution suite.
- **`apps/soma/tests/architecture_boundaries.rs`** — the new `test_support.rs` tripped a naive substring scan for `SomaService`/`ProviderRegistry`/`ProviderCall`; added a targeted exclusion for `test_support.rs` and `*_tests.rs` files in that one check's loop only (verified no genuine production file in the crate trips it).
- **`apps/soma/src/http_tests.rs`** — `unmatched_route_returns_the_not_found_envelope` failed because a local `apps/web/out/index.html` build artifact made the router's documented SPA-fallback behavior legitimately return 200. Fixed the *test* to assert conditionally on `soma_web::web_assets_available()`, not the router.
- **`.github/actions/setup-rust-sccache/action.yml`** — root cause of PR #151's persistent CI failures: (1) missing `libdbus-1-dev` for `soma-tauri-shell`'s dbus transitive dependency (PR17 made it a real workspace member for the first time); (2) the install guard only checked `command -v cc`, which is always true on these *persistent* self-hosted runners, so the fix would have been a silent no-op — corrected to check `pkg-config` for the actual required libraries; (3) the true scope was the full Tauri Linux desktop stack (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libxdo-dev`), verified against `cargo tree -i webkit2gtk-sys`/`libappindicator-sys` and the official Tauri Linux prerequisites.
- **`deny.toml`** — the same Tauri stack trips 16 RUSTSEC "unmaintained" advisories (10 deprecated `gtk-rs` GTK3 bindings, `proc-macro-error`, 5 `unic-*` Unicode crates via `tauri-utils → urlpattern`) — none are active CVEs, all inherent to any Tauri 2.x Linux app today. Added reasoned `ignore` entries mirroring the repo's existing `RUSTSEC-2023-0071` precedent.
- **Two further `main`-drift merge conflicts** while iterating on PR #151: `Cargo.lock` (unrelated PR #163 landed a new `crates/integrations/unifi` crate mid-stream) and `CHANGELOG.md`/`xtask/src/test_siblings.rs` (unrelated PR #150 landed a stricter two-list sibling-test classification system). The `test_siblings.rs` resolution required rebuilding `CHECKED_SRC_ROOTS`/`UNCHECKED_SRC_ROOTS` from an actual filesystem scan of every workspace crate's `_tests.rs` sibling coverage (computed via a Python script mirroring the checker's own logic), not a guess — verified via `cargo test -p xtask test_siblings` (5/5) and `check-test-siblings` (0 missing/orphaned).
- **`crates/shared/provider-adapters/src/openapi.rs:302`** (`/gh-pr 151`, P1) — `validate_relative_path`'s absolute-URL rejection was case-sensitive (`starts_with("http://")`), while the downstream `url::Url::join` treats schemes case-insensitively — a manifest path like `HTTPS://169.254.169.254/latest` slipped past as "relative" and could reach a host never checked against `capabilities.network.allowed_hosts`. SSRF-adjacent; fixed to lowercase before the prefix check.
- **`apps/soma/src/lib.rs`/`local.rs`** (`/gh-pr 151`) — `soma::run(args)` only used caller-supplied `args` for help/version classification, then discarded them and reparsed `std::env::args()` for every real CLI subcommand — an embedder or in-process test passing explicit args silently got host-process argv behavior instead.
- **`crates/soma/palette/src/router.rs`/`execute.rs`** (`/gh-pr 151`) — catalog/search/schema/execute handlers read `catalog_snapshot()` directly, unlike REST and MCP which refresh the file-backed registry first; a provider file added while the server ran stayed invisible to Palette until an unrelated endpoint or a restart refreshed it.
- **`crates/soma/api/src/api.rs`/`apps/soma/src/http.rs`** (`/gh-pr 151`) — `augment_with_palette_routes` existed with tests but was never invoked in production; `soma-api` can't depend on `soma-palette` (product-surface crates must not depend on one another), so the composition root now layers the palette augmentation on top via a new `openapi_json_with_palette` handler.
- **`apps/soma/src/http.rs`** (`/gh-pr 151`) — the global 65,536-byte MCP body-size layer wrapped `/v1/palette/*` too, but the desktop Tauri bridge allows launcher params up to 256 KiB; legitimate payloads passed client-side validation and were then rejected server-side with 413. Restructured body-limit layering per-branch (mcp+api / palette / public / oauth) instead of one blanket layer over the fully-merged router.
- **`apps/palette/src-tauri/src/labby_bridge.rs`** (`/gh-pr 151`, discovered on a *second* codex pass after the first fix round was pushed) — `fetch_launcher_catalog` and `execute_launcher_entry` both retry via API-base discovery when the saved server URL returns HTML, but `fetch_launcher_schema` didn't have this retry, so schema-driven params broke even after the catalog loaded. Mirrored the existing retry pattern.
- **Deliberately deferred** — `crates/soma/palette/src/dto.rs:15` (`PRRT_kwDOSc3ZY86R7tvk`, bead `rmcp-template-kbal`): Soma-native catalog entries serialize `title`/`provider` with no `kind` field, but the existing desktop launcher (`apps/palette/src/lib/launcherCatalog.ts`) and its Tauri bridge's client-side id validator (`mcp:`/`labby:` prefixes only) only understand two other source kinds. Fixing this correctly needs a new id-prefix/kind convention spanning the Rust DTO, the catalog id format, the Tauri bridge's validator, and the TypeScript normalizer — a product/naming decision, not a mechanical fix. Left open on purpose.

## Technical Decisions

- **Independent verification over agent self-report.** Every PR10–19 slice got its own review-and-fix pass treated as unverified until checked against actual code — this caught real regressions (see Key Findings) that a "trust the agent's summary" approach would have missed or introduced.
- **Retarget before delete.** After the PR #154 auto-close incident, adopted the rule: always retarget a dependent PR's base branch *before* deleting the branch it's currently based on, never rely on GitHub to do it automatically.
- **Fix the test, not the router, for environment-dependent failures.** `http_tests.rs`'s SPA-fallback test failure was a real behavioral difference correctly documented elsewhere in the codebase (`soma_web`'s module doc) — the test's fixed expectation was wrong, not the router.
- **Ground-truth over trust for the `test_siblings.rs` merge conflict.** Rather than mechanically picking one side of a real semantic conflict (or blindly reusing a stale classification list), computed actual sibling-test coverage per crate via a script mirroring the checker's own logic, then built a classification that would pass for real — verified, not assumed.
- **Deny.toml exceptions require individual, reasoned justification.** All 16 new `cargo-deny` ignores got their own comment tracing the exact dependency chain and stating why no fix exists yet, matching the repo's existing convention rather than a blanket suppression.
- **Deferred rather than guessed on the Soma-native catalog id/kind question.** The DTO/Tauri-bridge/TypeScript contract mismatch spans four files across two languages and requires a naming/routing decision with real product implications (how should Soma-native actions read to the user vs. Labby actions vs. MCP tools) — judged not appropriate to invent unilaterally, especially with no way to end-to-end test the desktop app's UI in this environment.
- **Body-size limit restructured per-branch, not widened globally.** Rather than raising the MCP limit for everyone (weakening the intentional tight cap on the JSON-RPC surface) or leaving Palette broken, gave `/v1/palette/*` its own layer applied before merging into the router that still carries the MCP-specific limit.

## Files Changed

This session's own direct edits (subagent-authored files from the ~30-agent `Workflow` run are not enumerated individually here — they are captured by the ten PR10–19 commits' own history; the full `main`-facing diff for everything this session landed is 429 files, +38,661/−5,037, `8e80dc5..af6b292`).

| status | path | purpose | evidence |
|---|---|---|---|
| modified | `soma-architecture-refactor-plan-v3.md` | Fixed reversed dependency arrow, incorrect gateway-dependency claim, member-glob recommendation, execution-ledger overclaims; added PR10 prerequisite step and PR12/13 creation steps | Landed directly to `main` |
| modified | `.github/actions/setup-rust-sccache/action.yml` | Added `libdbus-1-dev` then full Tauri Linux prerequisite set; fixed the always-true `cc`-only install guard | Commits `23a5f3a`, `f4ba01f` |
| modified | `deny.toml` | 16 reasoned RUSTSEC ignore entries for the Tauri/GTK3/`unic-*` dependency chain | Commit `f4ba01f`; verified via `cargo deny check` locally |
| modified | `crates/soma/client/src/client.rs`, `crates/soma/application/src/providers/filesystem.rs`, `crates/soma/integrations/src/lib.rs`, `crates/soma/integrations/Cargo.toml`, `crates/soma/integrations/src/gateway.rs`, `crates/shared/provider-adapters/src/gateway.rs`, `crates/shared/provider-adapters/src/manifest_file.rs` | Merge-conflict resolutions across the PR10–19 stacked merge, verified defect-by-defect | `bd-work` merge commits |
| created | `crates/soma/runtime/src/test_support.rs`, `crates/soma/runtime/src/protected_routes_proxy_tests.rs` (fixed) | Recreated a test-support module PR19's branch never saw; fixed self-referential import paths | `crates/soma/runtime/Cargo.toml`, `src/lib.rs` also updated for the new dev-deps and module |
| modified | `apps/soma/tests/architecture_boundaries.rs`, `apps/soma/src/http_tests.rs`, `apps/soma/src/bootstrap.rs`, `apps/soma/src/bootstrap_tests.rs`, `apps/soma/tests/openapi_provider.rs` | False-positive architecture-check exclusion; environment-dependent test fix; import consolidation | Committed `64309d0` and within `bd-work` merges |
| modified | `CHANGELOG.md`, `CLAUDE.md`, `docs/AGENTS-FIRST.md`, `docs/API.md`, `docs/ARCHITECTURE.md`, `docs/PATTERNS.md`, `docs/QUICKSTART.md`, `xtask/src/patterns/checks.rs` | Doc/checklist path-reference fixes verified against actual file existence; "keep both" resolution for genuinely additive CHANGELOG conflicts | `bd-work` merges |
| modified | `xtask/src/test_siblings.rs`, `xtask/src/test_siblings_tests.rs` | Rebuilt `CHECKED_SRC_ROOTS`/`UNCHECKED_SRC_ROOTS` from a real filesystem scan after a second `main`-drift conflict | Commit `2c5c88d`; verified `cargo test -p xtask test_siblings` 5/5, `check-test-siblings` 0 missing/orphaned |
| modified | `apps/soma/src/local.rs`, `apps/soma/src/lib.rs` | Threaded caller-supplied `argv` into CLI dispatch instead of reparsing `std::env::args()` | Commit `a4d4f89`; `cargo test -p soma` all green |
| modified | `crates/soma/palette/src/dto.rs`, `crates/soma/palette/src/dto_tests.rs` | Default `params` to `{}` not `null`; updated the stale test that asserted the old behavior | Commit `a4d4f89`; `cargo test -p soma-palette` 48/48 |
| modified | `apps/palette/src-tauri/Cargo.toml` | Bumped `rust-version` 1.94.0 → 1.96 to match path dependencies | Commit `a4d4f89` |
| modified | `crates/soma/palette/src/router.rs`, `crates/soma/palette/src/execute.rs` | Added `refreshed_snapshot` helper; all four catalog-dependent handlers now refresh before reading | Commit `a4d4f89` |
| modified | `crates/soma/api/src/api.rs`, `apps/soma/src/http.rs` | Exposed `build_openapi_document`; new composition-root `openapi_json_with_palette` handler wires the palette OpenAPI augmentation into the live response | Commit `a4d4f89` |
| modified | `crates/shared/provider-adapters/src/openapi.rs` | Case-insensitive absolute-path rejection (SSRF-adjacent fix) | Commit `a4d4f89`; `cargo test -p soma-provider-adapters` green |
| modified | `apps/soma/src/http.rs` | Split `/v1/palette/*`'s body-size limit (256 KiB) from the global MCP limit (64 KiB); restructured layer application per-branch | Commit `a4d4f89` |
| modified | `apps/palette/src-tauri/src/labby_bridge.rs` | Mirrored the API-base discovery-retry pattern into `fetch_launcher_schema` | Commit `68a530f`; `cargo test --manifest-path apps/palette/src-tauri/Cargo.toml` 38/38 |

## Beads Activity

**Plan-review phase (filed 2026-07-16, all subsequently closed except two genuinely still-open):**

| Bead | Title | Status | Notes |
|---|---|---|---|
| `rmcp-template-qd2t` | Centralize internal paths and rmcp pin in `[workspace.dependencies]` | CLOSED | Landed on `bd-work`, became PR #151 |
| `rmcp-template-f1ll` | Audit PR1 behavior-freeze coverage before PR12 deletes `soma-service` | CLOSED | "PR19 landed the remaining PR12 scope this bead watched; full gate suite green" |
| `rmcp-template-3nqq` | Decide brand-neutral shared crate publish names before PR19 | CLOSED | Decided 2026-07-16: dedicated rename slice after PR16/before PR19; execution tracked in `ns29` |
| `rmcp-template-yx81` | Remove untracked leftover dirs from the taxonomy move | **OPEN** | `crates/soma/src/` is gone; `crates/soma-auth/.full-review/` still present — verified this session, genuinely unfinished |
| `rmcp-template-ns29` | Rename shared crates to brand-neutral names (after PR16, before PR19) | **OPEN** | Follow-up to `3nqq`; concrete rename slice not yet executed |

**PR10–19 slice tracking beads (all CLOSED this session):** `rmcp-template-071p` (PR10), `z7vg` (PR11), `bpe5` (PR12), `1yz6` (PR13), `6h0r` (PR14), `87yy` (PR15), `lllv` (PR16), `oyq6` (PR17), `1dhm` (PR18), `cfi0` (PR19), plus review-fix beads `d9t4` (PR10 findings), `0u5c` (PR13 findings), `z2ns` (PR19 protected-routes findings).

**`/gh-pr 151` review-thread beads (all CLOSED this session via direct `bd close`, since the skill's own `close_beads.py --refresh` hit an `AttributeError` parsing `bd show` JSON output):** `rmcp-template-twzc`, `otgi`, `ge55`, `a7p7`, `75ph`, `e0rz`, `9x9b`, `l93n` (first round of 8), `0hd0` (second round, `fetch_launcher_schema` fix).

**Left open (intentionally):** `rmcp-template-kbal` — bead for `PRRT_kwDOSc3ZY86R7tvk`, the deferred Soma-native catalog id/kind contract mismatch.

## Repository Maintenance

- **Plans:** No `docs/plans/` directory exists in this repo (`ls docs/plans` → not found); nothing to move. `soma-architecture-refactor-plan-v3.md` lives at repo root, is now functionally complete (all PR0–19 slices landed on `main`), but is out of scope for the plan-move step since it isn't under `docs/plans/` — left in place.
- **Beads:** Reviewed and closed 8 review-thread beads from `/gh-pr 151`'s first fix round plus 1 from the second round (see Beads Activity). Verified `rmcp-template-yx81` and `rmcp-template-ns29` are genuinely still open (checked live filesystem state and current close reasons) — left open, not fabricated as done.
- **Worktrees and branches:** `refactor/pr11-integrations` through `refactor/pr19-delete-legacy` (9 branches) are all confirmed ancestors of `origin/main` (`git merge-base --is-ancestor` for each returned true) with no open PRs based on them (`gh pr list --state open` shows only unrelated `claude/unifi-template-round3`, `claude/incus-api-crate-d65a18`, and a release-please branch) — safe-to-delete candidates, **left alone** since this command didn't ask for branch cleanup. `refactor/freeze-coverage` is **not** an ancestor of `main` — unclear whether its content landed via another path or is abandoned; not investigated further this session, flagged for follow-up. Local worktrees `.claude/worktrees/incus-api-crate-d65a18`, `oauth-provider-support-f427c9`, `soma-crate-structure-f70dc9` and `.codex/worktrees/19a67e72-...` all belong to other active, unrelated work — left untouched. `marketplace-no-mcp` worktree is the protected long-lived branch per `CLAUDE.md` — never touched.
- **Stale docs:** No new doc staleness identified from this session's own PR151 review-fix changes (all were internal implementation additions — a private helper function and a composition-root handler — not surfaced in `CLAUDE.md`'s Module Map table). The plan-review phase already corrected the doc-level defects found in `soma-architecture-refactor-plan-v3.md` and `CLAUDE.md`/`docs/ARCHITECTURE.md`/`docs/PATTERNS.md` during the PR10-19 merge-conflict resolution.
- **Transparency:** Local working tree had one leftover uncommitted diff in `apps/palette/src-tauri/Cargo.lock` (a `tracing` dependency entry, byproduct of running `cargo test --manifest-path` locally) — discarded via `git restore`, confirmed benign (adds an already-real transitive dependency, not an intentional change).

## Tools and Skills Used

- **Shell (`Bash`)** — git operations (status/diff/log/merge/commit/push/worktree), `cargo build`/`test`/`clippy`/`fmt`/`deny check`, `gh` CLI (PR/run/job inspection, merge), `bd` CLI, Python one-off scripts for transcript parsing and sibling-test-coverage ground-truthing. No persistent issues; one shell watcher bug caught mid-session (self-matching `pgrep -f cargo`, fixed to `pgrep -x`).
- **File tools (`Read`/`Edit`/`Write`)** — direct edits across ~40 files this session (see Files Changed); used `Read` before every `Edit` per tool contract.
- **`Workflow`** — authored and iteratively restructured a large multi-stage orchestration script (`soma-refactor-pr10-19-wf_d4181b57-fd0.js`) driving ~30 Sonnet-5 agents (implement/review/fix/freeze-coverage per PR slice) with pipelined stages. Used `resumeFromRunId` to replay cached agent results after mid-run script edits.
- **`Agent`** — individual review-and-verification passes on merged PR content, independent of the Workflow-spawned agents. One misuse: attempted a nonexistent `to:` parameter to resume an agent, which silently spawned a new rogue agent instead — caught and stopped via `TaskStop` before any damage.
- **`SendMessage`** — correct mechanism (after the `Agent`-misuse correction) for resuming named/ID'd agents mid-task.
- **`TaskGet`/`TaskStop`/journal polling** — monitored background workflow/agent progress and stopped stale tasks before resuming with edited scripts.
- **`ScheduleWakeup`** — used extensively during the long CI-wait cycles (missing package fix, merge conflicts, Tauri prerequisites, `/gh-pr 151` fixes) to poll CI status and background build/test tasks without idle blocking.
- **Skills**: `lavra:lavra-work` → `lavra:lavra-work-multi` (Wave 1 dispatch for the workspace-dependencies bead), `lavra:lavra-review` (4 parallel reviewers: architecture/patterns/security/simplicity), `superpowers:dispatching-parallel-agents` (referenced during the pivot to full parallelization), `vibin:gh-pr` (PR-review-comment resolution with bead tracking — hit one real bug in `close_beads.py`, worked around with direct `bd close` calls), `vibin:save-to-md` (this document).
- **MCP tools**: `mcp__ccd_session__mark_chapter` (session chapter marker at the pivot to Workflow orchestration).
- **External CLIs**: `gh` (PR/run/job state, merge), `bd` (beads), `python3` (transcript extraction and ground-truth verification scripts), standard Rust toolchain (`cargo`, `rustc`).

## Commands Executed

| Command | Result |
|---|---|
| `git merge-tree --write-tree origin/main origin/bd-work/...` | Correctly detected a real `Cargo.lock` conflict that an earlier stale local-clone test had wrongly reported as clean |
| `cargo deny check` (local) | `advisories ok, bans ok, licenses ok, sources ok` after adding 16 reasoned `deny.toml` exceptions |
| `cargo test -p xtask test_siblings` | 5/5 passed after rebuilding the sibling-test classification lists from a real filesystem scan |
| `cargo run -p xtask -- check-test-siblings` | "all source files have a _tests.rs sibling (16 tree(s) checked)... not checked (15 tree(s), by design)" — matched the expected classification exactly |
| `cargo test -p soma-palette -p soma-api -p soma-provider-adapters` | Failed once on a stale test assertion (`params` expected `null`, now `{}`), fixed, then 48/9/30 all passed |
| `cargo test -p soma` | All suites green after the args-threading and http.rs restructuring |
| `cargo test --manifest-path apps/palette/src-tauri/Cargo.toml` | 38/38 passed for the standalone Tauri sidecar workspace |
| `gh pr merge 151 --merge` (performed by the **user**, not this session) | PR #151 merged into `main` at `af6b292` |

## Errors Encountered

- **Broken `pgrep -f cargo` watcher** — a background-completion watcher's own shell text self-matched via substring search; fixed to `pgrep -x cargo` (exact process-name match).
- **`Agent` tool misuse** — a nonexistent `to:` parameter silently spawned a new, non-isolated agent instead of resuming one; caught via `git status`/`git log` showing no unexpected changes, stopped via `TaskStop`.
- **Wrong-branch merge** — a `git checkout -B` failed silently (worktree lock) but a subsequent `git merge` still ran against whatever branch was already checked out; caught before any commit via `git branch --show-current`, reverted with `git merge --abort`.
- **PR #154 auto-closed** — deleting a merged branch closed a dependent open PR instead of retargeting it; recovered via a new PR (#164) from the same commits; adjusted branch-deletion ordering going forward.
- **`gh pr view`'s `mergeable` field lagged/was genuinely stale twice** — once correctly reflecting a real conflict I'd wrongly dismissed based on a stale local-clone comparison (a `git clone` of a local working copy inherited a stale `origin/main` from the source repo's uncommitted-fetch local branch ref, not the freshly-fetched remote-tracking ref); resolved by using `git merge-tree --write-tree` against explicitly-fetched refs as the authoritative check.
- **`close_beads.py --refresh` (gh-pr skill script)** — `AttributeError: 'list' object has no attribute 'get'` parsing `bd show` JSON output; worked around by closing beads directly via `bd close`.
- **`post_reply.py --text` doesn't exist** — usage requires a positional `MESSAGE` argument, not `--text`; corrected the invocation.

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| `soma::run(args)` for CLI subcommands | Caller-supplied `args` used only for help/version classification, then discarded — real subcommands reparsed `std::env::args()` | `local::run(&args)` parses the same slice `invocation::resolve` already classified |
| `/v1/palette/execute` with omitted `params` | Defaulted to `Value::Null`, failing zero-argument actions with `input_schema_failed` | Defaults to `{}` |
| Palette catalog/search/schema/execute | Read `catalog_snapshot()` directly, could serve stale data after a provider file changed | Refresh the file-backed registry first, matching REST/MCP |
| `/openapi.json` | Never included `/v1/palette/*` routes despite them being mounted | Composition root layers the palette augmentation on top of the REST document |
| `/v1/palette/*` request body limit | Capped at 65,536 bytes (the MCP-only limit), rejecting legitimate desktop payloads up to 256 KiB with 413 | Own 256 KiB layer; every other route keeps the original 64 KiB cap |
| `crates/shared/provider-adapters`'s absolute-OpenAPI-path rejection | Case-sensitive, so `HTTPS://...` bypassed the SSRF guard | Case-insensitive |
| Desktop `fetch_launcher_schema` when the saved server URL points at the web UI | Failed with the wrong-host hint even after the catalog itself loaded fine | Retries via the same API-base discovery as `fetch_launcher_catalog`/`execute_launcher_entry` |
| Self-hosted CI runner (Linux) | Missing `libdbus-1-dev`/GTK3/WebKitGTK/appindicator/rsvg/xdotool for the newly-real `soma-tauri-shell` workspace member; `cargo-deny` failed on 16 unavoidable advisories | Full prerequisite set installed via a corrected (previously always-true) guard; advisories reasoned-and-ignored |
| PR #151 mergeability | Blocked across three separate root causes over several hours | `MERGEABLE`/green, merged by the user |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo build --workspace --quiet` (post-merge, multiple rounds) | clean build | clean, exit 0 each time | pass |
| `cargo run -p xtask -- check-architecture` | 0 exceptions | "Architecture check passed (31 workspace packages, 83 internal edges)" | pass |
| `cargo test -p soma-runtime --all-features` | all tests pass incl. security-critical auth/proxy suite | 29/29 | pass |
| `cargo deny check` | advisories/bans/licenses/sources all ok | "advisories ok, bans ok, licenses ok, sources ok" | pass |
| `cargo test -p xtask test_siblings` | classification lists match real coverage | 5/5 | pass |
| `cargo test -p soma-palette -p soma-api -p soma-provider-adapters` | all green after review fixes | 48/9/30, 0 failures (after fixing 1 stale assertion) | pass |
| `cargo test -p soma` | all green after args-threading/http.rs changes | every suite 0 failed | pass |
| `cargo test --manifest-path apps/palette/src-tauri/Cargo.toml` | Tauri sidecar workspace green | 38/38 | pass |
| `gh pr view 151 --json mergeable` (final) | `MERGEABLE` | `MERGEABLE`, then `MERGED` | pass |

## Risks and Rollback

- All changes landed via normal PR merge (`af6b292`, a standard merge commit) — rollback is a straightforward `git revert -m 1 af6b292` if a regression surfaces, though this would revert the entire 10-PR refactor plus review fixes as one unit given the merge structure.
- The `deny.toml` advisory exceptions are a deliberate, documented risk acceptance (unmaintained-crate flags, not active CVEs) tied to Tauri's current Linux backend choice — revisit when `tauri-runtime-wry`/`tray-icon` ship a GTK4 or GTK-free backend, or when `tauri-utils` drops its `urlpattern`/`unic-*` dependency.
- The deferred Soma-native catalog id/kind mismatch (`rmcp-template-kbal`) means Soma-native palette entries currently render incorrectly (`undefined: undefined`) in the desktop launcher and cannot be executed from it (client-side id-format validation rejects them) — not a regression from this session (the feature was already broken pre-merge), but now confirmed and tracked rather than silently landed.

## Decisions Not Taken

- **Did not force-fit Soma-native catalog entries into the existing `labbyAction`/`mcpTool` desktop wire shape.** The review comment offered this as a valid, simpler alternative; rejected because it would mislabel the entries' actual routing/source to the user — deferred for an explicit product decision instead.
- **Did not widen the global MCP body-size limit to accommodate Palette.** Would have weakened an intentional cap on the JSON-RPC surface for an unrelated route family; restructured layering per-branch instead.
- **Did not attempt to fix the CI-runner infrastructure gap by SSHing into the runner or installing packages directly on the host.** Kept the fix scoped to the workflow-level `apt-get` step (`.github/actions/setup-rust-sccache/action.yml`), consistent with the runner's documented design (system prerequisites are installed per-job, not baked into the persistent container image).
- **Did not delete the now-fully-merged `refactor/pr11-integrations` … `pr19-delete-legacy` branches** despite confirming they're safe (ancestors of `main`, no dependent open PRs) — this command wasn't a cleanup request; listed as candidates instead.

## References

- `soma-architecture-refactor-plan-v3.md` (repo root) — the governing plan document for PR0–PR19
- `docs/CI.md`, `docs/LINUX-RUNNER.md`, `docs/WINDOWS-RUNNER.md` — self-hosted CI runner setup and troubleshooting
- PR #151 — https://github.com/jmagar/soma/pull/151 (this session's primary integration point)
- PR #164, #154 — https://github.com/jmagar/soma/pull/164, https://github.com/jmagar/soma/pull/154 (the auto-close recovery)
- `docs/sessions/2026-07-16-soma-architecture-refactor-pr0-pr9.md` — the prior session covering PR0–PR9

## Open Questions

- **`refactor/freeze-coverage` branch** — not an ancestor of `main`; unclear whether its content landed via another path (e.g. squashed into a different commit) or is genuinely stale/abandoned. Not investigated this session.
- **Soma-native catalog id/kind convention** — needs an explicit decision: what prefix/kind should Soma-native palette entries use, and should the desktop UI visually distinguish them from Labby actions and raw MCP tools? Tracked as `rmcp-template-kbal` / thread `PRRT_kwDOSc3ZY86R7tvk`.
- **`rmcp-template-yx81`** — `crates/soma-auth/.full-review/` leftover dir still present; low-priority, genuinely unfinished cleanup.
- **`rmcp-template-ns29`** — the brand-neutral shared-crate rename slice was decided but not yet executed; still real, open work.

## Next Steps

- **Decide and implement the Soma-native catalog id/kind convention** (bead `rmcp-template-kbal`) — the one deliberately-deferred item from `/gh-pr 151`. Needs a naming/routing decision spanning `crates/soma/palette/src/dto.rs` + `catalog.rs`, `apps/palette/src-tauri/src/labby_bridge.rs`'s `valid_launcher_id`, and `apps/palette/src/lib/launcherCatalog.ts`'s normalizer.
- **Consider a branch-cleanup pass** for the 9 confirmed-safe, fully-merged `refactor/pr11-*` … `pr19-*` branches, once explicitly requested.
- **Investigate `refactor/freeze-coverage`'s status** — confirm whether its content landed elsewhere or needs its own follow-up.
- **Execute the brand-neutral crate rename slice** (bead `rmcp-template-ns29`), decided but not yet done.
- **Clear the remaining `crates/soma-auth/.full-review/` leftover dir** (bead `rmcp-template-yx81`).
- No CI or build work is currently blocked — `main` is green as of the merge; this document is the immediate next action, followed by its own commit/push per the `save-to-md` contract.
