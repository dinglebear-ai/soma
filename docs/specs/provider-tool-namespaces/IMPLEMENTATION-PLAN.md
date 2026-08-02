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

Replace globally flat provider-tool dispatch with canonical
`ProviderToolId { provider, tool }` across provider-core, application policy,
CLI, MCP, REST/OpenAPI, clients, Palette, web, discovery, paging, refresh
events, generated artifacts, Python authoring, and compatibility behavior.

The implementation must preserve containment, auth, confirmation, capability
policy, immutable generations, validation, response limits, and thin surface
adapters. It must prove a fixture-backed Nexus provider through CLI, MCP, REST,
Palette, one-shot Python, persistent Python, and hot reload before live lab
collectors are considered.

## Research Baseline

The plan was revalidated against current code and authoritative standards. The
following facts drive the revised work:

- Provider-core indexes tools globally by `tool.name`; CLI aliases may differ
  from tool names, so compatibility must be surface-specific.
- Soma's built-in provider is currently named `static-rust`, not `soma`.
- Python catalog construction hard-codes manifest v1 in the SDK, embedded
  bridge, native path, and tests; non-executing inspection skips `.py` files.
- Product Python operations and MCP elicitation special cases branch on action
  alone.
- Confirmation preflight and final execution currently perform separate flat
  lookups; holding a generation lease while awaiting a human would pin retired
  Python generations.
- MCP schema generation merges parameter definitions globally with first-wins
  behavior and cannot represent pair-specific incompatible schemas.
- Shared MCP paging, refresh diffs, Palette, Rust client, web Tool Runner, and
  generated surfaces also use flat action identity.
- Runtime can use one generic Axum route, but OpenAPI needs concrete loaded
  paths to publish fixed per-tool schemas and identity extensions.
- `.github/workflows/ci.yml` currently has no native Windows job, and
  `packages/python/**` is not classified into the relevant PR CI paths.
- Soma and `soma-provider` are separately released components; compatibility
  removal cannot be part of this implementation epic.

Primary standards:

- [MCP tools and structured content](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
- [MCP JSON Schema usage](https://modelcontextprotocol.io/specification/2025-11-25/basic#json-schema-usage)
- [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12)
- [JSON Schema composed objects](https://json-schema.org/understanding-json-schema/reference/object#unevaluated-properties)
- [OpenAPI 3.1.1 Operation Object](https://spec.openapis.org/oas/v3.1.1.html#operation-object)
- [OpenAPI path templating](https://spec.openapis.org/oas/v3.1.1.html#path-templating)
- [Axum routing rules](https://docs.rs/axum/latest/axum/struct.Router.html#method.route)
- [HTTP Deprecation header](https://www.rfc-editor.org/rfc/rfc9745.html)
- [HTTP Sunset header](https://www.rfc-editor.org/rfc/rfc8594.html)

## Locked Policy Decisions

1. Canonical identity is a validated typed pair; display IDs are never parsed.
2. Built-in provider name migrates `static-rust` -> `soma`; kind remains
   `static-rust`.
3. Manifest v2 is a semantic policy over one Rust manifest model.
4. Python v2 authoring is explicit during compatibility, for example
   `provider(manifest_version=2, ...)`; unrelated schema/protocol/ABI versions
   are not bulk-bumped.
5. Non-executing Python inspection reports runtime validation required rather
   than pretending to discover arbitrary decorators.
6. CLI, MCP, and REST have independent v1 compatibility indexes.
7. Final execution uses one prepared entry/lease; interactive preflight does
   not hold the lease and stale confirmation is rejected.
8. Soma keeps its hand-written CLI parser unless a separate measured decision
   approves Clap.
9. Runtime uses a generic canonical REST route; live OpenAPI enumerates concrete
   paths per loaded tool with collision-safe operation IDs.
10. Palette, web, clients, paging, refresh events, and generated surfaces are
    in scope.
11. Nexus default CI is deterministic and fixture-backed. Live lab smoke is
    opt-in, trusted-local, read-only, and separately authorized.
12. Compatibility removal is a separate post-release/adoption-gated epic.

## Work Graph

```text
Research/design revision
        |
Migration policy + CI routing
        |
Core identity/indexes/layout validation
        |
Manifest v2 + Python SDK/bridge + built-in migration
        |
Application prepared dispatch + confirmation
        |
        +-----------+-----------+-----------+-----------+
        |           |           |           |
       CLI       REST/web      MCP/paging   Palette
        |           |           |           |
        +-----------+-----------+-----------+
                            |
                Discovery/generators/Nexus
                            |
              Full Linux + native Windows CI
                            |
                 Compatibility release N

Separate future epic after adoption gate: remove v1/flat compatibility
```

Lavra is configured for at most three parallel implementation agents. The four
Phase 4 surface beads are dependency-independent, but execution schedules them
in two batches of at most three workers.

## Phase 0: Repair Planning and CI Preconditions

### Files

- `.github/workflows/ci.yml`
- `xtask/src/ci_paths.rs`
- `xtask/src/ci_paths_tests.rs` or existing sibling tests
- `docs/CI.md`
- `docs/WINDOWS-RUNNER.md`
- `release/components.toml`
- Beads `rmcp-template-ob48`, children, and removal epic

### Work

1. Keep the research/design child complete before implementation begins.
2. Ensure `packages/python/**` changes enable Rust/Soma/Python/package/release
   jobs needed by the SDK/bridge contract.
3. Restore a native `windows-latest` PR job for namespace-sensitive parser,
   path, subprocess, Python package, and selected integration tests, or remove
   every Windows-proof claim. The locked choice is to restore it.
4. Keep the stable aggregate `CI Gate` dependent on the Windows result when
   relevant paths change.
5. Record that `soma` and `soma-provider` release separately and define the
   host/SDK matrix before v2 emission changes.
6. Confirm no implementation child depends on compatibility removal.

### Tests

- Changed-path fixtures for `packages/python/**`, provider-core, MCP paging,
  Palette, web, docs-only, and workflow-only changes.
- Workflow-shape tests prove native Windows is real, not cross-compilation.
- `cargo xtask changed-paths` results match the intended job matrix.
- `bd swarm validate rmcp-template-ob48` is acyclic and excludes removal work.

## Phase 1: Core Identity and Pure Layout Validation

### Files

- `crates/shared/provider-core/src/id.rs`
- new `crates/shared/provider-core/src/tool_id.rs` if file size warrants
- `crates/shared/provider-core/src/call.rs`
- `crates/shared/provider-core/src/registry/index.rs`
- `crates/shared/provider-core/src/registry/dispatch.rs`
- `crates/shared/provider-core/src/registry/snapshot.rs`
- `crates/shared/provider-core/src/registry/fingerprint.rs`
- provider-core tests and fixtures

### Work

1. Add `ToolId` and `ProviderToolId` with shared grammar and validating serde
   (`TryFrom<String>` or manual `Deserialize`). Harden
   `ProviderId` deserialization at the same boundary.
2. Change `RegisteredTool` to own canonical identity and manifest semantics.
3. Replace the global tool map with
   `BTreeMap<ProviderToolId, RegisteredTool>`.
4. Add provider-local CLI keys and custom REST exact/shape keys. Keep HTTP
   methods as validated strings/newtypes in transport-neutral provider-core.
5. Add independent legacy CLI command, MCP action, and REST action indexes.
   Populate them only from v1 tools exposed on each surface.
6. Make canonical REST lookup use the primary tool map directly; do not store a
   redundant derived route entry.
7. Expose a pure catalog layout/index validator that live registry and
   non-executing inspection can share for statically available catalogs.
8. Normalize route shapes and detect equivalent captures, static shadowing,
   catch-all overlap, infrastructure routes, and canonical-route collisions
   before router construction.
9. Change refresh diff primitives from flat strings to sorted identities.
10. Preserve fingerprinting through a canonicalized complete-catalog
    representation. Test existing overlay sensitivity before adding new state.

### Tests

- `alpha.status` and `beta.status` coexist and dispatch independently.
- Same-provider duplicate fails with `duplicate_provider_tool`.
- Invalid IDs fail constructors, JSON deserialization, path parsing,
  and manifest validation with the same stable codes.
- V1 tool `weather_current` with CLI command `weather` and alias `wx` builds the
  correct independent compatibility maps.
- A CLI alias collision does not make an MCP/REST action ambiguous.
- Route templates differing only in capture names fail before Axum; static
  shadow, encoded separator/dot, catch-all, and wrong-method fixtures exist.
- Adding `alpha.status` while retaining `beta.status` appears in refresh diffs.
- Fingerprints are registration-order independent and surface-overlay changes
  invalidate them.
- Scale fixture (minimum 100 providers x 20 tools) measures snapshot build,
  lookup, compatibility-map build, and deterministic serialization.

### Verification

```bash
cargo test -p soma-provider-core --all-features
cargo clippy -p soma-provider-core --all-targets --all-features -- -D warnings
```

## Phase 2: Manifest v2, Python Authoring, and Built-In Migration

### Files

- `crates/shared/provider-core/src/manifest.rs`
- `crates/shared/provider-core/src/validation.rs`
- `crates/shared/provider-core/provider-manifest.schema.json`
- `docs/contracts/provider-manifest.schema.json`
- `docs/contracts/examples/provider-manifests/*`
- `crates/soma/domain/src/provider_validation.rs`
- `crates/soma/application/src/providers/static_rust.rs`
- `crates/soma/application/src/providers/filesystem.rs`
- `crates/soma/application/src/providers/filesystem_uniqueness.rs`
- `crates/soma/application/src/providers/filesystem_python.rs`
- `crates/shared/provider-adapters/src/python_bridge.rs`
- `crates/shared/provider-adapters/src/python_bridge_tests.rs`
- `packages/python/python/soma_provider/__init__.py`
- `packages/python/python/soma_provider/_runtime.py`
- `packages/python/python/soma_provider/models.py`
- `packages/python/python/soma_provider/models.pyi`
- `packages/python/src/lib.rs`
- `packages/python/tests/*`
- `scripts/generate_python_models.py`
- `scripts/generate-docs.py`
- componentization embedded SDK copies/tests

### Work

1. Accept manifest versions `1 | 2` in one typed model; derive semantic policy.
2. Change the checked JSON Schema to mutually exclusive const-discriminated
   v1/v2 branches. Use Draft 2020-12 deliberately; use
   `unevaluatedProperties: false` when closing composed schemas.
3. Preserve v1 flat spellings before normalizing tool identity.
4. For v2, treat CLI command/aliases as local and move reserved-root validation
   to provider names. Generate the reserved set from one shared parser policy.
5. Migrate built-in catalog provider name to `soma`, keep kind `static-rust`,
   and update reports, skills, fixtures, errors, authz tests, and fingerprints.
6. Add explicit Python `manifest_version` authoring metadata without changing
   the meaning of runner protocol/native/decorator/component schema versions.
7. During compatibility, existing Python authoring defaults to v1; new Nexus
   examples opt into v2. Document the future default-change boundary.
8. Update pure Python runtime, embedded bridge, native binding, generated
   models/stubs, installed-wheel verification, componentization assets, and
   versioned release metadata together.
9. Keep non-executing `.py` inspection non-executing. Report
   `python_runtime_validation_required`, optionally reserve provisional file
   stem, and direct users to contained validation.
10. Live one-shot and persistent discovery must verify explicit provider name,
    semantics, source/catalog digest, and all collisions before publication.

### Tests

- V1/v2 schema branches are exclusive and reject missing/unknown versions.
- Rust schema, docs schema, Python models, generated stubs, and fixtures agree.
- V1 global CLI spellings remain compatibility-only; v2 local `help`/`status`
  are valid under a non-reserved provider.
- Every parser-owned top-level token is rejected as a provider name.
- Built-in identity is `soma` in catalogs while kind/auth safety remains
  `static-rust`.
- Python old SDK metadata -> v1; explicit new authoring -> v2; old host + v2
  fails with a clear unsupported-version error.
- Python one-shot and persistent catalogs produce the same identity/semantics.
- Non-executing lint never imports Python and reports its visibility limit.
- Filename mismatch, Windows separators/case, Unicode, uppercase, `%2F`, `%2e`,
  and invalid percent encoding have fixtures.

### Verification

```bash
cargo xtask check-provider-manifest-contract
cargo test -p soma-provider-adapters --features python --all-targets
just test-python
just test-python-package
cargo xtask check-docs
```

## Phase 3: Application Resolution, Policy, and Final Dispatch

### Files

- `crates/soma/application/src/types.rs`
- `crates/soma/application/src/app.rs`
- `crates/soma/application/src/app_python_operations.rs`
- `crates/soma/application/src/provider_registry.rs`
- `crates/soma/application/src/provider_registry/refresh.rs`
- `crates/soma/application/src/provider_registry/*_tests.rs`
- `crates/soma/application/src/capabilities.rs`
- application error/status mapping modules and sibling tests

### Work

1. Replace `ExecuteActionRequest { action }` for provider calls with canonical
   `ExecuteProviderToolRequest { id, params }`; keep explicit first-party
   adapters where product actions still use `SomaAction`.
2. Distinguish core `ProviderInvocation` from application
   `PreparedProviderExecution`; retain principal, auth mode, limits,
   confirmation, trace, request, progress, snapshot, entry, provider, and lease.
3. Add an explicit `LegacyProviderToolRequest { surface, flat_name, params }`.
   Canonical calls never attempt legacy fallback.
4. Branch Python control-plane actions only on full built-in identities.
5. Implement snapshot-bound preflight without a generation lease.
6. Bind confirmation proof to identity, snapshot, and destructive metadata;
   final preparation re-resolves and rejects changed policy/target.
7. Acquire one provider generation lease after final resolution; carry the same
   registered entry through authz, admin/scope, confirmation, input schema,
   capabilities, execution, response limit, output schema, result, and paging.
8. Do not hold registry locks during provider work.
9. Centralize stable errors and REST status mappings.
10. Emit success/failure/authz/capability/confirmation/refresh telemetry with
    provider/tool, bounded legacy counters, and no raw params/secrets.
11. Preserve Python candidate digest/fingerprint checks, immutable generations,
    rollback, drain, cancellation, and last-valid-snapshot behavior.

### Tests

- Dynamic `nexus.python_worker_cancel` never invokes the Soma control plane.
- Unknown provider differs from unknown tool in known provider.
- Each legacy surface independently returns unique/ambiguous results.
- Refresh between preflight and dispatch: unchanged target succeeds; changed
  safe->destructive or provider/tool policy returns stale confirmation.
- Confirmation cannot be reused across two providers with local `delete`.
- Waiting for CLI/MCP input holds no generation lease; cancellation releases
  preflight state.
- Final execution never performs a second flat lookup and does not hold a
  registry lock while awaiting provider work.
- Namespace collision candidates fail in one-shot and persistent Python modes,
  retain the old generation, and preserve rollback/status identity.
- Audit/log assertions include provider/tool and exclude payload secrets.

### Verification

```bash
cargo test -p soma-application --all-features
cargo xtask check-test-siblings
cargo xtask check-architecture
```

## Phase 4A: Nested CLI and Provider Management

### Files

- `crates/soma/cli/src/lib.rs`
- `crates/soma/cli/src/provider_command.rs`
- CLI sibling tests
- `apps/soma/tests/cli_parse.rs`
- `apps/soma/tests/provider_cli.rs`
- `apps/soma/src/local.rs`

### Work

1. Change dynamic command state to explicit provider/tool segments plus params.
2. Extend the hand-written parser for `soma PROVIDER TOOL`; keep built-ins first.
3. Parse unresolved structured segments, then resolve once through application
   policy after the live catalog is available.
4. Add provider and tool help from immutable discovery snapshots.
5. Change management testing to
   `soma providers test PROVIDER TOOL [--json JSON]`.
6. Parse provider-local aliases only within the selected namespace.
7. Route old root commands/aliases only through the v1 CLI compatibility map.
8. Prompt using `provider.tool`; carry snapshot-bound proof into final dispatch.
9. Emit one human deprecation warning on stderr and structured JSON warnings
   without contaminating machine stdout.

### Tests

- Nexus happy paths via flags and `--json`.
- Provider help, tool help, missing tool, missing provider, provider beginning
  `-`, all reserved roots, and same alias in two providers.
- V1 command != tool and v1 alias warnings/ambiguity.
- Same local destructive name in two providers prompts full identity.
- `--json` golden stdout and human stderr warnings on Linux and Windows.

### Verification

```bash
cargo test -p soma-cli --all-features
cargo test -p soma --test cli_parse --test provider_cli
```

## Phase 4B: Canonical REST, OpenAPI, Client, and Web

### Files

- `apps/soma/src/http.rs`
- `apps/soma/src/routes.rs` if still present
- `crates/soma/api/src/api.rs`
- `crates/soma/api/src/openapi.rs`
- `crates/soma/api/src/route_inventory.rs`
- API sibling tests and `apps/soma/tests/api_routes.rs`
- `crates/soma/application/src/provider_registry_openapi.rs`
- `crates/soma/client/src/client.rs`
- client tests
- generated TypeScript REST client/source
- `apps/web/lib/soma.ts`
- `apps/web/lib/api.ts`
- web tests and generated actions
- `crates/soma/web/assets/source/**`
- existing web asset synchronization generator/check
- `docs/generated/openapi.json`
- package README/API examples

### Work

1. Add the canonical generic Axum route before the existing wildcard.
2. Extract/decode provider and tool, validate typed IDs, and call canonical
   application dispatch directly.
3. Keep method/path lookup only for custom overlays.
4. Export one canonical reserved-route inventory to live and non-executing
   validators; reject normalized overlaps before router construction.
5. Generate one concrete live OpenAPI path per loaded provider tool with full
   schemas, concrete identity extensions, and collision-safe operation ID.
6. Mark the legacy flat route deprecated and emit RFC 9745 `Deprecation`,
   `Link rel=deprecation`, and RFC 8594 `Sunset` when scheduled.
7. Enforce path/query mappings for GET/HEAD/DELETE; reject body-dependent
   overlays on those methods.
8. Define response compatibility exactly: v2 canonical/custom routes use the
   identity envelope; v1 custom/flat and existing direct product routes retain
   current shapes during compatibility.
9. Update Rust client canonical method signatures and explicit legacy methods.
10. Update generated TypeScript client, web Tool Runner composite keys/routes,
    embedded mirror, tests, and package documentation atomically.
11. Cache generated OpenAPI once per immutable registry snapshot.

### Tests

- Two provider `status` tools have distinct canonical routes and web entries.
- Generic/custom route parity and v1 envelope compatibility.
- Decoded segment attack fixtures and normalized route overlap/shadow cases.
- OpenAPI validates: unique operation IDs, concrete extensions, per-tool
  schemas, deprecated legacy operation, no illegal GET-body contract.
- Operation-ID collision cases such as `a_b.c` vs `a.b_c` remain unique.
- Rust/TypeScript clients call the correct route and decode v1/v2 envelopes.
- Web source and embedded mirror generation/check are current.
- OpenAPI generation/size is measured at the 2,000-tool scale fixture.

### Verification

```bash
cargo test -p soma-api -p soma-client --all-features
cargo test -p soma --test api_routes
pnpm --dir apps/web test
cargo xtask check-schema-docs --check
```

## Phase 4C: MCP Schema, Dispatch, Paging, and Notifications

### Files

- `crates/soma/mcp/src/schemas.rs`
- `crates/soma/mcp/src/tools.rs`
- `crates/soma/mcp/src/rmcp_server.rs`
- `crates/soma/mcp/src/rmcp_server/catalog_subscriptions.rs`
- `crates/soma/mcp/src/protocol_errors.rs`
- MCP sibling/integration tests
- `crates/shared/mcp/server/src/response_paging.rs`
- shared paging tests
- `apps/soma/tests/mcp_http_roundtrip.rs`
- `scripts/mcp-smoke-test.sh`
- mcporter fixture/tests

### Work

1. Require `provider` plus MCP `action` for canonical calls.
2. Generate one complete Draft 2020-12 object branch per pair with const
   discriminators and tool-local parameter schema. Close composition safely.
3. Add pair-based metadata and output-schema branches.
4. Put `_soma_provider` and `_soma_action` inside `structuredContent` for every
   success; include identity in page/error shapes where known.
5. Mirror normalized structured content into a JSON text content block.
6. Resolve built-in elicitation only for `soma.elicit_name` and
   `soma.scaffold_intent`.
7. Bind destructive elicitation/preflight to canonical identity; final
   application enforcement remains authoritative.
8. Return unknown provider/action and validation/business failures as
   `CallToolResult` with `isError: true`; retain protocol errors for the
   existing documented policy boundaries.
9. Bind paging cache/cursors to provider/action, include identity on every page,
   and reject continuation substitution without execution.
10. Cache generated schemas per immutable registry generation.
11. Emit `notifications/tools/list_changed` only after successful
    schema-changing swaps; no notification on rejected candidates.
12. Test the protocol versions actually negotiated by pinned `rmcp` and older
    supported clients.

### Tests

- Two providers expose `status` with incompatible same-named parameter types,
  optional/required differences, and `additionalProperties: false`.
- Exactly one canonical branch matches each valid pair; wrong pairs fail before
  provider execution.
- Dynamic `nexus.elicit_name` does not invoke Soma elicitation.
- Success, structured error, and page objects validate advertised outputSchema.
- Text JSON normalizes identically to structuredContent.
- Cursor identity substitution fails and provider executes once.
- Successful hot reload emits one list-changed notification; rejected reload
  retains schema/fingerprint and emits none.
- Initialization/roundtrip tests record negotiated protocol version.
- Large-catalog tools/list schema bytes and latency have regression budgets.

### Verification

```bash
cargo test -p soma-mcp -p soma-mcp-server --all-features
cargo test -p soma --test mcp_http_roundtrip
cargo xtask check-schema-docs --check
./scripts/mcp-smoke-test.sh
```

## Phase 4D: Palette Composite Identity

### Files

- `crates/soma/palette/src/catalog.rs`
- `crates/soma/palette/src/schema.rs`
- `crates/soma/palette/src/execute.rs`
- `crates/soma/palette/src/dto.rs`
- Palette sibling and route tests
- Palette OpenAPI contract if DTO shape is published

### Work

1. Carry provider/tool fields in catalog and execute DTOs.
2. Key schema lookup and action selection by the pair.
3. Treat joined launcher/display IDs as opaque presentation data.
4. Bind confirmation and errors to full identity.
5. Preserve search/ranking display behavior without using it for dispatch.

### Tests

- Two provider-local `status` tools both list, resolve schema, and execute.
- Display strings are never parsed; malformed display text cannot redirect.
- Destructive confirmation names and binds the selected pair.

### Verification

```bash
cargo test -p soma-palette --all-features
```

## Phase 5: Discovery, Generators, Nexus, and Runtime Proof

### Files

- provider inspection/report and refresh event code
- `docs/PROVIDERS.md`
- `packages/python/README.md`
- `packages/soma-rmcp/README.md`
- `examples/providers/python/nexus.py` or dedicated trial fixture path
- deterministic Nexus collector fixtures/interfaces/tests
- docs/plugin/skill/provider-surface generators and checked outputs
- `xtask/generated_surfaces.rs`
- `scripts/python-platform-gates.py`
- container/MCP/CLI smoke scripts
- changelog and migration guide

### Work

1. Emit deterministic discovery with identity, manifest semantics, input/output
   schema, CLI/MCP/REST/Palette projections, aliases, compatibility state,
   generation, and fingerprint.
2. Expose canonical REST template plus concrete loaded URLs.
3. Update generated provider surfaces, skills, plugin package metadata, web,
   clients, package READMEs, and docs to composite identity.
4. Add fixture-backed v2 `nexus.py` with the requested tools: repos, shares,
   services, keys, nginx, and containers.
5. Keep collectors behind narrow interfaces. Default fixtures contain no real
   hosts, keys, proxy secrets, or network calls.
6. Normalize parity at the application result level, then assert each adapter's
   own envelope/discriminators/warnings/paging independently.
7. Exercise Nexus through CLI, canonical REST, mcporter MCP, and Palette in both
   Python one-shot and persistent modes.
8. Hot reload adds/removes a tool; invalid namespace collision retains the
   previous generation and emits no MCP list change.
9. Add opt-in trusted-local live Nexus smoke. It is read-only, scope/admin and
   redaction aware, prefers Labby for lab access, and never runs in default CI.
10. Extend performance policy beyond pure Python function calls to registry
    build, schema generation, persistent supervisor, and cross-surface routing.
11. Publish migration docs, compatibility start release N, SDK version floor,
    deprecation link, future removal gate, and bounded legacy metrics.

### Tests

- Generated artifacts and schema/docs freshness are deterministic.
- Fixture Nexus normalized parity includes success, unknown tool, validation,
  warning, paging, and identity.
- One-shot/persistent output and hot-reload semantics agree.
- No lab-specific collector code enters static provider or scaffold paths.
- Live smoke is opt-in and redacts key/nginx-sensitive material.
- Container smoke covers CLI + REST + MCP, not only flat REST.
- Scale budgets cover snapshot build, tools/list schema build/bytes, OpenAPI
  build/bytes, and warm canonical lookup.

## Phase 6: Full Verification and Compatibility Release

### Local Gates

```bash
cargo fmt -- --check
cargo xtask ci
cargo xtask check-docs
cargo xtask check-schema-docs --check
cargo xtask check-provider-manifest-contract
cargo xtask check-version-sync
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
just test-python
just test-python-package
pnpm --dir apps/web test
bd swarm validate rmcp-template-ob48
git diff --check
```

### CI/Runtime Matrix

| Boundary | Required proof |
|---|---|
| Linux Rust | Full workspace/xtask gates on self-hosted runner. |
| Native Windows | Parser/path/serde/provider-core/application/CLI/MCP/Python package and selected end-to-end tests on `windows-latest`. |
| Python one-shot | V1 + explicit v2 discovery, collision, invocation, and parity. |
| Python persistent | Same plus generation swap, cancel/drain/rollback, list-changed. |
| MCP stdio + HTTP | Negotiation, pair schemas, text/structured parity, paging, errors. |
| REST | Canonical/custom/legacy/direct envelope and deprecation behavior. |
| Palette/web/client | Duplicate local names remain distinct and executable. |
| Container | Full binary CLI + canonical REST + mcporter MCP Nexus fixture. |
| Performance | Registry/schema/OpenAPI/supervisor/cross-surface regression budgets. |

### Merge Conditions

- All required aggregate checks are green on the final commit.
- Native Windows ran, not merely cross-compiled.
- Python package changes actually selected relevant CI jobs.
- No unresolved high/critical research or review finding remains.
- Checked OpenAPI/docs/generated artifacts match runtime generation.
- Full identity appears in logs/errors without payload or credential leakage.
- Nexus fixture parity is green; live lab smoke is reported separately and is
  not used to mask deterministic failures.
- Release notes record compatibility release N and published SDK version.

## Rollout and Rollback

1. Ship host support that accepts v1 and v2 before requiring SDK v2 output.
2. Publish the Python SDK with explicit v2 authoring and retain v1 default
   during the compatibility release.
3. Preserve old-host + new-SDK failure as a clear unsupported manifest version;
   do not silently reinterpret v2 as flat.
4. On registry refresh failure, retain the previous immutable generation and
   emit no tools-list-changed notification.
5. If canonical routes regress after release, operators may remain on v1
   compatibility paths while a fix ships; do not disable canonical identity or
   weaken collision checks.
6. Record legacy-use metrics per surface/canonical target with bounded
   cardinality and no user/request/parameter data.
7. Hand removal to the separate adoption-gated epic only after the published
   release and SDK/version-matrix/telemetry conditions are met.

## Completion Criteria

- Canonical pair identity is the only internal execution key.
- Duplicate local tool names work on every provider execution surface.
- Legacy behavior is isolated per surface and observable.
- Confirmation cannot be replayed across identity or policy changes.
- Python v2 authoring, built-in migration, and inspection limits are explicit.
- Pair-specific MCP schemas and concrete OpenAPI operations are truthful.
- Palette, web, clients, paging, refresh events, and generators no longer flatten
  identity.
- Linux and native Windows CI, deterministic Nexus parity, one-shot/persistent
  Python, and full-binary smoke pass.
- Compatibility removal remains open in its separate post-release epic.
