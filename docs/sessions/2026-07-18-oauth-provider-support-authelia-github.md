---
date: 2026-07-18 04:53:30 EST
repo: git@github.com:jmagar/soma.git
branch: claude/oauth-provider-support-f427c9
head: a8c5982
plan: docs/superpowers/plans/2026-07-18-oauth-provider-trait.md
working directory: /home/jmagar/workspace/soma/.claude/worktrees/oauth-provider-support-f427c9
worktree: /home/jmagar/workspace/soma/.claude/worktrees/oauth-provider-support-f427c9
pr: #167 — feat(soma-auth): OAuth provider trait — add Authelia + GitHub support — https://github.com/jmagar/soma/pull/167
beads: rmcp-template-2sl4, rmcp-template-1ge3
---

## User Request

Explore adding support for OAuth providers other than Google to `crates/shared/auth` (`soma-auth`), then confirmed Authelia is viable as an OIDC provider and asked to add both Authelia and GitHub support. Follow-up instructions chained: run `/lavra-eng-review` against the plan, apply findings via `/writing-plans`, then execute via `/work-it` — later reinforced with "get to fixing everything."

## Session Overview

Generalized `soma-auth` from a Google-only OAuth/OIDC login flow into a multi-provider system (`OAuthProvider` trait + `GoogleProvider`/`AutheliaProvider`/`GitHubProvider`, simultaneous multi-provider login with an HTML picker). The work went through three full review waves (a 4-agent plan-level review before implementation, a 6-agent code-level review after implementation, and a 6-agent PR Review Toolkit sweep) — 16 review agents total, every finding fixed and re-verified. Landed as draft PR #167 with 36 commits, 252 passing tests, and green CI (pending final confirmation of the last run as this log is written). A dependent follow-up plan (wiring the new providers into `soma`'s own CLI/config surfaces) was deferred as a tracked bead rather than executed blind, after discovering the target files had moved in an unrelated upstream restructuring.

## Sequence of Events

