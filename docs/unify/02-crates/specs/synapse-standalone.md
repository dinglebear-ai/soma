# Standalone Synapse Product Specification

## Role

`apps/synapse` is the product composition and transport boundary for the complete canonical operations engine. It owns configuration, topology materialization, authorization policy, confirmation, activity, status, OpenAPI, CLI, REST, and MCP adapters. It does not own infrastructure behavior and does not link the imported donor runtime.

## Composition

One immutable topology snapshot and one routed local-or-strict-SSH command executor feed all concrete drivers. Per-host read, build, execution, source-transfer, and destination-transfer roots remain independent. Docker clients are revision-bound, remote Docker uses private SSH-forwarded sockets, and every surface delegates to `SynapseReadRuntime` or `SynapseMutationRuntime`.

## Product surfaces

- Canonical CLI commands: `operations`, `plan`, `run`, `serve`, and `mcp`.
- Optional `legacy flux|scout` request normalization without legacy result projection.
- REST plan and execute endpoints for all 59 operations.
- Three MCP tools: canonical `synapse` plus optional `flux` and `scout` aliases.
- HTTP and stdio MCP transports.
- Catalog-derived OpenAPI containing 59 execute paths and 21 mutation-plan paths.
- Public health/readiness/status and bounded recent activity.

## Authorization

Read operations execute after parameter validation. Mutations first produce a deterministic plan. CLI and REST require explicit confirmation, while MCP uses bounded elicitation requiring both `confirm` and `understood`. Product-issued authorization evidence binds operation, target, plan fingerprint, actor, deadline, and idempotency key. Automatic confirmation is disabled by default.

## Security invariants

1. No donor runtime module is linked.
2. No legacy result projector exists.
3. Filesystem, build, host-exec, and transfer policies are configured per host.
4. SSH uses strict known-host policy through `soma-fleet`.
5. Protected HTTP routes use a constant-time bearer comparison when configured.
6. Mutation confirmation cannot widen the planned operation or target.
7. Activity is bounded and strips control characters.
8. OpenAPI and MCP schemas derive from the checked-in canonical catalog.

## Verification

- all 59 operations are discoverable through CLI, REST/OpenAPI, and MCP;
- 35 reads and 21 mutations delegate to the canonical runtime;
- mutation confirmation returns or displays the exact plan;
- activity, health, readiness, status, and bearer policy tests pass;
- strict Clippy, warning-denied rustdoc, architecture, sibling, pattern, donor-isolation, and release checks pass.
