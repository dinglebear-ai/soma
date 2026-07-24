# Changelog

## 0.1.2-proposed - 2026-07-22

- Added `START-HERE.md` as the implementation entry point.
- Locked the v1 boundary, separate Axon/Cortex ingestion semantics, storage authority, first vertical slice, implementation order, and five non-negotiable rules into one concise guide.
- Updated package navigation, manifest, validation metadata, checksums, and archive contents.

## 0.1.1-proposed - 2026-07-21

- Standardized all proposed shared packages on the `soma-<one-word>` convention.
- Renamed crate specifications, repository paths, dependency references, ledgers, and validation coverage.
- Replaced compound crate concepts with concise names such as `primitives`, `sources`, `observations`, `collectors`, and `graph`.
- Kept crates.io availability and collision fallbacks as the only remaining naming decision.

## 0.1.0-proposed - 2026-07-21

Initial comprehensive v1 documentation package.

- Fixed v1 scope and explicit exclusions.
- Defined target architecture, storage, GraphRAG, security, and context broker.
- Proposed 16 coarse shared crates.
- Added combined JSON Schema and semantic contracts.
- Mapped Axon's 23 crates and Cortex subsystems into Soma.
- Defined 14 vertical implementation slices.
- Added product surface, testing, operations, ADR, delivery, risk, and north-star plans.
