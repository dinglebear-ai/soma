# Product Cutover Plan

## Entry criteria

- all required v1 capabilities complete;
- donor parity accepted;
- migration dry run complete;
- backup/restore tested;
- context north-star passes;
- performance and storage budgets pass;
- operator docs complete.

## Stages

### 1. Shadow

Soma indexes representative sources and ingests copied or mirrored observations. User-facing Axon/Cortex remain authoritative.

### 2. Read comparison

Run saved queries against donor products and Soma. Record coverage, relevance, citations, latency, and discrepancies.

### 3. Source-by-source activation

Move refreshable sources to Soma. Avoid duplicate crawls/indexing.

### 4. Observation cutover

Pause donor writers, import final delta, move receivers/collectors, verify no time gaps.

### 5. Surface activation

Expose context pages and commands as primary. Keep donor systems read-only during acceptance window.

### 6. Retirement

Archive donor snapshots and repositories as behavioral/historical references. Stop feature development there.

## Rollback

Rollback restores donor services and their last consistent snapshots. New Soma-only records are exported before rollback to avoid losing observations.

## Acceptance window

Monitor:

- source refresh success;
- receiver drops;
- indexing lag;
- dead citations;
- Qdrant/FTS consistency;
- query relevance;
- database growth;
- CPU/memory;
- backup success.
