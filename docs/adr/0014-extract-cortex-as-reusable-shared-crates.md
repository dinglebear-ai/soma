---
title: "ADR 0014: Extract Cortex as reusable shared crates"
created: 2026-08-17
updated: 2026-08-17
doc_type: "adr"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-08-17"
---

# ADR 0014: Extract Cortex as reusable shared crates

## Status

Active, 2026-08-17.

## Context

Cortex is a mature Rust product whose log intelligence, ingestion, fleet
inventory, investigation, agent, and operational capabilities are useful in
other products. Today those capabilities live in one `cortex` package. The
crate already has meaningful internal service boundaries, but product runtime,
SQLite row types, transport adapters, configuration, and public response models
remain coupled strongly enough that another project cannot reuse a capability
without depending on the whole Cortex product.

Soma already owns the fleet convention for reusable Rust engines under
`crates/shared/**`. ADRs 0002, 0003, 0004, 0009, and 0010 require extracted
capabilities to preserve full products, enforce downward dependency direction,
provide runtime builders, execute in reviewable lanes, and pass workspace-level
boundary verification.

The reviewed Cortex donor for the first extraction wave is commit
`7edf23fadb94650c2d2a2f9c80111fb44319eea8` from branch
`codex/graph-projection-lifecycle`. That donor contains two commits not yet in
`origin/main` at extraction start, so source inventory must record the donor
commit rather than silently treating the remote main branch as equivalent.

## Decision

Extract Cortex incrementally into namespaced reusable crates under
`crates/shared/cortex/**`, while preserving a buildable Cortex application as
a thin composition of those crates. Do not rewrite Cortex from scratch and do
not land the extraction as a single monolithic source move.

The target dependency direction is:

```text
apps/cortex
  -> cortex runtime and product surfaces
  -> cortex application/use-case layer
  -> reusable Cortex capability crates
  -> general Soma shared crates
  -> external crates
```

Every crate placed under `crates/shared/cortex/**` is a shared-layer crate for
Soma architecture enforcement. It may depend on other `crates/shared/**`
packages, but it must not depend on `apps/**`, `crates/soma/**`, or another
product layer.

### Target crate set

The extraction plan starts with these boundaries. Names may be refined before a
crate is first created, but moving responsibilities across layers requires an
update to the extraction spec and contract.

- `cortex-ingest-core`: transport-neutral normalization, signature, metadata
  bounding/redaction, and similarly pure ingest primitives.
- `cortex-domain`: storage- and transport-neutral request/response contracts,
  identifiers, invariants, and error taxonomy.
- `cortex-storage-sqlite`: SQLite pool, migrations, query persistence, and
  projection adapters.
- `cortex-ingest`: syslog, OTLP, file, Docker, shell, transcript, parser, and
  enrichment pipelines expressed against explicit sink/source ports.
- `cortex-inventory`: inventory schemas, collectors, cache, heartbeat, and
  investigation topology capabilities.
- `cortex-observatory`: agent observation, attribution, classification, and
  projector behavior.
- `cortex-agent`: host-local forwarding and heartbeat runtime suitable for
  embedding independently from the central Cortex server.
- `cortex-application`: the use-case facade and business/service policy shared
  by all Cortex surfaces.
- `cortex-api` and `cortex-mcp`: thin Cortex-specific REST and MCP adapters
  over the application facade.
- `cortex-runtime`: explicit runtime builder and maintenance-task composition.
- `cortex-ops`: setup, doctor, deploy, update, and local operational mechanics
  that remain useful outside the final binary.
- `apps/cortex`: canonical thin Cortex binary and CLI composition.

### Boundary rules

1. Domain contracts do not expose SQLite row types, Axum/RMCP request types, or
   process-global runtime handles. Mapping from storage rows into domain models
   belongs at the storage/application boundary.
2. Ingest engines depend on narrow ports for sinks and clocks where practical,
   rather than opening Cortex SQLite directly.
3. REST, MCP, CLI, and binary entrypoints stay thin. Validation and business
   policy live in the application/domain layers.
4. The final Cortex runtime exposes a library-level builder with explicit
   configuration and dependencies. The binary is a wrapper over that API.
5. Cortex authentication migrates from the pinned external `lab-auth` git
   dependency to Soma's reusable `soma-auth` crate or an explicit shared
   adapter. Extracted crates must not introduce a new direct Labby auth
   dependency.
6. Heavy transports and optional integrations use Cargo features where they can
   be omitted safely. Crates define explicit defaults instead of inheriting an
   accidental all-in runtime.
7. New crates start `publish = false`. Publishing is a later gate after the
   API is stable, tests cover the public boundary, and an independent consumer
   fixture exists.
8. Behavior preservation is the default. Any intentional behavior change is
   documented separately from the mechanical extraction and receives its own
   compatibility tests.

## First proof crate

The first lane extracts Cortex's existing `normalize.rs` and
`ingest_metadata.rs` behavior as `cortex-ingest-core`. This code is pure,
hot-path, and useful without storage or transports, making it a useful proof of
the shared-crate contract without forcing premature decisions about the larger
SQLite/application boundary.

The donor tests move with the implementation. Additional integration tests call
the crate strictly through its public API, proving another project can consume
it without the Cortex runtime. `NORMALIZER_VERSION` becomes a public
compatibility marker, and output-changing normalization edits must bump it.

## Execution

Follow ADR 0009. Each later extraction lane owns its crate-local source, README,
public API, tests, and source-parity evidence. A dedicated integration lane owns
workspace manifests, final Cortex composition, global routing, CI wiring, and
cross-crate conflict resolution. The progress tracker under
`docs/cortex-extraction/PROGRESS.md` is the authoritative migration ledger.

## Verification

Every merged lane must satisfy the checks in
`docs/cortex-extraction/VERIFICATION.md`. At minimum, architecture metadata
and physical location must agree, public docs must build without warnings,
crate tests and independent-consumer tests must pass, and integration waves must
pass Soma's all-features workspace check/test gates.

The extraction is not complete until a Cortex binary built from the extracted
crates passes its existing surface and behavior tests and the old monolithic
implementation can be removed without changing the user-visible product
contract.

## Consequences

Positive:

- Other projects can pull only the Cortex capabilities they need.
- Cortex remains a first-class product instead of becoming a source donor that
  drifts away from reusable crates.
- Soma's existing architecture checker enforces the dependency direction.
- Small extraction lanes make behavior changes and API mistakes reviewable.

Tradeoffs:

- There is a temporary duplication window while donor modules and extracted
  crates coexist. The tracker must keep that window explicit and short.
- Untangling public Cortex models from database rows requires real interface
  work before the domain crate can be extracted safely.
- More crates increase manifest and release bookkeeping, which is preferable to
  hidden coupling but still needs maintenance.

## References

- [ADR 0002](./0002-extract-reusable-platform-and-product-packages.md)
- [ADR 0003](./0003-shared-platform-and-product-runtime-crates.md)
- [ADR 0004](./0004-product-runtime-builders.md)
- [ADR 0009](./0009-extraction-execution-lanes.md)
- [ADR 0010](./0010-extraction-verification-gates.md)
- [Cortex extraction index](../cortex-extraction/README.md)
- [Cortex extraction specification](../cortex-extraction/SPEC.md)
