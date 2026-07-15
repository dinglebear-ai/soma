---
date: 2026-07-14 21:54:43 EST
repo: git@github.com:jmagar/soma.git
branch: main
head: 9157e0a
session id: b5327bd7-a631-410d-8f9d-c612d9b1e4a7
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-soma/b5327bd7-a631-410d-8f9d-c612d9b1e4a7.jsonl
working directory: /home/jmagar/workspace/soma
beads: rmcp-template-yf5g (closed), rmcp-template-7nyf (open, follow-up)
---

# Port PR #99's provider drop-in CLI onto main, ship as PR #130, close the loop

## User Request

Started as a factual question ("have we not merged pr 99?"), escalated through discovery of what PR #99 actually contained, then an explicit ask to begin porting it onto current `main`. Ended with merging the resulting PR and cleaning up the original.

## Session Overview

PR #99 ("Add provider drop-in CLI workflow") had been open on a stale base that predated a hard-break crate rename (`rtemplate-*` → `soma-*`). Investigated why it hadn't merged, discovered its base was 64 commits behind `main` and that a naive rebase would collide with functionality `main` had independently shipped in the meantime (PR #117's own `soma providers validate|inspect|test`). Ported the PR's actual value — a non-executing `soma providers list|lint|status` inspection CLI — onto current `main` as a fresh PR (#130), then hardened it through 10 rounds of automated adversarial code review (Codex + CodeRabbit bots), each round catching a real gap between what the lint tool checked and what the live registry/HTTP router/CLI parser actually enforce. Merged PR #130, closed PR #99 as superseded, filed a follow-up bead for the two features deliberately left out, and deleted all now-stale branches/worktrees.

## Sequence of Events

