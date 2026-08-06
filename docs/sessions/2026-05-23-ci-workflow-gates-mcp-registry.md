---
date: 2026-05-23 02:44:15 EDT
repo: git@github.com:jmagar/soma.git
branch: main
head: 0085d88
session id: 019e5357-c456-73d2-8a88-438090326886
transcript: /home/jmagar/.codex/sessions/2026/05/23/rollout-2026-05-23T01-38-41-019e5357-c456-73d2-8a88-438090326886.jsonl
working directory: /home/jmagar/workspace/soma
worktree: /home/jmagar/workspace/soma
beads: soma-490, soma-lei
---

# CI Workflow Gates and MCP Registry Setup

## User Request

The session started with: "Is there any other CI workflows that we can borrow from: ../lab, ../axon_rust, ../syslog-mcp". The follow-up scope was to add the reusable workflows except the Codex plugin scanner, then save the session to markdown.

## Session Overview

Added reusable CI/release workflow coverage to `soma`: actionlint, frontend artifact reuse, live MCP mcporter smoke, Compose/Docker smoke, version-sync gating, and MCP Registry publish support. Then moved the registry domain out of committed workflow YAML into a GitHub Actions repository variable.

## Sequence of Events

1. Compared `.github/workflows` in `soma` with sibling repos `lab`, `axon_rust`, and `syslog-mcp`.
2. Identified reusable workflow patterns and explicitly skipped the Codex plugin scanner when requested.
3. Implemented CI additions in `.github/workflows/ci.yml`, release web artifact reuse in `.github/workflows/release.yml`, MCP Registry publishing in `.github/workflows/docker-publish.yml`, and `.github/actionlint.yaml`.
4. Verified workflows locally and exercised the new MCP smoke path against `./target/debug/example` on port `40123`.
5. Committed and pushed `8a5c0d5`, then changed the registry domain to use `vars.MCP_REGISTRY_DOMAIN`, set the repo variable to `nashost.tv`, committed and pushed `0085d88`.
6. Checked live GitHub Actions runs; all push workflows failed before runner startup due to GitHub billing/spending-limit annotations.
7. Searched `~/workspace` for MCP Registry key/domain usage and confirmed sibling repos use `nashost.tv` plus `MCP_PRIVATE_KEY`; `soma` has the variable but not the secret.
8. Created follow-up beads for the missing secret and GitHub Actions billing blocker.

## Key Findings

- `soma` already had stronger equivalents for several sibling checks: CodeQL, gitleaks, cargo-deny, MSRV, Trivy, Dependabot auto-merge, and native Linux/Windows artifact builds.
- Sibling workflows consistently use DNS auth domain `nashost.tv` for MCP Registry publishing, for example `syslog-mcp/.github/workflows/docker-publish.yml`.
- `jmagar/soma` now has repository variable `MCP_REGISTRY_DOMAIN=nashost.tv`, verified with `gh variable list -R jmagar/soma`.
- `jmagar/soma` does not have a repository secret named `MCP_PRIVATE_KEY`; sibling repos such as `syslog-mcp`, `arcane-mcp`, `unifi-mcp`, `gotify-mcp`, `overseerr-mcp`, `swag-mcp`, and `axon_rust` do.
- GitHub Actions failures on pushed commits were not workflow-step failures. Check-run annotations reported: "The job was not started because recent account payments have failed or your spending limit needs to be increased."

## Technical Decisions

- Kept the Codex plugin scanner out of scope because the user explicitly rejected it.
- Put the MCP Registry publish job in `.github/workflows/docker-publish.yml` because `server.json` publishes an OCI package identifier, so registry publishing should run after the Docker image push succeeds.
- Used `vars.MCP_REGISTRY_DOMAIN` instead of committing `nashost.tv`, preserving template neutrality while still allowing this repository to publish under the user's DNS identity.
- Used port `40123` for the CI MCP smoke instead of `40060` after local verification found an installed `/usr/local/bin/example serve mcp` already listening on `40060`.
- Added `.github/actionlint.yaml` for self-hosted runner labels `soma` and `winhost`, so the new actionlint job accepts this repository's Windows runner label set.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| created | `.github/actionlint.yaml` | | Allow actionlint to recognize custom self-hosted runner labels | `.github/actionlint.yaml:1` |
| modified | `.github/workflows/ci.yml` | | Add actionlint, shared web export, live MCP smoke, container smoke, version-sync gate | `.github/workflows/ci.yml:41`, `.github/workflows/ci.yml:130`, `.github/workflows/ci.yml:270`, `.github/workflows/ci.yml:348`, `.github/workflows/ci.yml:425` |
| modified | `.github/workflows/docker-publish.yml` | | Add MCP Registry publish job and read registry domain from repo variable | `.github/workflows/docker-publish.yml:31`, `.github/workflows/docker-publish.yml:119` |
| modified | `.github/workflows/release.yml` | | Reuse a single web export artifact for release builds | `.github/workflows/release.yml:34`, `.github/workflows/release.yml:84` |
| created | `docs/sessions/2026-05-23-ci-workflow-gates-mcp-registry.md` | | Durable session record | this file |

