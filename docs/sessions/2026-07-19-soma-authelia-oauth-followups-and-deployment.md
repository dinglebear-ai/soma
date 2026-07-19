---
date: 2026-07-19 00:40:33 EST
repo: git@github.com:jmagar/soma.git
branch: main
head: b8e199eec87f4ef113e9ee02f7aaf15ae744b1df
plan: docs/superpowers/plans/2026-07-18-soma-oauth-provider-config.md
working directory: /home/jmagar/workspace/soma
worktree: /home/jmagar/workspace/soma
pr: "#167 feat(soma-auth): OAuth provider trait — add Authelia + GitHub support (merged) — https://github.com/jmagar/soma/pull/167"
beads: rmcp-template-2sl4, rmcp-template-1ge3, rmcp-template-mkag, rmcp-template-vr2l, rmcp-template-ci7c, rmcp-template-jzbe, rmcp-template-e2q1, rmcp-template-sqn7, rmcp-template-avvs
---

# Soma Authelia OAuth follow-ups and deployment

## User Request

Address the review comments for the OAuth-provider PR, execute three named Beads sequentially, confirm whether Authelia and GitHub can now act as OAuth providers, refresh stale documentation, and configure the live Soma deployment to use Authelia instead of Google.

## Session Overview

Completed the three requested Beads, applied and re-reviewed the OAuth follow-ups, refreshed operator documentation, and fixed a Windows executable lookup defect. Configured a confidential Authelia OIDC client on `squirts`, stored the Soma client secret only in protected runtime configuration, deployed the PR build, found and fixed a missing Axum `ConnectInfo` integration, and verified Soma now redirects Authelia login requests to the live `auth.tootie.tv` issuer. PR #167 subsequently merged into `main` with the stable CI and MSRV gates green.

## Sequence of Events

1. **Resolved the PR target.** The requested identifier `451` did not match the active Soma feature branch; live GitHub inspection identified PR #167 on `claude/oauth-provider-support-f427c9` as the relevant multi-provider OAuth PR.
2. **Executed the three Beads sequentially.** Rewrote the Soma configuration plan (`rmcp-template-2sl4`), deduplicated provider authorization and OIDC token flows (`rmcp-template-1ge3`), then fixed Windows bare-executable resolution (`rmcp-template-mkag`).
3. **Reviewed the changes.** Architecture, goal/spec, and security reviewers checked the combined result. The provider-kind refactor and public/upstream error separation were tracked instead of being folded into the narrow fixes.
4. **Refreshed stale documentation.** A dispatched documentation agent reconciled README, auth/config/operator guidance, generated environment metadata, plugin metadata, and repository instructions with the actual Google/Authelia/GitHub runtime surface.
5. **Configured Authelia.** Inspected `/mnt/appdata/authelia` and `/mnt/compose/authelia` on `squirts`, registered Soma as an OIDC client, backed up the configuration, restarted Authelia, and verified it healthy.
6. **Configured and deployed Soma.** Wrote protected Soma OAuth environment settings and a local Compose override, built the feature branch image, and changed the live deployment from trusted-gateway no-auth mode to mounted Authelia OAuth.
7. **Fixed the live-only failure.** The first `/auth/login` smoke returned 500 because rate-limited auth handlers extracted `ConnectInfo<SocketAddr>` but the shared server did not inject it. Added peer-address injection to both server paths, tested it, rebuilt, and redeployed.
8. **Closed the PR and maintenance loop.** CodeRabbit follow-ups were applied, PR #167 merged with all required checks green, the completed Windows portability follow-up was closed, and a new Bead was opened for the remaining typed setup/doctor/config plan.

## Key Findings

- `crates/shared/http-server/src/server.rs:81` and `:102` now call `into_make_service_with_connect_info::<SocketAddr>()`; without this, live OAuth login handlers failed before redirecting because their rate limiter requires a peer address.
- The authoritative Authelia issuer is `https://auth.tootie.tv`; the similarly named `authelia.tootie.tv` endpoint was a stale SWAG surface and was not used.
- `crates/soma/config/src/env_registry.rs:173-254` exposes the live Authelia, GitHub, callback, scope, and default-provider variables, but typed TOML, `soma setup`, `soma doctor`, and guided plugin options remain Google-oriented. Runtime works because bootstrap reads these raw variables through `soma-auth`.
- Authelia's confidential-client default is `client_secret_basic`; the final provider code documents and uses that behavior (`crates/shared/auth/src/authelia.rs:334`, `crates/shared/auth/src/oidc.rs:23`) while Google retains `client_secret_post`.
- The Windows doctor failure was specifically bare `soma` lookup: Cargo produces `soma.exe`, while the old PATH check did not append the platform executable suffix. The prior test used `cmd.exe` explicitly and missed that case.

