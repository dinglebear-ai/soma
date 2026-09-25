---
date: "2026-07-16 19:31:26 EDT"
repo: "git@github.com:jmagar/soma.git"
branch: "main (session began in a detached Codex worktree)"
head: "c2540c0f4fb441af51ed6e341d4bebcd3502112e"
plan: "soma-architecture-refactor-plan-v3.md"
session id: "019f6720-7e38-70b2-aac8-dd37c89543e2"
transcript: "/home/jmagar/.codex/sessions/2026/07/15/rollout-2026-07-15T14-53-30-019f6720-7e38-70b2-aac8-dd37c89543e2.jsonl"
working directory: "/home/jmagar/.codex/worktrees/8bed/soma"
worktree: "/home/jmagar/.codex/worktrees/8bed/soma (detached at b0b189f when save-to-md was invoked)"
beads: "rmcp-template-ub2l, rmcp-template-e4i3, rmcp-template-ynhq, rmcp-template-8lgu, rmcp-template-8lgu.1-.7, rmcp-template-3ies, rmcp-template-1h9y, rmcp-template-jl1z, rmcp-template-r44l, rmcp-template-fk5r, rmcp-template-fk5r.1-.10, rmcp-template-fq2i, rmcp-template-8kex, rmcp-template-4pux, rmcp-template-8ark, rmcp-template-d40t, rmcp-template-uwgj"
---

# Soma architecture refactor through PR9

## User Request

Create an isolated worktree, bring in and revise `soma-architecture-refactor-plan-v3.md`, then execute the reusable-crate architecture refactor through PR9. Preserve the repository as the source of truth for independently consumable Rust crates, merge the completed pull requests without losing work, clean up, and stop before PR10.

## Session Overview

The session refined the architecture around mix-and-match, crates.io-ready components and delivered PR0 through PR9. The resulting workspace has nested shared MCP role crates, architecture boundary enforcement, domain/application facades, CLI/REST/MCP/runtime adapters routed through `SomaApplication`, and a canonical shared provider core using `ToolSpec`.

PRs #134, #135, #137, #139, #140, #141, and #143-#149 were merged. PR9 passed the full local workspace suite and every required GitHub check, landed as merge `c2540c0`, and its branch/worktree were removed. The plan ledger records PR0-PR9 complete, PR10-PR19 remaining, and the session stopped at the requested boundary.

## Sequence of Events

1. Created an isolated worktree and copied the architecture plan from `main`.
2. Reviewed the existing MCP/gateway, auth, observability, API, CLI, provider, Palette, Tauri, test-support, and plugin-support boundaries; repeatedly revised the plan to match the user's reuse and publishing goals.
3. Chose the nested `crates/shared/mcp/{client,server,proxy,gateway}` layout, neutral shared engines with Soma adapters, minimal dependency cones, `provider-core`, and canonical `ToolSpec`.
4. Merged the traces and prerequisite gateway/Code Mode/OpenAPI work, then delivered PR0-PR3 for shared foundations, physical taxonomy, and enforceable architecture boundaries.
5. Delivered PR4-PR8 to introduce domain/application facades and route CLI, REST, MCP, and runtime composition through the same application layer.
6. Delivered PR9 to extract the canonical shared provider model and registry while retaining Soma-owned authorization, limits, adapters, and policy.
7. Ran two review sweeps, fixed all blocking findings, filed deferred pre-existing issues, waited for Linux/Windows/MSRV/conformance checks, merged PR9, and cleaned its branch/worktree.
8. Updated the plan ledger and stopped before PR10 as requested; during this save pass, closed the stale PR0 bead and audited plans, worktrees, branches, and docs.

## Key Findings

- The reusable gateway needs generic inbound auth and outbound credential/OAuth traits; Soma-specific defaults, policy, and configuration belong in product adapters rather than `crates/shared/mcp/gateway`.
- One domain implementation can serve all surfaces: CLI command names, conventional REST routes, and MCP router/individual/both exposure modes remain thin adapters over `SomaApplication`.
- The shared provider contract now lives in `crates/shared/provider-core/src/lib.rs:1`; its public model includes `ToolSpec`, provider call/output types, manifests, capabilities, validation, and registry primitives without Soma policy.
- Runtime composition builds `SomaApplication` once and stores it in `SomaRuntime` at `apps/soma/src/application_ports.rs:24`.
- PR9 exposed a Cargo feature-unification constraint: provider catalog ordering needs `serde_json/preserve_order`, while generated OpenAPI output must remain stable. The resolution is an opt-in/default provider feature with downstream dependencies disabling defaults except the runtime client path.
- The active plan now identifies two carryovers before destructive later slices: audit the distributed behavior freeze before PR12, and decide shared package naming before PR19.

## Technical Decisions

- Use physical paths `crates/shared/mcp/client`, `server`, `proxy`, and `gateway`; package names may remain distinct from folder names, but publishing names must be decided before the final artifact/documentation sweep.
- Favor independently reusable shared crates with narrow optional features, not a rule of zero internal dependencies. Dependency edges are acceptable when they represent genuine composition.
- Keep `soma-auth` and `soma-observability` reusable and product-neutral; expose generic traits at shared boundaries and put Soma defaults/auth/config in `crates/soma/*` or `apps/soma`.
- Name the provider abstraction crate `provider-core` and use `ToolSpec` as the canonical shared type. Do not retain `ProviderTool` unless compatibility evidence later requires an alias.
- Preserve one-action MCP routing while adding a configuration mode for individual MCP tools or both; REST remains conventional and CLI commands map to action names.
- Preserve `apps/palette` as the shipped desktop app, use a shared Tauri shell crate for generic Rust mechanics, and reserve a Soma Palette adapter for product routes/state.
- Remove the empty `plugin-support` crate rather than maintaining a placeholder abstraction.
- In provider dispatch, enforce surface eligibility, Soma policy, host input cap, core schema, provider call, host output cap, then output schema; typed compatibility normalization is not applied to arbitrary raw JSON.

## Files Changed

