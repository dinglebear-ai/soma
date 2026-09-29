# cortex-domain

`cortex-domain` is the storage- and transport-neutral semantic core extracted
from Cortex into Soma. It contains data and deterministic rules whose meaning
does not depend on SQLite, Axum, RMCP, CLI parsing, process globals, scanner
implementations, file-tail runtime state, or host configuration.

## What belongs here

- request identity used by application/domain policy;
- normalized log and incident entities;
- heartbeat state, pressure/status policy, and derived fleet/correlation summaries;
- graph entities, relationships, evidence, deterministic narratives, and confidence policy;
- AI incident/event entities used by deterministic finding engines;
- investigation claims and evidence summaries;
- topology findings and stable reason/category constants;
- deterministic hook/MCP/skill signal detectors;
- stable observatory identity-key construction;
- domain validation/not-found errors.

The deterministic incident, hook, MCP, and skill finding engines were moved with
their donor parity tests because they are pure rule evaluation: no database
queries and no model calls.

## What does not belong here

Transport request/response envelopes, REST/MCP-specific limit policy, SQLite
rows and maintenance results, database statistics, persistence conversions, OS
journal responses, file-tail operations, scanner health implementation types,
receiver counters, inventory collection/runtime state, notification runtime config, and
process/runtime state are intentionally excluded. Pure inventory snapshot contracts live in
`cortex-inventory`, not in the domain crate.

Database row conversions are owned by the planned `cortex-storage-sqlite`
adapter. Transport envelopes are owned by the planned `cortex-api` and
`cortex-mcp` crates. Runtime-only state belongs to the capability/runtime crate
that produces it.

## Provenance and compatibility

The extraction baseline is Cortex commit
`7edf23fadb94650c2d2a2f9c80111fb44319eea8`. Public semantic field names and
serde behavior are preserved for extracted types. See
`docs/cortex-extraction/MODEL-CLASSIFICATION.md` for the complete donor model
ownership inventory and `docs/cortex-extraction/VERIFICATION.md` for parity
gates.
