# Contract Conformance Tests

## Store suites

Each implementation runs shared conformance tests for:

```text
SourceLedger
JobStore
ObservationStore
CheckpointStore
VectorStore
GraphStore
MemoryStore
ArtifactStore where introduced
```

The suite covers normal behavior, invalid transitions, cancellation, idempotency, concurrency, bounded results, and secret-safe failures.

## Adapter suites

Every source adapter:

- routes from representative requests;
- advertises accurate capabilities;
- discovers a deterministic manifest;
- honors complete/incomplete discovery;
- acquires only requested changes;
- produces bounded normalized documents;
- handles cancellation;
- never leaks credentials.

Every observation adapter:

- produces valid `ObservationRecord`;
- separates observed and claimed identities;
- handles malformed input;
- obeys bounds;
- classifies errors;
- exposes health;
- supports restart/checkpoint semantics when applicable.

## RAG suites

- all chunks have valid locators;
- point IDs are deterministic;
- upsert is idempotent;
- query results hydrate canonical citations;
- backend unavailability does not corrupt canonical state;
- processing-fingerprint changes reindex;
- synthesis cannot cite nonexistent records.

## Graph suites

- no evidence-free edge or claim;
- merges are reversible/rebuildable;
- ambiguity is preserved;
- temporal filters work;
- bounded path queries terminate on cycles;
- expired evidence removes or weakens projections according to policy.

## Schema fixtures

Every example under `03-contracts/examples` validates against `schemas.json`.

Crate-specific fixtures generated from Rust types MUST remain reference-equivalent to the combined schema.