## Beads Activity

| bead | title | action | final status | why it mattered |
|---|---|---|---|---|
| `soma-490` | Add MCP_PRIVATE_KEY secret to soma | Created | open | Tracks the remaining MCP Registry publish prerequisite; `gh secret list -R jmagar/soma` did not show `MCP_PRIVATE_KEY`. |
| `soma-lei` | Resolve GitHub Actions billing blocker for soma workflows | Created | open | Tracks the non-code blocker preventing all push workflows from starting. |

Notes: both `bd create` commands succeeded, but emitted `Warning: auto-export: git add failed: exit status 1`. `bd show` confirmed both beads exist.

## Repository Maintenance

- Plans: `find docs/plans -maxdepth 2 -type f` returned no plan files, so no completed plan moves were available.
- Beads: searched for existing CI/registry/secret/billing issues before creating `soma-490` and `soma-lei`; no direct duplicates for these current blockers were found.
- Worktrees and branches: `git worktree list --porcelain` showed only `/home/jmagar/workspace/soma` on `main`; `git branch -vv` showed `main` even with `origin/main`; no cleanup was safe or needed.
- Stale docs: no repo docs were updated. The workflow comments now document `gh variable set MCP_REGISTRY_DOMAIN --body <domain>` in `.github/workflows/docker-publish.yml:31`.
- Git state: after the workflow commits, `git status --short --branch` showed `## main...origin/main` before this session note was written.

## Tools and Skills Used

- Skill: `save-to-md` was used to drive this session capture and maintenance pass.
- Shell/git: inspected workflows, branches, worktrees, diffs, logs, and pushed commits.
- GitHub CLI: inspected workflow runs, check-run annotations, repo variables, and repo secrets; set `MCP_REGISTRY_DOMAIN=nashost.tv`.
- Beads CLI: searched tracker state and created two follow-up beads.
- Docker CLI: validated Compose config locally with `docker compose ... config --quiet`.
- mcporter: live-tested MCP tool/resource smoke against the debug binary.
- No subagents were spawned. No browser automation was used.

## Commands Executed

- `find .github/workflows ../lab/.github/workflows ../axon_rust/.github/workflows ../syslog-mcp/.github/workflows -maxdepth 1 -type f -print`: inventoried workflow files.
- `go run github.com/rhysd/actionlint/cmd/actionlint@latest`: validated workflow syntax and shell snippets; initially exposed unknown self-hosted runner labels and a shell glob warning, then passed after fixes.
- `bash scripts/check-version-sync.sh`: passed with all three version-bearing files at `v0.4.0`.
- `docker compose --env-file .env.example -f docker-compose.prod.yml config --quiet` and `docker compose --env-file .env.example -f docker-compose.yml config --quiet`: passed.
- `cargo build --locked --bin example`: passed locally.
- `SOMA_MCP_HOST=127.0.0.1 SOMA_MCP_PORT=40123 bash tests/mcporter/test-mcp.sh --timeout-ms 20000`: passed with `10` pass, `0` fail, `2` skip.
- `gh variable set MCP_REGISTRY_DOMAIN --body nashost.tv -R jmagar/soma`: set the repository variable.
- `gh run list --branch main --limit 5 --json ...`: showed push workflow failures on `0085d88` and `8a5c0d5`.
- `gh api repos/jmagar/soma/check-runs/.../annotations`: showed GitHub billing/spending-limit failure annotations.
- `git pull --rebase && bd dolt push && git push && git status --short --branch`: pushed commits and Beads state.

## Errors Encountered

- Local MCP smoke initially failed because port `40060` was already occupied by `/usr/local/bin/example serve mcp`. Resolution: run the new CI smoke on `40123`.
- Local actionlint initially failed on custom self-hosted runner labels `soma` and `winhost`. Resolution: add `.github/actionlint.yaml`.
- Local actionlint also reported shellcheck `SC2035` for `sha256sum *`. Resolution: changed the release workflow to `sha256sum ./*`.
- GitHub Actions runs failed before job steps due to billing/spending-limit status, not due to workflow content. Follow-up bead: `soma-lei`.
- `bd create` emitted `Warning: auto-export: git add failed: exit status 1`; `bd show` confirmed both created issues exist.

