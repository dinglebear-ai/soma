# Soma Gateway Self-Contained Main Landing

## Metadata

- Date: 2026-07-15
- Repository: `/home/jmagar/workspace/soma`
- Landing worktree: `/home/jmagar/workspace/soma/.worktrees/soma-main-landing`
- Source worktree: `/home/jmagar/workspace/soma/.worktrees/soma-gateway-self-contained`
- Source branch: `codex/soma-gateway-self-contained`
- Source head before landing: `37c4e8e`
- Main landing merge commit: `1416f6b`
- Session artifact path: `docs/sessions/2026-07-15-soma-gateway-self-contained-main-landing.md`
- PR: `#137 Add self-contained Soma gateway` at `https://github.com/jmagar/soma/pull/137`
- Bead epic: `rmcp-template-0lnb`
- Plan: `docs/superpowers/plans/2026-07-15-self-contained-soma-gateway.md`
- Skills used in this workstream: `lavra:lavra-plan`, `lavra:lavra-research`, `lavra:lavra-eng-review`, `superpowers:writing-plans`, `vibin:work-it`, `vibin:save-to-md`, `superpowers:finishing-a-development-branch`

## User Request

Port Labby's gateway crate into Soma without depending on any other Labby or Soma product crates, while preserving the Labby gateway behavior that matters for MCP aggregation. The gateway had to be self-contained, use sibling test files, keep touched Rust files below 500 LOC, fix issues encountered in the same session, and avoid follow-up beads.

After implementation, the user asked whether the shipped gateway really had full Labby gateway parity and whether the CLI worked. The follow-up answer was corrected: the gateway runtime crate and product HTTP/MCP mounting are implemented, but `soma gateway ...` CLI commands are not wired yet because the CLI is a separate product surface.

The final request in this session was to save the work to markdown and make sure it lands on `main`.

## Executive Summary

The gateway branch is now merged into a clean landing worktree on top of current `origin/main`, with a normal merge commit because `origin/main` had advanced after the gateway branch was cut. The merged tree preserves the self-contained gateway crate and product mounting without introducing Labby crate dependencies.

The implementation includes live HTTP/SSE, stdio, and WebSocket upstream discovery and tool calls; tools/resources/prompts proxying; response caps; stdio process cleanup; relay/session isolation; subject-scoped OAuth cache wiring; upstream OAuth lifecycle actions; protected route metadata/auth/proxy behavior; virtual server and protected route projection; admin add/update/remove/reload/import/test dispatch; redaction, SSRF, and spawn guards; Soma HTTP/MCP integration; and feature-gated Code Mode/OpenAPI/palette adapter seams.

The CLI status is intentionally recorded clearly: the gateway does not yet expose a `soma gateway` command family. Existing `soma` CLI help and parser tests do not show gateway subcommands. That is a product CLI gap, not a gateway crate dependency failure.

## Timeline

1. Planned the port with Lavra and Superpowers workflows, then applied research and engineering review feedback back into the bead graph before implementation.
2. Implemented the self-contained `soma-gateway` crate with local config, security, transport, relay, manager, projection, dispatch, and adapter seams.
3. Added Soma product integration through REST and MCP mounting while keeping gateway state inside `GatewayManager`.
4. Reworked review findings that originally made the branch a foundation only: persisted config load, live transports, admin action behavior, response caps, route redaction, protected route mounting, spawn validation, and truthful closeout language.
5. Closed epic `rmcp-template-0lnb` and all 14 children after verification.
6. Audited the CLI and confirmed no `soma gateway` product command surface exists yet.
7. Created a fresh detached main landing worktree to avoid touching unrelated untracked files in `/home/jmagar/workspace/soma`.
8. Merged `codex/soma-gateway-self-contained` into the landing worktree with commit `1416f6b`.
9. Re-ran targeted post-merge gates in the merged tree.
10. Wrote this session artifact as a docs-only commit on top of the merge before pushing `main`.

## What Changed

The merge from current `origin/main` to `1416f6b` changes 185 files with 14,396 insertions and 1,553 deletions. The session artifact adds one more markdown file.

The main implementation groups are:

- Gateway crate: new self-contained runtime, config, manager, transport, relay, security, dispatch, usage, and feature adapters.
- API integration: REST gateway route, route inventory, response helpers, OpenAPI route visibility, and tests.
- MCP integration: live gateway proxy into MCP tools/resources/prompts, RMCP adapters, auth helpers, and tests.
- Runtime/product integration: shared gateway manager handle, protected route proxying, runtime wiring, and tests.
- Contracts and auth: `soma:admin` scope support and upstream OAuth feature wiring.
- Xtask hardening: test sibling checks, file-size checks, release/version command splits, and workspace command splits.
- Docs: full implementation plan under `docs/superpowers/plans/2026-07-15-self-contained-soma-gateway.md` plus this session artifact.

