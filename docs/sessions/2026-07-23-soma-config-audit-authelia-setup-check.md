---
date: 2026-07-23 16:18:59 EST
repo: git@github.com:dinglebear-ai/soma.git
branch: feat/docs-hardening
head: a6af269bd97c05d47602e96356f103bb3a9a8da9
session id: 0020bc0d-8f3d-473a-ad07-613df24f4fda
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-soma/0020bc0d-8f3d-473a-ad07-613df24f4fda.jsonl
working directory: /home/jmagar/workspace/soma
worktree: /home/jmagar/workspace/soma
beads: rmcp-template-ojgu
---

# Soma configuration audit and Authelia setup-check follow-up

## User Request

Audit the Rust services' canonical `.env` and `config.toml` files, confirm that their credentials and URLs are fully configured, and track any remaining Soma-specific defect. This log is intentionally scoped to the Soma repository and runtime only.

## Session Overview

The Soma runtime configuration was audited without exposing secret values. The canonical files exist at `/home/jmagar/.soma/.env` and `/home/jmagar/.soma/config.toml`; the environment file contains the expected upstream, bearer, public URL, and Authelia settings, while the running `soma:dev` container mounts `/home/jmagar/.soma` at `/data` and reports healthy.

The audit also isolated a setup-check defect: runtime auth supports an Authelia-only OAuth configuration, but the setup validator still unconditionally requires Google client credentials. Bead `rmcp-template-ojgu` was created and remains open. No implementation or Soma source change was made for that bead in this session.

## Sequence of Events

1. Inspected the canonical Soma environment and TOML configuration, checking file permissions, key presence, and configuration structure while redacting all values that could contain credentials.
2. Confirmed the effective auth selectors are OAuth with Authelia as the default provider and that the required Authelia issuer, client ID, client secret, public URL, admin email, upstream URL, and upstream API key are present.
3. Verified the live Docker container is running and healthy, mounts `/home/jmagar/.soma` at `/data`, publishes port `40060`, and returns `{"status":"ok"}` from `/health`.
4. Compared runtime auth validation with setup validation and found that shared auth accepts Authelia-only OAuth while `check_auth` still requires Google fields.
5. Created/read back bead `rmcp-template-ojgu`, left it open because no code or tests were implemented, pushed the Beads state, and performed evidence-based branch/worktree maintenance.

## Key Findings

- `/home/jmagar/.soma/.env` exists with mode `0600`; every observed assignment was non-empty, including `SOMA_API_URL`, `SOMA_API_KEY`, `SOMA_MCP_PUBLIC_URL`, `SOMA_MCP_AUTHELIA_ISSUER_URL`, `SOMA_MCP_AUTHELIA_CLIENT_ID`, `SOMA_MCP_AUTHELIA_CLIENT_SECRET`, and `SOMA_MCP_AUTH_ADMIN_EMAIL`.
- `/home/jmagar/.soma/config.toml` exists with mode `0640` and contains `[soma]`, `[mcp]`, and `[mcp.auth]` sections. Its configured keys include host, port, server name, auth database/key paths, token lifetimes, rate limits, and redirect URI policy.
- The effective non-secret selectors are `SOMA_MCP_AUTH_MODE=oauth` and `SOMA_MCP_AUTH_DEFAULT_PROVIDER=authelia`; Docker inspection showed `/home/jmagar/.soma:/data`, and `curl http://127.0.0.1:40060/health` returned an OK health response.
- `crates/soma/cli/src/setup.rs:273-291` unconditionally requires `google_client_id` and `google_client_secret` whenever OAuth mode is selected.
- `crates/shared/auth/src/config.rs:281-419` validates Google, Authelia, and GitHub independently, while tests at `crates/shared/auth/src/config.rs:788-845` explicitly accept Authelia-only OAuth. This proves the mismatch is in setup validation rather than runtime provider support.

## Technical Decisions

- Configuration evidence records only key presence, file mode, section/key structure, and non-secret selectors. Credential and URL values were not copied into logs.
- The running container and `/health` endpoint were used as the runtime truth source; file presence alone was not treated as proof that Soma was healthy.
- The defect was tracked instead of patched because the Lavra implementation wave had not started before the closeout request, and closing the bead without code and regression coverage would be incorrect.
- Existing dirty files on `feat/docs-hardening` were treated as pre-existing user work and preserved.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| created | `docs/sessions/2026-07-23-soma-config-audit-authelia-setup-check.md` | — | Record the Soma-scoped audit, defect, tracker state, verification, and maintenance | path-limited session-log commit |

