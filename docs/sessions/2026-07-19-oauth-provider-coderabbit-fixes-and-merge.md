```yaml
date: 2026-07-19 00:38:37 EST
repo: git@github.com:jmagar/soma.git
branch: claude/oauth-provider-support-f427c9
head: 6e2a5f3
working directory: /home/jmagar/workspace/soma/.claude/worktrees/oauth-provider-support-f427c9
worktree: /home/jmagar/workspace/soma/.claude/worktrees/oauth-provider-support-f427c9
pr: "#167 feat(soma-auth): OAuth provider trait — add Authelia + GitHub support — https://github.com/jmagar/soma/pull/167 (MERGED)"
beads: none created this session; verified rmcp-template-2sl4, rmcp-template-1ge3, rmcp-template-mkag remain closed
```

## User Request

Continuing from a compacted prior conversation whose standing instruction was "resolve the conflicts and get it merged into main without losing any work." This session picks up immediately after that merge conflict had already been resolved and pushed (commit `0d1117c`), with a fresh `Soma Contracts: fail` CI event and 7 unfetched-but-summarized CodeRabbit review comments still outstanding on PR #167.

## Session Overview

Investigated and fixed a `Soma Contracts` (coupled-file gate) CI failure, triaged and fixed all 7 outstanding CodeRabbit review findings on PR #167 (3 real P2 correctness bugs, 3 minor/trivial cleanups, 1 doc-accuracy clarification), discovered and fixed two knock-on issues introduced by those fixes (a module-size hard-limit violation and a doctest compile failure), verified everything locally and in CI, then merged PR #167 into `main` via a standard merge commit. Closed out with post-merge CI verification on `main` itself.

## Sequence of Events