## Complete Changed File Inventory

Legend: `A` added, `M` modified. LOC impact is summarized by the merge stat above; individual Rust source and test files were checked under the 500 LOC policy.

```text
M .github/workflows/ci.yml
M Cargo.lock
M Cargo.toml
M crates/soma-api/Cargo.toml
M crates/soma-api/src/api.rs
M crates/soma-api/src/api_tests.rs
A crates/soma-api/src/gateway.rs
A crates/soma-api/src/gateway_tests.rs
M crates/soma-api/src/lib.rs
A crates/soma-api/src/openapi.rs
A crates/soma-api/src/openapi_tests.rs
A crates/soma-api/src/probes.rs
A crates/soma-api/src/probes_tests.rs
A crates/soma-api/src/responses.rs
A crates/soma-api/src/responses_tests.rs
A crates/soma-api/src/route_inventory.rs
A crates/soma-api/src/route_inventory_tests.rs
M crates/soma-auth/Cargo.toml
M crates/soma-contracts/src/lib.rs
A crates/soma-contracts/src/scopes.rs
A crates/soma-contracts/src/scopes_tests.rs
A crates/soma-gateway/AGENTS.md
A crates/soma-gateway/CLAUDE.md
A crates/soma-gateway/Cargo.toml
A crates/soma-gateway/GEMINI.md
A crates/soma-gateway/src/codemode_journal.rs
A crates/soma-gateway/src/codemode_journal_tests.rs
A crates/soma-gateway/src/config.rs
A crates/soma-gateway/src/config/defaults.rs
A crates/soma-gateway/src/config/defaults_tests.rs
A crates/soma-gateway/src/config/protected_routes.rs
A crates/soma-gateway/src/config/protected_routes_tests.rs
A crates/soma-gateway/src/config/upstream.rs
A crates/soma-gateway/src/config/upstream_tests.rs
A crates/soma-gateway/src/config/virtual_servers.rs
A crates/soma-gateway/src/config/virtual_servers_tests.rs
A crates/soma-gateway/src/config_tests.rs
A crates/soma-gateway/src/dispatch_helpers.rs
A crates/soma-gateway/src/dispatch_helpers_tests.rs
A crates/soma-gateway/src/gateway.rs
A crates/soma-gateway/src/gateway/catalog.rs
A crates/soma-gateway/src/gateway/catalog_tests.rs
A crates/soma-gateway/src/gateway/code_mode.rs
A crates/soma-gateway/src/gateway/code_mode/catalog.rs
A crates/soma-gateway/src/gateway/code_mode/catalog_tests.rs
A crates/soma-gateway/src/gateway/code_mode/host.rs
A crates/soma-gateway/src/gateway/code_mode/host_tests.rs
A crates/soma-gateway/src/gateway/code_mode_tests.rs
A crates/soma-gateway/src/gateway/config_store.rs
A crates/soma-gateway/src/gateway/config_store_tests.rs
A crates/soma-gateway/src/gateway/dispatch.rs
A crates/soma-gateway/src/gateway/dispatch_tests.rs
A crates/soma-gateway/src/gateway/manager.rs
A crates/soma-gateway/src/gateway/manager/core.rs
A crates/soma-gateway/src/gateway/manager/core_tests.rs
A crates/soma-gateway/src/gateway/manager/mcp_routes.rs
A crates/soma-gateway/src/gateway/manager/mcp_routes_tests.rs
A crates/soma-gateway/src/gateway/manager/mcp_scoped_routes.rs
A crates/soma-gateway/src/gateway/manager/mcp_scoped_routes_tests.rs
A crates/soma-gateway/src/gateway/manager/oauth_lifecycle.rs
A crates/soma-gateway/src/gateway/manager/oauth_lifecycle_tests.rs
A crates/soma-gateway/src/gateway/manager/pool_lifecycle.rs
A crates/soma-gateway/src/gateway/manager/pool_lifecycle_tests.rs
A crates/soma-gateway/src/gateway/manager/protected_routes.rs
A crates/soma-gateway/src/gateway/manager/protected_routes_tests.rs
A crates/soma-gateway/src/gateway/manager/virtual_servers.rs
A crates/soma-gateway/src/gateway/manager/virtual_servers_tests.rs
A crates/soma-gateway/src/gateway/manager_tests.rs
A crates/soma-gateway/src/gateway/oauth.rs
A crates/soma-gateway/src/gateway/oauth_tests.rs
A crates/soma-gateway/src/gateway/openapi.rs
A crates/soma-gateway/src/gateway/openapi_tests.rs
A crates/soma-gateway/src/gateway/palette.rs
A crates/soma-gateway/src/gateway/palette_tests.rs
A crates/soma-gateway/src/gateway/params.rs
A crates/soma-gateway/src/gateway/params_tests.rs
A crates/soma-gateway/src/gateway/projection.rs
A crates/soma-gateway/src/gateway/projection_tests.rs
A crates/soma-gateway/src/gateway/protected_routes.rs
A crates/soma-gateway/src/gateway/protected_routes_tests.rs
A crates/soma-gateway/src/gateway/runtime.rs
A crates/soma-gateway/src/gateway/runtime_tests.rs
A crates/soma-gateway/src/gateway/view_models.rs
A crates/soma-gateway/src/gateway/view_models_tests.rs
A crates/soma-gateway/src/gateway/virtual_servers.rs
A crates/soma-gateway/src/gateway/virtual_servers_tests.rs
A crates/soma-gateway/src/gateway_tests.rs
A crates/soma-gateway/src/lib.rs
A crates/soma-gateway/src/lib_tests.rs
A crates/soma-gateway/src/net.rs
A crates/soma-gateway/src/net_tests.rs
A crates/soma-gateway/src/process.rs
A crates/soma-gateway/src/process/guard.rs
A crates/soma-gateway/src/process/guard_tests.rs
A crates/soma-gateway/src/process/stderr.rs
A crates/soma-gateway/src/process/stderr_tests.rs
A crates/soma-gateway/src/process/stdio.rs
A crates/soma-gateway/src/process/stdio_tests.rs
A crates/soma-gateway/src/process/windows.rs
A crates/soma-gateway/src/process/windows_tests.rs
A crates/soma-gateway/src/process_tests.rs
A crates/soma-gateway/src/registry.rs
A crates/soma-gateway/src/registry_tests.rs
A crates/soma-gateway/src/security.rs
A crates/soma-gateway/src/security/env.rs
A crates/soma-gateway/src/security/env_tests.rs
A crates/soma-gateway/src/security/redact.rs
A crates/soma-gateway/src/security/redact_tests.rs
A crates/soma-gateway/src/security/ssrf.rs
A crates/soma-gateway/src/security/ssrf_tests.rs
A crates/soma-gateway/src/security_tests.rs
A crates/soma-gateway/src/upstream.rs
A crates/soma-gateway/src/upstream/http_body_cap.rs
A crates/soma-gateway/src/upstream/http_body_cap_tests.rs
A crates/soma-gateway/src/upstream/http_client.rs
A crates/soma-gateway/src/upstream/http_client_tests.rs
A crates/soma-gateway/src/upstream/pool.rs
A crates/soma-gateway/src/upstream/pool/connect_stdio.rs
A crates/soma-gateway/src/upstream/pool/connect_stdio_tests.rs
A crates/soma-gateway/src/upstream/pool/discovery.rs
A crates/soma-gateway/src/upstream/pool/discovery_tests.rs
A crates/soma-gateway/src/upstream/pool/health.rs
A crates/soma-gateway/src/upstream/pool/health_tests.rs
A crates/soma-gateway/src/upstream/pool/live.rs
A crates/soma-gateway/src/upstream/pool/live_tests.rs
A crates/soma-gateway/src/upstream/pool/prompts.rs
A crates/soma-gateway/src/upstream/pool/prompts_tests.rs
A crates/soma-gateway/src/upstream/pool/resources.rs
A crates/soma-gateway/src/upstream/pool/resources_tests.rs
A crates/soma-gateway/src/upstream/pool/subject.rs
A crates/soma-gateway/src/upstream/pool/subject_tests.rs
A crates/soma-gateway/src/upstream/pool/tools.rs
A crates/soma-gateway/src/upstream/pool/tools_tests.rs
A crates/soma-gateway/src/upstream/pool_tests.rs
A crates/soma-gateway/src/upstream/relay.rs
A crates/soma-gateway/src/upstream/relay/cache.rs
A crates/soma-gateway/src/upstream/relay/cache_tests.rs
A crates/soma-gateway/src/upstream/relay/lifecycle.rs
A crates/soma-gateway/src/upstream/relay/lifecycle_tests.rs
A crates/soma-gateway/src/upstream/relay/session.rs
A crates/soma-gateway/src/upstream/relay/session_tests.rs
A crates/soma-gateway/src/upstream/relay_tests.rs
A crates/soma-gateway/src/upstream/transport.rs
A crates/soma-gateway/src/upstream/transport/websocket.rs
A crates/soma-gateway/src/upstream/transport/websocket_tests.rs
A crates/soma-gateway/src/upstream/transport_tests.rs
A crates/soma-gateway/src/upstream_tests.rs
A crates/soma-gateway/src/usage.rs
A crates/soma-gateway/src/usage_tests.rs
M crates/soma-mcp/Cargo.toml
A crates/soma-mcp/src/gateway_proxy.rs
A crates/soma-mcp/src/gateway_proxy_tests.rs
M crates/soma-mcp/src/lib.rs
A crates/soma-mcp/src/rmcp_adapters.rs
A crates/soma-mcp/src/rmcp_adapters_tests.rs
A crates/soma-mcp/src/rmcp_auth.rs
A crates/soma-mcp/src/rmcp_auth_tests.rs
M crates/soma-mcp/src/rmcp_server.rs
M crates/soma-mcp/src/rmcp_server_tests.rs
M crates/soma-runtime/Cargo.toml
M crates/soma-runtime/src/server.rs
M crates/soma-runtime/src/server_tests.rs
M crates/soma/Cargo.toml
M crates/soma/src/lib.rs
A crates/soma/src/protected_routes.rs
A crates/soma/src/protected_routes_proxy.rs
A crates/soma/src/protected_routes_proxy_tests.rs
A crates/soma/src/protected_routes_tests.rs
M crates/soma/src/routes.rs
M crates/soma/src/routes_tests.rs
M crates/soma/src/runtime.rs
M crates/soma/src/runtime_tests.rs
A crates/soma/tests/api_gateway_routes.rs
M crates/soma/tests/api_routes.rs
M crates/soma/tests/architecture_boundaries.rs
A crates/soma/tests/support.rs
A docs/superpowers/plans/2026-07-15-self-contained-soma-gateway.md
M xtask/src/main.rs
M xtask/src/patterns/surfaces.rs
A xtask/src/release_commands.rs
A xtask/src/release_commands_tests.rs
A xtask/src/test_siblings.rs
A xtask/src/test_siblings_tests.rs
A xtask/src/workspace_commands.rs
A xtask/src/workspace_commands_tests.rs
A docs/sessions/2026-07-15-soma-gateway-self-contained-main-landing.md
```

