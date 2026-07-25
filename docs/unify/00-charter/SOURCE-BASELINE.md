# Source Baseline

This package was prepared against the following public `main` snapshots on 2026-07-21.

| Repository | Baseline commit | Role |
|---|---|---|
| Soma | `0418156` | Destination product, existing gateway/auth/provider/surfaces |
| Axon | `1ab47e4` | Knowledge, source pipeline, RAG, jobs, ledger, memory |
| Cortex | `9633fc3` | Observations, SQLite/FTS, telemetry, correlation, graph |

## Baseline policy

- Implementation work MUST pin full commit SHAs in a donor lock file.
- Each extraction PR MUST identify exact donor paths and commits.
- Donor code is a behavioral reference, not a Cargo dependency.
- New donor changes after the pinned commit require an explicit baseline update.
- Generated contracts MUST record input hashes.

## Audit note

This package is a source-level architecture and contract audit of the public repositories at the recorded short commits. The implementation program MUST resolve and pin full 40-character commit SHAs before extraction work begins.

A local clone/compile was not available while the package was generated, so Cargo feature unification, full dependency metadata, and test execution remain implementation-phase validation tasks rather than claims made by this package.