The canonical runtime files were inspected but not changed during this closeout. No Soma source file was changed to implement `rmcp-template-ojgu`.

## Beads Activity

| bead | title | actions | final status | why it mattered |
|---|---|---|---|---|
| `rmcp-template-ojgu` | Accept Authelia-only OAuth in setup check | Created earlier in the session; read back during closeout; dependency list checked; Dolt state pushed | open, P2 bug, no dependencies | Tracks the confirmed mismatch between Authelia-capable runtime auth and Google-only setup validation |

The bead was not claimed or closed because no implementation or regression test was completed.

## Repository Maintenance

- **Plans:** `find docs/plans -maxdepth 2 -type f` returned no files, so there was no completed plan to move.
- **Beads:** `bd show rmcp-template-ojgu --json` confirmed the bead is open, and `bd dep list rmcp-template-ojgu --json` returned no dependencies. `bd dolt push` completed successfully.
- **Branches:** removed `fix/next-16.2.11-security` after proving its tip was an ancestor of `origin/main`. Removed `ci/soldr-remove-cachedir` and `ci/soldr-warm-cache-workdir` after GitHub reported PRs 208 and 210 merged and their remotes were gone. Normal `git branch -d` refused because the current topic branch is behind `main`; exact, evidence-backed `git branch -D` deletion was then used.
- **Worktrees:** retained clean worktrees for open PRs 207, 179, 180, and 197. Retained `marketplace-no-mcp` and its registered worktree because `CLAUDE.md` explicitly protects them from broad cleanup. No dirty, active, or unclear worktree was removed.
- **Other branches:** retained `ci/revert-warm-cache` because PR 205 was closed without merge and the branch is not an ancestor of `origin/main`. Retained dirty `feat/docs-hardening`; it is 20 commits behind and zero commits ahead of `origin/main` before this documentation commit.
- **Stale docs:** `README.md`, `docs/AUTH.md`, `docs/ENV.md`, and `docs/CONFIG.md` already document Authelia as supported. The contradiction is executable setup validation, already captured by the bead, so no product documentation was rewritten.

## Tools and Skills Used

- **Shell and file inspection:** `rg`, `awk`, `stat`, `nl`, `find`, and redacted key-presence checks were used to inspect configuration and source without printing secrets.
- **Docker and HTTP tools:** `docker ps`, `docker inspect`, `ss`, and `curl` established the actual running container, mount, listener, and health state.
- **Git and GitHub CLI:** inspected divergence, merge ancestry, worktrees, local/remote branches, and PR states; fetched/pruned remotes and removed only proven-safe local branches.
- **Beads CLI:** read the defect and dependencies, verified it remained open, and pushed Dolt state.
- **`superpowers:systematic-debugging`:** guided separation of runtime provider support from setup-check validation.
- **`lavra:lavra-work`:** was invoked for planning the bead work, but no Soma implementation agent ran before the session-close request.
- **`vibin:save-to-md`:** drove this repository-scoped maintenance, documentation, path-limited commit, default-branch landing, and cleanup workflow.

## Commands Executed

| command | result |
|---|---|
| `stat ~/.soma/.env ~/.soma/config.toml` | Confirmed files exist with modes `0600` and `0640` |
| redacted `awk` key/section inspection | All observed `.env` assignments were set; expected TOML sections and keys were present |
| `docker ps` / `docker inspect soma` | Container `soma` was running healthy as image `soma:dev`; `/home/jmagar/.soma` was mounted at `/data` |
| `curl -fsS http://127.0.0.1:40060/health` | Returned `{"status":"ok"}` |
| `soma setup check` | Local installed binary stopped on `invalid_auth_policy` because it was compiled without the auth/OAuth feature; port `40060` was also correctly reported as already in use |
| `bd show rmcp-template-ojgu --json` | Confirmed open P2 bug and exact title |
| `bd dep list rmcp-template-ojgu --json` | Returned `[]` |
| `bd dolt push` | Completed successfully |
| `git fetch --prune origin` | Refreshed remote state before cleanup decisions |
| `gh pr view` / `gh pr list` | Proved which worktree branches are active and which gone branches correspond to merged PRs |
| `git branch -D` on three exact refs | Removed only the three branches proven safe by ancestry or merged-PR evidence |