## Behavior Delivered

- `soma-gateway` is a standalone crate with empty default features.
- The gateway crate does not depend on Labby crates.
- The gateway crate does not depend on Soma product/shim crates such as `soma`, `soma-runtime`, `soma-service`, `soma-contracts`, `soma-mcp`, `soma-api`, or `soma-cli`.
- The only allowed internal dependency observed in the gateway all-features graph is the optional leaf support crate `soma-auth`.
- Live upstream paths cover HTTP/SSE, stdio, and WebSocket discovery/call behavior in tests.
- Resource and prompt proxying obey proxy flags and filters.
- Response caps are shared across gateway response surfaces.
- Stdio spawn validation covers command, args, and environment before spawn.
- Unix process group cleanup and Windows cleanup policy are represented without importing Labby process crates.
- SSRF policies are local to the gateway and checked at config and dispatch boundaries.
- Redaction is shared across config, usage, Code Mode journal, protected routes, and error surfaces.
- Relay sessions are gateway minted; forged user session IDs are rejected.
- OAuth paths include subject identity checks and protected-route bearer stripping behavior.
- Protected-route metadata avoids backend URL leaks and enforces route/path/host semantics.
- Soma exposes gateway REST and MCP product paths through thin adapters.

## CLI Status

The runtime gateway implementation is not the same thing as a product CLI surface. The current shipped tree does not yet add `soma gateway ...` CLI commands. Evidence from the earlier live audit:

