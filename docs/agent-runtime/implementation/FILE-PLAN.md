---
title: "Agent Runtime File Plan"
created: 2026-08-05
updated: 2026-08-05
doc_type: "implementation-plan"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# File Plan

This is the proposed mutation map. Re-audit names against current main before each PR. Do not create empty modules solely to satisfy this list.

## PR 1: Contracts and generated fixtures

### Add

~~~text
docs/generated/agent-runtime/*.schema.json
apps/soma/tests/agent_runtime_contracts.rs
apps/soma/tests/fixtures/agent-runtime/*
~~~

### Edit

~~~text
scripts/generate-docs.py
xtask/src/generated_surfaces.rs
xtask/src/patterns/surfaces.rs
apps/soma/tests/architecture_boundaries.rs
docs/CLAUDE.md
~~~

The generated source must point back to <code>docs/agent-runtime/schemas</code>; do not maintain two manually edited schema sets.

## PR 2: Paths and config

### Add

~~~text
crates/soma/config/src/agent_runtime_paths.rs
crates/soma/config/src/agent_runtime_paths_tests.rs
crates/soma/config/src/agent_runtime.rs
crates/soma/config/src/agent_runtime_tests.rs
~~~

### Edit

~~~text
crates/soma/config/src/lib.rs
crates/soma/config/src/config.rs
crates/soma/config/src/config_tests.rs
crates/soma/config/src/env_registry.rs
crates/soma/config/src/env_registry_tests.rs
apps/soma/src/bootstrap.rs
crates/soma/cli/src/setup.rs
crates/soma/cli/src/setup_tests.rs
crates/soma/cli/src/doctor.rs
crates/soma/cli/src/doctor/checks.rs
config.toml
.env.example
docs/CONFIG.md
docs/ENV.md
~~~

## PR 3: Domain and application boundaries

### Add domain

~~~text
crates/soma/domain/src/agent_runtime.rs
crates/soma/domain/src/agent_runtime/ids.rs
crates/soma/domain/src/agent_runtime/capability.rs
crates/soma/domain/src/agent_runtime/context.rs
crates/soma/domain/src/agent_runtime/disclosure.rs
crates/soma/domain/src/agent_runtime/run.rs
crates/soma/domain/src/agent_runtime/snippet.rs
crates/soma/domain/src/agent_runtime/stack.rs
crates/soma/domain/src/agent_runtime/synthesis.rs
crates/soma/domain/src/agent_runtime_tests.rs
~~~

### Add application

~~~text
crates/soma/application/src/agent_runtime.rs
crates/soma/application/src/agent_runtime/context.rs
crates/soma/application/src/agent_runtime/disclosure.rs
crates/soma/application/src/agent_runtime/package.rs
crates/soma/application/src/agent_runtime/run.rs
crates/soma/application/src/agent_runtime/runtime.rs
crates/soma/application/src/agent_runtime/snippet.rs
crates/soma/application/src/agent_runtime/stack.rs
crates/soma/application/src/agent_runtime/synthesis.rs
crates/soma/application/src/agent_runtime_tests.rs
~~~

### Edit

~~~text
crates/soma/domain/src/lib.rs
crates/soma/application/src/lib.rs
crates/soma/application/src/ports.rs
crates/soma/application/src/app.rs
crates/soma/application/src/app_tests.rs
apps/soma/src/bootstrap.rs
apps/soma/src/bootstrap_tests.rs
~~~

Keep <code>SomaApplication::new</code> signature stable by adding the bundle inside <code>ApplicationPorts</code> rather than a fourth constructor argument.

## PR 4: Durable jobs and control store

The exact shared crate name follows context-v1 implementation. Expected additions or edits:

~~~text
crates/shared/jobs/src/...
crates/soma/runtime/src/agent_runtime.rs
crates/soma/runtime/src/agent_runtime/control_store.rs
crates/soma/runtime/src/agent_runtime/migrations.rs
crates/soma/runtime/src/agent_runtime/worker.rs
crates/soma/runtime/src/agent_runtime/outbox.rs
crates/soma/runtime/src/agent_runtime/recovery.rs
crates/soma/runtime/src/agent_runtime_tests.rs
~~~

Transplant from Axon unified job modules with donor provenance comments and parity tests. Do not create <code>crates/soma/jobs</code> if the shared job crate is already landing through context v1.

## PR 5: Shared snippet store

### Edit shared Code Mode

~~~text
crates/shared/codemode/src/snippet.rs
crates/shared/codemode/src/snippet/store.rs
crates/shared/codemode/src/snippet/index.rs
crates/shared/codemode/src/snippet/io.rs
crates/shared/codemode/src/snippet/resolve.rs
crates/shared/codemode/src/types/catalog.rs
crates/shared/codemode/src/host.rs
crates/shared/codemode/src/runner_drive/snippet.rs
~~~

### Add

~~~text
crates/shared/codemode/src/snippet/filesystem.rs
crates/shared/codemode/src/snippet/frontmatter.rs
crates/shared/codemode/src/snippet/markdown.rs
crates/shared/codemode/src/snippet/promotion.rs
crates/shared/codemode/src/snippet/requirements.rs
crates/shared/codemode/src/snippet/*_tests.rs
crates/soma/integrations/src/snippets.rs
crates/soma/integrations/src/snippets_tests.rs
~~~

Port LABBY behavior, preserving Soma's execution budgets and public naming.

## PR 6: Context compiler and store

The context-v1 implementation determines final shared crate paths. Soma product files should be:

~~~text
crates/soma/application/src/agent_runtime/context.rs
crates/soma/application/src/agent_runtime/context/compiler.rs
crates/soma/application/src/agent_runtime/context/validation.rs
crates/soma/application/src/agent_runtime/context/planning.rs
crates/soma/application/src/agent_runtime/context/enrichment.rs
crates/soma/runtime/src/agent_runtime/context_store.rs
crates/soma/integrations/src/context.rs
crates/soma/integrations/src/context_tests.rs
~~~

Edit product exports, ports, and bootstrap wiring. Shared Axon/Cortex-derived query engines remain behind ports.

## PR 7: Materializers

~~~text
crates/soma/application/src/agent_runtime/context/materialization.rs
crates/soma/runtime/src/agent_runtime/materialization.rs
crates/soma/runtime/src/agent_runtime/materialization/manifest.rs
crates/soma/runtime/src/agent_runtime/materialization/briefing.rs
crates/soma/runtime/src/agent_runtime/materialization/filesystem.rs
crates/soma/runtime/src/agent_runtime/materialization/graph.rs
crates/soma/runtime/src/agent_runtime/materialization/jsonl.rs
crates/soma/integrations/src/context_resources.rs
~~~

Reuse <code>soma-codemode</code> artifacts instead of adding an unrelated artifact store.

## PR 8: LABBY loadouts

### Add

~~~text
crates/soma/integrations/src/labby_loadout.rs
crates/soma/integrations/src/labby_loadout_tests.rs
crates/soma/runtime/src/agent_runtime/loadout_store.rs
~~~

### Edit

~~~text
crates/soma/integrations/src/lib.rs
crates/soma/application/src/agent_runtime/runtime.rs
apps/soma/src/bootstrap.rs
~~~

If a product-neutral policy type is missing from Soma's shared gateway crate, port it from LABBY into <code>crates/shared/mcp/gateway</code> rather than depending on the LABBY product crate.

## PR 9: Incus workload APIs

### Add shared client resources

~~~text
crates/shared/incus-client/src/resources/instance_exec.rs
crates/shared/incus-client/src/resources/instance_exec_tests.rs
crates/shared/incus-client/src/resources/instance_files.rs
crates/shared/incus-client/src/resources/instance_files_tests.rs
crates/shared/incus-client/src/resources/instance_state.rs
crates/shared/incus-client/src/resources/instance_state_tests.rs
~~~

### Edit

~~~text
crates/shared/incus-client/src/resources.rs
crates/shared/incus-client/src/lib.rs
crates/shared/incus-client/src/operations.rs
crates/shared/incus-client/src/operations_tests.rs
~~~

### Add Soma adapter

~~~text
crates/soma/integrations/src/incus_runtime.rs
crates/soma/integrations/src/incus_runtime_tests.rs
~~~

## PR 10: Codex supervisor and runtime adapter

Prefer a narrow new binary only after confirming it cannot live as a subcommand/mode of the existing app:

~~~text
apps/soma-agent-supervisor/Cargo.toml
apps/soma-agent-supervisor/src/main.rs
apps/soma-agent-supervisor/src/bootstrap.rs
apps/soma-agent-supervisor/src/runtime.rs
apps/soma-agent-supervisor/src/telemetry.rs
~~~

Soma adapter:

~~~text
crates/soma/integrations/src/codex_runtime.rs
crates/soma/integrations/src/codex_runtime_tests.rs
crates/soma/runtime/src/agent_runtime/supervisor.rs
~~~

Do not copy protocol code from <code>codex-app-server-client</code>.

## PR 11: Disclosure

~~~text
crates/soma/application/src/agent_runtime/disclosure.rs
crates/soma/application/src/agent_runtime/disclosure/policy.rs
crates/soma/application/src/agent_runtime/disclosure/selection.rs
crates/soma/runtime/src/agent_runtime/disclosure_store.rs
crates/soma/integrations/src/disclosure.rs
~~~

## PR 12: Cortex lifecycle events

~~~text
crates/soma/runtime/src/agent_runtime/outbox.rs
crates/soma/integrations/src/cortex_events.rs
crates/soma/integrations/src/cortex_events_tests.rs
~~~

Shared event/observation types should land in the context-v1 observations crate, not a second event framework.

## PR 13: Context-aware Code Mode

~~~text
crates/soma/integrations/src/codemode.rs
crates/soma/integrations/src/codemode/context_host.rs
crates/soma/integrations/src/codemode/context_tools.rs
crates/soma/integrations/src/codemode/research_tools.rs
crates/shared/codemode/src/protocol.rs
crates/shared/codemode/src/runner_drive/snippet.rs
~~~

The input-protocol fix must serve application Code Mode and provider Code Mode together.

## PR 14: Synthesis and Axon jobs

~~~text
crates/soma/application/src/agent_runtime/synthesis.rs
crates/soma/application/src/agent_runtime/synthesis/investigation.rs
crates/soma/application/src/agent_runtime/synthesis/questions.rs
crates/soma/application/src/agent_runtime/synthesis/claims.rs
crates/soma/integrations/src/axon_research.rs
crates/soma/integrations/src/axon_research_tests.rs
crates/soma/runtime/src/agent_runtime/synthesis_store.rs
~~~

Reuse the shared Axon-derived retrieval, jobs, graph, and LLM crates selected by context v1.

## PR 15: APM adapter

~~~text
crates/soma/integrations/src/apm.rs
crates/soma/integrations/src/apm_tests.rs
crates/soma/runtime/src/agent_runtime/package_cache.rs
~~~

Use the shared process/operations crate for bounded execution. Do not add Python resolver code to Soma.

## PR 16: Surfaces and E2E

### CLI

~~~text
crates/soma/cli/src/agent_runtime.rs
crates/soma/cli/src/agent_runtime_tests.rs
crates/soma/cli/src/lib.rs
~~~

### API/OpenAPI

~~~text
crates/soma/api/src/agent_runtime.rs
crates/soma/api/src/agent_runtime_tests.rs
crates/soma/runtime/src/protected_routes.rs
~~~

### MCP

~~~text
crates/soma/mcp/src/agent_runtime.rs
crates/soma/mcp/src/agent_runtime_tests.rs
crates/soma/domain/src/actions.rs
~~~

### Web

~~~text
apps/web/app/runs/...
apps/web/app/contexts/...
apps/web/app/snippets/...
apps/web/lib/soma.ts
crates/soma/web/assets/source/...
~~~

### E2E

~~~text
apps/soma/tests/agent_runtime_e2e.rs
apps/soma/tests/fixtures/agent-runtime/*
scripts/agent-runtime-smoke.sh
~~~

Keep web generated source and embedded assets synchronized through existing tooling.
