---
title: "Cortex Shared-Crate Extraction Specification"
created: 2026-08-17
updated: 2026-08-18
doc_type: "spec"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-08-18"
---

# Cortex Shared-Crate Extraction Specification

## Goal

Make Cortex capabilities independently reusable from Soma's `crates/shared`
workspace without sacrificing Cortex as a buildable, testable, first-class
product. A consumer should be able to select one capability without inheriting
the complete Cortex daemon, SQLite schema, transport stack, CLI, or deployment
logic unless that capability genuinely requires them.

## Non-goals

This extraction does not redesign Cortex behavior for novelty, publish unstable
crates prematurely, or force every module into a separate package. The work is
about durable dependency boundaries and composition, not maximizing crate count.

## Target topology

```text
apps/cortex
  |
  +--> cortex-runtime ------> soma-auth / soma-observability / shared HTTP runtime
  |       |
  |       +--> cortex-application
  |       +--> cortex-ingest
  |       +--> cortex-agent
  |
  +--> cortex-api ----------> cortex-application + soma-http-*
  +--> cortex-mcp ----------> cortex-application + soma-mcp-*
  +--> cortex-ops ----------> soma-cli-core + soma-self-update

          cortex-application
             |      |      |
             |      |      +--> cortex-observatory
             |      +---------> cortex-inventory
             +----------------> cortex-storage-sqlite
                                  |
                                  +--> cortex-domain
                                  +--> cortex-ingest-core
                                  +--> cortex-inventory (pure snapshot contract)

          cortex-ingest --------> cortex-domain + cortex-ingest-core + ports
          cortex-agent ---------> cortex-domain + cortex-ingest-core + ports
          cortex-inventory -----> standalone snapshot contract in Wave 2; collector behavior remains Wave 4
          cortex-observatory ---> cortex-domain
          cortex-domain --------> serde + serde_json + thiserror + chrono (pure time policy)
          cortex-ingest-core ---> serde + serde_json + sha2
```

The exact dependency graph may become even narrower as ports are introduced. It
must never become broader by adding upward product dependencies to make a move
compile.

## Crate responsibilities

### cortex-ingest-core

Own deterministic pure transformations used by ingest paths. Initial contents:
normalization, signature hashes, bounded/redacted metadata. It must remain
usable in a process that has no async runtime, database, network stack, or Cortex
configuration.

### cortex-domain

Own stable semantic types and invariants shared across Cortex capabilities. It
may define traits/ports needed to invert dependencies, but it does not know
SQLite row structs, Axum extractors, RMCP protocol DTOs, filesystem layout, or
process globals. Wave 1 classifies all 255 donor public model declarations and
extracts all 65 semantic contracts. Database-row mappings remain owned by the
SQLite adapter, and transport/runtime projections remain outside the domain API.

### cortex-storage-sqlite

Own the existing SQLite pool, migrations, persistence models, FTS/query
implementation, retention/storage-budget persistence, and projection storage. It
implements domain/application ports and owns conversions from database rows to
domain contracts.

### cortex-ingest

Own receiver/parsing/enrichment pipelines and ingest source adapters. Syslog,
OTLP, Docker, files, shell history, and transcript watchers are features or
submodules of this capability. The core pipeline writes through an explicit sink
contract rather than requiring direct knowledge of the product runtime.

### cortex-inventory

Own normalized inventory types and reusable collectors for SSH, Docker, Unraid,
UniFi, Tailscale, AdGuard, media services, process/config discovery, and cache
refresh. External integrations should be feature-gated where practical.

### cortex-observatory

Own agent/tool/skill observation identity, attribution, classification, source
normalization, lifecycle rules, and projection inputs. Persistence is accessed
through an explicit port so the reasoning engine can be tested independently.

### cortex-agent

Own host-local forwarding behavior, heartbeat collection, Docker streaming, and
agent deployment/runtime pieces that need to run outside the central Cortex
server. Keep the agent embeddable and avoid requiring the central SQLite store.

### cortex-application

Own `CortexService`-equivalent use cases, validation, caps, correlation policy,
assessment policy, maintenance operations, and orchestration over lower-level
ports. This becomes the single business-logic facade for REST, MCP, CLI, and
internal jobs.

