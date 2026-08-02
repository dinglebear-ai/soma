---
title: "Provider Tool Namespace Implementation Plan"
created: 2026-08-02
updated: 2026-08-02
doc_type: "implementation-plan"
status: "proposed"
owner: "soma"
scope: "product"
---

# Provider Tool Namespace Implementation Plan

## Objective

Replace flat provider-tool dispatch with canonical `(provider, tool)` identity
across provider-core, Soma application policy, CLI, MCP, REST, OpenAPI,
inspection, generators, and compatibility behavior without weakening auth,
confirmation, validation, paging, hot reload, or failure isolation.

## Locked Boundaries

- Thin transport shims remain thin.
- Provider identity is declared, not derived from a path except for legacy
  Python inference.
- Registry snapshots remain immutable and atomically published.
- One release of v1 compatibility is allowed only for unique flat names.
- Prompts/resources/tasks/elicitation namespacing is separate follow-up work.
- Long-form design stays in this package; `CLAUDE.md` remains compact.

## Phase 1: Core Identity and Indexes

### Files

- `crates/shared/provider-core/src/id.rs`
- `crates/shared/provider-core/src/call.rs`
- `crates/shared/provider-core/src/registry/index.rs`
- `crates/shared/provider-core/src/registry/snapshot.rs`
- `crates/shared/provider-core/src/registry/dispatch.rs`
- `crates/shared/provider-core/tests/*`

### Work

1. Add `ToolId` and `ProviderToolId` with deterministic serde/order/hash.
2. Change tool indexes to `BTreeMap<ProviderToolId, RegisteredTool>`.
3. Add provider-local CLI keys, canonical REST keys, and explicit legacy-flat
   unique/ambiguous resolution.
4. Change `ProviderCall` to carry `ProviderToolId` as one field.
5. Ensure dispatch obtains provider and tool from one snapshot/index lookup.
6. Include canonical surface mappings in registry fingerprints.

### Tests

- Same tool name in two providers succeeds.
- Duplicate within one provider fails.
- Legacy unique and ambiguous maps are deterministic.
- Dispatch cannot mix a tool from one snapshot with a provider from another.
- Fingerprint changes when namespace or surface mapping changes.

## Phase 2: Manifest v2 and Source Validation

### Files

- `crates/shared/provider-core/src/manifest.rs`
- `crates/shared/provider-core/src/validation.rs`
- `docs/contracts/provider-manifest.schema.json`
- `crates/soma/application/src/providers/filesystem*.rs`
- `crates/shared/provider-adapters/src/python_bridge.rs`
- provider fixture directories under `docs/contracts/examples/`

### Work

1. Accept schema versions 1 and 2 with explicit semantics.
2. Interpret v2 CLI command/aliases as provider-local.
3. Reserve built-in provider namespaces.
4. Add Python filename/declared-name diagnostics without importing Python in
   non-executing inspection.
5. Make cross-file inspection match live registry namespace validation.
6. Add valid same-tool/different-provider and invalid same-provider fixtures.

### Tests

- Schema validation covers every Rust manifest field.
- JSON, TS, WASM, and Python catalog paths resolve identical identities.
- Windows path spelling does not change declared identity.
- Non-executing inspection never imports Python.

## Phase 3: Application Dispatch and Compatibility

### Files

- `crates/soma/application/src/provider_registry.rs`
- `crates/soma/application/src/app.rs`
- `crates/soma/application/src/service.rs`
- `crates/soma/application/src/errors.rs`
- nearest sibling `*_tests.rs` files

### Work

1. Add `ExecuteProviderToolRequest { provider, tool, params }`.
2. Resolve one registered entry before authorization and confirmation.
3. Add stable lookup and migration errors.
4. Centralize flat compatibility resolution and warning construction.
5. Include canonical identity in results, diagnostics, traces, and audit data.

### Tests

- Unknown provider differs from unknown tool.
- Ambiguous flat name never selects a provider.
- Scope, admin, and destructive confirmation use the canonical registered tool.
- Hot reload retains the previous snapshot on namespace collision.

## Phase 4: Nested CLI

### Files

- `crates/soma/cli/src/lib.rs`
- `crates/soma/cli/src/cli_tests.rs`
- `apps/soma/tests/provider_cli.rs`
- generated CLI/help documentation inputs

### Work

1. Parse `soma PROVIDER TOOL` after built-in command matching.
2. Generate provider and tool help from one catalog snapshot.
3. Treat v2 commands/aliases as provider-local.
4. Preserve built-in short commands.
5. Add one-release v1 flat aliases with visible deprecation warnings.

