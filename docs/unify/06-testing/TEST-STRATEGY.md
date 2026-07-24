# Test Strategy

## Test pyramid

```text
Pure unit/property/fuzz tests
        ↓
Crate contract and fake-provider tests
        ↓
Backend integration tests
        ↓
Donor parity tests
        ↓
Soma application use-case tests
        ↓
Surface projection tests
        ↓
End-to-end appliance scenarios
        ↓
GraphRAG quality and operational soak
```

## Required test classes

### Unit

Pure algorithms, state machines, identity, validation, parsing, chunking, redaction, scoring, and retention decisions.

### Property and fuzz

Untrusted parsers, nested metadata, manifests, locators, syslog/OTLP, transcript JSONL, code/Markdown chunkers, and stable ID invariants.

### Contract

Every trait has a deterministic fake and conformance suite. Store/provider implementations run the same suite.

### Integration

SQLite, Qdrant, TEI, Spider/Chrome, GitHub and other network clients use controlled fixtures or test services. External live tests are quarantined and never the only validation.

### Parity

Pinned Axon/Cortex fixtures compare old and new behavior.

### Product

Application use cases verify authorization, jobs, progress, citations, and shared surface projection.

### E2E

One running Soma appliance with SQLite, Qdrant, TEI, and Chrome executes full source and observation flows.

### Quality evaluation

Saved queries measure retrieval, graph usefulness, citation correctness, and synthesis faithfulness.

## Test data policy

- Synthetic secrets test redaction.
- Real user data is not committed.
- Donor fixtures are reviewed for credentials and licensing.
- Large corpora are content-addressed test assets or generated locally.
- Every fixture records producer version and expected contract version.

## CI tiers

### Fast PR

- formatting/lints;
- unit/property subset;
- schema/architecture checks;
- default and key feature builds;
- SQLite contract tests;
- no external services where avoidable.

### Full PR or merge queue

- Qdrant/TEI/Chrome integration;
- complete feature matrix;
- donor parity;
- GraphRAG evaluation subset;
- package checks.

### Nightly

- fuzzing;
- large-corpus crawl/index;
- throughput/soak;
- full evaluation suite;
- backup/restore;
- migration;
- vulnerability/license checks.

### Release

- all prior tiers;
- clean package consumers;
- upgrade from prior release;
- fresh appliance install;
- rollback rehearsal;
- North-star scenario.