1. Investigated the `Soma Contracts: fail` CI event on merge commit `0d1117c` via `gh api .../actions/jobs/{id}/logs`; found `cargo xtask check-coupled-files` failing because `scripts/generate-docs.py` and `plugins/soma/*` had changed (in prior commits) without matching `scripts/README.md` / `docs/PLUGINS.md` updates.
2. Fixed the coupled-file gate by adding real, accurate content to `scripts/README.md` (env-var curation note) and `docs/PLUGINS.md` (multi-provider `auth_mode` clarification) — not just touching the files to satisfy the check.
3. Re-read all 7 previously-fetched CodeRabbit findings against current code and fixed each:
   - **P2** `config.rs`: the callback-path collision check compared raw config strings against `FIXED_ROUTE_PATHS`/each other, so a path missing its leading `/` (e.g. `authorize`) would sail through the check yet still mount at `/authorize` via `state.rs`'s `build_provider_redirect_uri` normalization, panicking axum's duplicate-route guard at startup. Fixed by normalizing (`format!("/{}", path.trim_start_matches('/'))`) before comparing.
   - **P2** `config.rs`/`state.rs`: `GoogleConfig`/`AutheliaConfig`/`GitHubConfig` derived `Default`, which gives `callback_path: String::new()` instead of the `#[serde(default = "fn")]` value (serde's per-field default only wires into `Deserialize`, never `Default`). `AuthState::new` calls `config.validate()` first, so a struct-literal `AuthConfig` configuring only Authelia/GitHub (relying on `..AuthConfig::default()` for the untouched `google` field) failed validation on Google's empty callback path. Fixed by hand-rolling `impl Default` for all three provider configs to match their serde defaults.
   - **P2** `oidc.rs`: Authelia token exchange unconditionally posted `client_secret` in the form body (`client_secret_post`), but Authelia's documented default token-endpoint auth method for confidential clients is `client_secret_basic` (HTTP `Authorization: Basic` header). Added a `TokenAuthMethod` enum (`ClientSecretPost` default / `ClientSecretBasic`) to `OidcVerifier`, wired Authelia to `ClientSecretBasic`, left Google on the existing `ClientSecretPost` default.
   - **Minor** `CLAUDE.md`: clarified that `SOMA_MCP_AUTHELIA_*`/`SOMA_MCP_GITHUB_*`/`SOMA_MCP_AUTH_DEFAULT_PROVIDER` are read directly from process env by `soma_auth::AuthConfigBuilder` (via `crates/soma/integrations/src/auth.rs`), not through `crates/soma/config::Config`'s typed struct — confirmed by tracing `http_auth_policy()` → `AuthConfigBuilder::build_from_sources(std::env::vars())` in `apps/soma/src/bootstrap.rs`.
   - **Nitpick** `oauth_provider.rs`: removed the unused `scope` field from `AuthorizeUrlRequest` (verified via grep that no `authorize_url` implementation or `build_authorize_url` reads it) and updated every construction site (`authorize.rs` x2, `google.rs`, `authelia.rs`, `github.rs`, `provider_http.rs` test).
   - **Trivial** `crates/soma/cli/src/doctor/checks.rs`: removed a dead, fully-redundant `#[cfg(windows)]` fallback block in `check_binary_in_path` — the preceding `[binary, suffixed.as_str()]` loop already covers the same EXE-suffix case.
   - **Trivial** `scripts/generate-docs.py`: added missing backticks around the `SOMA_MCP_AUTH_DEFAULT_PROVIDER` default (`automatic` → `` `automatic` ``) for consistency with sibling entries; regenerated `docs/ENV.md` via `cargo xtask generate-docs`.
4. Removed 4 now-stale test comments/workarounds in `authorize.rs` that explicitly set `config.github.scopes` to work around the pre-fix `GitHubConfig::default()` bug (fixed in step 3's `Default` impl, making the explicit override redundant and its explanatory comment inaccurate).
5. Ran `cargo test -p soma-auth --lib` (98 passed), `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` (clean) — all green.
6. Ran `cargo xtask patterns`; found a new real failure: `config.rs` had grown to 735 effective lines against the 700 hard limit (the `Default` impls, collision-check normalization, and new tests pushed it over). Split `GoogleConfig`/`AutheliaConfig`/`GitHubConfig` and their default-value helper functions into a new sibling file `crates/shared/auth/src/config_providers.rs` (declared via `#[path = "config_providers.rs"] mod config_providers;`, matching the `sqlite.rs`/`sqlite_rows.rs` precedent from earlier in this PR), re-exporting the three structs so external call sites are unaffected.
7. Ran `cargo test --workspace` in the background; it surfaced a doctest compile failure (`circular modules: config_providers.rs -> config_providers.rs`) caused by a fenced ` ```rust ` code block in `config_providers.rs`'s module doc comment (unlike `sqlite_rows.rs`, this new module is not `#[cfg(test)]`-gated, so rustdoc's non-test doctest pass actually tried to compile the snippet as a standalone crate). Fixed by rewriting the doc comment to describe the declaration in prose instead of a fenced code block.
8. Re-ran the full workspace test suite: 1 unrelated flaky failure (`soma-codemode`'s `budget_rejects_operations_over_configured_limit`, untouched by this diff, confirmed passing in isolation) — accepted as pre-existing/load-induced, not a regression.
9. Staged and committed all fixes as `6e2a5f3`, verified `cargo xtask check-coupled-files origin/main HEAD` passed, pushed to `origin/claude/oauth-provider-support-f427c9`.
10. Monitored PR #167's CI to green (all 24 checks passing, including `CI Gate`), confirmed `mergeable: MERGEABLE` / `mergeStateStatus: CLEAN`.
11. Checked repo merge-strategy convention (`git log --merges`, recent merged PRs) — found the repo consistently uses merge commits (`Merge pull request #N from ...`), not squash. Merged via `gh pr merge 167 --merge`.
12. Verified `origin/main` fast-forwarded to merge commit `2af3049`, and monitored the resulting post-merge `CI`/`MSRV`/`MCP Conformance` workflow runs on `main` to completion — all succeeded.

## Key Findings

- `crates/shared/auth/src/state.rs:389-419` (`build_provider_redirect_uri`, `state.rs:438`): unconditionally strips any leading `/` from `callback_path` and re-adds exactly one, meaning the router (`routes.rs:37-38`, via `provider.callback_path()`) mounts at the *normalized* path regardless of what the operator typed — the validation gap in `config.rs`'s collision check operated on the raw, un-normalized string, so it could miss collisions the router would actually hit.
- `crates/shared/auth/src/config.rs` (pre-fix): `GoogleConfig`/`AutheliaConfig`/`GitHubConfig` derived `Default` while also carrying `#[serde(default = "fn")]` attributes — these two mechanisms are independent in serde/Rust; only `Deserialize` honors the field-level `default =`. No existing test in the crate constructed a struct-literal `AuthConfig` without explicitly setting every provider's `callback_path`, so this bug was latent and untested until this session's regression test (`validate_accepts_a_struct_literal_config_configuring_only_authelia`, `config.rs`).
- `apps/soma/src/bootstrap.rs:271-279` (`http_auth_policy`, `AuthPolicyKind::MountedOAuth` arm): confirms `SOMA_MCP_AUTHELIA_*`/`SOMA_MCP_GITHUB_*`/`SOMA_MCP_AUTH_DEFAULT_PROVIDER` are live, functioning env vars in production — `soma_auth_config_builder().build_from_sources(std::env::vars())` reads raw process env, entirely bypassing `crates/soma/config::Config`. CodeRabbit's underlying premise (these vars aren't consumed) was only true of the typed `Config` struct specifically, not of the running server.
- `xtask/src/patterns/util.rs:24-65` (`size_limit`): the effective hard limit for a `.rs` file without a special-cased override is `350 * 2 = 700` (`checks.rs:113`, `loc > limit * 2`); `config.rs` has no override entry, so it was always subject to the default, not the `700` figure sqlite.rs uses (that one has an explicit `Some(700)` *soft target*, giving it a 1400 hard limit — a different, unrelated override).
- `xtask/src/patterns/util.rs:157-163` (`strip_inline_test_module`): only strips a *trailing* `#[cfg(test)] mod tests { ... }` block from effective-line counting, and rustdoc's non-test doctest-extraction pass only skips `#[cfg(test)]`-gated modules entirely — a non-test sibling module's doc comments are always live doctest candidates, which is why `config_providers.rs` needed prose instead of a fenced ` ```rust ` snippet (`sqlite_rows.rs` avoids the issue by simply not using a fenced block in its header doc).

## Technical Decisions

- **Callback-path normalization scope**: normalized only for the collision check (leading-`/` insertion, matching what `build_provider_redirect_uri` does when `public_url` has no path prefix); did not attempt to also account for a non-empty `public_url` path prefix in the check, since that's a materially larger design change out of scope for this fix and the existing check already only ever validated against the no-prefix case.
- **`TokenAuthMethod` design**: added as an enum + builder method on `OidcVerifier` (mirroring the existing `with_alt_issuer` pattern) rather than a boolean flag, defaulting to `ClientSecretPost` so Google's behavior is byte-for-byte unchanged and only `AutheliaProvider::new`/`with_endpoints` opt into `ClientSecretBasic` — minimizes blast radius to a single call site per provider.
- **`Default` impl fix over validation-relaxation**: chose to make `GoogleConfig::default()` match its serde default (restoring correctness/consistency) rather than relaxing `validate()`'s unconditional Google callback-path check to be conditional on `google_configured` — the latter would have masked the same class of bug for any future field, whereas the `Default` fix makes `Default::default()` and "deserialize an empty object" permanently equivalent, which is the invariant CodeRabbit's finding was really pointing at.
- **`config_providers.rs` split boundary**: extracted exactly the three provider config structs + their `Debug`/`Default` impls + their default-value helper functions (a cohesive, self-contained unit with no other internal config.rs dependencies) rather than splitting along a different seam (e.g. `AuthConfigBuilder`), keeping the change minimal and mirroring the established `sqlite.rs`/`sqlite_rows.rs` precedent from earlier in this same PR.
- **Merge strategy**: used `gh pr merge 167 --merge` (a real merge commit) after confirming via `git log --merges origin/main` and `gh pr list --state merged` that recent PRs in this repo consistently merge this way, rather than defaulting to squash.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `CLAUDE.md` | — | Clarify OAuth env-var wiring (soma-auth builder, not typed Config); satisfies CodeRabbit finding #4 | commit `6e2a5f3` |
| modified | `crates/shared/auth/src/authelia.rs` | — | Opt Authelia into `TokenAuthMethod::ClientSecretBasic`; add regression test for HTTP Basic auth | commit `6e2a5f3` |
| modified | `crates/shared/auth/src/authorize.rs` | — | Remove dead `scope:` field from 2 `AuthorizeUrlRequest` construction sites; remove 4 now-stale `config.github.scopes` test workarounds | commit `6e2a5f3` |
| modified | `crates/shared/auth/src/config.rs` | — | Normalize callback-path collision check; extract provider configs to `config_providers.rs`; add 3 regression tests | commit `6e2a5f3` |
| created | `crates/shared/auth/src/config_providers.rs` | — | `GoogleConfig`/`AutheliaConfig`/`GitHubConfig` + hand-rolled `Default` impls + default-value helpers, split out to satisfy the `xtask patterns` file-size gate | commit `6e2a5f3` |
| modified | `crates/shared/auth/src/github.rs` | — | Remove dead `scope:` field from test `AuthorizeUrlRequest` construction | commit `6e2a5f3` |
| modified | `crates/shared/auth/src/google.rs` | — | Remove dead `scope:` field from test `AuthorizeUrlRequest` construction | commit `6e2a5f3` |
| modified | `crates/shared/auth/src/oauth_provider.rs` | — | Remove unused `scope: String` field from `AuthorizeUrlRequest` | commit `6e2a5f3` |
| modified | `crates/shared/auth/src/oidc.rs` | — | Add `TokenAuthMethod` enum, `with_token_auth_method` builder, `token_request` helper; branch `client_secret_post` vs `client_secret_basic` | commit `6e2a5f3` |
| modified | `crates/shared/auth/src/provider_http.rs` | — | Remove dead `scope:` field from test `AuthorizeUrlRequest` construction | commit `6e2a5f3` |
| modified | `crates/soma/cli/src/doctor/checks.rs` | — | Remove dead redundant `#[cfg(windows)]` block in `check_binary_in_path` | commit `6e2a5f3` |
| modified | `docs/ENV.md` | — | Regenerated via `cargo xtask generate-docs` after the `generate-docs.py` backtick fix | commit `6e2a5f3` |
| modified | `docs/PLUGINS.md` | — | Document multi-provider `auth_mode=oauth` behavior; satisfy coupled-file gate | commit `6e2a5f3` |
| modified | `scripts/README.md` | — | Document per-provider env-var curation in `generate-docs.py`; satisfy coupled-file gate | commit `6e2a5f3` |
| modified | `scripts/generate-docs.py` | — | Backtick-quote `SOMA_MCP_AUTH_DEFAULT_PROVIDER`'s default value | commit `6e2a5f3` |
| created | `docs/sessions/2026-07-19-oauth-provider-coderabbit-fixes-and-merge.md` | — | This session log | this commit |

## Beads Activity

No beads were created, claimed, or edited this session. Verified (via `bd show`) that the 3 beads spawned by earlier phases of this same overall PR effort remain `CLOSED`:
- `rmcp-template-2sl4` (P3) — "Rewrite soma-oauth-provider-config plan against current crates/soma/config+domain structure" — closed by a prior continuation session.
- `rmcp-template-1ge3` (P4) — "Deduplicate Google/Authelia OIDC exchange+refresh..." — closed by a prior continuation session.
- `rmcp-template-mkag` (P2, bug) — "soma doctor --json reports 3 issues instead of 2 on Windows CI" — closed by a prior continuation session.

No new follow-up work surfaced during this session that warranted a new bead — every CodeRabbit finding was fixed and verified in-session, and CI is fully green on both the PR and `main`.

## Repository Maintenance

- **Plans**: This repo has no `docs/plans/` or `docs/plans/complete/` directory/convention — plans live permanently in `docs/superpowers/plans/` as historical work records per `docs/CLAUDE.md` ("Historical work records. Useful evidence, not source of truth."), so no move was performed or needed. `docs/superpowers/plans/2026-07-18-oauth-provider-trait.md` and `2026-07-18-soma-oauth-provider-config.md` remain in place as-is, consistent with that convention.
- **Beads**: Checked `bd show` for the 3 beads tied to this PR effort — all already `CLOSED` with recorded close reasons; no state change needed (see Beads Activity above).
- **Worktrees and branches**: Ran `git worktree list --porcelain` and reviewed local/remote branch lists. The PR branch `claude/oauth-provider-support-f427c9` is now merged into `main` (`2af3049`) but was **not** deleted — this session is running from that exact worktree (`/home/jmagar/workspace/soma/.claude/worktrees/oauth-provider-support-f427c9`), so deleting it from within is unsafe; it should be cleaned up (`git worktree remove` + branch delete, local and `origin`) in a later session once this worktree is no longer in use. `repo.delete_branch_on_merge` is `false`, so GitHub did not auto-delete it either. Numerous other stale worktrees/branches (`worktree-agent-*`, `worktree-wf_*`, several `refactor/pr*` branches) were observed but are unrelated to this session's work and were left untouched — no evidence was gathered on their merge status, so touching them would be out-of-scope guesswork.
- **Stale docs**: `CLAUDE.md`, `docs/PLUGINS.md`, `docs/ENV.md`, and `scripts/README.md` were the stale-doc updates this session — all driven directly by CodeRabbit findings and the coupled-file gate (see Files Changed). No other doc staleness was identified in scope.
- **Transparency**: All actions above are evidenced by the commands in "Commands Executed" and the CI check results in "Verification Evidence".

## Tools and Skills Used

- **Shell commands (Bash)**: `git` (log, diff, show, status, add, commit, push, fetch), `gh` (pr checks/view/merge, run list/view, api for job logs and runner status), `cargo` (test, clippy, fmt, xtask patterns/check-coupled-files/check-blob-size/generate-docs/check-docs), `grep`/`sed` for code investigation. No failures beyond the two documented below.
- **File tools (Read/Edit/Write)**: Used throughout for all code and doc changes; no issues.
- **Monitor tool**: Used 5 times to watch PR CI checks and post-merge `main` CI without polling. Two script authoring mistakes (documented in Errors Encountered) required re-arming with corrected scripts; otherwise worked as intended, including catching the async `Build Windows`/`Test` completions and the post-merge `CI Gate`.
- **ScheduleWakeup**: Called once by mistake (intended for `/loop` dynamic mode, not general background-task waiting) and immediately self-corrected by stopping it — task-notifications already cover this case.
- **`vibin:save-to-md` skill**: Used to produce this session log per explicit user request ("save-to-md").
- **`bd` (beads CLI)**: Used read-only (`bd list`, `bd show`) to verify existing bead state; no writes needed this session.

## Commands Executed

| Command | Result |
|---|---|
| `gh api repos/jmagar/soma/actions/jobs/{id}/logs` | Identified `check-coupled-files` failure: `scripts/generate-docs.py`/`plugins/soma/*` changed without `scripts/README.md`/`docs/PLUGINS.md` |
| `cargo test -p soma-auth --lib` | 98 passed, 0 failed (after all CodeRabbit fixes + `config_providers.rs` split) |
| `cargo clippy --workspace --all-targets -- -D warnings` | Clean, no warnings |
| `cargo fmt --all -- --check` | Clean |
| `cargo xtask patterns` | Initially failed on `config.rs: 735 effective lines (hard limit 700)`; passed after the `config_providers.rs` split (real failures only — `.cargo/registry/` mod.rs noise is untracked local-only cache, confirmed via `git ls-files`) |
| `cargo xtask check-coupled-files origin/main HEAD` | Failed before doc fixes; `Coupled-file check passed (origin/main..HEAD)` after commit `6e2a5f3` |
| `cargo test --workspace` (background) | Surfaced doctest failure `circular modules: config_providers.rs -> config_providers.rs`; fixed, re-run clean except 1 unrelated flaky `soma-codemode` test (passed in isolation) |
| `git push` | `0d1117c..6e2a5f3 claude/oauth-provider-support-f427c9 -> claude/oauth-provider-support-f427c9` |
| `gh pr merge 167 --merge --subject "Add multi-provider OAuth support: Authelia and GitHub (#167)"` | PR merged; `state: MERGED`, `mergeCommit: 2af3049` |
| `gh run list --branch main ...` / `gh run view 29671518819` | Post-merge `CI`, `MSRV`, `MCP Conformance` workflows all `success` on `main` |

## Errors Encountered

- **Monitor script: `read-only variable: status` (zsh)**: A watch-loop script assigned to a shell variable named `status`, which zsh reserves for the last command's exit code. Fixed by renaming to `run_status`. Root cause: reserved-word collision, not a logic error.
- **Doctest compile failure (`circular modules`)**: `config_providers.rs`'s module doc comment included a fenced ` ```rust ` snippet showing its own `#[path = ...] mod config_providers;` declaration; rustdoc's non-`cfg(test)` doctest pass tried to compile it as a standalone crate and failed with a self-referential module error. Root cause: the new module isn't `#[cfg(test)]`-gated (unlike the same-pattern `sqlite_rows.rs`/`checks_tests.rs` precedents, whose doc comments only exist under `cfg(test)` and are therefore skipped by the non-test doctest pass). Fixed by rewriting the doc comment as prose instead of a fenced code block, matching `sqlite_rows.rs`'s style.
- **`xtask patterns` file-size hard-limit violation**: The CodeRabbit fixes (three `Default` impls, path normalization, new tests) pushed `config.rs` from under 700 effective lines to 735, past the default `.rs` hard limit (`350 * 2`). Root cause: no local pre-commit habit catches this gate (confirmed as a known gap from earlier in the overall PR effort, per prior summary). Fixed by splitting into `config_providers.rs`.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Authelia token exchange auth | `client_secret` posted in form body (`client_secret_post`) unconditionally | Sent via `Authorization: Basic` header (`client_secret_basic`), matching Authelia's documented default for confidential clients |
| `config.rs` callback-path collision check | Compared raw, un-normalized `callback_path` strings; missed collisions from paths without a leading `/` | Normalizes (guarantees leading `/`) before comparing, matching what the router actually mounts |
| `GoogleConfig`/`AutheliaConfig`/`GitHubConfig::default()` | `callback_path: String::new()`, `scopes: vec![]` (derived `Default`) | `callback_path`/`scopes` match the same non-empty values a deserialized empty config would get |
| `AuthorizeUrlRequest` | Had an unused `scope: String` field | Field removed |
| `soma doctor`'s `check_binary_in_path` on Windows | Redundant/dead `#[cfg(windows)]` fallback block present (no behavior change, but dead code) | Block removed; behavior identical (the preceding suffix loop already covered it) |
| `crates/shared/auth/src/config.rs` module structure | Single file, 735 effective lines | Split into `config.rs` + `config_providers.rs`, both under the file-size hard limit |
| PR #167 / `main` | Open, CI red (`Soma Contracts` fail), 7 unresolved CodeRabbit findings | Merged into `main` (`2af3049`); all CI green on both the PR and post-merge `main` |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test -p soma-auth --lib` | all pass | 98 passed, 0 failed | pass |
| `cargo test -p soma-auth --doc` | all pass (after doctest fix) | 0 passed, 0 failed, 1 ignored | pass |
| `cargo test --workspace` | all pass | 1 unrelated pre-existing flaky failure (`soma-codemode`), confirmed passing in isolation and untouched by this diff | pass (with documented exception) |
| `cargo clippy --workspace --all-targets -- -D warnings` | no warnings | clean | pass |
| `cargo fmt --all -- --check` | no diff | clean | pass |
| `cargo xtask patterns` | no real (non-`.cargo/`) failures | clean after `config_providers.rs` split | pass |
| `cargo xtask check-coupled-files origin/main HEAD` | pass | `Coupled-file check passed (origin/main..HEAD)` | pass |
| `cargo xtask check-docs` | generated docs current | `generated docs are current` | pass |
| `gh pr checks 167` | all pass | 24/24 checks pass, including `CI Gate`, `Soma Contracts`, `CodeRabbit` | pass |
| `gh pr view 167 --json mergeable,mergeStateStatus` | `MERGEABLE`/`CLEAN` | `MERGEABLE`/`CLEAN` | pass |
| `gh run view <CI run on main>` | success | `CI: success` (plus `MSRV: success`, `MCP Conformance: success`) | pass |

## Risks and Rollback

Low risk: all changes are either (a) narrowly-scoped bug fixes with new regression test coverage (callback-path normalization, `Default` impls, HTTP Basic auth), (b) pure dead-code removal, or (c) a mechanical file split with no behavior change. The Authelia HTTP-Basic-auth change is the only one with real external-system dependency (it assumes the operator's Authelia client is configured for `client_secret_basic`, which is Authelia's own documented default for confidential clients not explicitly overridden to `client_secret_post`). If an operator's Authelia client is unusually configured for `client_secret_post`, Authelia login would start failing post-deploy; rollback is `git revert 6e2a5f3` (isolated to this commit) or reverting just the `oidc.rs`/`authelia.rs` hunks, since Google is entirely unaffected (still defaults to `ClientSecretPost`). No database migrations, no data changes.

## Next Steps

- No unfinished work from this session — PR #167 is merged, all CI is green on both the PR and `main`, and all 7 CodeRabbit findings are resolved with regression tests.
- **Follow-on (not started, low priority)**: clean up the merged `claude/oauth-provider-support-f427c9` worktree/branch (local `git worktree remove` + `git branch -d` + `git push origin --delete`) once this worktree is no longer in use — deferred because this session ran from inside it.
- **Follow-on (not started, out of scope)**: the many `worktree-agent-*`/`worktree-wf_*`/stale `refactor/pr*` branches observed in `git branch -a` were not investigated for merge/staleness status and may be worth a dedicated cleanup pass in a future session.
- Immediate next command if resuming cleanup: `cd /home/jmagar/workspace/soma && git worktree remove .claude/worktrees/oauth-provider-support-f427c9 && git branch -d claude/oauth-provider-support-f427c9 && git push origin --delete claude/oauth-provider-support-f427c9` (only after confirming no other process still needs this worktree).
