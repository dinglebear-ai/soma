# Axon Extraction Program

## Goal

Transfer Axon's reusable knowledge machinery into coarse Soma shared crates, then compose it through Soma's existing product and surfaces.

The program does not preserve Axon's 23-crate topology.

## Extraction regions

```text
1. Contract vocabulary and generated references
2. Source routing
3. Source adapters
4. Spider web crawling
5. Document preparation, parsing and chunking
6. Embedding/vector/retrieval/synthesis
7. Source generation ledger
8. Durable jobs
9. AI sessions
10. Evidence graph
11. Memory
12. Live orchestration mined from axon-services
```

## First walking skeleton

```text
local directory
    ↓ route
resolved source
    ↓ discover/diff
generation
    ↓ normalize
documents
    ↓ prepare
Markdown/code/prose chunks
    ↓ embed/upsert
FTS + Qdrant
    ↓ query
cited results through CLI/API/MCP/Web
```

This executes synchronously before the job runtime is introduced.

## Contract extraction

Move runtime-neutral types into their owning shared crates. Port Axon's schema generation patterns into Soma's existing `xtask`.

Do not move:

- Axon REST/MCP/CLI DTO envelopes;
- Axon auth scopes;
- global configuration;
- broad `axon-core` utilities;
- product service context.

## RAG extraction

`soma-rag` is assembled from actual working behavior across Axon document, parse, embedding, vector, retrieval, LLM, and service orchestration. It is not a folder concatenation.

The extraction must trace the live call path and write parity fixtures for:

- chunk boundaries;
- processing identity;
- vector payloads;
- hybrid query results;
- publication visibility;
- citations;
- synthesis inputs.

## Jobs extraction

First make the source pipeline callable directly. Then wrap it in generic durable jobs. Product runners remain in Soma.

## Memory extraction

Memory follows context/RAG integration so it can depend on stable retrieval and evidence seams.

## Completion

Axon extraction is complete when:

- every required source family runs in Soma;
- current Axon fixtures or equivalent parity fixtures pass;
- no shared crate depends on Axon;
- Soma owns the only active product composition;
- derived indexes can be rebuilt;
- migration/cutover procedures are tested.
