---
title: "Proposed Repository Layout"
created: 2026-07-24
updated: 2026-07-31
---

# Proposed Repository Layout

```text
crates/
  shared/
    context/
      primitives/     # soma-primitives
      sanitize/       # soma-sanitize
      transcript/     # soma-transcript
      graph/          # soma-graph

    runtime/
      process/        # soma-process
      jobs/           # soma-jobs

    knowledge/
      route/          # soma-route
      sources/        # soma-sources
      crawl/          # soma-crawl
      ledger/         # soma-ledger
      memory/         # soma-memory

    semantic/
      llm/            # soma-llm
      rag/             # soma-rag

    observations/
      model/          # soma-observations
      ingest/         # soma-ingest
      collectors/     # soma-collectors

    operations/
      ops/             # soma-ops
      fleet/           # soma-fleet
      infra/           # soma-infra

    incus-client/      # existing neutral Incus protocol client

  integrations/
    # Existing pure clients remain here until normalized into the shared lane.

  labby/
    application/
    cli/
    api/
    mcp/
    web/
    runtime/

  axon/
    application/
    cli/
    api/
    mcp/
    web/
    runtime/

  cortex/
    application/
    cli/
    api/
    mcp/
    web/
    runtime/

  synapse/
    application/
    cli/
    api/
    mcp/
    web/
    runtime/

  soma/
    domain/
    application/
    integrations/
    runtime/
    api/
    cli/
    mcp/
    web/

apps/
  labby/              # standalone gateway composition root
  axon/               # standalone research/RAG composition root
  cortex/             # standalone observation/graph composition root
  synapse/            # standalone infrastructure operations composition root
  soma/               # integrated superset composition root
  web/                # shared Aurora application where applicable
  palette/            # design-system development surface

docs/
  unify/
  contracts/
  generated/

xtask/
  src/
    context_contracts/
    donor_parity/
    product_family/
```

Nested paths organize the workspace. Public package names remain the names listed in the crate catalog.

Every proposed public shared package follows `soma-<one-word>`. Repository leaf directories are also one word, though organizational parent directories may group related crates.

## Layout rules

- `crates/shared/*` contains independently consumable neutral crates only.
- `crates/<product>/*` contains product policy, compatibility, and surfaces.
- `apps/<product>` is the only final process composition root for that product.
- Shared crates accept explicit configuration and do not read product environment variables.
- A standalone product must not depend on `crates/soma/*` or `apps/soma`.
- Soma may depend on shared crates and Soma-owned adapters, not other products' CLI, MCP, API, or web internals.
- New product crates are created only where product-specific behavior requires a stable boundary.