## Technical Decisions

- Shared authorization URL construction and Google/Authelia OIDC exchange/refresh logic were centralized; GitHub remained separate because its token error shape and user/email lookup flow differ materially.
- The cross-cutting `ProviderKind` migration was split into `rmcp-template-vr2l` rather than mixed with a narrow deduplication change that touches persisted provider strings and public request fields.
- The Authelia client uses authorization-code plus refresh-token grants, PKCE-compatible code responses, `openid profile email offline_access`, two-factor authorization, an implicit consent policy, and the exact callback `https://soma.dinglebear.ai/auth/authelia/callback`.
- The plaintext client secret is stored only in `/home/jmagar/.soma/.env` with mode `0600`; Authelia stores its hash. The session log intentionally contains neither value.
- Live deployment-specific overrides live in `/home/jmagar/.soma/docker-compose.oauth.yml` rather than changing the repository's general Compose file.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.env.example` | — | Document provider runtime variables | `f50885f` |
| modified | `CLAUDE.md` | — | Correct OAuth runtime/config and provider guidance | `f50885f`, `6e2a5f3` |
| modified | `README.md` | — | Document Authelia/GitHub support and variables | `f50885f` |
| modified | `config.soma.toml` | — | Reconcile sample configuration guidance | `f50885f` |
| modified | `config.toml` | — | Reconcile sample configuration guidance | `f50885f` |
| modified | `apps/soma/tests/doctor_cli.rs` | — | Exercise the exact Cargo executable path | `81ef13d` |
| modified | `crates/shared/auth/src/authelia.rs` | — | Use shared OIDC flow and final Authelia authentication semantics | `b173866`, `6e2a5f3` |
| modified | `crates/shared/auth/src/authorize.rs` | — | Update provider-aware authorization documentation/behavior | `f50885f`, `6e2a5f3` |
| modified | `crates/shared/auth/src/config.rs` | — | Normalize and validate provider configuration | `f50885f`, `6e2a5f3` |
| created | `crates/shared/auth/src/config_providers.rs` | — | Keep provider config/default logic below the module-size gate | `6e2a5f3` |
| modified | `crates/shared/auth/src/github.rs` | — | Use shared authorization URL helper and preserve GitHub-specific exchange | `b173866`, `6e2a5f3` |
| modified | `crates/shared/auth/src/google.rs` | — | Use shared authorization and OIDC helpers | `b173866`, `6e2a5f3` |
| modified | `crates/shared/auth/src/middleware.rs` | — | Refresh multi-provider guidance | `f50885f` |
| modified | `crates/shared/auth/src/oauth_provider.rs` | — | Simplify provider authorization request contract | `6e2a5f3` |
| modified | `crates/shared/auth/src/oidc.rs` | — | Centralize Google/Authelia exchange and refresh | `b173866`, `6e2a5f3` |
| modified | `crates/shared/auth/src/provider_http.rs` | — | Centralize base authorization URL/error context handling | `b173866`, `6e2a5f3` |
| modified | `crates/shared/http-server/src/server.rs` | — | Inject `SocketAddr` peer `ConnectInfo` | `31087ff` |
| modified | `crates/shared/http-server/src/server_tests.rs` | — | Add live-server peer-address regression coverage | `31087ff` |
| modified | `crates/soma/cli/src/doctor/checks.rs` | — | Resolve bare Windows executable names and remove dead fallback | `81ef13d`, `6e2a5f3` |
| modified | `crates/soma/cli/src/doctor/checks_tests.rs` | — | Cover platform executable suffix lookup | `81ef13d` |
| modified | `crates/soma/config/src/env_registry.rs` | — | Register Authelia/GitHub/default-provider runtime variables | `f50885f` |
| modified | `crates/soma/config/src/env_registry_tests.rs` | — | Test the expanded environment registry | `f50885f` |
| modified | `docs/AUTH.md` | — | Add operator setup, callbacks, scopes, and provider behavior | `f50885f` |
| modified | `docs/CI.md` | — | Keep coupled CI documentation synchronized | `f50885f` |
| modified | `docs/CONFIG.md` | — | Distinguish raw runtime variables from typed config | `f50885f` |
| modified | `docs/ENV.md` | — | Regenerate the provider environment reference | `f50885f`, `6e2a5f3` |
| modified | `docs/PATTERNS.md` | — | Refresh generated pattern documentation | `f50885f` |
| modified | `docs/PLUGINS.md` | — | Keep plugin contract documentation synchronized | `6e2a5f3` |
| modified | `docs/RMCP_README_GUIDE.md` | — | Update downstream OAuth documentation guidance | `f50885f` |
| modified | `docs/superpowers/plans/2026-07-18-soma-oauth-provider-config.md` | — | Rewrite the setup/doctor/config plan against current crates | `8ec8248`, `51aef47` |
| modified | `packages/soma-rmcp/README.md` | — | Refresh packaged operator guidance | `f50885f` |
| modified | `plugins/README.md` | — | Refresh plugin documentation | `f50885f` |
| modified | `plugins/soma/.claude-plugin/plugin.json` | — | Refresh generated environment metadata; remain versionless | `f50885f` |
| modified | `plugins/soma/gemini-extension.json` | — | Refresh generated environment metadata; remain versionless | `f50885f` |
| modified | `scripts/README.md` | — | Keep generation workflow documentation synchronized | `6e2a5f3` |
| modified | `scripts/generate-docs.py` | — | Generate consistent provider/default metadata | `f50885f`, `6e2a5f3` |
| modified | `/mnt/appdata/authelia/configuration.yml` | — | Register the confidential Soma OIDC client | live host inspection |
| created | `/mnt/appdata/authelia/configuration.yml.bak.soma-oidc.20260718_174614` | — | Pre-change Authelia configuration backup | live host inspection |
| modified | `/home/jmagar/.soma/.env` | — | Select Authelia OAuth and protected auth storage paths | mode `0600`; live deployment |
| created | `/home/jmagar/.soma/.env.bak.authelia.20260718_174714` | — | Pre-change Soma environment backup | mode `0600` |
| created | `/home/jmagar/.soma/docker-compose.oauth.yml` | — | Disable trusted-gateway no-auth and set the container home | mode `0600`; live deployment |
| created | `docs/sessions/2026-07-19-soma-authelia-oauth-followups-and-deployment.md` | — | Record this continuation and live rollout | this commit |