## Behavior Changes

| before | after |
|---|---|
| CI had no actionlint job. | CI now runs actionlint with repository-specific runner-label config. |
| CI release-build jobs rebuilt web assets separately. | CI and release workflows upload/download a `web-out` artifact. |
| No push/PR live MCP smoke ran through mcporter. | CI starts the debug binary on loopback and runs the mcporter harness. |
| Docker image publishing had no MCP Registry publication step. | Tag-time Docker Publish can prepare `server.json`, authenticate with `mcp-publisher`, and publish when `MCP_PRIVATE_KEY` is configured. |
| Registry domain was temporarily committed as a placeholder. | Workflow now reads `vars.MCP_REGISTRY_DOMAIN`; GitHub stores `nashost.tv`. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `go run github.com/rhysd/actionlint/cmd/actionlint@latest` | No workflow lint errors | Passed after `.github/actionlint.yaml` and `sha256sum ./*` fix | pass |
| `python3` YAML parse over `.github/workflows/*.yml` | Valid YAML | `yaml ok` | pass |
| `bash scripts/check-version-sync.sh` | Versions in sync | `[version-sync] OK — all 3 files at v0.4.0` | pass |
| `docker compose --env-file .env.example -f docker-compose.prod.yml config --quiet` | Valid prod compose config | No output, exit 0 | pass |
| `docker compose --env-file .env.example -f docker-compose.yml config --quiet` | Valid dev compose config | No output, exit 0 | pass |
| `cargo build --locked --bin example` | Debug binary builds | Finished dev profile | pass |
| `tests/mcporter/test-mcp.sh --timeout-ms 20000` on port `40123` | MCP smoke passes | `PASS 10`, `FAIL 0`, `SKIP 2` | pass |
| `gh variable list -R jmagar/soma` | Domain stored as repo variable | `MCP_REGISTRY_DOMAIN nashost.tv` | pass |
| `gh secret list -R jmagar/soma` | Show required publish secret | Did not show `MCP_PRIVATE_KEY` | blocked |
| GitHub Actions push workflows | Jobs start and produce logs | Jobs failed before startup due to billing/spending-limit annotation | blocked |

## Risks and Rollback

- Workflow action versions still include some semver tags inherited from earlier files (`actions/checkout@v6`, Docker actions by major version). This session did not convert every existing action use to commit SHAs.
- MCP Registry publishing will skip if `MCP_PRIVATE_KEY` is unset, but a tag release will not actually register until `soma-490` is resolved.
- Rollback path: revert commits `0085d88` and `8a5c0d5`, then remove repo variable `MCP_REGISTRY_DOMAIN` if desired.

## Decisions Not Taken

- Did not add the Codex plugin scanner because the user explicitly said they did not want it.
- Did not hardcode `nashost.tv` in the workflow after the user asked how to use the domain without committing it.
- Did not try to fix GitHub Actions by editing workflow YAML after the live evidence showed billing/spending-limit annotations.
- Did not delete or clean worktrees/branches because only the main worktree and `main` branch were registered.

## References

- `../lab/.github/workflows/ci.yml` and `../lab/.github/workflows/release.yml` for frontend artifact and release-smoke patterns.
- `../axon_rust/.github/workflows/compose-smoke.yml` and `../axon_rust/.github/workflows/ci.yml` for compose/image smoke and MCP smoke patterns.
- `../syslog-mcp/.github/workflows/docker-publish.yml` for MCP Registry publish steps.
- GitHub Actions runs for `8a5c0d5` and `0085d88` in `jmagar/soma`.

## Open Questions

- Where is the original raw MCP Registry DNS private key stored outside `~/workspace`? It was not found in local `~/workspace` env/secret files, and GitHub repo secrets cannot be read back.
- Whether to pin every remaining major-version GitHub Action in `docker-publish.yml` and `release.yml` to full commit SHAs, matching the stricter pattern used elsewhere in the repo.

## Next Steps

1. Resolve `soma-lei`: fix GitHub Actions billing or spending limit, then rerun the failed workflows for `0085d88`.
2. Resolve `soma-490`: set `MCP_PRIVATE_KEY` on `jmagar/soma` using the existing registry DNS key.
3. After billing and secret setup, push a harmless commit or rerun workflows to verify CI, MSRV, CodeQL, Docker Publish, and tag-time registry publish behavior.
4. Optional hardening: pin remaining unpinned major-version actions in `docker-publish.yml` and `release.yml` to commit SHAs.