1. Confirmed PR #99 was genuinely unmerged (not squash-merged) via `gh api` (`merged: false`) and a non-empty branch diff — the `providers/` directory the user spotted on `main` came from a separate, already-merged PR #117, not #99.
2. Explained the distinction: #117 shipped provider *execution* (discovery + dispatch through the live registry); #99 adds non-executing *inspection* tooling layered on top, and used old pre-rename crate names, meaning it needed a substantial rebase before it could land.
3. Created bead `rmcp-template-yf5g`, set up an isolated worktree via `vibin:worktree-setup` at `.worktrees/codex-provider-drop-in-ux`, and attempted a mechanical rebase — hit real structural conflicts within the first commit, revealing that `main` had independently shipped its own `soma providers validate|inspect|test` (via #117), colliding with #99's own `providers` subcommand naming.
4. Asked the user how to reconcile the collision; user chose to fold #99's non-executing commands into the existing `ProviderCommand` enum, renaming the colliding `validate` to `lint`.
5. Aborted the mechanical rebase after determining most of #99's raw diff was stale drift already superseded independently on `main` (action registry, provider snapshot/reports split, package-generate, several test-file splits). Squash-ported only the genuinely new value onto a fresh branch off `origin/main`.
6. Deliberately scoped out two of #99's later features — a structured `providers/{tools,prompts,resources}` directory layout with its own trust-boundary contract, and markdown-file-as-MCP-prompt support — as separate, security-sensitive follow-up work.
7. Verified the ported feature end-to-end (full workspace build/test/clippy/fmt, live smoke tests against `examples/providers/`), then asked the user how to land it; opened a fresh PR (#130) rather than force-pushing over #99's stale history.
8. Closed bead `rmcp-template-yf5g`.
9. Worked through 10 rounds of CI-driven adversarial review on PR #130 (see Key Findings), each round: verified the finding against real code, fixed it with a regression test and a live smoke test, ran the full local-equivalent of CI, pushed, replied to the review thread, resolved it, and updated a persistent `bd remember` note capturing the lesson.
10. Fixed a genuine CI infrastructure failure mid-cycle: `cargo xtask patterns`' 700-effective-line module hard limit, then a second, separate `check-test-siblings` sub-check that the first fix's new files didn't satisfy.
11. Confirmed all 14 review threads resolved and full CI green (including the previously-failing "Soma Contracts" job and a slow self-hosted Windows build).
12. User asked whether the PR had merged; it hadn't — asked for merge method, user chose squash; merged as `d71fcfe`.
13. User asked to clean up the merged worktree/branch; removed the worktree and deleted the merged branches (local `provider-drop-in-ux-port`, remote `codex/provider-drop-in-ux-v2`).
14. User asked to also clean up the *original* PR #99 branch, asserting "we have everything in 130." Corrected that assumption (the two deferred features were not ported), then — per the user's choice — filed bead `rmcp-template-7nyf` capturing both deferred features with full context, closed PR #99 with an explanatory comment linking the bead, and deleted its branch (local + remote).

## Key Findings

Ten adversarial-review rounds on PR #130, each surfacing a real gap between "the lint tool says this manifest is fine" and "the live system actually accepts/routes it":

1. **Python execution safety** (`crates/soma-service/src/providers/filesystem.rs`) — `inspect()` called the same loader the live registry uses; for `.py` files that spawns a sidecar which `exec_module()`s the file, so "non-executing" `list/lint/status` was silently executing Python. Fixed by adding `ProviderFileInspectionStatus::Skipped` and never calling `load_catalog` for `.py`.
2. **Missing structural validation** — deserializing successfully isn't the same as passing `validate_provider_manifest()` (duplicate tool names, reserved CLI commands).
3. **Missing schema compilation** — added `jsonschema::validator_for()` checks on each tool's `input_schema`/`output_schema`.
4. **CodeRabbit quality pass** — stronger `assert_eq!` test assertions, a missing `stdout` flush before `process::exit(1)` in the lint-failure path, and de-duplicated directory-scan filter logic by reusing `provider_paths()`.
5. **Missing cross-file uniqueness** (`filesystem_uniqueness.rs`, new) — two individually-valid files can still collide once loaded together (duplicate provider/action/REST route/CLI command+alias/MCP primitive name), mirroring `provider_registry::{provider_map, build_snapshot}`.
6. **Missing built-in-provider seeding** — a single drop-in file could collide with the built-in `static-rust` provider (`StaticRustProvider::catalog_static()`) with zero other drop-in files present.
7. **CLI flag-parsing footgun** — `soma providers lint --dir --json` silently treated `--json` as the directory path, hit "directory doesn't exist" (a valid empty result), and exited 0 — a lint step that never ran, passing silently in CI.
8. **CI module-size limit** (`cargo xtask patterns`) — split `filesystem.rs` (783 effective lines) into `filesystem.rs` + `filesystem_uniqueness.rs`, and `lib.rs` (710) into `lib.rs` + `provider_command.rs`; a follow-up push then hit a *separate* `check-test-siblings` sub-check requiring `_tests.rs` siblings for the two new files.
9. **Manifest-schema validation gap** — `validate_provider_manifest()` never checked schema-only constraints like `rest.path`'s `^/v1(/.*)?$` pattern; the HTTP router only mounts custom routes under `/v1/{*path}` (`crates/soma/src/routes.rs:62`). **Self-caught bug**: the first fix attempt validated a re-serialized typed struct instead of the raw pre-deserialization `Value`, turning every omitted `Option` field into explicit JSON `null` and false-positiving all three of my own example providers — caught by re-running lint against `examples/providers/` before pushing. Also found a genuine bug in my own `hello-openapi.json` (`rest.path` was `/hello`, truly unreachable) and fixed it.
10. **Router-level shadowing invisible to the live registry** — `/v1/capabilities`, `/v1/providers`, `/v1/tools/{action}` are hardcoded in `routes.rs` with no `ACTION_SPECS` entry, so a colliding provider passes `soma providers validate` too, not just lint. Reserved explicitly.
11. **Reserved-word list drift** — the CLI parser's `reserved_cli_command()` and the manifest validator's `RESERVED_CLI_COMMANDS` had silently diverged (`package` was missing from the latter).
12. **Method-independent route shadowing** — round 10's fix reserved infra routes as exact `(method, path)` tuples, but Axum resolves literal routes by path *alone*; a mismatched method still 405s rather than falling through to the dynamic dispatcher. Broadened to all six literal routes, method-independent.
13. **Schema/struct drift, flip side of #9** — `docs/contracts/provider-manifest.schema.json`'s `restOverlay` definition was itself stale relative to the Rust `RestOverlay` struct (missing `path_params`/`query_params`/`request_body_schema`, which the struct has always accepted). This was a pre-existing bug, invisible until round 9 activated schema validation on this path for the first time. Fixed the schema, not the manifests.

## Technical Decisions

- **Squash-port instead of commit-by-commit rebase**: PR #99's 16 commits mostly represented drift already superseded independently on `main` (action registry moved crates, provider snapshot/reports split differently, package-generate implemented inline). Replaying all 16 would have meant resolving the same conflicts repeatedly; porting only the final, genuinely-new diff was far less redundant.
- **Fold `ProvidersCommand` into `ProviderCommand` rather than namespace separately**: kept one CLI enum, renaming the colliding non-executing `validate` to `lint`, so `soma providers` has one coherent mental model (`validate/inspect/test` = executing; `list/lint/status` = non-executing) instead of two parallel command families.
- **New branch + new PR instead of force-pushing over #99**: the ported feature's architecture, naming, and scope differed enough from #99's original diff that force-pushing would have made the PR's history unrecognizable against what would have been reviewed there.
- **Defer the structured `providers/{tools,prompts,resources}` layout and markdown-as-MCP-prompts**: both are materially larger and one is security-sensitive (path-traversal/trust-boundary enforcement); bundling either into a CLI-focused PR would have hidden a change that deserves its own dedicated review.
- **Reuse `provider_paths()`/`load_catalog`/`validate_provider_manifest` rather than reimplement**: every fix that could reuse existing live-registry validation logic did so (e.g., `load_catalog_value()` as a sibling of `load_catalog()`, not a parallel reimplementation), to avoid the exact "two copies of one fact drift apart" bug found in round 11.
- **Reserve infrastructure routes as a path list, not the exact-tuple map used for genuine provider-vs-provider collisions**: the two failure modes are different — Axum's dynamic dispatcher legitimately allows different methods on the same custom path (method-keyed), but literal infrastructure routes shadow by path alone regardless of method.

## Files Changed

| status | path | purpose | evidence |
|---|---|---|---|
| modified | `CHANGELOG.md` | Document the new `providers list/lint/status` actions | PR #130 diff |
| modified | `CLAUDE.md` | Document the two `soma providers` CLI surfaces (executing vs non-executing) | PR #130 diff |
| modified | `README.md` | Add usage examples and troubleshooting notes for the new commands | PR #130 diff |
| modified | `crates/soma-cli/src/cli_tests.rs` | Parse tests for `list/lint/status`, `--dir` flag rejection | PR #130 diff |
| modified | `crates/soma-cli/src/lib.rs` | Wire `ProviderCommand` variants; later split out `provider_command.rs` for module-size | commits b141eab, 8f00cd8 |
| created | `crates/soma-cli/src/provider_command.rs` | `ProviderCommand` enum + `validate/inspect/test` dispatch (extracted from `lib.rs`) | commit 8f00cd8 |
| created | `crates/soma-cli/src/provider_command_tests.rs` | Direct unit tests for the extracted module | commit b82ec52 |
| created | `crates/soma-cli/src/providers.rs` | `list/lint/status` non-executing CLI report formatting | commit b141eab |
| created | `crates/soma-cli/src/providers_tests.rs` | Tests for report formatting | commit b141eab |
| modified | `crates/soma-contracts/src/provider_validation.rs` | Added `"package"` to `RESERVED_CLI_COMMANDS`, cross-referenced with the CLI parser | commit d2d3b94 |
| modified | `crates/soma-contracts/src/provider_validation_tests.rs` | Regression test for the reserved-command fix | commit d2d3b94 |
| modified | `crates/soma-service/src/providers/filesystem.rs` | `FileProviderSource::inspect()`, `load_catalog_value()`, per-file semantic validation chain | commits b141eab, 38229b7, 1a35073, 2ec93df, 8f00cd8 |
| modified | `crates/soma-service/src/providers/filesystem_tests.rs` | Regression tests for every round's finding | multiple commits |
| created | `crates/soma-service/src/providers/filesystem_uniqueness.rs` | Cross-provider/directory-wide uniqueness checks, built-in + infra route reservation | commits 76b7f87, 0b4b728, 06b4371, d04729a |
| created | `crates/soma-service/src/providers/filesystem_uniqueness_tests.rs` | Direct unit tests for the uniqueness module | commits b82ec52, 76b7f87, 06b4371, d04729a |
| created | `crates/soma/tests/provider_cli.rs` | End-to-end CLI integration tests against the built binary | commit b141eab |
| created | `docs/PROVIDERS.md` | User-facing documentation for both `soma providers` surfaces | commits b141eab, multiple updates |
| modified | `docs/contracts/examples/provider-manifests/openapi.valid.json` | Exercise `path_params`/`query_params`/`request_body_schema` in the contract fixture | commit d06d972 |
| modified | `docs/contracts/provider-manifest.schema.json` | Fixed stale `restOverlay` schema definition | commit d06d972 |
| created | `examples/providers/README.md` | Usage note for the example providers | commit b141eab |
| created | `examples/providers/hello-ai-sdk.ts` | Example AI SDK provider | commit b141eab |
| created | `examples/providers/hello-openapi.json` | Example OpenAPI provider (fixed `rest.path` bug in round 9) | commits b141eab, 2ec93df |
| created | `examples/providers/hello-static.json` | Example static-Rust provider | commit b141eab |
| created | `examples/providers/openapi/hello.yaml` | Example upstream OpenAPI spec | commit b141eab |

## Beads Activity

- **`rmcp-template-yf5g`** — "Rebase PR #99 (provider drop-in CLI) onto renamed soma-* crates." Created and claimed at the start of the porting work; closed once PR #130 was opened, with a close reason noting the two deliberately-deferred features. Mattered because it tracked a non-trivial, multi-step rebase/port decision through to a documented outcome rather than leaving it implicit in commit messages.
- **`rmcp-template-7nyf`** — "Port structured provider layout + markdown-as-MCP-prompts from old PR #99 branch." Created at session end, before deleting the `codex/provider-drop-in-ux` branch, so the two deferred features (structured directory layout with trust-boundary contract; markdown-as-MCP-prompt support) aren't silently lost with the branch. Left open as P2 follow-up work with commit references and file paths for whoever picks it up.

## Repository Maintenance

- **Plans**: `docs/plans/` does not exist in this repo — no plan files to review or move. N/A, confirmed via `ls`.
- **Beads**: Both beads touched this session are accounted for above; no other open beads were reviewed or modified in this session.
- **Worktrees and branches**: Removed `.worktrees/codex-provider-drop-in-ux` via the `vibin:worktree-setup` skill's safe-teardown script (`worktree-rm.sh --delete-branch`), which deleted the local `provider-drop-in-ux-port` branch. Deleted the remote `codex/provider-drop-in-ux-v2` (merged into `main` as `d71fcfe`) and the original `codex/provider-drop-in-ux` (local + remote, PR #99 now closed) only after confirming via `git worktree list` that nothing had it checked out. Left every other worktree/branch untouched — `marketplace-no-mcp` (explicitly protected per `CLAUDE.md`), `claude/codex-app-server-api-4798cc`, `claude/labby-auth-crate-port-aeb44c`, `codex/pr101-review-fixes`, and the `release-please--*` branch all belong to unrelated, still-active work and were out of scope for this session.
- **Stale docs**: Checked for lingering references to `codex/provider-drop-in-ux` or `pull/99` in `README.md`, `CLAUDE.md`, and `docs/` — none found (`grep` returned no matches), so no additional stale-doc cleanup was needed beyond what already landed in PR #130 itself (`docs/PROVIDERS.md`, `CLAUDE.md`, `README.md` updates were part of the shipped feature).
- **Transparency**: All four maintenance checks above were performed with command evidence (`ls docs/plans/`, `bd show` on both beads, `git worktree list` before each deletion, `grep -rln` for stale references) rather than assumed complete.

## Tools and Skills Used

- **Shell commands (`Bash`)**: `git` (status, diff, log, worktree, branch, push), `cargo` (build/test/clippy/fmt, `cargo run -p xtask -- <check>`), `gh` (`pr view/checks/merge/close`, `api` for GraphQL review-thread queries/mutations and REST comment replies), `bd` (beads create/claim/close/show/remember/memories). No failures beyond the ones documented as findings above (which were real bugs, not tool failures).
- **`vibin:worktree-setup` skill**: used to create and safely tear down the isolated worktree for the port work. One friction point: `worktree-new.sh`/`worktree-sync.sh` were not executable at their cached path initially (`permission denied`), worked around by invoking them via `bash <script>` directly.
- **`AskUserQuestion`**: used three times — reconciling the `ProviderCommand`/`ProvidersCommand` naming collision, deciding how to land the ported branch (new PR vs force-push), and deciding merge method for PR #130. All three decisions materially changed the approach taken.
- **`ScheduleWakeup`**: used repeatedly to wait on the self-hosted CI runner between pushes rather than polling synchronously; each wakeup correctly re-checked and either found more work (a new review comment) or confirmed progress.
- **No subagents, browser tools, or external CLIs beyond `gh`/`git`/`cargo`/`bd`** were used this session.

## Commands Executed

| command | result |
|---|---|
| `gh api repos/jmagar/soma/pulls/99 --jq '{state,merged,merged_at}'` | `merged: false` — confirmed PR #99 genuinely unmerged |
| `git diff origin/main...origin/codex/provider-drop-in-ux --stat` | 61 files, ~4357 insertions still outstanding — confirmed non-empty diff |
| `git rebase origin/main` (in worktree) | Conflicts within the first commit — revealed the `ProviderCommand`/`ProvidersCommand` collision |
| `cargo build/test/clippy/fmt --workspace --all-features` (repeated ~14 times, once per fix commit) | All green by the final push |
| `cargo run -p xtask -- patterns` | Initially FAILed (783/710 effective lines); passed after the module split |
| `cargo run -p xtask -- check-test-siblings` | Initially FAILed (2 missing siblings); passed after adding them |
| `cargo run -p xtask -- check-provider-manifest-contract` / `check-schema-docs --check` | Passed after the schema fix (round 13) |
| `./target/debug/soma providers lint --dir ./examples/providers` (repeated after every schema-related fix) | Caught the first broken schema-validation attempt (all 3 examples failed), confirmed the fix, then confirmed the `hello-openapi.json` route bug |
| `gh pr merge 130 --squash --delete-branch=false` | Merged as `d71fcfe` |
| `gh pr close 99 --comment "..."` | Closed with explanatory comment linking the follow-up bead |
| `bash worktree-rm.sh provider-drop-in-ux-port --delete-branch` | Removed worktree + local branch |
| `git push origin --delete codex/provider-drop-in-ux-v2` / `codex/provider-drop-in-ux` | Deleted both merged/superseded remote branches |

## Errors Encountered

- **Broken schema-validation fix (round 9, self-caught before user impact)**: the first attempt validated `serde_json::to_value(&catalog)` (a re-serialized typed struct) against `provider-manifest.schema.json`. Every `#[serde(default)]` field omitted in the original file round-trips through `Option::None` to an explicit JSON `null`, which the schema's `additionalProperties: false` + typed properties rejects. Root cause: validating the wrong representation (post-deserialization) instead of the raw pre-deserialization `Value`. Caught by re-running `soma providers lint` against `examples/providers/` before pushing — all three example providers failed, which shouldn't happen for well-formed manifests. Fixed by adding `load_catalog_value()`, a sibling of `load_catalog()` that stops at the parsed `Value`.
- **`worktree-new.sh`/`worktree-sync.sh` permission denied**: cached skill scripts weren't executable at their path. Worked around with `bash <script>` instead of direct invocation; no underlying script bug.
- **CI module-size and test-sibling gate failures**: not logic errors, but the `cargo xtask patterns` (700-effective-line hard limit) and `check-test-siblings` contract checks failing after the module split. Root cause: two *separate* sub-checks in the same "Soma Contracts" CI job, each needing an independent fix (line-count split, then missing `_tests.rs` siblings for the newly-split files).

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| `soma providers` CLI | Only `validate\|inspect\|test` (executing, via #117) | Adds `list\|lint\|status` (non-executing filesystem inspection), folded into the same `ProviderCommand` enum |
| Non-executing inspection safety | N/A (didn't exist) | Never executes Python, JS/TS handlers, MCP calls, or OpenAPI fetches — verified with a regression test that plants an import-time side effect and confirms it never fires |
| `provider-manifest.schema.json` `restOverlay` | Missing `path_params`/`query_params`/`request_body_schema` (silently rejected by any strict schema validator, though nothing exercised this before) | Matches the actual accepted `RestOverlay` Rust struct |
| `crates/soma-cli/src/lib.rs` / `providers/filesystem.rs` module size | 710 / 783 effective lines (over the CI hard limit) | 573 / 651 effective lines, split into `provider_command.rs` / `filesystem_uniqueness.rs` |
| PR #99 | Open, stale, on pre-rename crate names | Closed, superseded by #130; deferred scope tracked in `rmcp-template-7nyf` |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test --workspace --all-features` (final push) | 0 failures | 0 failures across every crate | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` (final push) | 0 warnings | 0 warnings | pass |
| `cargo fmt --check` (final push) | clean | clean | pass |
| `cargo run -p xtask -- patterns` (final push) | no FAIL lines | `PATTERNS CLEAN` | pass |
| `cargo run -p xtask -- check-test-siblings` (final push) | all siblings present | `all source files have a _tests.rs sibling` | pass |
| `gh pr checks 130` (final check before merge) | all required checks pass | all pass, including "Soma Contracts" and "Build Windows" | pass |
| `gh pr view 130 --json mergeable,mergeStateStatus` | `MERGEABLE` / `CLEAN` | `MERGEABLE` / `CLEAN` | pass |
| GraphQL review-thread query, post-merge | 14/14 resolved | 14/14 resolved | pass |

## Risks and Rollback

- The squash-merge (`d71fcfe`) collapses 14 commits into one on `main`; individual round-by-round history is preserved on GitHub's PR page (`gh pr view 130 --json commits`) but not in `git log` on `main`. Rollback path if a regression surfaces: `git revert d71fcfe` (single clean revert since it's one squashed commit).
- PR #99's branch is deleted; its content is still reachable via the closed PR's diff view and the follow-up bead `rmcp-template-7nyf`, which records the exact commit SHA (`35209ff`) and file paths needed to re-derive the deferred work if `gh` access is ever lost.

## Decisions Not Taken

- **Reusing `provider_registry::{provider_map, build_snapshot}` directly for the directory-wide uniqueness check** (round 5) instead of a purpose-built `DirectoryNamespace`: rejected because those functions are fail-fast (return on the first error for the whole batch) with no per-file attribution, which a lint report needs to say *which* file is the problem.
- **Fixing the live registry's `load_catalog()` to run schema validation for JSON/TS/WASM providers too** (round 9): would have fixed the root asymmetry (only Python providers get schema-validated at load time) but is a behavior change to the live loading path with real backward-compatibility risk for already-deployed providers; scoped to the non-executing inspection path only.
- **Bundling the structured `providers/{tools,prompts,resources}` layout or markdown-as-MCP-prompts into PR #130**: both exist in PR #99's history and could have been ported in the same pass, but were deliberately deferred as separate, larger, and (for the layout) security-sensitive work.

## References

- [PR #99](https://github.com/jmagar/soma/pull/99) — original, closed as superseded
- [PR #130](https://github.com/jmagar/soma/pull/130) — merged, `d71fcfe`
- [PR #117](https://github.com/jmagar/soma/pull/117) — "Finalize Soma provider runtime packaging," the already-merged PR that shipped provider execution and explains the `providers/` directory the user first noticed on `main`

## Open Questions

- None outstanding for the shipped work; the two deferred features are explicitly tracked in `rmcp-template-7nyf`, not left as an implicit gap.

## Next Steps

- **Unfinished / follow-on**: `rmcp-template-7nyf` (P2, open) — port the structured `providers/{tools,prompts,resources}` layout (recommend as its own security-reviewed PR given the trust-boundary/path-traversal contract) and the markdown-as-MCP-prompts feature from PR #99's git history, applying the same crate-rename treatment #130 required.
- **Recommended immediate next command** (when picking up `rmcp-template-7nyf`): `bd show rmcp-template-7nyf` for full context, then inspect the closed PR #99's diff at commit `35209ff` via `gh pr diff 99` or `git show 35209ff` (PR is closed but the commit is still reachable through GitHub's PR view) for the exact source to port.