The table below contains this session artifact plus the deduplicated union of merge-parent diffs for PRs #134, #135, #137, #139, #140, #141, and #143-#149. It covers 1,332 observed paths, including transient pre-taxonomy paths that were later renamed. GitHub's PR-files endpoint returned HTTP 503 during closeout, so the implementation inventory was generated from the locally available merge commits and their first parents.

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| created | `docs/sessions/2026-07-16-soma-architecture-refactor-pr0-pr9.md` | - | Durable full-session record | `vibin:save-to-md` |
| created | `.full-review/00-scope.md` | - | PR review evidence artifact | PR #149 |
| created | `.full-review/01-quality-architecture.md` | - | PR review evidence artifact | PR #149 |
| created | `.full-review/02-security-performance.md` | - | PR review evidence artifact | PR #149 |
| created | `.full-review/03-testing-documentation.md` | - | PR review evidence artifact | PR #149 |
| created | `.full-review/04-best-practices-ci.md` | - | PR review evidence artifact | PR #149 |
| created | `.full-review/05-final-report.md` | - | PR review evidence artifact | PR #149 |
| created | `.full-review/state.json` | - | PR review evidence artifact | PR #149 |
| modified | `.github/workflows/ci.yml` | - | CI, release, or repository automation | PR #135, #137, #139, #140, #141 |
| modified | `.github/workflows/conformance.yml` | - | CI, release, or repository automation | PR #139 |
| modified | `.github/workflows/docker-publish.yml` | - | CI, release, or repository automation | PR #139 |
| modified | `.github/workflows/release-please.yml` | - | CI, release, or repository automation | PR #139 |
| modified | `.github/workflows/release.yml` | - | CI, release, or repository automation | PR #139 |
| modified | `.gitleaks.toml` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| modified | `.mise.toml` | - | Workspace configuration, source, test, or documentation | PR #139 |
| modified | `CHANGELOG.md` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137, #139, #141, #143, #144, #145, #146, #148 |
| modified | `CLAUDE.md` | - | Workspace configuration, source, test, or documentation | PR #139, #148 |
| modified | `Cargo.lock` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137, #141, #143, #144, #145, #146, #148, #149 |
| modified | `Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137, #139, #141, #143, #149 |
| modified | `Justfile` | - | Workspace configuration, source, test, or documentation | PR #139 |
| modified | `README.md` | - | Workspace configuration, source, test, or documentation | PR #139 |
| renamed | `apps/soma/Cargo.toml` | `crates/soma/Cargo.toml` | Soma composition root or integration test | PR #139, #140, #144, #145, #147, #148 |
| created | `apps/soma/src/application_ports.rs` | - | Soma composition root or integration test | PR #145, #146, #148 |
| created | `apps/soma/src/application_ports_tests.rs` | - | Soma composition root or integration test | PR #145, #146 |
| renamed | `apps/soma/src/bin/soma.rs` | `crates/soma/src/bin/soma.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/src/bin/soma_tests.rs` | `crates/soma/src/bin/soma_tests.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/src/gateway_auth.rs` | `crates/soma/src/gateway_auth.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/src/gateway_auth_tests.rs` | `crates/soma/src/gateway_auth_tests.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/src/lib.rs` | `crates/soma/src/lib.rs` | Soma composition root or integration test | PR #139, #145, #146, #147, #148 |
| renamed | `apps/soma/src/protected_routes.rs` | `crates/soma/src/protected_routes.rs` | Soma composition root or integration test | PR #139, #146, #148 |
| renamed | `apps/soma/src/protected_routes_proxy.rs` | `crates/soma/src/protected_routes_proxy.rs` | Soma composition root or integration test | PR #139, #148 |
| renamed | `apps/soma/src/protected_routes_proxy_tests.rs` | `crates/soma/src/protected_routes_proxy_tests.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/src/protected_routes_tests.rs` | `crates/soma/src/protected_routes_tests.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/src/routes.rs` | `crates/soma/src/routes.rs` | Soma composition root or integration test | PR #139, #145, #146, #148 |
| renamed | `apps/soma/src/routes_tests.rs` | `crates/soma/src/routes_tests.rs` | Soma composition root or integration test | PR #139, #148 |
| renamed | `apps/soma/src/runtime.rs` | `crates/soma/src/runtime.rs` | Soma composition root or integration test | PR #139, #144, #146, #148 |
| renamed | `apps/soma/src/runtime_tests.rs` | `crates/soma/src/runtime_tests.rs` | Soma composition root or integration test | PR #139, #144 |
| renamed | `apps/soma/tests/README.md` | `crates/soma/tests/README.md` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/ai_sdk_provider.rs` | `crates/soma/tests/ai_sdk_provider.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/api_gateway_routes.rs` | `crates/soma/tests/api_gateway_routes.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/api_routes.rs` | `crates/soma/tests/api_routes.rs` | Soma composition root or integration test | PR #139, #148, #149 |
| renamed | `apps/soma/tests/architecture_boundaries.rs` | `crates/soma/tests/architecture_boundaries.rs` | Soma composition root or integration test | PR #139, #146, #147, #148 |
| renamed | `apps/soma/tests/cli_parse.rs` | `crates/soma/tests/cli_parse.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/cli_remote_api.rs` | `crates/soma/tests/cli_remote_api.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/dispatch_logging.rs` | `crates/soma/tests/dispatch_logging.rs` | Soma composition root or integration test | PR #139, #148 |
| renamed | `apps/soma/tests/drop_provider_probe.rs` | `crates/soma/tests/drop_provider_probe.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/gateway_architecture_boundaries.rs` | `crates/soma/tests/gateway_architecture_boundaries.rs` | Soma composition root or integration test | PR #139, #146 |
| renamed | `apps/soma/tests/generated_surfaces.rs` | `crates/soma/tests/generated_surfaces.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/mcp_provider.rs` | `crates/soma/tests/mcp_provider.rs` | Soma composition root or integration test | PR #139, #146, #149 |
| renamed | `apps/soma/tests/mcporter/test-mcp.sh` | `crates/soma/tests/mcporter/test-mcp.sh` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/openapi_provider.rs` | `crates/soma/tests/openapi_provider.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/palette_manifest.rs` | `crates/soma/tests/palette_manifest.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/plugin_contract.rs` | `crates/soma/tests/plugin_contract.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/provider_cli.rs` | `crates/soma/tests/provider_cli.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/provider_contract.rs` | `crates/soma/tests/provider_contract.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/provider_registry.rs` | `crates/soma/tests/provider_registry.rs` | Soma composition root or integration test | PR #139, #149 |
| renamed | `apps/soma/tests/provider_security.rs` | `crates/soma/tests/provider_security.rs` | Soma composition root or integration test | PR #139, #146 |
| renamed | `apps/soma/tests/provider_surfaces.rs` | `crates/soma/tests/provider_surfaces.rs` | Soma composition root or integration test | PR #139, #148 |
| renamed | `apps/soma/tests/python_provider.rs` | `crates/soma/tests/python_provider.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/soma_invariants.rs` | `crates/soma/tests/soma_invariants.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/soma_serve.rs` | `crates/soma/tests/soma_serve.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/stdio_mcp.rs` | `crates/soma/tests/stdio_mcp.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/stdio_remote_api.rs` | `crates/soma/tests/stdio_remote_api.rs` | Soma composition root or integration test | PR #139, #146 |
| renamed | `apps/soma/tests/support.rs` | `crates/soma/tests/support.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/tool_dispatch.rs` | `crates/soma/tests/tool_dispatch.rs` | Soma composition root or integration test | PR #139, #146, #148, #149 |
| renamed | `apps/soma/tests/wasm_provider.rs` | `crates/soma/tests/wasm_provider.rs` | Soma composition root or integration test | PR #139 |
| renamed | `apps/soma/tests/workflow_shapes.rs` | `crates/soma/tests/workflow_shapes.rs` | Soma composition root or integration test | PR #139 |
| modified | `config/Dockerfile` | - | Workspace configuration, source, test, or documentation | PR #139 |
| created | `crates/rmcp-traces/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #141 |
| created | `crates/rmcp-traces/README.md` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #141 |
| created | `crates/rmcp-traces/src/http.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #141 |
| created | `crates/rmcp-traces/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #141 |
| created | `crates/rmcp-traces/src/trace_context.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #141 |
| created | `crates/rmcp-traces/tests/core_trace_context.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #141 |
| created | `crates/rmcp-traces/tests/http_propagation.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #141 |
| renamed | `crates/shared/auth/Cargo.toml` | `crates/soma-auth/Cargo.toml` | Reusable shared crate | PR #139, #140 |
| renamed | `crates/shared/auth/src/at_rest.rs` | `crates/soma-auth/src/at_rest.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/auth_context.rs` | `crates/soma-auth/src/auth_context.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/authorize.rs` | `crates/soma-auth/src/authorize.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/cimd.rs` | `crates/soma-auth/src/cimd.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/cimd/document.rs` | `crates/soma-auth/src/cimd/document.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/cimd/ssrf.rs` | `crates/soma-auth/src/cimd/ssrf.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/config.rs` | `crates/soma-auth/src/config.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/error.rs` | `crates/soma-auth/src/error.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/google.rs` | `crates/soma-auth/src/google.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/jwt.rs` | `crates/soma-auth/src/jwt.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/lib.rs` | `crates/soma-auth/src/lib.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/metadata.rs` | `crates/soma-auth/src/metadata.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/middleware.rs` | `crates/soma-auth/src/middleware.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/redirect_uri.rs` | `crates/soma-auth/src/redirect_uri.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/registration.rs` | `crates/soma-auth/src/registration.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/routes.rs` | `crates/soma-auth/src/routes.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/session.rs` | `crates/soma-auth/src/session.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/sqlite.rs` | `crates/soma-auth/src/sqlite.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/state.rs` | `crates/soma-auth/src/state.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/test_support.rs` | `crates/soma-auth/src/test_support.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/token.rs` | `crates/soma-auth/src/token.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/types.rs` | `crates/soma-auth/src/types.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream.rs` | `crates/soma-auth/src/upstream.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream/cache.rs` | `crates/soma-auth/src/upstream/cache.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream/config.rs` | `crates/soma-auth/src/upstream/config.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream/encryption.rs` | `crates/soma-auth/src/upstream/encryption.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream/manager.rs` | `crates/soma-auth/src/upstream/manager.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream/manager/client.rs` | `crates/soma-auth/src/upstream/manager/client.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream/refresh.rs` | `crates/soma-auth/src/upstream/refresh.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream/runtime.rs` | `crates/soma-auth/src/upstream/runtime.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream/store.rs` | `crates/soma-auth/src/upstream/store.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/upstream/types.rs` | `crates/soma-auth/src/upstream/types.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/auth/src/util.rs` | `crates/soma-auth/src/util.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/AGENTS.md` | `crates/soma-codemode/AGENTS.md` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/CLAUDE.md` | `crates/soma-codemode/CLAUDE.md` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/Cargo.toml` | `crates/soma-codemode/Cargo.toml` | Reusable shared crate | PR #139, #140 |
| renamed | `crates/shared/codemode/GEMINI.md` | `crates/soma-codemode/GEMINI.md` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/artifacts.rs` | `crates/soma-codemode/src/artifacts.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/artifacts/path.rs` | `crates/soma-codemode/src/artifacts/path.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/artifacts/path_tests.rs` | `crates/soma-codemode/src/artifacts/path_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/artifacts/prune.rs` | `crates/soma-codemode/src/artifacts/prune.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/artifacts/prune_tests.rs` | `crates/soma-codemode/src/artifacts/prune_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/artifacts/store.rs` | `crates/soma-codemode/src/artifacts/store.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/artifacts/store_tests.rs` | `crates/soma-codemode/src/artifacts/store_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/artifacts_tests.rs` | `crates/soma-codemode/src/artifacts_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/bin/soma-codemode-runner.rs` | `crates/soma-codemode/src/bin/soma-codemode-runner.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/bin/soma-codemode-runner_tests.rs` | `crates/soma-codemode/src/bin/soma-codemode-runner_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/broker.rs` | `crates/soma-codemode/src/broker.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/broker_tests.rs` | `crates/soma-codemode/src/broker_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/config.rs` | `crates/soma-codemode/src/config.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/config_tests.rs` | `crates/soma-codemode/src/config_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/error.rs` | `crates/soma-codemode/src/error.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/error_tests.rs` | `crates/soma-codemode/src/error_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute.rs` | `crates/soma-codemode/src/execute.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/budget.rs` | `crates/soma-codemode/src/execute/budget.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/budget_tests.rs` | `crates/soma-codemode/src/execute/budget_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/call_tool.rs` | `crates/soma-codemode/src/execute/call_tool.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/call_tool_tests.rs` | `crates/soma-codemode/src/execute/call_tool_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/discovery.rs` | `crates/soma-codemode/src/execute/discovery.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/discovery_tests.rs` | `crates/soma-codemode/src/execute/discovery_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/internal.rs` | `crates/soma-codemode/src/execute/internal.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/internal_tests.rs` | `crates/soma-codemode/src/execute/internal_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/proxy.rs` | `crates/soma-codemode/src/execute/proxy.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/proxy_tests.rs` | `crates/soma-codemode/src/execute/proxy_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/result.rs` | `crates/soma-codemode/src/execute/result.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/result_tests.rs` | `crates/soma-codemode/src/execute/result_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/runner.rs` | `crates/soma-codemode/src/execute/runner.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/runner_tests.rs` | `crates/soma-codemode/src/execute/runner_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/tool_dispatch.rs` | `crates/soma-codemode/src/execute/tool_dispatch.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute/tool_dispatch_tests.rs` | `crates/soma-codemode/src/execute/tool_dispatch_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/execute_tests.rs` | `crates/soma-codemode/src/execute_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git.rs` | `crates/soma-codemode/src/git.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/command.rs` | `crates/soma-codemode/src/git/command.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/command_tests.rs` | `crates/soma-codemode/src/git/command_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/output.rs` | `crates/soma-codemode/src/git/output.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/output_tests.rs` | `crates/soma-codemode/src/git/output_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/provider.rs` | `crates/soma-codemode/src/git/provider.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/provider_dispatch.rs` | `crates/soma-codemode/src/git/provider_dispatch.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/provider_dispatch_tests.rs` | `crates/soma-codemode/src/git/provider_dispatch_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/provider_tests.rs` | `crates/soma-codemode/src/git/provider_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/safety.rs` | `crates/soma-codemode/src/git/safety.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git/safety_tests.rs` | `crates/soma-codemode/src/git/safety_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/git_tests.rs` | `crates/soma-codemode/src/git_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/home.rs` | `crates/soma-codemode/src/home.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/home_tests.rs` | `crates/soma-codemode/src/home_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/host.rs` | `crates/soma-codemode/src/host.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/host_tests.rs` | `crates/soma-codemode/src/host_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/javy.rs` | `crates/soma-codemode/src/javy.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/javy_tests.rs` | `crates/soma-codemode/src/javy_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/lib.rs` | `crates/soma-codemode/src/lib.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/lib_tests.rs` | `crates/soma-codemode/src/lib_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/local_provider.rs` | `crates/soma-codemode/src/local_provider.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/local_provider_tests.rs` | `crates/soma-codemode/src/local_provider_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/normalize.rs` | `crates/soma-codemode/src/normalize.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/normalize_tests.rs` | `crates/soma-codemode/src/normalize_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/openapi_feature.rs` | `crates/soma-codemode/src/openapi_feature.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/openapi_feature_tests.rs` | `crates/soma-codemode/src/openapi_feature_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/path_safety.rs` | `crates/soma-codemode/src/path_safety.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/path_safety_tests.rs` | `crates/soma-codemode/src/path_safety_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool.rs` | `crates/soma-codemode/src/pool.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/checkout.rs` | `crates/soma-codemode/src/pool/checkout.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/checkout_tests.rs` | `crates/soma-codemode/src/pool/checkout_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/config.rs` | `crates/soma-codemode/src/pool/config.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/config_tests.rs` | `crates/soma-codemode/src/pool/config_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/disposition.rs` | `crates/soma-codemode/src/pool/disposition.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/disposition_tests.rs` | `crates/soma-codemode/src/pool/disposition_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/job_guard.rs` | `crates/soma-codemode/src/pool/job_guard.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/job_guard_tests.rs` | `crates/soma-codemode/src/pool/job_guard_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/runner_handle.rs` | `crates/soma-codemode/src/pool/runner_handle.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool/runner_handle_tests.rs` | `crates/soma-codemode/src/pool/runner_handle_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/pool_tests.rs` | `crates/soma-codemode/src/pool_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble.rs` | `crates/soma-codemode/src/preamble.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/catalog.rs` | `crates/soma-codemode/src/preamble/catalog.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/catalog_tests.rs` | `crates/soma-codemode/src/preamble/catalog_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/discovery.rs` | `crates/soma-codemode/src/preamble/discovery.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/discovery_tests.rs` | `crates/soma-codemode/src/preamble/discovery_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/local.rs` | `crates/soma-codemode/src/preamble/local.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/local_tests.rs` | `crates/soma-codemode/src/preamble/local_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/names.rs` | `crates/soma-codemode/src/preamble/names.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/names_tests.rs` | `crates/soma-codemode/src/preamble/names_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/openapi.rs` | `crates/soma-codemode/src/preamble/openapi.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble/openapi_tests.rs` | `crates/soma-codemode/src/preamble/openapi_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/preamble_tests.rs` | `crates/soma-codemode/src/preamble_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/process_tree.rs` | `crates/soma-codemode/src/process_tree.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/process_tree/noop.rs` | `crates/soma-codemode/src/process_tree/noop.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/process_tree/noop_tests.rs` | `crates/soma-codemode/src/process_tree/noop_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/process_tree/unix.rs` | `crates/soma-codemode/src/process_tree/unix.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/process_tree/unix_tests.rs` | `crates/soma-codemode/src/process_tree/unix_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/process_tree/windows.rs` | `crates/soma-codemode/src/process_tree/windows.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/process_tree/windows_tests.rs` | `crates/soma-codemode/src/process_tree/windows_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/process_tree_tests.rs` | `crates/soma-codemode/src/process_tree_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/protocol.rs` | `crates/soma-codemode/src/protocol.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/protocol_tests.rs` | `crates/soma-codemode/src/protocol_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/redact.rs` | `crates/soma-codemode/src/redact.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/redact_tests.rs` | `crates/soma-codemode/src/redact_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner.rs` | `crates/soma-codemode/src/runner.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/jail.rs` | `crates/soma-codemode/src/runner/jail.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/jail_tests.rs` | `crates/soma-codemode/src/runner/jail_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/js_args.rs` | `crates/soma-codemode/src/runner/js_args.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/js_args_tests.rs` | `crates/soma-codemode/src/runner/js_args_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/limits.rs` | `crates/soma-codemode/src/runner/limits.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/limits_tests.rs` | `crates/soma-codemode/src/runner/limits_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/runtime.rs` | `crates/soma-codemode/src/runner/runtime.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/runtime_tests.rs` | `crates/soma-codemode/src/runner/runtime_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/steps.rs` | `crates/soma-codemode/src/runner/steps.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner/steps_tests.rs` | `crates/soma-codemode/src/runner/steps_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive.rs` | `crates/soma-codemode/src/runner_drive.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/artifact.rs` | `crates/soma-codemode/src/runner_drive/artifact.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/artifact_tests.rs` | `crates/soma-codemode/src/runner_drive/artifact_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/internal.rs` | `crates/soma-codemode/src/runner_drive/internal.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/internal_tests.rs` | `crates/soma-codemode/src/runner_drive/internal_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/limits.rs` | `crates/soma-codemode/src/runner_drive/limits.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/limits_tests.rs` | `crates/soma-codemode/src/runner_drive/limits_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/local.rs` | `crates/soma-codemode/src/runner_drive/local.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/local_tests.rs` | `crates/soma-codemode/src/runner_drive/local_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/openapi.rs` | `crates/soma-codemode/src/runner_drive/openapi.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/openapi_tests.rs` | `crates/soma-codemode/src/runner_drive/openapi_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/outcome.rs` | `crates/soma-codemode/src/runner_drive/outcome.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/outcome_tests.rs` | `crates/soma-codemode/src/runner_drive/outcome_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/snippet.rs` | `crates/soma-codemode/src/runner_drive/snippet.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/snippet_tests.rs` | `crates/soma-codemode/src/runner_drive/snippet_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/state.rs` | `crates/soma-codemode/src/runner_drive/state.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/state_tests.rs` | `crates/soma-codemode/src/runner_drive/state_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/step.rs` | `crates/soma-codemode/src/runner_drive/step.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/step_tests.rs` | `crates/soma-codemode/src/runner_drive/step_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/tool_call.rs` | `crates/soma-codemode/src/runner_drive/tool_call.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive/tool_call_tests.rs` | `crates/soma-codemode/src/runner_drive/tool_call_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_drive_tests.rs` | `crates/soma-codemode/src/runner_drive_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_exe.rs` | `crates/soma-codemode/src/runner_exe.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_exe_tests.rs` | `crates/soma-codemode/src/runner_exe_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_io.rs` | `crates/soma-codemode/src/runner_io.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_io_tests.rs` | `crates/soma-codemode/src/runner_io_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/runner_tests.rs` | `crates/soma-codemode/src/runner_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/schema.rs` | `crates/soma-codemode/src/schema.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/schema_tests.rs` | `crates/soma-codemode/src/schema_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/shape.rs` | `crates/soma-codemode/src/shape.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/shape_tests.rs` | `crates/soma-codemode/src/shape_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet.rs` | `crates/soma-codemode/src/snippet.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet/index.rs` | `crates/soma-codemode/src/snippet/index.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet/index_tests.rs` | `crates/soma-codemode/src/snippet/index_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet/io.rs` | `crates/soma-codemode/src/snippet/io.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet/io_tests.rs` | `crates/soma-codemode/src/snippet/io_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet/resolve.rs` | `crates/soma-codemode/src/snippet/resolve.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet/resolve_tests.rs` | `crates/soma-codemode/src/snippet/resolve_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet/store.rs` | `crates/soma-codemode/src/snippet/store.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet/store_tests.rs` | `crates/soma-codemode/src/snippet/store_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/snippet_tests.rs` | `crates/soma-codemode/src/snippet_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state.rs` | `crates/soma-codemode/src/state.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/path.rs` | `crates/soma-codemode/src/state/path.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/path_tests.rs` | `crates/soma-codemode/src/state/path_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/provider.rs` | `crates/soma-codemode/src/state/provider.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/provider_dispatch.rs` | `crates/soma-codemode/src/state/provider_dispatch.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/provider_dispatch_tests.rs` | `crates/soma-codemode/src/state/provider_dispatch_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/provider_tests.rs` | `crates/soma-codemode/src/state/provider_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/quota.rs` | `crates/soma-codemode/src/state/quota.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/quota_tests.rs` | `crates/soma-codemode/src/state/quota_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace.rs` | `crates/soma-codemode/src/state/workspace.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace_archive.rs` | `crates/soma-codemode/src/state/workspace_archive.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace_archive_tests.rs` | `crates/soma-codemode/src/state/workspace_archive_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace_edit.rs` | `crates/soma-codemode/src/state/workspace_edit.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace_edit_tests.rs` | `crates/soma-codemode/src/state/workspace_edit_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace_files.rs` | `crates/soma-codemode/src/state/workspace_files.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace_files_tests.rs` | `crates/soma-codemode/src/state/workspace_files_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace_meta.rs` | `crates/soma-codemode/src/state/workspace_meta.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace_meta_tests.rs` | `crates/soma-codemode/src/state/workspace_meta_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state/workspace_tests.rs` | `crates/soma-codemode/src/state/workspace_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/state_tests.rs` | `crates/soma-codemode/src/state_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/trace.rs` | `crates/soma-codemode/src/trace.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/trace_tests.rs` | `crates/soma-codemode/src/trace_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/truncate.rs` | `crates/soma-codemode/src/truncate.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/truncate_tests.rs` | `crates/soma-codemode/src/truncate_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/ts_signatures.rs` | `crates/soma-codemode/src/ts_signatures.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/ts_signatures_tests.rs` | `crates/soma-codemode/src/ts_signatures_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types.rs` | `crates/soma-codemode/src/types.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/caller.rs` | `crates/soma-codemode/src/types/caller.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/caller_tests.rs` | `crates/soma-codemode/src/types/caller_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/catalog.rs` | `crates/soma-codemode/src/types/catalog.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/catalog_tests.rs` | `crates/soma-codemode/src/types/catalog_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/history.rs` | `crates/soma-codemode/src/types/history.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/history_tests.rs` | `crates/soma-codemode/src/types/history_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/id.rs` | `crates/soma-codemode/src/types/id.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/id_tests.rs` | `crates/soma-codemode/src/types/id_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/response.rs` | `crates/soma-codemode/src/types/response.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/response_tests.rs` | `crates/soma-codemode/src/types/response_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/scope.rs` | `crates/soma-codemode/src/types/scope.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types/scope_tests.rs` | `crates/soma-codemode/src/types/scope_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/types_tests.rs` | `crates/soma-codemode/src/types_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/util.rs` | `crates/soma-codemode/src/util.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/util_tests.rs` | `crates/soma-codemode/src/util_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/wrapper.rs` | `crates/soma-codemode/src/wrapper.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codemode/src/wrapper_tests.rs` | `crates/soma-codemode/src/wrapper_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/Cargo.toml` | `crates/codex-app-server-client/Cargo.toml` | Reusable shared crate | PR #139, #140 |
| renamed | `crates/shared/codex-app-server-client/README.md` | `crates/codex-app-server-client/README.md` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/build.rs` | `crates/codex-app-server-client/build.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/examples/approval_handler.rs` | `crates/codex-app-server-client/examples/approval_handler.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/examples/basic.rs` | `crates/codex-app-server-client/examples/basic.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/examples/compatibility.rs` | `crates/codex-app-server-client/examples/compatibility.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/examples/daemon.rs` | `crates/codex-app-server-client/examples/daemon.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/examples/rest_server.rs` | `crates/codex-app-server-client/examples/rest_server.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/examples/session_turn.rs` | `crates/codex-app-server-client/examples/session_turn.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/schema/CODEX_VERSION.txt` | `crates/codex-app-server-client/schema/CODEX_VERSION.txt` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/schema/methods.json` | `crates/codex-app-server-client/schema/methods.json` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/schema/protocol.schema.json` | `crates/codex-app-server-client/schema/protocol.schema.json` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/approvals.rs` | `crates/codex-app-server-client/src/approvals.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/build_support.rs` | `crates/codex-app-server-client/src/build_support.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/builders.rs` | `crates/codex-app-server-client/src/builders.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/client.rs` | `crates/codex-app-server-client/src/client.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/client/dispatch.rs` | `crates/codex-app-server-client/src/client/dispatch.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/compat.rs` | `crates/codex-app-server-client/src/compat.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/daemon.rs` | `crates/codex-app-server-client/src/daemon.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/error.rs` | `crates/codex-app-server-client/src/error.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/events.rs` | `crates/codex-app-server-client/src/events.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/lib.rs` | `crates/codex-app-server-client/src/lib.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/protocol.rs` | `crates/codex-app-server-client/src/protocol.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/rest.rs` | `crates/codex-app-server-client/src/rest.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/rest/backend.rs` | `crates/codex-app-server-client/src/rest/backend.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/rest/routes.rs` | `crates/codex-app-server-client/src/rest/routes.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/rest/types.rs` | `crates/codex-app-server-client/src/rest/types.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/session.rs` | `crates/codex-app-server-client/src/session.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/src/transport.rs` | `crates/codex-app-server-client/src/transport.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/tests/batteries.rs` | `crates/codex-app-server-client/tests/batteries.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/tests/rest.rs` | `crates/codex-app-server-client/tests/rest.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/codex-app-server-client/tests/smoke.rs` | `crates/codex-app-server-client/tests/smoke.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/mcp/client/AGENTS.md` | `crates/soma-gateway/AGENTS.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/CLAUDE.md` | `crates/soma-mcp-client/CLAUDE.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/Cargo.toml` | `crates/soma-mcp-client/Cargo.toml` | Reusable MCP role crate | PR #139, #140 |
| renamed | `crates/shared/mcp/client/GEMINI.md` | `crates/soma-gateway/GEMINI.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/config.rs` | `crates/soma-mcp-client/src/config.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/config_tests.rs` | `crates/soma-mcp-client/src/config_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/lib.rs` | `crates/soma-mcp-client/src/lib.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/lib_tests.rs` | `crates/soma-mcp-client/src/lib_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/net.rs` | `crates/soma-mcp-client/src/net.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/net_tests.rs` | `crates/soma-mcp-client/src/net_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/oauth.rs` | `crates/soma-mcp-client/src/oauth.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/oauth_tests.rs` | `crates/soma-mcp-client/src/oauth_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process.rs` | `crates/soma-mcp-client/src/process.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process/guard.rs` | `crates/soma-mcp-client/src/process/guard.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process/guard_tests.rs` | `crates/soma-mcp-client/src/process/guard_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process/stderr.rs` | `crates/soma-mcp-client/src/process/stderr.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process/stderr_tests.rs` | `crates/soma-mcp-client/src/process/stderr_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process/stdio.rs` | `crates/soma-mcp-client/src/process/stdio.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process/stdio_tests.rs` | `crates/soma-mcp-client/src/process/stdio_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process/windows.rs` | `crates/soma-mcp-client/src/process/windows.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process/windows_tests.rs` | `crates/soma-mcp-client/src/process/windows_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/process_tests.rs` | `crates/soma-mcp-client/src/process_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/security.rs` | `crates/soma-mcp-client/src/security.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/security/env.rs` | `crates/soma-mcp-client/src/security/env.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/security/env_tests.rs` | `crates/soma-mcp-client/src/security/env_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/security/redact.rs` | `crates/soma-mcp-client/src/security/redact.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/security/redact_tests.rs` | `crates/soma-mcp-client/src/security/redact_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/security/ssrf.rs` | `crates/soma-mcp-client/src/security/ssrf.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/security/ssrf_tests.rs` | `crates/soma-mcp-client/src/security/ssrf_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/security_tests.rs` | `crates/soma-mcp-client/src/security_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream.rs` | `crates/soma-mcp-client/src/upstream.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/http_body_cap.rs` | `crates/soma-mcp-client/src/upstream/http_body_cap.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/http_body_cap_tests.rs` | `crates/soma-mcp-client/src/upstream/http_body_cap_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/http_client.rs` | `crates/soma-mcp-client/src/upstream/http_client.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/http_client_tests.rs` | `crates/soma-mcp-client/src/upstream/http_client_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool.rs` | `crates/soma-mcp-client/src/upstream/pool.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/connect_stdio.rs` | `crates/soma-mcp-client/src/upstream/pool/connect_stdio.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/connect_stdio_tests.rs` | `crates/soma-mcp-client/src/upstream/pool/connect_stdio_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/discovery.rs` | `crates/soma-mcp-client/src/upstream/pool/discovery.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/discovery_tests.rs` | `crates/soma-mcp-client/src/upstream/pool/discovery_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/health.rs` | `crates/soma-mcp-client/src/upstream/pool/health.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/health_tests.rs` | `crates/soma-mcp-client/src/upstream/pool/health_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/live.rs` | `crates/soma-mcp-client/src/upstream/pool/live.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/live_tests.rs` | `crates/soma-mcp-client/src/upstream/pool/live_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/prompts.rs` | `crates/soma-mcp-client/src/upstream/pool/prompts.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/prompts_tests.rs` | `crates/soma-mcp-client/src/upstream/pool/prompts_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/resources.rs` | `crates/soma-mcp-client/src/upstream/pool/resources.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/resources_tests.rs` | `crates/soma-mcp-client/src/upstream/pool/resources_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/subject.rs` | `crates/soma-mcp-client/src/upstream/pool/subject.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/subject_tests.rs` | `crates/soma-mcp-client/src/upstream/pool/subject_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/tools.rs` | `crates/soma-mcp-client/src/upstream/pool/tools.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool/tools_tests.rs` | `crates/soma-mcp-client/src/upstream/pool/tools_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/pool_tests.rs` | `crates/soma-mcp-client/src/upstream/pool_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/relay.rs` | `crates/soma-mcp-client/src/upstream/relay.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/relay/cache.rs` | `crates/soma-mcp-client/src/upstream/relay/cache.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/relay/cache_tests.rs` | `crates/soma-mcp-client/src/upstream/relay/cache_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/relay/lifecycle.rs` | `crates/soma-mcp-client/src/upstream/relay/lifecycle.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/relay/lifecycle_tests.rs` | `crates/soma-mcp-client/src/upstream/relay/lifecycle_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/relay/session.rs` | `crates/soma-mcp-client/src/upstream/relay/session.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/relay/session_tests.rs` | `crates/soma-mcp-client/src/upstream/relay/session_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/relay_tests.rs` | `crates/soma-mcp-client/src/upstream/relay_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/transport.rs` | `crates/soma-mcp-client/src/upstream/transport.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/transport/websocket.rs` | `crates/soma-mcp-client/src/upstream/transport/websocket.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/transport/websocket_tests.rs` | `crates/soma-mcp-client/src/upstream/transport/websocket_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream/transport_tests.rs` | `crates/soma-mcp-client/src/upstream/transport_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/client/src/upstream_tests.rs` | `crates/soma-mcp-client/src/upstream_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/AGENTS.md` | `crates/soma-mcp-client/AGENTS.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/CLAUDE.md` | `crates/soma-gateway/CLAUDE.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/Cargo.toml` | `crates/soma-gateway/Cargo.toml` | Reusable MCP role crate | PR #139, #140 |
| renamed | `crates/shared/mcp/gateway/GEMINI.md` | `crates/soma-mcp-client/GEMINI.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/codemode_journal.rs` | `crates/soma-gateway/src/codemode_journal.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/codemode_journal_tests.rs` | `crates/soma-gateway/src/codemode_journal_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/config.rs` | `crates/soma-gateway/src/config.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/config/defaults.rs` | `crates/soma-gateway/src/config/defaults.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/config/defaults_tests.rs` | `crates/soma-gateway/src/config/defaults_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/config/protected_routes.rs` | `crates/soma-gateway/src/config/protected_routes.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/config/protected_routes_tests.rs` | `crates/soma-gateway/src/config/protected_routes_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/config/virtual_servers.rs` | `crates/soma-gateway/src/config/virtual_servers.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/config/virtual_servers_tests.rs` | `crates/soma-gateway/src/config/virtual_servers_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/config_tests.rs` | `crates/soma-gateway/src/config_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/dispatch_helpers.rs` | `crates/soma-gateway/src/dispatch_helpers.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/dispatch_helpers_tests.rs` | `crates/soma-gateway/src/dispatch_helpers_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway.rs` | `crates/soma-gateway/src/gateway.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/catalog.rs` | `crates/soma-gateway/src/gateway/catalog.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/catalog_tests.rs` | `crates/soma-gateway/src/gateway/catalog_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/code_mode.rs` | `crates/soma-gateway/src/gateway/code_mode.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/code_mode/catalog.rs` | `crates/soma-gateway/src/gateway/code_mode/catalog.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/code_mode/catalog_tests.rs` | `crates/soma-gateway/src/gateway/code_mode/catalog_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/code_mode/host.rs` | `crates/soma-gateway/src/gateway/code_mode/host.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/code_mode/host_tests.rs` | `crates/soma-gateway/src/gateway/code_mode/host_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/code_mode_tests.rs` | `crates/soma-gateway/src/gateway/code_mode_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/config_store.rs` | `crates/soma-gateway/src/gateway/config_store.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/config_store_tests.rs` | `crates/soma-gateway/src/gateway/config_store_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/dispatch.rs` | `crates/soma-gateway/src/gateway/dispatch.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/dispatch_tests.rs` | `crates/soma-gateway/src/gateway/dispatch_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager.rs` | `crates/soma-gateway/src/gateway/manager.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/core.rs` | `crates/soma-gateway/src/gateway/manager/core.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/core_tests.rs` | `crates/soma-gateway/src/gateway/manager/core_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/mcp_routes.rs` | `crates/soma-gateway/src/gateway/manager/mcp_routes.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/mcp_routes_tests.rs` | `crates/soma-gateway/src/gateway/manager/mcp_routes_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/mcp_scoped_routes.rs` | `crates/soma-gateway/src/gateway/manager/mcp_scoped_routes.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/mcp_scoped_routes_tests.rs` | `crates/soma-gateway/src/gateway/manager/mcp_scoped_routes_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/oauth_lifecycle.rs` | `crates/soma-gateway/src/gateway/manager/oauth_lifecycle.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/oauth_lifecycle_tests.rs` | `crates/soma-gateway/src/gateway/manager/oauth_lifecycle_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/pool_lifecycle.rs` | `crates/soma-gateway/src/gateway/manager/pool_lifecycle.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/pool_lifecycle_tests.rs` | `crates/soma-gateway/src/gateway/manager/pool_lifecycle_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/protected_routes.rs` | `crates/soma-gateway/src/gateway/manager/protected_routes.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/protected_routes_tests.rs` | `crates/soma-gateway/src/gateway/manager/protected_routes_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/virtual_servers.rs` | `crates/soma-gateway/src/gateway/manager/virtual_servers.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager/virtual_servers_tests.rs` | `crates/soma-gateway/src/gateway/manager/virtual_servers_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/manager_tests.rs` | `crates/soma-gateway/src/gateway/manager_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/oauth.rs` | `crates/soma-gateway/src/gateway/oauth.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/oauth_tests.rs` | `crates/soma-gateway/src/gateway/oauth_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/openapi.rs` | `crates/soma-gateway/src/gateway/openapi.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/openapi_tests.rs` | `crates/soma-gateway/src/gateway/openapi_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/palette.rs` | `crates/soma-gateway/src/gateway/palette.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/palette_tests.rs` | `crates/soma-gateway/src/gateway/palette_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/params.rs` | `crates/soma-gateway/src/gateway/params.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/params_tests.rs` | `crates/soma-gateway/src/gateway/params_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/projection.rs` | `crates/soma-gateway/src/gateway/projection.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/projection_tests.rs` | `crates/soma-gateway/src/gateway/projection_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/protected_routes.rs` | `crates/soma-gateway/src/gateway/protected_routes.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/protected_routes_tests.rs` | `crates/soma-gateway/src/gateway/protected_routes_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/runtime.rs` | `crates/soma-gateway/src/gateway/runtime.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/runtime_tests.rs` | `crates/soma-gateway/src/gateway/runtime_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/view_models.rs` | `crates/soma-gateway/src/gateway/view_models.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/view_models_tests.rs` | `crates/soma-gateway/src/gateway/view_models_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/virtual_servers.rs` | `crates/soma-gateway/src/gateway/virtual_servers.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway/virtual_servers_tests.rs` | `crates/soma-gateway/src/gateway/virtual_servers_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/gateway_tests.rs` | `crates/soma-gateway/src/gateway_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/lib.rs` | `crates/soma-gateway/src/lib.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/lib_tests.rs` | `crates/soma-gateway/src/lib_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/registry.rs` | `crates/soma-gateway/src/registry.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/registry_tests.rs` | `crates/soma-gateway/src/registry_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/usage.rs` | `crates/soma-gateway/src/usage.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/gateway/src/usage_tests.rs` | `crates/soma-gateway/src/usage_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/proxy/AGENTS.md` | `crates/soma-mcp-proxy/AGENTS.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/proxy/CLAUDE.md` | `crates/soma-mcp-proxy/CLAUDE.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/proxy/Cargo.toml` | `crates/soma-mcp-proxy/Cargo.toml` | Reusable MCP role crate | PR #139, #140 |
| renamed | `crates/shared/mcp/proxy/GEMINI.md` | `crates/soma-mcp-proxy/GEMINI.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/proxy/src/lib.rs` | `crates/soma-mcp-proxy/src/lib.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/proxy/src/lib_tests.rs` | `crates/soma-mcp-proxy/src/lib_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/server/AGENTS.md` | `crates/soma-mcp-server/AGENTS.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/server/CLAUDE.md` | `crates/soma-mcp-server/CLAUDE.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/server/Cargo.toml` | `crates/soma-mcp-server/Cargo.toml` | Reusable MCP role crate | PR #139, #140 |
| renamed | `crates/shared/mcp/server/GEMINI.md` | `crates/soma-mcp-server/GEMINI.md` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/server/src/lib.rs` | `crates/soma-mcp-server/src/lib.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/server/src/lib_tests.rs` | `crates/soma-mcp-server/src/lib_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/server/src/response_paging.rs` | `crates/soma-mcp-server/src/response_paging.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/mcp/server/src/response_paging_tests.rs` | `crates/soma-mcp-server/src/response_paging_tests.rs` | Reusable MCP role crate | PR #139 |
| renamed | `crates/shared/observability/Cargo.toml` | `crates/soma-observability/Cargo.toml` | Reusable shared crate | PR #139, #140 |
| renamed | `crates/shared/observability/src/binary_status.rs` | `crates/soma-observability/src/binary_status.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/binary_status_tests.rs` | `crates/soma-observability/src/binary_status_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/lib.rs` | `crates/soma-observability/src/lib.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/logging.rs` | `crates/soma-observability/src/logging.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/logging/aurora.rs` | `crates/soma-observability/src/logging/aurora.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/logging/aurora_tests.rs` | `crates/soma-observability/src/logging/aurora_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/logging/formatter.rs` | `crates/soma-observability/src/logging/formatter.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/logging/formatter_tests.rs` | `crates/soma-observability/src/logging/formatter_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/logging_tests.rs` | `crates/soma-observability/src/logging_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/metrics.rs` | `crates/soma-observability/src/metrics.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/observability/src/metrics_tests.rs` | `crates/soma-observability/src/metrics_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/AGENTS.md` | `crates/soma-openapi/AGENTS.md` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/CLAUDE.md` | `crates/soma-openapi/CLAUDE.md` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/Cargo.toml` | `crates/soma-openapi/Cargo.toml` | Reusable shared crate | PR #139, #140 |
| renamed | `crates/shared/openapi/GEMINI.md` | `crates/soma-openapi/GEMINI.md` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/config.rs` | `crates/soma-openapi/src/config.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/config_tests.rs` | `crates/soma-openapi/src/config_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/convert.rs` | `crates/soma-openapi/src/convert.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/convert_tests.rs` | `crates/soma-openapi/src/convert_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/dispatch.rs` | `crates/soma-openapi/src/dispatch.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/dispatch_tests.rs` | `crates/soma-openapi/src/dispatch_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/error.rs` | `crates/soma-openapi/src/error.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/error_tests.rs` | `crates/soma-openapi/src/error_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http.rs` | `crates/soma-openapi/src/http.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http/body.rs` | `crates/soma-openapi/src/http/body.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http/body_tests.rs` | `crates/soma-openapi/src/http/body_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http/client.rs` | `crates/soma-openapi/src/http/client.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http/client_tests.rs` | `crates/soma-openapi/src/http/client_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http/params.rs` | `crates/soma-openapi/src/http/params.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http/params_tests.rs` | `crates/soma-openapi/src/http/params_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http/resolve.rs` | `crates/soma-openapi/src/http/resolve.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http/resolve_tests.rs` | `crates/soma-openapi/src/http/resolve_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/http_tests.rs` | `crates/soma-openapi/src/http_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/lib.rs` | `crates/soma-openapi/src/lib.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/lib_tests.rs` | `crates/soma-openapi/src/lib_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/registry.rs` | `crates/soma-openapi/src/registry.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/registry_tests.rs` | `crates/soma-openapi/src/registry_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/ssrf.rs` | `crates/soma-openapi/src/ssrf.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/src/ssrf_tests.rs` | `crates/soma-openapi/src/ssrf_tests.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/openapi/tests/live_petstore.rs` | `crates/soma-openapi/tests/live_petstore.rs` | Reusable shared crate | PR #139 |
| created | `crates/shared/provider-core/Cargo.toml` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/README.md` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/provider-manifest.schema.json` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/call.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/error.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/id.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/lib.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/manifest.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/output.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/provider.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/registry.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/registry/builder.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/registry/dispatch.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/registry/fingerprint.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/registry/index.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/registry/snapshot.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/surface.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/src/validation.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/fake_provider.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/fixtures/hello_static_omitted_fields.json` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/fixtures/pre_extraction_catalogs.json` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/fixtures/pre_extraction_hello_static_catalogs.json` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/manifest_compatibility.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/registry_dispatch.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/registry_duplicates.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/registry_fingerprint.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/registry_surfaces.rs` | - | Reusable shared crate | PR #149 |
| created | `crates/shared/provider-core/tests/transport_boundary.rs` | - | Reusable shared crate | PR #149 |
| renamed | `crates/shared/traces/Cargo.toml` | `crates/rmcp-traces/Cargo.toml` | Reusable shared crate | PR #139, #140 |
| renamed | `crates/shared/traces/README.md` | `crates/rmcp-traces/README.md` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/traces/src/http.rs` | `crates/rmcp-traces/src/http.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/traces/src/lib.rs` | `crates/rmcp-traces/src/lib.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/traces/src/trace_context.rs` | `crates/rmcp-traces/src/trace_context.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/traces/tests/core_trace_context.rs` | `crates/rmcp-traces/tests/core_trace_context.rs` | Reusable shared crate | PR #139 |
| renamed | `crates/shared/traces/tests/http_propagation.rs` | `crates/rmcp-traces/tests/http_propagation.rs` | Reusable shared crate | PR #139 |
| modified | `crates/soma-api/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #137 |
| modified | `crates/soma-api/src/api.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| modified | `crates/soma-api/src/api_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/gateway.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/gateway_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| modified | `crates/soma-api/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/openapi.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/openapi_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/probes.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/probes_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/responses.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/responses_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/route_inventory.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-api/src/route_inventory_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| modified | `crates/soma-auth/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137, #141 |
| created | `crates/soma-codemode/AGENTS.md` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/CLAUDE.md` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/GEMINI.md` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/artifacts.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/artifacts/path.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/artifacts/path_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/artifacts/prune.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/artifacts/prune_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/artifacts/store.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/artifacts/store_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/artifacts_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/bin/soma-codemode-runner.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/bin/soma-codemode-runner_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/broker.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/broker_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/config.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/config_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/error.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/error_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/budget.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/budget_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/call_tool.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/call_tool_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/discovery.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/discovery_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/internal.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/internal_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/proxy.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/proxy_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/result.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/result_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/runner.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/runner_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/tool_dispatch.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute/tool_dispatch_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/execute_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/command.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/command_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/output.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/output_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/provider.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/provider_dispatch.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/provider_dispatch_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/provider_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/safety.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git/safety_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/git_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/home.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/home_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/host.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/host_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/javy.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/javy_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/lib_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/local_provider.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/local_provider_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/normalize.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/normalize_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/openapi_feature.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/openapi_feature_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/path_safety.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/path_safety_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/checkout.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/checkout_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/config.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/config_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/disposition.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/disposition_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/job_guard.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/job_guard_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/runner_handle.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool/runner_handle_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/pool_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/catalog.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/catalog_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/discovery.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/discovery_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/local.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/local_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/names.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/names_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/openapi.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble/openapi_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/preamble_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/process_tree.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/process_tree/noop.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/process_tree/noop_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/process_tree/unix.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/process_tree/unix_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/process_tree/windows.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/process_tree/windows_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/process_tree_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/protocol.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/protocol_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/redact.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/redact_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/jail.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/jail_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/js_args.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/js_args_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/limits.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/limits_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/runtime.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/runtime_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/steps.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner/steps_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/artifact.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/artifact_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/internal.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/internal_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/limits.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/limits_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/local.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/local_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/openapi.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/openapi_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/outcome.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/outcome_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/snippet.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/snippet_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/state.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/state_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/step.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/step_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/tool_call.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive/tool_call_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_drive_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_exe.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_exe_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_io.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_io_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/runner_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/schema.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/schema_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/shape.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/shape_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet/index.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet/index_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet/io.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet/io_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet/resolve.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet/resolve_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet/store.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet/store_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/snippet_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/path.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/path_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/provider.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/provider_dispatch.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/provider_dispatch_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/provider_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/quota.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/quota_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace_archive.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace_archive_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace_edit.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace_edit_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace_files.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace_files_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace_meta.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace_meta_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state/workspace_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/state_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/trace.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/trace_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/truncate.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/truncate_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/ts_signatures.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/ts_signatures_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/caller.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/caller_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/catalog.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/catalog_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/history.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/history_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/id.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/id_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/response.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/response_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/scope.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types/scope_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/types_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/util.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/util_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/wrapper.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-codemode/src/wrapper_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| modified | `crates/soma-contracts/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-contracts/src/scopes.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-contracts/src/scopes_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/AGENTS.md` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/CLAUDE.md` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/GEMINI.md` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/codemode_journal.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/codemode_journal_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/config.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/config/defaults.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/config/defaults_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/config/protected_routes.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/config/protected_routes_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/config/upstream.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/config/upstream_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/config/virtual_servers.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/config/virtual_servers_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/config_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/dispatch_helpers.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/dispatch_helpers_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/catalog.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/catalog_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/code_mode.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/code_mode/catalog.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/code_mode/catalog_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/code_mode/host.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/code_mode/host_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/code_mode_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/config_store.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/config_store_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/dispatch.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/dispatch_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/manager.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/manager/core.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/manager/core_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager/mcp_routes.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/manager/mcp_routes_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/manager/mcp_scoped_routes.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager/mcp_scoped_routes_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager/oauth_lifecycle.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/manager/oauth_lifecycle_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager/pool_lifecycle.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager/pool_lifecycle_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager/protected_routes.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager/protected_routes_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager/virtual_servers.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager/virtual_servers_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/manager_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/oauth.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/oauth_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/openapi.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/gateway/openapi_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/palette.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/palette_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/params.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/params_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/projection.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/projection_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/protected_routes.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/protected_routes_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/runtime.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/runtime_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/view_models.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/view_models_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/virtual_servers.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway/virtual_servers_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/gateway_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| created | `crates/soma-gateway/src/lib_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/net.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/net_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process/guard.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process/guard_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process/stderr.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process/stderr_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process/stdio.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process/stdio_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process/windows.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process/windows_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/process_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/registry.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/registry_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/security.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/security/env.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/security/env_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/security/redact.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/security/redact_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/security/ssrf.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/security/ssrf_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/security_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/http_body_cap.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/http_body_cap_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/http_client.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/http_client_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/connect_stdio.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/connect_stdio_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/discovery.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/discovery_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/health.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/health_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/live.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/live_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/prompts.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/prompts_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/resources.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/resources_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/subject.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/subject_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/tools.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool/tools_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/pool_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/relay.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/relay/cache.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/relay/cache_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/relay/lifecycle.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/relay/lifecycle_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/relay/session.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/relay/session_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/relay_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/transport.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/transport/websocket.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/transport/websocket_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream/transport_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/upstream_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/usage.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-gateway/src/usage_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-mcp-client/AGENTS.md` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-client/CLAUDE.md` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-client/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-client/GEMINI.md` | - | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/config.rs` | `crates/soma-gateway/src/config/upstream.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/config_tests.rs` | `crates/soma-gateway/src/config/upstream_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-client/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-client/src/lib_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/net.rs` | `crates/soma-gateway/src/net.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/net_tests.rs` | `crates/soma-gateway/src/net_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-client/src/oauth.rs` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-client/src/oauth_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process.rs` | `crates/soma-gateway/src/process.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process/guard.rs` | `crates/soma-gateway/src/process/guard.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process/guard_tests.rs` | `crates/soma-gateway/src/process/guard_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process/stderr.rs` | `crates/soma-gateway/src/process/stderr.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process/stderr_tests.rs` | `crates/soma-gateway/src/process/stderr_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process/stdio.rs` | `crates/soma-gateway/src/process/stdio.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process/stdio_tests.rs` | `crates/soma-gateway/src/process/stdio_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process/windows.rs` | `crates/soma-gateway/src/process/windows.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process/windows_tests.rs` | `crates/soma-gateway/src/process/windows_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/process_tests.rs` | `crates/soma-gateway/src/process_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/security.rs` | `crates/soma-gateway/src/security.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/security/env.rs` | `crates/soma-gateway/src/security/env.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/security/env_tests.rs` | `crates/soma-gateway/src/security/env_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/security/redact.rs` | `crates/soma-gateway/src/security/redact.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/security/redact_tests.rs` | `crates/soma-gateway/src/security/redact_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/security/ssrf.rs` | `crates/soma-gateway/src/security/ssrf.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/security/ssrf_tests.rs` | `crates/soma-gateway/src/security/ssrf_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/security_tests.rs` | `crates/soma-gateway/src/security_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream.rs` | `crates/soma-gateway/src/upstream.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/http_body_cap.rs` | `crates/soma-gateway/src/upstream/http_body_cap.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/http_body_cap_tests.rs` | `crates/soma-gateway/src/upstream/http_body_cap_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/http_client.rs` | `crates/soma-gateway/src/upstream/http_client.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/http_client_tests.rs` | `crates/soma-gateway/src/upstream/http_client_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool.rs` | `crates/soma-gateway/src/upstream/pool.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/connect_stdio.rs` | `crates/soma-gateway/src/upstream/pool/connect_stdio.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/connect_stdio_tests.rs` | `crates/soma-gateway/src/upstream/pool/connect_stdio_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/discovery.rs` | `crates/soma-gateway/src/upstream/pool/discovery.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/discovery_tests.rs` | `crates/soma-gateway/src/upstream/pool/discovery_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/health.rs` | `crates/soma-gateway/src/upstream/pool/health.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/health_tests.rs` | `crates/soma-gateway/src/upstream/pool/health_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/live.rs` | `crates/soma-gateway/src/upstream/pool/live.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/live_tests.rs` | `crates/soma-gateway/src/upstream/pool/live_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/prompts.rs` | `crates/soma-gateway/src/upstream/pool/prompts.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/prompts_tests.rs` | `crates/soma-gateway/src/upstream/pool/prompts_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/resources.rs` | `crates/soma-gateway/src/upstream/pool/resources.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/resources_tests.rs` | `crates/soma-gateway/src/upstream/pool/resources_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/subject.rs` | `crates/soma-gateway/src/upstream/pool/subject.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/subject_tests.rs` | `crates/soma-gateway/src/upstream/pool/subject_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/tools.rs` | `crates/soma-gateway/src/upstream/pool/tools.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool/tools_tests.rs` | `crates/soma-gateway/src/upstream/pool/tools_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/pool_tests.rs` | `crates/soma-gateway/src/upstream/pool_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/relay.rs` | `crates/soma-gateway/src/upstream/relay.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/relay/cache.rs` | `crates/soma-gateway/src/upstream/relay/cache.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/relay/cache_tests.rs` | `crates/soma-gateway/src/upstream/relay/cache_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/relay/lifecycle.rs` | `crates/soma-gateway/src/upstream/relay/lifecycle.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/relay/lifecycle_tests.rs` | `crates/soma-gateway/src/upstream/relay/lifecycle_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/relay/session.rs` | `crates/soma-gateway/src/upstream/relay/session.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/relay/session_tests.rs` | `crates/soma-gateway/src/upstream/relay/session_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/relay_tests.rs` | `crates/soma-gateway/src/upstream/relay_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/transport.rs` | `crates/soma-gateway/src/upstream/transport.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/transport/websocket.rs` | `crates/soma-gateway/src/upstream/transport/websocket.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/transport/websocket_tests.rs` | `crates/soma-gateway/src/upstream/transport/websocket_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream/transport_tests.rs` | `crates/soma-gateway/src/upstream/transport_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-client/src/upstream_tests.rs` | `crates/soma-gateway/src/upstream_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-proxy/AGENTS.md` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-proxy/CLAUDE.md` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-proxy/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-proxy/GEMINI.md` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-proxy/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-proxy/src/lib_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-server/AGENTS.md` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-server/CLAUDE.md` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-server/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-server/GEMINI.md` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-server/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-mcp-server/src/lib_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-server/src/response_paging.rs` | `crates/soma-mcp/src/response_paging.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| renamed | `crates/soma-mcp-server/src/response_paging_tests.rs` | `crates/soma-mcp/src/response_paging_tests.rs` | Workspace configuration, source, test, or documentation | PR #141 |
| modified | `crates/soma-mcp/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137, #141 |
| created | `crates/soma-mcp/src/gateway_proxy.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-mcp/src/gateway_proxy_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| modified | `crates/soma-mcp/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137, #141 |
| modified | `crates/soma-mcp/src/response_paging.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135 |
| modified | `crates/soma-mcp/src/response_paging_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135 |
| created | `crates/soma-mcp/src/rmcp_adapters.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-mcp/src/rmcp_adapters_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-mcp/src/rmcp_auth.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| created | `crates/soma-mcp/src/rmcp_auth_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137 |
| modified | `crates/soma-mcp/src/rmcp_server.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137, #141 |
| modified | `crates/soma-mcp/src/rmcp_server_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137, #141 |
| modified | `crates/soma-mcp/src/schemas.rs` | - | Workspace configuration, source, test, or documentation | PR #141 |
| created | `crates/soma-openapi/AGENTS.md` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/CLAUDE.md` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/GEMINI.md` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/config.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/config_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/convert.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/convert_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/dispatch.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/dispatch_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/error.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/error_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http/body.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http/body_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http/client.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http/client_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http/params.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http/params_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http/resolve.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http/resolve_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/http_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/lib_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/registry.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/registry_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/ssrf.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/src/ssrf_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-openapi/tests/live_petstore.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-plugin-support/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| created | `crates/soma-plugin-support/src/lib.rs` | - | Workspace configuration, source, test, or documentation | PR #135, #141 |
| modified | `crates/soma-runtime/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| modified | `crates/soma-runtime/src/server.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| modified | `crates/soma-runtime/src/server_tests.rs` | - | Workspace configuration, source, test, or documentation | PR #137, #141 |
| modified | `crates/soma-service/Cargo.toml` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #141 |
| modified | `crates/soma-service/src/providers/filesystem_resources.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137 |
| modified | `crates/soma-service/src/providers/resource_files.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137 |
| modified | `crates/soma-service/src/providers/resource_uri.rs` | - | Workspace configuration, source, test, or documentation | PR #134, #135, #137 |
| modified | `crates/soma/Cargo.toml` | - | Soma product crate | PR #134, #135, #137, #141 |
| renamed | `crates/soma/api/Cargo.toml` | `crates/soma-api/Cargo.toml` | Soma product crate | PR #139, #140, #145, #148 |
| renamed | `crates/soma/api/src/api.rs` | `crates/soma-api/src/api.rs` | Soma product crate | PR #139, #145 |
| renamed | `crates/soma/api/src/api_tests.rs` | `crates/soma-api/src/api_tests.rs` | Soma product crate | PR #139, #145 |
| renamed | `crates/soma/api/src/gateway.rs` | `crates/soma-api/src/gateway.rs` | Soma product crate | PR #139, #145 |
| renamed | `crates/soma/api/src/gateway_tests.rs` | `crates/soma-api/src/gateway_tests.rs` | Soma product crate | PR #139, #145 |
| renamed | `crates/soma/api/src/lib.rs` | `crates/soma-api/src/lib.rs` | Soma product crate | PR #139, #145 |
| renamed | `crates/soma/api/src/openapi.rs` | `crates/soma-api/src/openapi.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/api/src/openapi_tests.rs` | `crates/soma-api/src/openapi_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/api/src/probes.rs` | `crates/soma-api/src/probes.rs` | Soma product crate | PR #139, #145 |
| renamed | `crates/soma/api/src/probes_tests.rs` | `crates/soma-api/src/probes_tests.rs` | Soma product crate | PR #139, #145 |
| renamed | `crates/soma/api/src/responses.rs` | `crates/soma-api/src/responses.rs` | Soma product crate | PR #139, #145 |
| renamed | `crates/soma/api/src/responses_tests.rs` | `crates/soma-api/src/responses_tests.rs` | Soma product crate | PR #139, #145 |
| renamed | `crates/soma/api/src/route_inventory.rs` | `crates/soma-api/src/route_inventory.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/api/src/route_inventory_tests.rs` | `crates/soma-api/src/route_inventory_tests.rs` | Soma product crate | PR #139 |
| created | `crates/soma/api/src/state.rs` | - | Soma product crate | PR #145 |
| created | `crates/soma/api/src/state_tests.rs` | - | Soma product crate | PR #145 |
| created | `crates/soma/application/Cargo.toml` | - | Soma product crate | PR #143, #146, #148 |
| created | `crates/soma/application/src/app.rs` | - | Soma product crate | PR #143, #144, #145, #146 |
| created | `crates/soma/application/src/app_tests.rs` | - | Soma product crate | PR #143, #144, #145, #146, #149 |
| created | `crates/soma/application/src/context.rs` | - | Soma product crate | PR #143 |
| created | `crates/soma/application/src/error.rs` | - | Soma product crate | PR #143, #144, #146 |
| created | `crates/soma/application/src/lib.rs` | - | Soma product crate | PR #143, #144, #146 |
| created | `crates/soma/application/src/ports.rs` | - | Soma product crate | PR #143, #144, #145, #146 |
| created | `crates/soma/application/src/types.rs` | - | Soma product crate | PR #144, #146 |
| renamed | `crates/soma/cli/Cargo.toml` | `crates/soma-cli/Cargo.toml` | Soma product crate | PR #139, #140, #144, #148 |
| renamed | `crates/soma/cli/src/cli_tests.rs` | `crates/soma-cli/src/cli_tests.rs` | Soma product crate | PR #139, #144 |
| renamed | `crates/soma/cli/src/doctor.rs` | `crates/soma-cli/src/doctor.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/cli/src/doctor/checks.rs` | `crates/soma-cli/src/doctor/checks.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/cli/src/doctor/checks_tests.rs` | `crates/soma-cli/src/doctor/checks_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/cli/src/doctor_tests.rs` | `crates/soma-cli/src/doctor_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/cli/src/lib.rs` | `crates/soma-cli/src/lib.rs` | Soma product crate | PR #139, #144 |
| renamed | `crates/soma/cli/src/provider_command.rs` | `crates/soma-cli/src/provider_command.rs` | Soma product crate | PR #139, #144 |
| renamed | `crates/soma/cli/src/provider_command_tests.rs` | `crates/soma-cli/src/provider_command_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/cli/src/providers.rs` | `crates/soma-cli/src/providers.rs` | Soma product crate | PR #139, #144 |
| renamed | `crates/soma/cli/src/providers_tests.rs` | `crates/soma-cli/src/providers_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/cli/src/setup.rs` | `crates/soma-cli/src/setup.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/cli/src/setup_tests.rs` | `crates/soma-cli/src/setup_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/cli/src/watch.rs` | `crates/soma-cli/src/watch.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/cli/src/watch_tests.rs` | `crates/soma-cli/src/watch_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/Cargo.toml` | `crates/soma-contracts/Cargo.toml` | Soma product crate | PR #139, #140, #149 |
| renamed | `crates/soma/contracts/src/actions.rs` | `crates/soma-contracts/src/actions.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/actions_tests.rs` | `crates/soma-contracts/src/actions_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/config.rs` | `crates/soma-contracts/src/config.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/config_tests.rs` | `crates/soma-contracts/src/config_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/env_registry.rs` | `crates/soma-contracts/src/env_registry.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/env_registry_tests.rs` | `crates/soma-contracts/src/env_registry_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/errors.rs` | `crates/soma-contracts/src/errors.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/errors_tests.rs` | `crates/soma-contracts/src/errors_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/lib.rs` | `crates/soma-contracts/src/lib.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/provider_validation.rs` | `crates/soma-contracts/src/provider_validation.rs` | Soma product crate | PR #139, #149 |
| renamed | `crates/soma/contracts/src/provider_validation_tests.rs` | `crates/soma-contracts/src/provider_validation_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/providers.rs` | `crates/soma-contracts/src/providers.rs` | Soma product crate | PR #139, #149 |
| renamed | `crates/soma/contracts/src/providers_tests.rs` | `crates/soma-contracts/src/providers_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/scopes.rs` | `crates/soma-contracts/src/scopes.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/scopes_tests.rs` | `crates/soma-contracts/src/scopes_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/token_limit.rs` | `crates/soma-contracts/src/token_limit.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/contracts/src/token_limit_tests.rs` | `crates/soma-contracts/src/token_limit_tests.rs` | Soma product crate | PR #139 |
| created | `crates/soma/domain/Cargo.toml` | - | Soma product crate | PR #143 |
| created | `crates/soma/domain/src/execution.rs` | - | Soma product crate | PR #143 |
| created | `crates/soma/domain/src/lib.rs` | - | Soma product crate | PR #143 |
| created | `crates/soma/domain/src/principal.rs` | - | Soma product crate | PR #143, #146 |
| renamed | `crates/soma/mcp/Cargo.toml` | `crates/soma-mcp/Cargo.toml` | Soma product crate | PR #139, #140, #146, #148 |
| renamed | `crates/soma/mcp/src/conformance.rs` | `crates/soma-mcp/src/conformance.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/mcp/src/conformance_tests.rs` | `crates/soma-mcp/src/conformance_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/mcp/src/gateway_proxy.rs` | `crates/soma-mcp/src/gateway_proxy.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/gateway_proxy_tests.rs` | `crates/soma-mcp/src/gateway_proxy_tests.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/lib.rs` | `crates/soma-mcp/src/lib.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/mcp_tests.rs` | `crates/soma-mcp/src/mcp_tests.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/prompts.rs` | `crates/soma-mcp/src/prompts.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/prompts_tests.rs` | `crates/soma-mcp/src/prompts_tests.rs` | Soma product crate | PR #139, #146 |
| created | `crates/soma/mcp/src/protocol_errors.rs` | - | Soma product crate | PR #146 |
| created | `crates/soma/mcp/src/protocol_errors_tests.rs` | - | Soma product crate | PR #146 |
| renamed | `crates/soma/mcp/src/rmcp_adapters.rs` | `crates/soma-mcp/src/rmcp_adapters.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/rmcp_adapters_tests.rs` | `crates/soma-mcp/src/rmcp_adapters_tests.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/rmcp_auth.rs` | `crates/soma-mcp/src/rmcp_auth.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/rmcp_auth_tests.rs` | `crates/soma-mcp/src/rmcp_auth_tests.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/rmcp_server.rs` | `crates/soma-mcp/src/rmcp_server.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/rmcp_server_tests.rs` | `crates/soma-mcp/src/rmcp_server_tests.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/schemas.rs` | `crates/soma-mcp/src/schemas.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/schemas_tests.rs` | `crates/soma-mcp/src/schemas_tests.rs` | Soma product crate | PR #139 |
| created | `crates/soma/mcp/src/state.rs` | - | Soma product crate | PR #146 |
| created | `crates/soma/mcp/src/state_tests.rs` | - | Soma product crate | PR #146 |
| renamed | `crates/soma/mcp/src/tools.rs` | `crates/soma-mcp/src/tools.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/tools_tests.rs` | `crates/soma-mcp/src/tools_tests.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/transport.rs` | `crates/soma-mcp/src/transport.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/mcp/src/transport_tests.rs` | `crates/soma-mcp/src/transport_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/runtime/Cargo.toml` | `crates/soma-runtime/Cargo.toml` | Soma product crate | PR #139, #140, #148 |
| renamed | `crates/soma/runtime/src/lib.rs` | `crates/soma-runtime/src/lib.rs` | Soma product crate | PR #139, #148 |
| renamed | `crates/soma/runtime/src/server.rs` | `crates/soma-runtime/src/server.rs` | Soma product crate | PR #139, #148 |
| renamed | `crates/soma/runtime/src/server_tests.rs` | `crates/soma-runtime/src/server_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/Cargo.toml` | `crates/soma-service/Cargo.toml` | Soma product crate | PR #139, #140, #149 |
| renamed | `crates/soma/service/src/app.rs` | `crates/soma-service/src/app.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/app_tests.rs` | `crates/soma-service/src/app_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/capabilities.rs` | `crates/soma-service/src/capabilities.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/capabilities_tests.rs` | `crates/soma-service/src/capabilities_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/lib.rs` | `crates/soma-service/src/lib.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/provider_errors.rs` | `crates/soma-service/src/provider_errors.rs` | Soma product crate | PR #139, #146, #149 |
| renamed | `crates/soma/service/src/provider_errors_tests.rs` | `crates/soma-service/src/provider_errors_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/provider_registry.rs` | `crates/soma-service/src/provider_registry.rs` | Soma product crate | PR #139, #149 |
| renamed | `crates/soma/service/src/provider_registry/enforcement.rs` | `crates/soma-service/src/provider_registry/enforcement.rs` | Soma product crate | PR #139, #149 |
| renamed | `crates/soma/service/src/provider_registry/enforcement_tests.rs` | `crates/soma-service/src/provider_registry/enforcement_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/provider_registry/refresh.rs` | `crates/soma-service/src/provider_registry/refresh.rs` | Soma product crate | PR #139, #149 |
| renamed | `crates/soma/service/src/provider_registry/refresh_tests.rs` | `crates/soma-service/src/provider_registry/refresh_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/provider_registry/reports.rs` | `crates/soma-service/src/provider_registry/reports.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/provider_registry/reports_tests.rs` | `crates/soma-service/src/provider_registry/reports_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/provider_registry/resources.rs` | `crates/soma-service/src/provider_registry/resources.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/provider_registry/resources_tests.rs` | `crates/soma-service/src/provider_registry/resources_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/provider_registry_tests.rs` | `crates/soma-service/src/provider_registry_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers.rs` | `crates/soma-service/src/providers.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/ai_sdk.rs` | `crates/soma-service/src/providers/ai_sdk.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/ai_sdk_tests.rs` | `crates/soma-service/src/providers/ai_sdk_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/filesystem.rs` | `crates/soma-service/src/providers/filesystem.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/filesystem_prompts.rs` | `crates/soma-service/src/providers/filesystem_prompts.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/filesystem_prompts_tests.rs` | `crates/soma-service/src/providers/filesystem_prompts_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/filesystem_resources.rs` | `crates/soma-service/src/providers/filesystem_resources.rs` | Soma product crate | PR #139, #149 |
| renamed | `crates/soma/service/src/providers/filesystem_resources_tests.rs` | `crates/soma-service/src/providers/filesystem_resources_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/filesystem_tests.rs` | `crates/soma-service/src/providers/filesystem_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/filesystem_uniqueness.rs` | `crates/soma-service/src/providers/filesystem_uniqueness.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/filesystem_uniqueness_tests.rs` | `crates/soma-service/src/providers/filesystem_uniqueness_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/filesystem_wasm.rs` | `crates/soma-service/src/providers/filesystem_wasm.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/filesystem_wasm_tests.rs` | `crates/soma-service/src/providers/filesystem_wasm_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/mcp.rs` | `crates/soma-service/src/providers/mcp.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/mcp_tests.rs` | `crates/soma-service/src/providers/mcp_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/openapi.rs` | `crates/soma-service/src/providers/openapi.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/openapi_tests.rs` | `crates/soma-service/src/providers/openapi_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/python.rs` | `crates/soma-service/src/providers/python.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/python_bridge.rs` | `crates/soma-service/src/providers/python_bridge.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/python_bridge_tests.rs` | `crates/soma-service/src/providers/python_bridge_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/python_tests.rs` | `crates/soma-service/src/providers/python_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/remote.rs` | `crates/soma-service/src/providers/remote.rs` | Soma product crate | PR #139, #146 |
| renamed | `crates/soma/service/src/providers/remote_tests.rs` | `crates/soma-service/src/providers/remote_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/resource_files.rs` | `crates/soma-service/src/providers/resource_files.rs` | Soma product crate | PR #139, #149 |
| renamed | `crates/soma/service/src/providers/resource_files_tests.rs` | `crates/soma-service/src/providers/resource_files_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/resource_uri.rs` | `crates/soma-service/src/providers/resource_uri.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/resource_uri_tests.rs` | `crates/soma-service/src/providers/resource_uri_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/sidecar.rs` | `crates/soma-service/src/providers/sidecar.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/sidecar_tests.rs` | `crates/soma-service/src/providers/sidecar_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/static_rust.rs` | `crates/soma-service/src/providers/static_rust.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/static_rust_tests.rs` | `crates/soma-service/src/providers/static_rust_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/wasm.rs` | `crates/soma-service/src/providers/wasm.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers/wasm_tests.rs` | `crates/soma-service/src/providers/wasm_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/providers_tests.rs` | `crates/soma-service/src/providers_tests.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/soma.rs` | `crates/soma-service/src/soma.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/service/src/soma_tests.rs` | `crates/soma-service/src/soma_tests.rs` | Soma product crate | PR #139 |
| created | `crates/soma/service/tests/legacy_provider_source.rs` | - | Soma product crate | PR #149 |
| created | `crates/soma/service/tests/provider_core_integration.rs` | - | Soma product crate | PR #149 |
| created | `crates/soma/src/gateway_auth.rs` | - | Soma product crate | PR #141 |
| created | `crates/soma/src/gateway_auth_tests.rs` | - | Soma product crate | PR #141 |
| modified | `crates/soma/src/lib.rs` | - | Soma product crate | PR #137, #141 |
| created | `crates/soma/src/protected_routes.rs` | - | Soma product crate | PR #137 |
| created | `crates/soma/src/protected_routes_proxy.rs` | - | Soma product crate | PR #137 |
| created | `crates/soma/src/protected_routes_proxy_tests.rs` | - | Soma product crate | PR #137 |
| created | `crates/soma/src/protected_routes_tests.rs` | - | Soma product crate | PR #137 |
| modified | `crates/soma/src/routes.rs` | - | Soma product crate | PR #137 |
| modified | `crates/soma/src/routes_tests.rs` | - | Soma product crate | PR #137 |
| modified | `crates/soma/src/runtime.rs` | - | Soma product crate | PR #137, #141 |
| modified | `crates/soma/src/runtime_tests.rs` | - | Soma product crate | PR #137 |
| renamed | `crates/soma/test-support/Cargo.toml` | `crates/soma-test-support/Cargo.toml` | Soma product crate | PR #139, #140, #144, #148 |
| renamed | `crates/soma/test-support/src/lib.rs` | `crates/soma-test-support/src/lib.rs` | Soma product crate | PR #139, #144, #146 |
| renamed | `crates/soma/test-support/src/tracing_capture.rs` | `crates/soma-test-support/src/tracing_capture.rs` | Soma product crate | PR #139 |
| created | `crates/soma/tests/api_gateway_routes.rs` | - | Soma product crate | PR #137 |
| modified | `crates/soma/tests/api_routes.rs` | - | Soma product crate | PR #137 |
| modified | `crates/soma/tests/architecture_boundaries.rs` | - | Soma product crate | PR #135, #137, #141 |
| modified | `crates/soma/tests/drop_provider_probe.rs` | - | Soma product crate | PR #134, #135, #137 |
| created | `crates/soma/tests/gateway_architecture_boundaries.rs` | - | Soma product crate | PR #141 |
| created | `crates/soma/tests/support.rs` | - | Soma product crate | PR #137 |
| renamed | `crates/soma/web/Cargo.toml` | `crates/soma-web/Cargo.toml` | Soma product crate | PR #139, #140 |
| renamed | `crates/soma/web/assets/source/.env.example` | `crates/soma-web/assets/source/.env.example` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/AGENTS.md` | `crates/soma-web/assets/source/AGENTS.md` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/CLAUDE.md` | `crates/soma-web/assets/source/CLAUDE.md` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/GEMINI.md` | `crates/soma-web/assets/source/GEMINI.md` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/README.md` | `crates/soma-web/assets/source/README.md` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/app/api/page.tsx` | `crates/soma-web/assets/source/app/api/page.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/app/globals.css` | `crates/soma-web/assets/source/app/globals.css` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/app/icon.svg` | `crates/soma-web/assets/source/app/icon.svg` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/app/layout.tsx` | `crates/soma-web/assets/source/app/layout.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/app/page.tsx` | `crates/soma-web/assets/source/app/page.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/app/tools/page.tsx` | `crates/soma-web/assets/source/app/tools/page.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/biome.json` | `crates/soma-web/assets/source/biome.json` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components.json` | `crates/soma-web/assets/source/components.json` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/api/action-card.tsx` | `crates/soma-web/assets/source/components/api/action-card.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/api/code-block.tsx` | `crates/soma-web/assets/source/components/api/code-block.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/api/endpoint-row.tsx` | `crates/soma-web/assets/source/components/api/endpoint-row.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/aurora.css` | `crates/soma-web/assets/source/components/aurora.css` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/dashboard/action-button.tsx` | `crates/soma-web/assets/source/components/dashboard/action-button.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/dashboard/card.tsx` | `crates/soma-web/assets/source/components/dashboard/card.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/tools/param-input.tsx` | `crates/soma-web/assets/source/components/tools/param-input.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/tools/response-panel.tsx` | `crates/soma-web/assets/source/components/tools/response-panel.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/tools/submit-button.tsx` | `crates/soma-web/assets/source/components/tools/submit-button.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/ui/badge.tsx` | `crates/soma-web/assets/source/components/ui/badge.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/ui/button.tsx` | `crates/soma-web/assets/source/components/ui/button.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/ui/card.tsx` | `crates/soma-web/assets/source/components/ui/card.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/ui/input.tsx` | `crates/soma-web/assets/source/components/ui/input.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/ui/progress.tsx` | `crates/soma-web/assets/source/components/ui/progress.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/ui/separator.tsx` | `crates/soma-web/assets/source/components/ui/separator.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/ui/skeleton.tsx` | `crates/soma-web/assets/source/components/ui/skeleton.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/components/ui/tabs.tsx` | `crates/soma-web/assets/source/components/ui/tabs.tsx` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/lib/api.test.ts` | `crates/soma-web/assets/source/lib/api.test.ts` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/lib/api.ts` | `crates/soma-web/assets/source/lib/api.ts` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/lib/generated-actions.ts` | `crates/soma-web/assets/source/lib/generated-actions.ts` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/lib/soma.test.ts` | `crates/soma-web/assets/source/lib/soma.test.ts` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/lib/soma.ts` | `crates/soma-web/assets/source/lib/soma.ts` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/lib/utils.ts` | `crates/soma-web/assets/source/lib/utils.ts` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/next-env.d.ts` | `crates/soma-web/assets/source/next-env.d.ts` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/next.config.ts` | `crates/soma-web/assets/source/next.config.ts` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/package.json` | `crates/soma-web/assets/source/package.json` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/pnpm-lock.yaml` | `crates/soma-web/assets/source/pnpm-lock.yaml` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/pnpm-workspace.yaml` | `crates/soma-web/assets/source/pnpm-workspace.yaml` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/postcss.config.mjs` | `crates/soma-web/assets/source/postcss.config.mjs` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/tsconfig.json` | `crates/soma-web/assets/source/tsconfig.json` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/assets/source/vitest.config.ts` | `crates/soma-web/assets/source/vitest.config.ts` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/src/lib.rs` | `crates/soma-web/src/lib.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/src/web.rs` | `crates/soma-web/src/web.rs` | Soma product crate | PR #139 |
| renamed | `crates/soma/web/src/web_tests.rs` | `crates/soma-web/src/web_tests.rs` | Soma product crate | PR #139 |
| modified | `docs/AGENTS-FIRST.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/API.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/ARCHITECTURE.md` | - | Architecture, API, release, or contributor documentation | PR #135, #139, #141 |
| modified | `docs/AUTH.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/CI.md` | - | Architecture, API, release, or contributor documentation | PR #140 |
| modified | `docs/CLAUDE.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/CONFIG.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/DOCS.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/ENV.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/MCPORTER.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/MCP_SCHEMA.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/PATTERNS.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/PLUGINS.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/PROVIDERS.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/QUICKSTART.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/SERVICE_SURFACE_SUGGESTIONS.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/TESTING.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/WEB.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/XTASKS.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| created | `docs/adr/0012-rmcp-traces-rmcp-2-2.md` | - | Architecture, API, release, or contributor documentation | PR #134, #135, #141 |
| modified | `docs/contracts/drop-in-provider-layout.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/contracts/plugin-stdio-adapter.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/contracts/provider-manifest.schema.json` | - | Architecture, API, release, or contributor documentation | PR #149 |
| modified | `docs/generated/openapi.json` | - | Architecture, API, release, or contributor documentation | PR #139 |
| modified | `docs/generated/plugin-settings.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| created | `docs/sessions/2026-07-15-rmcp-traces-gh-76.md` | - | Architecture, API, release, or contributor documentation | PR #134, #135, #141 |
| created | `docs/sessions/2026-07-15-rmcp-traces-http-bridge.md` | - | Architecture, API, release, or contributor documentation | PR #134, #135, #141 |
| created | `docs/sessions/2026-07-15-soma-codemode-openapi-port.md` | - | Architecture, API, release, or contributor documentation | PR #135, #141 |
| deleted | `docs/sessions/2026-07-16-codex-app-server-rest-review-and-sync.md` | - | Architecture, API, release, or contributor documentation | PR #149 |
| modified | `docs/specs/scaffold-intent-handoff.md` | - | Architecture, API, release, or contributor documentation | PR #139 |
| created | `docs/superpowers/plans/2026-07-15-http-trace-header-bridge.md` | - | Architecture, API, release, or contributor documentation | PR #134, #135, #141 |
| created | `docs/superpowers/plans/2026-07-15-rmcp-traces.md` | - | Architecture, API, release, or contributor documentation | PR #134, #135, #141 |
| created | `docs/superpowers/plans/2026-07-15-self-contained-soma-gateway.md` | - | Architecture, API, release, or contributor documentation | PR #137 |
| created | `docs/superpowers/plans/2026-07-15-soma-codemode-openapi-port.md` | - | Architecture, API, release, or contributor documentation | PR #135, #141 |
| modified | `lefthook.yml` | - | Workspace configuration, source, test, or documentation | PR #139 |
| modified | `packages/soma-rmcp/README.md` | - | Workspace configuration, source, test, or documentation | PR #139 |
| modified | `plugins/README.md` | - | Workspace configuration, source, test, or documentation | PR #139 |
| modified | `plugins/soma/CLAUDE.md` | - | Workspace configuration, source, test, or documentation | PR #139 |
| modified | `plugins/soma/README.md` | - | Workspace configuration, source, test, or documentation | PR #139 |
| modified | `release-please-config.json` | - | Workspace configuration, source, test, or documentation | PR #139 |
| modified | `release/components.toml` | - | Workspace configuration, source, test, or documentation | PR #139 |
| modified | `scripts/README.md` | - | Repository maintenance or generation script | PR #139, #140 |
| modified | `scripts/blob-size-allowlist.txt` | - | Repository maintenance or generation script | PR #139 |
| modified | `scripts/ci/changed_paths.py` | - | Repository maintenance or generation script | PR #139 |
| modified | `scripts/ci/pre_push.py` | - | Repository maintenance or generation script | PR #140 |
| modified | `scripts/generate-docs.py` | - | Repository maintenance or generation script | PR #139 |
| created | `soma-architecture-refactor-plan-v3.md` | - | Workspace configuration, source, test, or documentation | PR #135, #139, #141, #149 |
| modified | `xtask/Cargo.toml` | - | Workspace contract and automation tooling | PR #139, #140 |
| modified | `xtask/README.md` | - | Workspace contract and automation tooling | PR #139, #140 |
| created | `xtask/src/architecture.rs` | - | Workspace contract and automation tooling | PR #140, #143, #145, #146 |
| created | `xtask/src/architecture_graph.rs` | - | Workspace contract and automation tooling | PR #140 |
| created | `xtask/src/architecture_graph_tests.rs` | - | Workspace contract and automation tooling | PR #140 |
| created | `xtask/src/architecture_tests.rs` | - | Workspace contract and automation tooling | PR #140, #146 |
| modified | `xtask/src/cargo_generate_post.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/ci_paths.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/codex_schema.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/codex_schema/bisect.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/codex_schema/naming.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/codex_schema/regen.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/main.rs` | - | Workspace contract and automation tooling | PR #135, #137, #140 |
| modified | `xtask/src/patterns/actions.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/patterns/checks.rs` | - | Workspace contract and automation tooling | PR #139, #144, #145, #146 |
| modified | `xtask/src/patterns/surfaces.rs` | - | Workspace contract and automation tooling | PR #137, #139, #145 |
| modified | `xtask/src/patterns/util.rs` | - | Workspace contract and automation tooling | PR #139 |
| created | `xtask/src/release_commands.rs` | - | Workspace contract and automation tooling | PR #137 |
| created | `xtask/src/release_commands_tests.rs` | - | Workspace contract and automation tooling | PR #137 |
| modified | `xtask/src/release_versions_tests.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/rmcp_release_monitor.rs` | - | Workspace contract and automation tooling | PR #134, #135, #139, #141 |
| modified | `xtask/src/scaffold.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/scripts.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/scripts_lane_c.rs` | - | Workspace contract and automation tooling | PR #139 |
| modified | `xtask/src/scripts_lane_d.rs` | - | Workspace contract and automation tooling | PR #139, #141 |
| created | `xtask/src/test_siblings.rs` | - | Workspace contract and automation tooling | PR #137, #139 |
| created | `xtask/src/test_siblings_tests.rs` | - | Workspace contract and automation tooling | PR #137, #139 |
| modified | `xtask/src/web_source.rs` | - | Workspace contract and automation tooling | PR #139 |
| created | `xtask/src/workspace_commands.rs` | - | Workspace contract and automation tooling | PR #137, #140 |
| created | `xtask/src/workspace_commands_tests.rs` | - | Workspace contract and automation tooling | PR #137 |

## Beads Activity

| bead | title | actions | final status | why it mattered |
|---|---|---|---|---|
| `rmcp-template-ub2l` | Execute Soma architecture refactor PR0 prep | created, claimed, updated, commented, closeout-corrected | closed | Tracked shared MCP/gateway foundations; stale status was corrected after merge evidence was verified. |
| `rmcp-template-e4i3` | Reconcile architecture PR0 with merged app-server gateway work | created and completed | closed | Reconciled prerequisite gateway work with the plan. |
| `rmcp-template-ynhq` | Sync bundled web source with `apps/web` | created and completed | closed | Captured a review-sweep consistency fix. |
| `rmcp-template-8lgu` | Apply Soma physical workspace taxonomy | created, claimed, updated, closed | closed | Tracked PR2's nested workspace migration. |
| `rmcp-template-8lgu.1` | Route `apps/soma` changes through CI classifier | created and fixed | closed | Restored CI path coverage after the move. |
| `rmcp-template-8lgu.2` | Point action registry docs at contracts crate | created and fixed | closed | Corrected moved ownership documentation. |
| `rmcp-template-8lgu.3` | Enforce shared crate product-boundary guardrails | created and fixed | closed | Prevented shared-to-product dependency regressions. |
| `rmcp-template-8lgu.4` | Restore Justfile and lefthook coupling after path move | created and fixed | closed | Kept local quality workflows functional. |
| `rmcp-template-8lgu.5` | Align Python pre-push path classifier with xtask | created and fixed | closed | Kept duplicate classifiers behaviorally aligned. |
| `rmcp-template-8lgu.6` | Update architecture and web docs for nested taxonomy | created and fixed | closed | Removed stale pre-move paths. |
| `rmcp-template-8lgu.7` | Consolidate repeated workspace taxonomy docs | created as follow-up | open | Low-priority documentation deduplication remains. |
| `rmcp-template-3ies` | Add Soma architecture enforcement | created, claimed, updated, closed | closed | Tracked PR3's executable boundary checks. |
| `rmcp-template-1h9y` | Introduce Soma domain and application facades | created, claimed, updated, closed | closed | Tracked PR4's central orchestrator boundary. |
| `rmcp-template-jl1z` | Migrate Soma CLI to `SomaApplication` | created, claimed, updated, closed | closed | Tracked PR5's thin CLI adapter. |
| `rmcp-template-r44l` | Migrate Soma REST API to `SomaApplication` | created, claimed, updated, closed | closed | Tracked PR6's thin REST adapter. |
| `rmcp-template-fk5r` | Migrate Soma MCP to `SomaApplication` | created, claimed, reviewed, closed | closed | Tracked PR7's protocol migration. |
| `rmcp-template-fk5r.1` | Preserve MCP structured error contracts | created and fixed | closed | Prevented tool-error compatibility regression. |
| `rmcp-template-fk5r.2` | Restore MCP discovery and use-time scope semantics | created and fixed | closed | Preserved authorization behavior. |
| `rmcp-template-fk5r.3` | Preserve valid `traceparent` with invalid optional metadata | created and fixed | closed | Kept valid tracing context usable. |
| `rmcp-template-fk5r.4` | Prevent upstream error-body disclosure through MCP | created and fixed | closed | Closed an information-disclosure path. |
| `rmcp-template-fk5r.5` | Enforce the `soma-mcp` Cargo dependency boundary | created and fixed | closed | Made the architecture rule executable. |
| `rmcp-template-fk5r.6` | Avoid redundant MCP catalog clones and scope allocations | created and fixed | closed | Reduced avoidable request-path work. |
| `rmcp-template-fk5r.7` | Remove duplicate gateway route-scope conversion | created and fixed | closed | Centralized conversion behavior. |
| `rmcp-template-fk5r.8` | Reuse composed `McpState` for protected middleware | created and fixed | closed | Avoided inconsistent state construction. |
| `rmcp-template-fk5r.9` | Compile application ports for lean stdio builds | created and fixed in PR #147 | closed | Restored lean artifact compilation. |
| `rmcp-template-fk5r.10` | Make MCP guard line-ending independent | created and fixed | closed | Kept architecture checks portable. |
| `rmcp-template-fq2i` | Convert runtime state to `SomaApplication` facade | created, claimed, updated, closed | closed | Tracked PR8's runtime composition change. |
| `rmcp-template-8kex` | Extract shared provider-core crate | created, claimed, reviewed, commented, closed | closed | Tracked PR9 through final merge `c2540c0`. |
| `rmcp-template-4pux` | Improve Docker dependency cache layering | created as review follow-up | open | Pre-existing optimization, intentionally deferred. |
| `rmcp-template-8ark` | Apply service scope to MCP resource templates | created and commented | open | Pre-existing P2 authorization inconsistency. |
| `rmcp-template-d40t` | Implement MCP discovery pagination | created and commented | open | Pre-existing P2 protocol scalability gap. |
| `rmcp-template-uwgj` | Bound MCP response paging cache | created and commented | open | Pre-existing P2 memory/lock-contention risk. |

## Repository Maintenance

- **Plans:** `find docs/plans -maxdepth 2 -type f` found no plan files to archive. The root `soma-architecture-refactor-plan-v3.md` is active and partial, so it remains in place. During artifact creation it had a concurrent uncommitted edit; that edit was preserved and excluded from this path-limited commit.
- **Beads:** Read the architecture and follow-up beads with `bd show`. Closed stale `rmcp-template-ub2l` only after confirming PR #141 merge `7c52d42` and the plan ledger; added an evidence comment and ran `bd dolt push`. The five open follow-ups remain open because their work is not complete.
- **Worktrees:** Confirmed the PR9 worktree had already been removed. Left detached Codex worktrees alone because ownership was unclear, left the active Claude REST worktree because it was in use, and left the protected `marketplace-no-mcp` worktree untouched by policy.
- **Branches:** Confirmed architecture topic refs had been merged/removed during delivery. Unrelated local/remote branches were not changed. No force operations were used.
- **Stale docs:** The architecture plan was updated during delivery with the PR ledger, carryovers, and current stopping point. No broader documentation rewrite was attempted during closeout; `rmcp-template-8lgu.7` tracks taxonomy-doc consolidation.
- **Inventory fallback:** `gh api` for PR file lists returned a GitHub 503 Unicorn page. Local merge-parent diffs supplied the complete file inventory without changing repository state.

## Tools and Skills Used

- **Skills:** `superpowers:using-git-worktrees`, `superpowers:executing-plans`, parallel/subagent development skills, two `lavra:lavra-review` sweeps, and `vibin:save-to-md` for this closeout.
- **Shell and file tools:** `rg`, `sed`, `jq`, `git`, `apply_patch`, and repository scripts were used to inspect, migrate, patch, and audit the workspace.
- **Rust/web/build tools:** `cargo`, `cargo nextest`, `cargo xtask`, `pnpm`, and Docker validated Rust, generated contracts, web assets, and container paths.
- **GitHub CLI:** `gh` created, reviewed, monitored, merged, and cleaned PRs. Self-hosted checks were slow, stale runs required force cancellation, and the PR-files API returned 503 during final documentation.
- **Beads CLI:** `bd` tracked every implementation slice and review finding; `bd dolt push` synchronized tracker state.
- **Subagents/review agents:** Parallel agents implemented and reviewed bounded slices. Some delegated calls hit usage limits, so work continued in the primary agent; final reviews completed successfully.
- **External/runtime access:** `ssh winhost` timed out during Windows investigation, so no remote host change was made. No browser automation or MCP server was required for implementation.

## Commands Executed

| command | result |
|---|---|
| `git worktree add ...` / `git worktree list --porcelain` | Isolated implementation slices and audited cleanup ownership. |
| `cargo test --workspace --all-features` | Passed after PR9 fixes. |
| `cargo clippy --all-targets --locked -- -D warnings` | Passed on reviewed slices and CI. |
| `cargo xtask patterns` | Passed; emitted only existing size/surface warnings. |
| `cargo xtask check-openapi --check` | Passed after correcting feature unification. |
| `pnpm -C apps/web validate` | Passed for taxonomy/web changes. |
| `gh pr checks 149 --watch` | Final CI, MSRV, Linux, Windows, MCP, and security checks passed. |
| `gh pr merge 149 --merge` | Merged PR9 as `c2540c0`. |
| `git pull --rebase`, `bd dolt push`, `git push` | Synchronized the completed implementation and tracker state. |
| `bd close rmcp-template-ub2l ...` | Corrected the stale PR0 tracker status during closeout. |

## Errors Encountered

- PR9 initially violated `mod_module_files = "deny"` with `registry/mod.rs`; it was moved to sibling `registry.rs`.
- Enabling `serde_json/preserve_order` globally changed generated OpenAPI bytes. Feature activation was narrowed so runtime catalog compatibility and generator stability both hold.
- Review found ordering, surface-exposure, schema-validation, and fingerprint compatibility regressions in the first provider extraction. Dispatch hooks and compatibility handling were corrected and regression-tested.
- Stale GitHub Actions jobs continued consuming self-hosted runners after ordinary cancellation. The force-cancel endpoint cleared them; long Windows/MSRV jobs were then allowed to finish.
- `ssh winhost` timed out, so the investigation did not modify the Windows host.
- GitHub's PR-files API returned HTTP 503 during documentation; local merge-parent diffs were used as the evidence source.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Workspace layout | Flat/mixed crate ownership | Explicit `apps/`, `crates/shared/`, nested `crates/shared/mcp/`, and `crates/soma/` taxonomy. |
| Surface orchestration | CLI, REST, MCP, and runtime reached legacy service layers directly | All four compose or call `SomaApplication`; adapters remain thin. |
| Architecture enforcement | Boundaries were mostly documentary | `cargo xtask patterns` and dependency guards enforce layer rules. |
| Provider contracts | Canonical provider types and registry lived in Soma service code | Neutral `provider-core` owns `ToolSpec`, manifests, validation, capabilities, provider calls, and registry primitives. |
| Provider policy | Shared mechanics and Soma policy were interleaved | Soma retains auth/admin/destructive policy and limits; shared core remains product-neutral. |
| MCP foundations | Gateway/client/server/proxy concerns were coupled | Reusable role crates exist under a nested MCP group with generic auth/OAuth boundaries. |
| Plugin support | Empty placeholder crate remained | Stub crate removed from the planned architecture. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test --workspace --all-features` | Full workspace green | Passed | pass |
| `cargo xtask patterns` | No architecture violations | Passed with pre-existing warnings only | pass |
| `cargo xtask check-openapi --check` | Generated OpenAPI current | Current | pass |
| Provider manifest/palette contract checks | Generated contracts current | Passed | pass |
| `cargo check -p soma-service --all-features` | Runtime client feature graph compiles | Passed | pass |
| `gh pr checks 149` | Every required PR9 check green | CI Gate, MSRV Gate, Linux, Windows, conformance, security, and review checks passed | pass |
| `git merge-base --is-ancestor 0d729cd c2540c0` and parent inspection | PR9 head is in merge | `0d729cd` is merge second parent | pass |
| `git status` after implementation cleanup | Main clean and synchronized | Clean/up-to-date before the later concurrent plan edit | pass |