## Beads Activity

| bead | title | actions | final status | why it mattered |
|---|---|---|---|---|
| `rmcp-template-2sl4` | Rewrite Soma OAuth provider config plan | claimed, investigated, edited, commented, closed | closed | Replaced stale pre-refactor file targets with current config/domain/CLI paths. |
| `rmcp-template-1ge3` | Deduplicate OAuth provider flows | claimed, implemented, commented, closed | closed | Removed repeated authorization and Google/Authelia OIDC logic without conflating GitHub. |
| `rmcp-template-mkag` | Windows doctor reports an extra issue | claimed, fixed, verified, commented, closed | closed | Restored bare executable discovery on Windows; self-hosted Windows proof passed. |
| `rmcp-template-vr2l` | Introduce typed `ProviderKind` | created as a scoped follow-up | open | Preserves the wider persisted-data/API migration for a deliberate design pass. |
| `rmcp-template-ci7c` | Separate public OAuth errors from upstream diagnostics | created during security review | open | Tracks a pre-existing risk that upstream response snippets can reach public error JSON. |
| `rmcp-template-jzbe` | Make env-template test portable without `HOME` | created from Windows CI; closed during maintenance | closed | Commit `3003a7e` is on `main`, the focused test passes, and PR #166's Windows/CI gates are green. |
| `rmcp-template-e2q1` | Refresh OAuth provider operator documentation | created, claimed, implemented, closed by docs agent | closed | Reconciled operator and generated docs with the runtime provider surface. |
| `rmcp-template-sqn7` | Inject `ConnectInfo` into live HTTP routes | created from live smoke, fixed, verified, closed | closed | Removed the 500 response that blocked the Authelia redirect. |
| `rmcp-template-avvs` | Execute Soma OAuth setup/doctor/config provider plan | created during session maintenance | open | Tracks the remaining product-support UX parity work after runtime support merged. |

## Repository Maintenance