- `soma --help` did not show a `gateway` command group.
- `crates/soma-cli/src/lib.rs` did not contain a `Command::Gateway` variant.
- `crates/soma/tests/cli_parse.rs` did not include gateway command parser coverage.

This means the HTTP/MCP gateway can be exercised through product server surfaces, but a human/operator CLI like `soma gateway list/add/remove/test` remains unwired.

No follow-up bead was created for that gap because the user explicitly instructed not to leave follow-up beads. The gap is documented here instead.

## Verification Completed Before Main Push

These gates passed on the merged landing tree at commit `1416f6b`:

- `cargo test -p soma-gateway --all-features`
- `cargo clippy -p soma-gateway --all-targets --all-features -- -D warnings`
- `cargo xtask check-test-siblings`
- `cargo xtask check-file-size`
- `cargo test -p soma-mcp --no-default-features`
- `cargo test -p soma --no-default-features --features mcp-http,test-support`
- `cargo fmt --all -- --check`

Earlier gateway-branch validation also passed:

- `cargo test -p soma-gateway --no-default-features`
- `cargo test -p soma-gateway --no-default-features --features codemode`
- `cargo test -p soma-gateway --no-default-features --features openapi`
- `cargo test -p soma-gateway --no-default-features --features palette`
- `cargo test -p soma-gateway --no-default-features --features oauth,protected-routes`
- `cargo test -p soma-gateway --all-features`
- Fresh `CARGO_TARGET_DIR` gateway all-features test
- `cargo test -p soma-mcp --no-default-features`
- `cargo test -p soma-mcp --no-default-features --features oauth`
- Soma protected-route/runtime feature slices
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo clippy -p soma-gateway --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `cargo xtask check-test-siblings`
- `cargo xtask check-file-size`
- `cargo xtask patterns`
- `cargo xtask check-version-sync`
- `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`
- Touched Rust LOC check under 500
- `cargo tree -p soma-gateway --all-features` dependency proof

