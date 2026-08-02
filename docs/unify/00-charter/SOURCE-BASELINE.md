---
title: "Source Baseline"
created: 2026-07-24
updated: 2026-07-31
---

# Source Baseline

The original context package was prepared against the Soma, Axon, and Cortex public `main` snapshots observed on 2026-07-21. The operations-plane extension adds the Synapse public `main` snapshot observed on 2026-07-31.

| Repository | Baseline commit | Role |
|---|---|---|
| Soma | `0418156` | Destination product, existing gateway/auth/provider/surfaces |
| Axon | `1ab47e4` | Knowledge, source pipeline, RAG, jobs, ledger, memory |
| Cortex | `9633fc3` | Observations, SQLite/FTS, telemetry, correlation, graph |
| Synapse | `b92552900c1458aa03b370c80edc812884c77f31` | Docker, Compose, SSH, host, file, log, process, ZFS, and infrastructure-operation behavior |

## Baseline policy

- Implementation work MUST pin full commit SHAs in a donor lock file.
- Each extraction PR MUST identify exact donor paths and commits.
- Donor code is a behavioral reference, not a Cargo or runtime dependency.
- New donor changes after a pinned commit require an explicit baseline update.
- Generated contracts MUST record input hashes.
- A product donor remains authoritative until its standalone distribution is released from the monorepo with parity and rollback evidence.

## Audit note

The 2026-07-21 context package was initially a source-level architecture audit. Implementation work now validates donor behavior from local clones and isolated worktrees. No extraction PR may rely only on the short historical commits above: its donor lock and fixtures must record the full source commit used for that slice.