- **Plans:** `find docs/plans -maxdepth 2 -type f` returned no plan files, so nothing was moved. The applicable unchecked plan is under `docs/superpowers/plans/2026-07-18-soma-oauth-provider-config.md`; it remains active and is now tracked by `rmcp-template-avvs`.
- **Beads:** Read all nine relevant issues before mutation. Closed `rmcp-template-jzbe` only after confirming `3003a7e` is an ancestor of `origin/main`, running its focused test successfully, and observing green PR #166 Windows and aggregate gates. Created `rmcp-template-avvs`, then pushed Beads state with `bd dolt push`.
- **Worktrees/branches:** PR #167's feature tip is an ancestor of `origin/main`, but its worktree contains untracked Cargo cache files and was not removed. The Incus worktree is merged but has extensive untracked build artifacts; the trace and cortex worktrees back open PRs #168 and #170; the detached Codex worktree has an untracked plan; `marketplace-no-mcp` remains protected and its registration is currently broken. No worktree or branch was deleted.
- **Stale docs:** The dispatched documentation pass updated the relevant runtime and operator surfaces in `f50885f`, with final CodeRabbit-coupled corrections in `6e2a5f3`. No additional stale OAuth document was identified after `main` was refreshed.
- **Transparency:** The injected Claude transcript candidate belonged to an unrelated architecture-refactor session, so it was inspected but excluded as evidence; no current-session transcript path was available. Main gained untracked `.cargo` cache files during verification and they were preserved rather than swept into this docs-only commit.

## Tools and Skills Used

- **Skills/plugins:** `vibin:gh-pr` for review-comment handling, `lavra:lavra-work` for the three requested Beads in sequence, and `vibin:save-to-md` for this evidence-driven closeout.
- **Agents:** Lavra architecture, goal/spec, and security reviewers checked the combined implementation; the explicitly dispatched `oauth_docs_refresh` agent performed the stale-doc pass and committed `f50885f`.
- **Shell/file tools:** `git`, `gh`, `bd`, `cargo`, `rg`, `sed`, `curl`, `ssh`, Docker/Compose, and patch-based file editing were used for repository inspection, implementation, CI proof, deployment, and documentation.
- **Remote operations:** SSH to `squirts` inspected and updated Authelia, while local Docker/Compose rebuilt and restarted Soma. No browser automation or domain MCP server was needed.
- **Issues/workarounds:** The local Cargo wrapper emitted a very large metadata stream during a focused maintenance test; rerunning with `SOLDR_BYPASS=1 CARGO_TIMINGS=0` produced a clean exit. A zsh unmatched glob during filename collision checking was benign and the target was independently confirmed absent.

## Commands Executed

| command | result |
|---|---|
| `cargo test --workspace` | passed after the requested Bead implementations |
| `cargo clippy --workspace --all-targets -- -D warnings` | passed |
| `cargo xtask patterns` and release/version checks | passed before feature push |
| Windows CI doctor tests | `doctor_cli` 2/2 and `soma-cli` 87/87 passed |
| `cargo test -p soma-config` plus docs-generation checks | 28 tests passed; docs, patterns, version, and README guide checks passed |
| `cargo test -p soma-http-server` | 22 unit tests and 1 doc test passed |
| `curl -I 'https://soma.dinglebear.ai/auth/login?provider=authelia'` | HTTP 302 to `https://auth.tootie.tv/api/oidc/authorization` |
| `ssh squirts 'docker ps --filter name=authelia ...'` | Authelia healthy; MariaDB and Redis up |
| `gh pr view 167 ...` | PR merged; CI Gate and MSRV Gate succeeded |
| `git merge-base --is-ancestor <session-commit> origin/main` | all seven follow-up commits confirmed on `main` |
| `SOLDR_BYPASS=1 CARGO_TIMINGS=0 cargo test -q -p soma-provider-adapters expand_env_templates_substitutes_multiple_variables` | passed with exit status 0 |
| `bd dolt push` | tracker updates pushed successfully |

## Errors Encountered

