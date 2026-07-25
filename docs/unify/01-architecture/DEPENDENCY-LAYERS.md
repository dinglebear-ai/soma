# Dependency Layers

## Layer 0: leaf primitives

- `soma-primitives`
- `soma-sanitize`
- `soma-process`

Constraints:

- no Soma product dependencies;
- no database clients by default;
- no network clients by default;
- no ambient product configuration;
- minimal default features.

## Layer 1: domain protocols

- `soma-route`
- `soma-transcript`
- `soma-observations`
- `soma-llm`
- `soma-graph`

## Layer 2: engines and adapters

- `soma-sources`
- `soma-crawl`
- `soma-ledger`
- `soma-jobs`
- `soma-rag`
- `soma-memory`
- `soma-ingest`
- `soma-collectors`

## Layer 3: Soma product composition

Existing `crates/soma/{domain,application,runtime,integrations}` modules compose the shared crates.

## Layer 4: surfaces

Existing `crates/soma/{cli,api,mcp,web}` call application use cases only.

## Layer 5: executable

`apps/soma` owns configuration loading, concrete backend construction, worker lifecycle, routing, and shutdown.

## Prohibited edges

- shared -> `crates/soma/*`;
- shared -> `apps/*`;
- observation adapters -> RAG;
- source adapters -> Qdrant;
- jobs -> source adapters/RAG/ledger;
- RAG -> source acquisition;
- graph -> Soma web/API/MCP;
- surfaces -> database clients;
- integrations -> product types.