1. **Exploration.** Read the current Google-only implementation of `crates/shared/auth` (`google.rs`, `state.rs`, `config.rs`, `authorize.rs`, `routes.rs`, `token.rs`, `sqlite.rs`) to scope what a multi-provider generalization would require. Gave a short exploratory recommendation (trait-based abstraction) per house style for open-ended "what could we do" questions.
2. **Design clarification.** Asked the user via `AskUserQuestion` whether the design should support one active provider at a time (config-switchable) or simultaneous multi-provider login. User chose simultaneous multi-provider — this decision drove the DB schema (a `provider` column on 4 tables) and the HTML login-picker requirement.
3. **Plan authoring.** Wrote two implementation plans under `docs/superpowers/plans/`: `2026-07-18-oauth-provider-trait.md` (13 tasks, the `soma-auth` crate core) and `2026-07-18-soma-oauth-provider-config.md` (5 tasks, wiring into `soma`'s own CLI/setup/doctor, explicitly split out per the `writing-plans` skill's "multiple independent subsystems" guidance since Plan 2 depends on Plan 1 merging first).
4. **Plan-level engineering review.** Ran `/lavra-eng-review` (adapted since no real bead existed — treated the two plan documents as the review target directly). Dispatched 4 parallel agents (`architecture-strategist`, `code-simplicity-reviewer`, `security-sentinel`, `performance-oracle`) against the plan text. Synthesized findings and applied every one directly into the plan documents via edits (not a separate approval round) per the user's explicit "apply all feedback" instruction.
5. **Worktree sync.** Invoked `vibin:work-it`, which required `vibin:worktree-setup` first. Discovered the worktree branch was 75 commits behind `origin/main`, which had undergone a large architectural restructuring (`crates/soma/contracts` deleted and split into `crates/soma/config` + `crates/soma/domain`; `crates/soma/cli`, `apps/soma`, and several `crates/shared/*` crates reorganized). Confirmed the branch had zero unique commits of its own (`git rev-list --left-right --count HEAD...origin/main` → `0 75`), so fast-forwarded safely (`git merge --ff-only origin/main`). Verified `crates/shared/auth` (Plan 1's target) was essentially untouched by the restructuring (one Cargo.toml line). Left Plan 2 un-rewritten rather than guessing at the new structure blind.
6. **PR creation.** Committed both plan documents, pushed the branch, opened draft PR #167.
7. **Plan 1 implementation.** Dispatched a background implementation agent to execute all 13 tasks of `2026-07-18-oauth-provider-trait.md` via `superpowers:executing-plans`. Landed 15 commits. Agent's own verification: 235/235 tests (default), full `--all-features` suite, clippy clean, fmt clean, `cargo check --workspace` clean. Independently re-verified rather than trusting the report.
8. **Code-level review wave (6 agents).** Dispatched `architecture-strategist`, `code-simplicity-reviewer`, `security-sentinel`, `performance-oracle`, `pattern-recognition-specialist`, `data-integrity-guardian` against the actual committed diff (not the plan). Synthesized 10 actionable findings (1 P1: `GitHubProvider`'s derived `Debug` leaking `client_secret`; several P2s). Dispatched a fix agent; landed 9 more commits, 252 tests passing (+14 new tests).
9. **PR Review Toolkit wave (6 agents).** Ran `vibin:review-pr` in apply-fixes mode: `code-reviewer`, `pr-test-analyzer`, `comment-analyzer`, `silent-failure-hunter`, `type-design-analyzer`, `code-simplifier`. This wave found a genuine regression (Google's accepted ID-token issuer form narrowed from two values to one during the `oidc.rs` extraction) plus 19 more findings. Dispatched a third fix agent with a 20-item punch list, explicitly scoping out three larger deduplication/enum-introduction refactors as deliberate follow-ups. Landed 10 more commits, 252 tests still passing (net: some tests replaced/strengthened, not just added).
10. **CI.** Pushed to trigger CI. Two CI-only failures surfaced that no local command had caught: (a) `cargo fmt --all -- --check` (workspace-wide, not crate-scoped) failed on pre-existing drift in `apps/soma/src/http.rs` and `crates/soma/api/src/api.rs` — files this PR never touched; fixed with a formatting-only commit. (b) `xtask patterns` (`PATTERNS.md` module-size hard-limit gate) failed: `crates/shared/auth/src/sqlite.rs` had grown to 1404 effective (non-test) lines against a 1400 hard limit. Split into `sqlite.rs` (CRUD/migration logic), `sqlite_rows.rs` (row-mapping helpers), `sqlite_tests.rs` (the existing test module) using this crate's established `#[path = "..."]` sibling-file convention. A third CI job (`Frontend Assets`) failed on a transient `ETIMEDOUT` artifact-upload error unrelated to any code; resolved by the same push re-running it fresh.
11. **Session closeout.** Created two follow-up beads for tracked remaining work. Wrote this session log via `vibin:save-to-md`.

## Key Findings

- **`crates/shared/auth/src/config.rs:241`** — `AuthConfig::validate()` was private and called only from `AuthConfigBuilder::build_from_sources`; `AuthState::new` never called it, so a downstream consumer constructing `AuthConfig` via struct literal (a pattern already used in this crate's own tests) would silently bypass the new HTTPS-issuer and callback-collision checks. Fixed by making `validate()` `pub(crate)` and calling it first inside `AuthState::new`.
- **`crates/shared/auth/src/oidc.rs`** (post-extraction) — the refactor that moved Google's JWKS/ID-token verification into a shared `OidcVerifier` accidentally narrowed the accepted issuer set from two values (`https://accounts.google.com` OR bare `accounts.google.com`, both real values Google's tokens can carry) down to one. Caught by the PR Review Toolkit's `code-reviewer` pass, which diffed against pre-PR `google.rs` to confirm. Fixed with an `alt_issuer` field on `OidcVerifier`.
- **`crates/shared/auth/src/github.rs`** — GitHub's token endpoint returns HTTP 200 with an error body (`{"error":"bad_verification_code",...}`) on an invalid/reused code — no non-2xx status — so the generic `error_for_status()` path never triggered and the response fell through to a JSON-deserialize failure classified as a 500, not the correct 400 `invalid_grant`. One reviewer wrote and ran a throwaway wiremock reproduction to confirm before reporting it. Fixed with a `GitHubTokenResult` untagged enum.
- **`crates/shared/auth/src/github.rs`** — a `debug_assert!` guarding "GitHub never gets a refresh token, so `refresh_token_grant` routing to `GitHubProvider::refresh()` is unreachable" was compiled out entirely in release builds (no `debug-assertions = true` override in `[profile.release]`) and was tautological even in debug builds (checked a value set two lines above in the same function). Replaced with a real, always-on guard in `token.rs::refresh_token_grant`.
- **`crates/shared/auth/src/oauth_provider.rs`** — `ProviderExchange`'s doc comment explicitly claimed hardening against accidental secret exposure, but its `Debug` impl was plain-derived (not redacted), printing `access_token`/`refresh_token`/`id_token` in full via `{:?}` — inconsistent with every other secret-bearing type in the same PR, which all got hand-rolled `Debug` impls. Confirmed independently by both the code-level `pattern-recognition-specialist` and the PR-toolkit `type-design-analyzer`.
- **CI gate not reproducible locally without the right scope**: `cargo fmt -p soma-auth -- --check` (crate-scoped) was green throughout, but CI runs `cargo fmt --all -- --check` (workspace-wide) — the actual failure was in unrelated files. Similarly, `xtask patterns`'s module-size gate has no equivalent local-habit check; it only surfaced via CI.

## Technical Decisions

- **Simultaneous multi-provider over single-active-provider**: user's explicit choice via `AskUserQuestion`, accepted knowing it required a DB schema change (4 tables gain a `provider` column) and a browser login picker, versus the simpler config-switchable alternative.
- **`dyn OAuthProvider` + `async-trait` over a closed 3-variant enum**: raised as a real fork by the plan-level simplicity review; kept the trait because `soma-auth`'s own package description states it is "vendored... for soma and derived servers" and the root `CLAUDE.md` documents it as shared across the `rmcp-server` family (rustifi, cortex, rustarr, etc.) — external extensibility without patching this crate is a stated, not hypothetical, design goal.
- **Google's subject stays unprefixed; Authelia/GitHub get `{provider}:{raw_subject}` namespacing**: deliberate asymmetry to avoid invalidating already-issued Google sessions/refresh tokens on upgrade, while still preventing cross-provider subject collisions for the two new providers, which have no existing data to protect.
- **Shared-allowlist security trade-off resolved with a startup warning, not a schema change**: the plan-level security review flagged that a shared, cross-provider admin allowlist means the deployment's effective security floor is that of its weakest configured provider. Chose a `tracing::warn!` at `AuthState::new` plus a `docs/AUTH.md` section over full per-provider allowlist scoping, judged disproportionate for this crate's actual deployment shape (single-operator homelab/small-fleet, not multi-tenant SaaS).
- **Deferred three legitimate but non-blocking refactors** (Google/Authelia's ~180-line duplicated exchange/refresh flow, the triplicated `authorize_url` base-param building, and a `ProviderKind` enum to replace stringly-typed provider identifiers) rather than applying them in the third fix wave — judged the risk of introducing a new bug in security-critical OIDC code during a "final polish" pass as outweighing the DRY benefit at that point in the review cycle. Tracked as bead `rmcp-template-1ge3` instead of silently dropped.
- **Plan 2 deferred rather than rewritten blind**: after discovering the upstream crate restructuring, chose not to guess at the new `crates/soma/config`/`crates/soma/domain` shape under time pressure; tracked as bead `rmcp-template-2sl4` for a dedicated pass once Plan 1 is merged.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| created | `crates/shared/auth/src/oauth_provider.rs` | — | `OAuthProvider` trait, `ProviderExchange`, `AuthorizeUrlRequest`, `namespaced_subject` | commit `a0d11cd` |
| created | `crates/shared/auth/src/provider_http.rs` | — | shared HTTP/tracing/error-classification helper for all 3 providers | commits `1023cdc`, `01d6e0a` |
| created | `crates/shared/auth/src/oidc.rs` | — | shared JWKS/RS256 ID-token verifier (Google + Authelia) | commits `b59ee36`, `7e3fced` |
| created | `crates/shared/auth/src/authelia.rs` | — | `AutheliaProvider` (OIDC) | commits `d91fc35`, `757f690`, `29fe44e` |
| created | `crates/shared/auth/src/github.rs` | — | `GitHubProvider` (plain OAuth2) | commits `f711f2e`, `f9df06e`, `8a69d1d` |
| created | `crates/shared/auth/src/sqlite_rows.rs` | — | row-mapping helpers, split from `sqlite.rs` for module-size gate | commit `a8c5982` |
| created | `crates/shared/auth/src/sqlite_tests.rs` | — | test module, split from `sqlite.rs` | commit `a8c5982` |
| modified | `crates/shared/auth/src/google.rs` | — | rebuilt on `oidc.rs`/`provider_http.rs`, implements `OAuthProvider` | commits `eeb1825`, `c579c9a` |
| modified | `crates/shared/auth/src/config.rs` | — | `AutheliaConfig`/`GitHubConfig`, multi-provider `validate()` | commits `9193a6f`, `b9de9d2`, `266c163` |
| modified | `crates/shared/auth/src/types.rs` | — | `provider: String` field on 4 row types | commit `ac3c321` |
| modified | `crates/shared/auth/src/sqlite.rs` | — | `provider` column migration, CRUD updates, split for module-size gate | commits `ac3c321`, `ee27141`, `a8c5982` |
| modified | `crates/shared/auth/src/state.rs` | — | `AuthState.providers` map, `default_provider`, `validate()` enforcement | commits `ea1bc23`, `0fae5bd`, `32ae8f9` |
| modified | `crates/shared/auth/src/routes.rs` | — | per-provider callback mounting, `auth_dispatch_action` generalized | commits `92673ee`, `1b30beb` |
| modified | `crates/shared/auth/src/authorize.rs` | — | provider selection, HTML login picker, provider-agnostic callback | commits `e893c52`, `e805711` |
| modified | `crates/shared/auth/src/token.rs` | — | provider propagation through grants, GitHub-refresh guard | commits `c22f989`, `b888699` |
| modified | `crates/shared/auth/src/metadata.rs`, `session.rs` | — | test fixtures fixed after `validate()` enforcement surfaced latent gaps | commit `0fae5bd` |
| modified | `crates/shared/auth/src/lib.rs` | — | new module registrations | multiple commits |
| modified | `crates/shared/auth/Cargo.toml` | — | `async-trait` dependency | commit `a0d11cd` |
| modified | `docs/AUTH.md` | — | multi-provider security posture section | commits `facb4c7`, `355a5d3` |
| modified | `CHANGELOG.md` | — | `[Unreleased]` entry | commit `facb4c7` |
| modified | `apps/soma/src/http.rs`, `crates/soma/api/src/api.rs` | — | pre-existing rustfmt drift, unrelated to this PR, fixed to unblock CI | commit `cc7c958` |
| created | `docs/superpowers/plans/2026-07-18-oauth-provider-trait.md` | — | Plan 1, executed in full | commit `31eae1b` |
| created | `docs/superpowers/plans/2026-07-18-soma-oauth-provider-config.md` | — | Plan 2, deferred (bead `rmcp-template-2sl4`) | commit `31eae1b` |
| created | `docs/sessions/2026-07-18-oauth-provider-support-authelia-github.md` | — | this file | this commit |

## Beads Activity

- **`rmcp-template-2sl4`** (created, P3, open) — "Rewrite soma-oauth-provider-config plan against current crates/soma/config+domain structure." Tracks Plan 2's rewrite once Plan 1 merges. Not blocking — runtime OAuth already works via raw env vars regardless of this bead.
- **`rmcp-template-1ge3`** (created, P4, open) — "Deduplicate Google/Authelia OIDC exchange+refresh and authorize_url base-param building in soma-auth." Tracks the three deliberately-deferred simplification/type-design follow-ups from the third review wave.

No other bead activity occurred; this session otherwise used the plan-doc + PR review workflow explicitly requested by the user rather than `bd`-tracked implementation.

## Repository Maintenance

- **Plans**: `docs/superpowers/plans/` is this repo's plan directory (not the generic `docs/plans/` the `save-to-md` skill defaults to checking) and has no established `complete/` subdirectory convention — confirmed via `ls docs/superpowers/plans/`. Left both plan documents in place; Plan 1 is fully executed (documented above) and Plan 2 is tracked as open follow-up work via bead `rmcp-template-2sl4`, so neither is a stale/orphaned artifact.
- **Beads**: created 2 follow-up beads (above) for known remaining work rather than leaving it only in prose.
- **Worktrees and branches**: `git worktree list --porcelain` shows 6 other worktrees (main checkout on `bd-work/workspace-deps-and-freeze-audit`, a Codex worktree, `marketplace-no-mcp` — protected per this repo's `CLAUDE.md`, do not touch — plus 3 other `claude/*` feature worktrees). None are stale relative to this session's work; none were created, modified, or removed by this session. Noted in passing: `origin/claude/labby-auth-crate-port-aeb44c` (a branch not touched this session) has a commit with a similar-sounding title ("split oversized modules to satisfy xtask patterns file-size gate") — unrelated to this session's `sqlite.rs` split, flagged here only as an observation, not investigated further (out of scope).
- **Stale docs**: `docs/AUTH.md` was updated as part of the implementation itself (not a post-hoc fix) to document the new multi-provider security posture — not a maintenance-pass finding. No other docs were found stale or contradicted by this session's changes.
- **Transparency**: no cleanup was skipped or blocked; the only maintenance-pass deviation from the skill's generic instructions is the `docs/plans/` vs `docs/superpowers/plans/` path mismatch noted above.

## Tools and Skills Used

- **Shell (Bash)**: git operations (fetch/log/diff/commit/push/merge --ff-only), `cargo test`/`clippy`/`fmt`/`check` at both crate and workspace scope, `gh pr`/`gh run`/`gh api` for PR/CI inspection, `bd create` for follow-up beads, `sed`/`wc`/manual file splitting for the `sqlite.rs` module-size fix.
- **File tools (Read/Write/Edit)**: extensive use for reading the pre-existing crate, authoring both plan documents (~5000 lines combined), and applying ~30 targeted edits to the plan documents after the first review wave.
- **Agent tool (subagents)**: 4 parallel agents for the plan-level review, 1 implementation agent for Plan 1, 6 parallel agents for the code-level review, 1 fix agent, 6 parallel agents for the PR Review Toolkit wave, 1 more fix agent, 1 fix agent for the fmt/module-size CI fixes handled directly by the coordinator instead. All ran as background agents with task-notification callbacks; no failures or degraded behavior observed across ~18 agent dispatches.
- **Skills**: `writing-plans` (plan authoring), `lavra:lavra-eng-review` (adapted for a non-bead plan-doc target), `worktree-setup` (sync/fast-forward), `work-it` (overall orchestration), `lavra:lavra-review` (adapted for a non-bead code target), `review-pr` (PR Review Toolkit sweep), `quick-push` → `save-to-md` (this session log).
- **MCP/Monitor tool**: used twice to watch GitHub Actions CI checks to completion via polling loops (`gh pr checks`), each ending with an "ALL CHECKS TERMINAL" summary line rather than silent polling.
- **AskUserQuestion**: used once, for the single-vs-simultaneous-multi-provider design fork — the one decision genuinely requiring user input rather than engineering judgment.
- No browser, MCP domain-service, or external CLI tools beyond `git`/`gh`/`cargo`/`bd` were used this session.

## Commands Executed

| command | result |
|---|---|
| `git rev-list --left-right --count HEAD...origin/main` | `0  75` — confirmed safe fast-forward |
| `git merge --ff-only origin/main` | fast-forwarded `89e616b..1c58e79`, 3081-line `Cargo.lock` diff, confirmed `crates/shared/auth` nearly untouched |
| `cargo test -p soma-auth --all-features` (run ~6 times across the session) | final: `252 passed; 0 failed` |
| `cargo clippy -p soma-auth --all-targets --all-features -- -D warnings` | clean throughout after each fix wave |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --all -- --check` (workspace-wide) | failed once on pre-existing drift in 2 unrelated files, clean after fix |
| `cargo run -p xtask -- patterns` | failed once (`sqlite.rs` 1404 > 1400 hard limit), clean after 3-way file split (1317 effective lines) |
| `gh pr checks 167` (polled via Monitor) | converged to all-pass after 2 rounds of CI-driven fixes |
| `bd create` ×2 | created `rmcp-template-2sl4`, `rmcp-template-1ge3` |

## Errors Encountered

- **CI `Format` job failed** on `apps/soma/src/http.rs`/`crates/soma/api/src/api.rs` — pre-existing drift on `origin/main`, not introduced by this PR (confirmed via `git diff 1c58e79..HEAD --name-only` showing neither file in this PR's own diff before the fix). Root cause: local verification only ever ran `cargo fmt -p soma-auth`, crate-scoped; CI runs workspace-wide. Resolved with a dedicated formatting-only commit (`cc7c958`).
- **CI `Soma Contracts` job failed** (`xtask patterns` module-size gate): `crates/shared/auth/src/sqlite.rs` reached 1404 effective lines against a 1400 hard limit, pushed over by the cumulative fix-wave additions. No local command surfaces this gate. Resolved by splitting into `sqlite.rs`/`sqlite_rows.rs`/`sqlite_tests.rs` (commit `a8c5982`), bringing it to 1317 effective lines.
- **CI `Frontend Assets` job failed** on `Failed to CreateArtifact: Unable to make request: ETIMEDOUT` — a transient self-hosted-runner network timeout uploading a Next.js build artifact; the build itself succeeded. Not a code issue; resolved by the natural re-run triggered by the next push (an explicit `gh run rerun --failed` attempt was rejected because the parent workflow run was still in progress at the time).

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Supported OAuth providers | Google only | Google, Authelia (OIDC), GitHub (plain OAuth2) — any combination configurable simultaneously |
| `/auth/login` with 2+ providers configured | N/A (single provider always) | Renders an HTML picker when `?provider=` is omitted |
| `force_consent` (refresh-token guarantee) scope | global (`has_any_refresh_token()` across all logins) | per-provider (`has_any_refresh_token_for_provider(...)`) |
| Google ID-token issuer acceptance | accepted both `https://accounts.google.com` and bare `accounts.google.com` | same (was briefly narrowed mid-session by the `oidc.rs` extraction, then restored) |
| GitHub token-exchange error classification | N/A (provider didn't exist) | HTTP-200-with-error-body correctly classified as `invalid_grant`/400, not `server_error`/500 |
| `AuthState::new` config validation | did not call `AuthConfig::validate()` | calls it first, closing a bypass path for struct-literal-constructed configs |
| `crates/shared/auth/src/sqlite.rs` structure | one 2355-line file | split into `sqlite.rs` (1611 lines) + `sqlite_rows.rs` (124) + `sqlite_tests.rs` (744) |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test -p soma-auth --all-features` | all pass | 252 passed, 0 failed | pass |
| `cargo clippy -p soma-auth --all-targets --all-features -- -D warnings` | no warnings | clean | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | no warnings | clean | pass |
| `cargo fmt --all -- --check` | no diff | clean (after `cc7c958`) | pass |
| `cargo run -p xtask -- patterns` | no `FAIL:` lines | clean, `sqlite.rs` now a soft `WARN` not hard `FAIL` (after `a8c5982`) | pass |
| `cargo check --workspace` | compiles | clean | pass |
| `gh pr checks 167` | all required checks green | converging at time of writing — `Soma Contracts`, `Format`, `Official MCP Conformance`, `Changes`, `MSRV Changes` confirmed pass; `Frontend Assets` re-running after transient failure; full terminal state not yet confirmed when this log was written | in progress |

## Risks and Rollback

- All changes are additive/generalizing to `soma-auth`; existing Google-only deployments are unaffected by default (no new required env vars, `default_provider` auto-resolves to `google` when only Google is configured, matching pre-PR behavior exactly).
- The `provider` column migration (`ALTER TABLE ... ADD COLUMN ... DEFAULT 'google'`) is backward-compatible and covered by a dedicated regression test that reopens a hand-written pre-migration schema.
- Rollback path: revert PR #167 (single squash-mergeable unit) or `git revert` the merge commit; no data migration is destructive (adding a nullable-with-default column), so a rollback after a merge+deploy would leave a harmless extra column on any live DB.
- Deferred refactors (bead `rmcp-template-1ge3`) carry no risk by construction — they were never applied.

## Decisions Not Taken

- **Per-provider allowlist schema scoping** (full fix for the shared-allowlist security trade-off) — rejected in favor of a startup warning + doc note; judged disproportionate for this crate's single-operator/small-fleet deployment shape versus a schema change's added complexity.
- **`ProviderKind` enum** to replace `provider_id() -> &'static str` and stringly-typed SQL columns — legitimate, deferred to bead `rmcp-template-1ge3` as a follow-up-scale improvement, not a blocker.
- **Merging Google/Authelia's duplicated exchange/refresh flow into one shared helper**, and **deduplicating the triplicated `authorize_url` base-param building** — both flagged by the code-simplifier in the third review wave; deferred to the same bead rather than risked in a late-cycle polish pass.
- **Rewriting Plan 2 blind against the guessed-at new crate structure** — rejected in favor of tracking it as bead `rmcp-template-2sl4` for a dedicated, correctly-scoped pass.

## Open Questions

- Final CI terminal state for PR #167 was not fully confirmed at the time this log was written (the `Frontend Assets` re-run and a few remaining jobs were still in flight). Check `gh pr checks 167` before merging.
- PR #167 remains in draft; no instruction was given to mark it ready for review.

## Next Steps

1. Confirm PR #167's CI is fully green (`gh pr checks 167`), then decide whether to mark it ready for review and merge.
2. When ready to continue the OAuth work, pick up bead `rmcp-template-2sl4` (Plan 2 rewrite against `crates/soma/config`/`crates/soma/domain`) — this is what makes Authelia/GitHub configurable through `soma setup`/`soma doctor`, not just raw env vars.
3. Bead `rmcp-template-1ge3` (dedup + `ProviderKind` enum) is available whenever a lower-priority cleanup pass is convenient; not blocking anything.
4. No other unfinished work from this session.