## Beads Activity

- `rmcp-template-0lnb` was closed with a correction note that superseded the earlier "foundation only" closeout.
- All 14 child beads were closed.
- The epic correction records the implemented gateway-owned Labby port and the exact verification suite.
- `bd dolt push` was already run after bead closeout and should be run again before the final git push for this session.

Closed child beads:

- `rmcp-template-0lnb.1` - upstream OAuth prerequisite reconciliation
- `rmcp-template-0lnb.2` - gateway crate scaffold and boundary gates
- `rmcp-template-0lnb.3` - config model and persistence
- `rmcp-template-0lnb.4` - security primitives
- `rmcp-template-0lnb.5` - process and stdio cleanup
- `rmcp-template-0lnb.6` - upstream pool transports and discovery
- `rmcp-template-0lnb.7` - relay sessions and subject cache
- `rmcp-template-0lnb.8` - upstream OAuth lifecycle
- `rmcp-template-0lnb.9` - manager lifecycle and runtime state
- `rmcp-template-0lnb.10` - protected routes and virtual projection
- `rmcp-template-0lnb.11` - Code Mode, OpenAPI, palette, and journal adapters
- `rmcp-template-0lnb.12` - action catalog, params, dispatch, and views
- `rmcp-template-0lnb.13` - product integration
- `rmcp-template-0lnb.14` - final gates and closeout

## Repository Maintenance

- Plan files were inspected. The active implementation plan lives at `docs/superpowers/plans/2026-07-15-self-contained-soma-gateway.md`.
- Worktrees and branches were inspected. The protected `marketplace-no-mcp` worktree and branch were left untouched.
- The normal main checkout at `/home/jmagar/workspace/soma` had unrelated untracked files:
  - `docs/sessions/2026-07-15-soma-codemode-openapi-port.md`
  - `docs/superpowers/plans/2026-07-15-self-contained-soma-gateway.md`
  - `soma-architecture-refactor-plan-v3.md`
- Because one untracked file collided with the gateway plan path, main landing was done in `/home/jmagar/workspace/soma/.worktrees/soma-main-landing` instead of touching the dirty main checkout.
- The `codex/soma-gateway-self-contained` worktree was left intact after landing so the user can still inspect PR branch state.

## Errors And Corrections

- The first branch closeout wording claimed a "foundation" and not full live gateway runtime behavior. That was corrected in the bead graph and in the implementation.
- The first attempt to fast-forward main ran from the source worktree because the chained command did not change directories into the landing worktree. No state changed incorrectly; the merge was then rerun inside `/home/jmagar/workspace/soma/.worktrees/soma-main-landing`.
- `origin/main` had advanced, so a fast-forward merge was impossible. A normal merge commit was created instead.
- A transcript glob check for Claude JSONL logs found no matching path for the Codex worktree. The session artifact therefore relies on live git, bead, test, and command evidence rather than a Claude transcript file.

## Final State To Push

At the time this artifact was written, the landing tree had:

- Merge commit: `1416f6b`
- One pending docs-only session artifact: `docs/sessions/2026-07-15-soma-gateway-self-contained-main-landing.md`
- Green post-merge verification listed above
- No intended changes outside the merge and this session artifact

The final closeout sequence is:

1. Stage only this session artifact with `git add -f -- docs/sessions/2026-07-15-soma-gateway-self-contained-main-landing.md`.
2. Commit only this artifact with `git commit -m "docs: save session log" --only -- docs/sessions/2026-07-15-soma-gateway-self-contained-main-landing.md`.
3. Run `bd dolt push`.
4. Push the detached landing HEAD to `origin/main`.
5. Verify remote `main` points at the pushed commit.