## Risks and Rollback

- PR10-PR19 remain intentionally unimplemented. Do not delete compatibility crates in PR12/PR13 until the distributed PR1 behavior-freeze coverage is audited.
- Shared package naming remains unresolved; decide before PR19 to avoid repeating documentation, scaffold, release, and CI changes.
- Open authorization, pagination, and cache-bound follow-ups are pre-existing and tracked in Beads; they should be addressed before presenting the MCP infrastructure as hardened for very large public catalogs.
- Each merged slice is isolated by PR and merge commit. Revert the relevant merge commit, beginning with `c2540c0` for provider-core, rather than manually reversing cross-workspace moves.

## Decisions Not Taken

- Did not force every shared crate to have zero internal dependencies; compositional dependency edges remain where they express the actual role graph.
- Did not put Soma defaults/auth/config inside the neutral gateway engine.
- Did not rename `provider-core` to `provider-kit`; `core` better describes the canonical contracts and registry primitives.
- Did not replace one-action MCP routing; the plan preserves it and adds configurable individual/both exposure later.
- Did not merge, delete, or otherwise clean the protected `marketplace-no-mcp` branch/worktree.
- Did not begin PR10 after PR9, per the explicit stop instruction.

## References

- Architecture plan: `soma-architecture-refactor-plan-v3.md`
- PRs: [#134](https://github.com/jmagar/soma/pull/134), [#135](https://github.com/jmagar/soma/pull/135), [#137](https://github.com/jmagar/soma/pull/137), [#139](https://github.com/jmagar/soma/pull/139), [#140](https://github.com/jmagar/soma/pull/140), [#141](https://github.com/jmagar/soma/pull/141), [#143](https://github.com/jmagar/soma/pull/143), [#144](https://github.com/jmagar/soma/pull/144), [#145](https://github.com/jmagar/soma/pull/145), [#146](https://github.com/jmagar/soma/pull/146), [#147](https://github.com/jmagar/soma/pull/147), [#148](https://github.com/jmagar/soma/pull/148), [#149](https://github.com/jmagar/soma/pull/149)
- Final PR9 merge: `c2540c0f4fb441af51ed6e341d4bebcd3502112e`
- Full Codex transcript: `/home/jmagar/.codex/sessions/2026/07/15/rollout-2026-07-15T14-53-30-019f6720-7e38-70b2-aac8-dd37c89543e2.jsonl`

## Open Questions

- Should shared Cargo package names be made brand-neutral before crates.io publication, or are the existing `soma-*` names intentional?
- Does the distributed PR1 characterization suite cover every behavior that PR12 and PR13 will remove from legacy crates?
- Is the current concurrent uncommitted plan revision intended for the next PR10 worktree? It was preserved untouched.

## Next Steps

1. Start PR10 in a fresh worktree from updated `main`; do not reuse the detached save-session worktree.
2. First centralize internal paths in `[workspace.dependencies]` as the plan's PR0 carryover, preserving one resolved `rmcp` version.
3. Audit the PR1 behavior-freeze checklist before PR12 deletes legacy crates.
4. Execute PR10's provider-adapter split while preserving PR9's dispatch ordering and compatibility tests.
5. Resolve shared package naming before PR19's docs/scaffold/release sweep.
6. Schedule the open Beads follow-ups: `8lgu.7`, `4pux`, `8ark`, `d40t`, and `uwgj`.