### cortex-api and cortex-mcp

Own Cortex-specific wire schemas and transport adaptation. They parse requests,
construct application calls, enforce transport-specific auth/scope mechanics,
and serialize results. They do not contain business calculations or direct SQL.

### cortex-runtime

Own explicit process composition: configuration, auth-policy construction,
runtime state, maintenance task spawning, listener lifecycle, and router/surface
assembly. It exposes a builder rather than making the binary the only valid
composition root.

A target shape is:

```rust,ignore
pub struct CortexRuntimeBuilder {
    // explicit configuration and replaceable dependencies
}

pub struct CortexRuntime {
    // application handle and surface/runtime fragments
}

impl CortexRuntimeBuilder {
    pub async fn build(self) -> anyhow::Result<CortexRuntime>;
}
```

### cortex-ops

Own local setup/repair, compose ownership, doctor, deploy, update, and related
operator mechanics when those mechanics are reusable independently. CLI parsing
and formatting should reuse `soma-cli-core` where it reduces duplicate fleet
behavior without distorting Cortex semantics.

### apps/cortex

Own the canonical executable identity and command-mode dispatch. It should be
small enough that product behavior can be tested through the library crates
without spawning the binary except for CLI/smoke coverage.

## Dependency inversion rules

The current Cortex code often allows an ingest component or response model to
reach directly into `db::*`. The extraction replaces those edges with explicit
contracts where they cross a reusable boundary. Good examples include:

- `IngestSink` for writing normalized batches;
- repository/query traits for application use cases;
- clock/process/filesystem ports only when deterministic tests benefit;
- mapping functions or implementations in the adapter crate that owns the raw
  database type.

Do not create a giant `cortex-common` crate to break cycles. A cycle signals a
boundary error that should be resolved by ownership or an intentionally narrow
port.

## Cargo and repository conventions

Each new shared Cortex crate must:

- live below `crates/shared/cortex/`;
- inherit workspace edition, rust-version, authors, homepage, and repository;
- declare `[package.metadata.soma-architecture] layer = "shared"`;
- inherit workspace lints;
- define an explicit `[features] default = [...]` entry, including an empty
  default when no default features are needed;
- carry crate-level documentation and a crate README;
- start at an extraction-local semantic version and remain `publish = false`
  until the publication gate is deliberately opened;
- use workspace dependency aliases for internal shared crates once consumed;
- avoid path references outside the Soma repository.

## Migration sequence

1. Foundation and proof: decision/spec/contracts/inventory/tracker plus
   `cortex-ingest-core`.
2. Domain seam: remove storage/runtime types from public semantic contracts.
3. SQLite adapter: extract persistence and migrate callers behind explicit APIs.
4. Ingest engines: split pure pipeline from source adapters and storage sink.
5. Inventory, observatory, and agent capabilities.
6. Application facade over the extracted ports.
7. REST/MCP adapters and transport parity.
8. Runtime/ops composition and `apps/cortex`.
9. Cutover: run Cortex parity suites against the composed application and delete
   obsolete donor copies.
10. Publication review for crates with stable external APIs.

## Compatibility policy

Mechanical extraction should not change behavior. Public behavior that already
has tests moves with those tests. Persisted contracts such as normalizer
versions, SQLite migrations, metadata shapes, action names, REST/MCP schemas,
and configuration semantics receive explicit parity checks before ownership
changes.

Temporary compatibility shims are allowed when they keep the donor product
buildable during a lane, but the progress tracker must name their owner and
removal wave.

## Success criteria

A successful final extraction has all of these properties:

- another Rust project can depend on a selected Cortex crate without the Cortex
  binary or unrelated capability crates;
- Soma's architecture checker reports no upward dependency edges;
- the Cortex product builds from the extracted crates and exposes its expected
  CLI, REST, MCP, ingest, and agent behaviors;
- existing behavioral tests either moved with their owner or exercise the new
  public boundary;
- documentation names one owner for every retained capability;
- the monolithic donor implementation no longer contains duplicate business
  logic.