### Tests

- `soma nexus repos --repo soma` dispatches `nexus.repos`.
- `soma nexus --help` lists only Nexus tools.
- Identical tool aliases in different providers coexist.
- Reserved namespace and ambiguous legacy errors are stable.

## Phase 5: Canonical REST and OpenAPI

### Files

- `apps/soma/src/http.rs`
- `crates/soma/api/src/api.rs`
- `crates/soma/api/src/openapi.rs` and submodules
- `crates/soma/api/src/route_inventory.rs`
- `apps/soma/tests/api_routes.rs`
- `docs/generated/openapi.json`

### Work

1. Add `POST /v1/providers/{provider}/tools/{tool}`.
2. Resolve custom REST overlays to `ProviderToolId`.
3. Retain `/v1/tools/{action}` as the bounded v1 compatibility route.
4. Emit identity-bearing success/error envelopes.
5. Generate stable operation IDs and `x-soma-provider`/`x-soma-tool`.

### Tests

- Same tool name across providers dispatches through distinct canonical routes.
- Custom route collision remains globally rejected.
- Generic and custom routes reach the same canonical tool.
- OpenAPI checked-in output matches live generation on Linux and Windows line
  endings.

## Phase 6: MCP Schema, Dispatch, and Output

### Files

- `crates/soma/mcp/src/schemas.rs`
- `crates/soma/mcp/src/tools.rs`
- `crates/soma/mcp/src/rmcp_server.rs`
- `crates/soma/mcp/src/protocol_errors.rs`
- MCP sibling/integration tests and `scripts/mcp-smoke-test.sh`

### Work

1. Require `provider` plus `action` for canonical calls.
2. Generate conditional input branches for each pair.
3. Add provider to metadata and per-action output schemas.
4. Add `_soma_provider` beside `_soma_action`.
5. Route provider-less v1 actions through centralized compatibility logic.
6. Update paging continuations so identity is retained without re-execution.

### Tests

- Schema permits `nexus.status` and `weather.status` with different parameters.
- Wrong provider/action pairing is rejected before dispatch.
- MCP errors and paged results retain identity.
- Full-binary mcporter smoke has zero failures.

## Phase 7: Discovery, Generators, and Documentation

### Files

- provider inspection/report code
- docs generator inputs and generated outputs
- plugin/skill/OpenAPI generation code
- `docs/PROVIDERS.md`
- `packages/python/README.md`
- Python examples and Nexus trial fixture

### Work

1. Publish canonical identities and surface projections in inspection output.
2. Group CLI/help/plugin/skill docs by provider.
3. Update Python decorators/examples to demonstrate local tool names.
4. Add a minimal Nexus provider proving CLI, MCP, and REST parity.
5. Document migration warnings and removal timing.

## Phase 8: Removal Release

After one released compatibility cycle:

1. Remove manifest v1 loading.
2. Remove flat CLI aliases, provider-less MCP calls, and `/v1/tools/{action}`.
3. Replace deprecation tests with `legacy_action_removed` tests.
4. Publish migration notes in the changelog and release documentation.

## Verification Matrix

Run focused checks during each phase, then the full gate after integration:

```bash
cargo test -p soma-provider-core
cargo test -p soma-application
cargo test -p soma-cli
cargo test -p soma-mcp
cargo test -p soma-api
cargo test -p soma --test provider_cli --test api_routes --test provider_registry
cargo xtask check-test-siblings
cargo xtask check-docs
cargo xtask check-schema-docs
cargo xtask check-version-sync
cargo xtask ci
```

Before repo guards that use `git ls-files`, stage all newly added files so the
guard actually sees them. Run Windows CI before declaring path/case behavior
verified.

## Rollout and Rollback

- Land phases behind manifest semantics and compatibility resolution rather
  than a runtime feature flag.
- Keep old paths as adapters to the canonical registry; do not maintain two
  execution engines.
- If a refresh finds a namespace conflict, retain the previous snapshot.
- Rollback is a normal release rollback because v1 paths remain adapters during
  the compatibility release.
- Do not remove compatibility until telemetry/log review shows callers have
  moved and the removal release is explicitly cut.

## Completion Criteria

- The acceptance criteria in `SPEC.md` pass on Linux and Windows CI.
- Schema, contract, Rust types, and all three wire surfaces agree.
- No dynamic execution path dispatches on a flat action internally.
- Nexus can declare local tools such as `repos`, `services`, and `status`
  without prefixing every tool name.
- Generated documentation and OpenAPI are deterministic and current.
