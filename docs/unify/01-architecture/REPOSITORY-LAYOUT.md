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
      rag/            # soma-rag

    observations/
      model/          # soma-observations
      ingest/         # soma-ingest
      collectors/     # soma-collectors

  integrations/
    # Existing pure clients remain here.
    # New pure TEI/OpenAI/Gemini clients may be added only if
    # they satisfy Soma's integration-lane rules.

  soma/
    domain/
      src/
        knowledge/
        observations/
        graph/
        context/
        memory/
    application/
      src/
        sources/
        observations/
        context/
        graph/
        memory/
        jobs/
    runtime/
      src/
        knowledge/
        observations/
        projection/
        graph/
        storage/
    api/
    cli/
    mcp/
    web/
    # existing product crates remain

apps/
  soma/
    # composition root

docs/
  convergence/
    context-v1/
  contracts/
  generated/

xtask/
  src/
    context_contracts/
    donor_parity/
```

Nested paths organize the workspace. Public package names remain the names listed in the crate catalog.

Every proposed public package follows `soma-<one-word>`. Repository leaf directories are also one word, though organizational parent directories may group related crates.

No new product crate is required merely because a new module exists.
