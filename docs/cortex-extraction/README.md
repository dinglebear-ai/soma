---
title: "Cortex Shared-Crate Extraction"
created: 2026-08-17
updated: 2026-08-18
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-08-18"
---

# Cortex Shared-Crate Extraction

This directory is the working control plane for extracting Cortex into reusable
Soma shared crates while preserving Cortex as a complete product assembled from
the same libraries.

The extraction is intentionally contract-first. A folder move is not considered
an extraction unless the new crate has a narrow public API, independent consumer
proof, explicit dependency direction, donor-parity evidence, crate docs, and
workspace architecture verification.

## Donor baseline

The first reviewed donor snapshot is Cortex commit
`7edf23fadb94650c2d2a2f9c80111fb44319eea8` on
`codex/graph-projection-lifecycle`. At the start of this work that branch was
two commits ahead of Cortex `origin/main`, so the immutable commit is the
source reference for parity work.

## Documents

| Document | Purpose |
| --- | --- |
| [SPEC.md](SPEC.md) | Target crate architecture, dependency graph, runtime composition, and migration sequence. |
| [CONTRACTS.md](CONTRACTS.md) | Rules every extracted crate and adapter must satisfy. |
| [SOURCE-INVENTORY.md](SOURCE-INVENTORY.md) | Current Cortex modules, coupling hotspots, and planned destinations. |
| [MODEL-CLASSIFICATION.md](MODEL-CLASSIFICATION.md) | Complete ownership classification for all 255 public donor model declarations. |
| [PROGRESS.md](PROGRESS.md) | Dedicated lane-by-lane extraction tracker and definition of done. |
| [VERIFICATION.md](VERIFICATION.md) | Required build, test, docs, architecture, parity, and smoke gates. |
| [REVIEW.md](REVIEW.md) | Review passes, findings, resolutions, and final evidence for this extraction branch. |
| [ADR 0014](../adr/0014-extract-cortex-as-reusable-shared-crates.md) | Fleet-level architectural decision governing the extraction. |

## Current implementation

The shared family now has two implemented crates. `cortex-ingest-core` contains
message normalization/signature logic and bounded metadata redaction.
`cortex-domain` contains all 65 donor public model declarations classified as
semantic contracts, plus deterministic incident-finding rules and the narrow
domain error taxonomy. Neither crate depends on SQLite, Axum, RMCP, Labby auth,
process runtime, or deployment.

The domain lane also records ownership for all 255 public donor model
declarations. Transport DTOs, storage/query projections, runtime/collector
state, and every database-row conversion remain explicitly assigned to later
adapter crates instead of leaking into the reusable domain API. Both crates use
workspace package inheritance, `layer = "shared"` architecture metadata,
explicit features, `publish = false` during stabilization, README and crate-level
docs, donor parity tests, and independent public-consumer tests.

## Completion condition

The migration is complete only when the canonical Cortex binary is a thin
composition over extracted crates, all retained Cortex behavior is covered by
parity/surface tests, no shared crate depends upward into a product layer, and
the obsolete monolithic source can be deleted. Until then, this tracker should
make duplicated ownership and unfinished cutovers visible rather than hiding
them behind a partially moved module tree.