## Errors Encountered

- The local installed `/home/jmagar/.local/bin/soma setup check` exited with `invalid_auth_policy` because that binary was compiled without the `auth`/`oauth` feature. It also reported the expected advisory that port `40060` is in use by the healthy container. This did not invalidate the running container check, but it prevented the local binary from reproducing the later Google-field validation path.
- Initial safe branch deletion with `git branch -d` refused three candidates because Git compared them with the old current topic branch rather than `origin/main`. After collecting ancestry and merged-PR evidence, the same exact refs were deleted with `git branch -D`.
- The registered `marketplace-no-mcp` worktree reports a broken Git administrative path when queried directly. It was not pruned or repaired because project instructions explicitly protect that long-lived branch/worktree from broad cleanup.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Runtime configuration assurance | Credential and URL completeness had not been summarized per repository | Required Soma configuration keys, permissions, mount, auth selectors, and live health are recorded and verified |
| Setup-check defect tracking | Authelia-only false positive was an observed mismatch | Open bead `rmcp-template-ojgu` records the implementation and test follow-up |
| Runtime behavior | Healthy Authelia-configured container | Unchanged; no runtime or source mutation was made during closeout |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| redacted `.env` presence audit | Required Soma, public URL, token, and Authelia values are non-empty | All observed assignments reported `set` | pass |
| `stat ~/.soma/.env ~/.soma/config.toml` | Private canonical files | Modes `0600` and `0640` | pass |
| `docker ps` / `docker inspect soma` | Running service uses canonical data mount | Healthy `soma:dev`; `/home/jmagar/.soma:/data` | pass |
| `curl -fsS http://127.0.0.1:40060/health` | Healthy response | `{"status":"ok"}` | pass |
| source inspection of setup and shared auth | Setup mismatch is isolated and runtime supports Authelia | Google required at `setup.rs:273-291`; Authelia accepted by shared auth and tests | pass |
| `bd show` and `bd dep list` | Follow-up remains explicit and unblocked | Open P2 bug; no dependencies | pass |

## Risks and Rollback

- This session did not change runtime credentials or Soma source. The session-log commit can be reverted independently if needed.
- The local installed `soma` binary is not feature-equivalent to the healthy container image, so future setup-check reproduction should use an auth-enabled build or run inside the deployed image.
- The existing `feat/docs-hardening` dirt remains untouched and must not be discarded during later branch synchronization.

## Decisions Not Taken

- Did not implement or close `rmcp-template-ojgu`; no code/test evidence exists yet.
- Did not print credential or URL values into the session artifact.
- Did not remove open-PR worktrees, the protected marketplace worktree, the unmerged closed-PR branch, or the dirty current branch.
- Did not rewrite auth documentation because it already describes supported providers correctly; the defect is in setup behavior.

## References

- `crates/soma/cli/src/setup.rs:255-304`
- `crates/soma/cli/src/setup_tests.rs:87-170`
- `crates/shared/auth/src/config.rs:281-419`
- `crates/shared/auth/src/config.rs:788-845`
- `README.md:579-584`
- `docs/AUTH.md:62-71`
- Bead `rmcp-template-ojgu`

## Open Questions

- Whether the local `/home/jmagar/.local/bin/soma` should be rebuilt with the auth/OAuth feature is separate from the setup validator bug and was not decided in this session.
- The `marketplace-no-mcp` worktree registration is broken, but project instructions prohibit broad cleanup or retirement of that worktree without an explicit request naming it.

## Next Steps

- **Unfinished session work:** implement `rmcp-template-ojgu` by making setup validation accept any complete supported OAuth provider and add regression coverage for Authelia-only configuration.
- **Verification after implementation:** run focused setup/auth tests, then the relevant workspace quality gates and an auth-enabled `soma setup check` against a redacted fixture.
- **Separate follow-up:** decide whether to rebuild or replace the local installed `soma` binary with auth/OAuth support; do not confuse that packaging issue with the Google-only validation defect.
- **Preservation requirement:** keep the existing `feat/docs-hardening` changes intact until their owner lands or discards them deliberately.