- **Wrong PR identifier:** the requested `451` did not identify the branch's live PR. Resolved by inspecting the active worktree and GitHub head ref; the relevant PR was #167.
- **Windows doctor mismatch:** bare `soma` was not resolved as `soma.exe`. Fixed with platform suffix resolution and verified on the self-hosted Windows runner.
- **Unrelated Windows `HOME` failure:** the first authoritative Windows run passed the doctor fix, then failed a provider-adapters test that assumed `HOME`. Tracked as `rmcp-template-jzbe`; later fixed in `3003a7e`, merged, retested, and closed during this maintenance pass.
- **First Soma OAuth deployment failed to persist auth state:** default paths resolved beneath read-only `/home/soma/.soma`. Resolved with explicit `/data/auth.db` and `/data/auth-jwt.pem` runtime paths.
- **First live login returned 500:** missing Axum peer `ConnectInfo` prevented auth rate limiting. Fixed in `31087ff`, covered by regression tests, rebuilt, and verified with a 302 redirect.
- **Authelia restart emitted one transient database-close error:** it occurred during shutdown; the restarted Authelia container became healthy and remained so in the closeout check.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Soma OAuth providers | Google-only implementation | Google, Authelia OIDC, and GitHub OAuth2 runtime providers |
| Live provider | trusted-gateway no-auth / Google-oriented environment | mounted OAuth with Authelia as the configured default provider |
| Authelia integration | no Soma OIDC client | confidential `soma` client with exact callback and refresh scope |
| OAuth login request | live 500 from missing peer address | 302 to Authelia authorization with PKCE and `offline_access` |
| Shared provider code | repeated URL and Google/Authelia OIDC flow | centralized helpers, with GitHub-specific behavior retained |
| Windows doctor | bare `soma` could be reported missing | resolves the platform executable suffix |
| Operator docs | Google-focused/stale | describe runtime Authelia/GitHub variables, callbacks, selection, and refresh behavior |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test --workspace` | all workspace tests pass | passed | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | no warnings | passed | pass |
| focused Windows CI | bare executable and doctor totals pass | 2/2 `doctor_cli`; 87/87 `soma-cli` | pass |
| documentation-agent checks | docs/generated metadata stay synchronized | all focused checks passed | pass |
| `cargo test -p soma-http-server` | regression and existing server tests pass | 22 unit + 1 doc test passed | pass |
| Soma health | public service healthy | `{"status":"ok"}` | pass |
| Authelia login smoke | redirect to registered issuer/callback | HTTP 302 with `client_id=soma`, exact callback, PKCE, and `offline_access` | pass |
| Authelia container check | healthy after restart | `authelia Up ... (healthy)` | pass |
| PR #167 checks | required gates green | CI Gate and MSRV Gate succeeded; PR merged | pass |
| provider-adapters portability test | no `HOME` assumption failure | exit status 0 | pass |

## Risks and Rollback

- The Authelia configuration backup remains at `/mnt/appdata/authelia/configuration.yml.bak.soma-oidc.20260718_174614`; the prior Soma environment remains at `/home/jmagar/.soma/.env.bak.authelia.20260718_174714`. Restore those files and restart the respective Compose projects to roll back configuration.
- The temporary Docker tag `soma:pre-authelia-20260718-174946` created during deployment was not present during closeout, so it is not a valid current rollback point. Rebuild or deploy a known earlier git revision if code rollback is required.
- Only the redirect leg was exercised automatically. Completing the browser login still requires the user's Authelia credentials and second factor; no session cookie or token was captured by the agent.
- `rmcp-template-ci7c` remains open because provider error display text can still mix operator diagnostics with public OAuth errors.

## Decisions Not Taken

- GitHub credentials were not configured on the live deployment because the request was specifically to replace Google with Authelia; GitHub runtime support remains available.
- The typed setup/doctor/TOML plan was not implemented during the credential rollout. Runtime raw-env support was sufficient for the requested deployment, and the remaining UX work is isolated in `rmcp-template-avvs`.
- Merged or apparently stale worktrees were not deleted when dirty, attached to an open PR, detached with untracked work, protected, or registered ambiguously.
- The stale `authelia.tootie.tv` endpoint was not repaired because the verified issuer is `auth.tootie.tv` and proxy cleanup was outside the requested OAuth credential scope.

## References

- PR #167: https://github.com/jmagar/soma/pull/167
- PR #166: https://github.com/jmagar/soma/pull/166
- Initial implementation log: `docs/sessions/2026-07-18-oauth-provider-support-authelia-github.md`
- Active product-support plan: `docs/superpowers/plans/2026-07-18-soma-oauth-provider-config.md`
- Operator documentation: `docs/AUTH.md`, `docs/CONFIG.md`, and `docs/ENV.md`

## Open Questions

- Has the user completed an interactive Authelia login and consent flow at `https://soma.dinglebear.ai/auth/login?provider=authelia`? The redirect is proven, but user-authenticated completion was intentionally not automated.
- When should `rmcp-template-avvs` be scheduled to add typed config, setup persistence, doctor provider labels, and guided plugin options?

## Next Steps

1. Open `https://soma.dinglebear.ai/auth/login?provider=authelia`, complete Authelia two-factor authentication and consent, then confirm the MCP client receives and refreshes Soma tokens.
2. Execute `rmcp-template-avvs` using `docs/superpowers/plans/2026-07-18-soma-oauth-provider-config.md` for product-support UX parity.
3. Address the security follow-up `rmcp-template-ci7c`; treat `rmcp-template-vr2l` as lower-priority architectural cleanup.
4. If GitHub should also be enabled live, create a GitHub OAuth App with callback `https://soma.dinglebear.ai/auth/github/callback`, then add its client credentials without removing the Authelia provider.
