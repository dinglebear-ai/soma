# Health and Recovery

## Component health

### SQLite

- open/migration status;
- integrity check status;
- WAL size/checkpoint age;
- writer queue;
- busy/retry rate;
- disk pressure.

### Qdrant

- reachability;
- collection existence/schema/dimensions;
- upsert/search errors;
- pending projection backlog;
- orphan/dead-evidence count.

### TEI

- model/dimensions;
- reachability;
- queue/latency;
- failure/cooldown;
- batch constraints.

### Chrome/crawler

- browser bootstrap;
- active sessions;
- crash/restart count;
- crawl queue.

### Sources

- last discovery/refresh;
- committed generation;
- lease/job state;
- last error;
- cleanup debt.

### Observations

- receiver/collector state;
- accepted/dropped;
- queue depth;
- checkpoint lag;
- last event;
- projection backlog.

### Graph/memory

- ready projection version;
- rebuild state;
- unresolved candidate/conflict counts;
- dead evidence;
- review backlog.

## Degraded operation

| Failure | Expected behavior |
|---|---|
| Qdrant unavailable | Canonical writes and FTS continue; semantic tasks queue |
| TEI unavailable | Canonical writes continue; embedding jobs retry |
| Chrome unavailable | Browser-required crawls fail/degrade; other sources continue |
| one receiver fails | Other receivers and public app continue |
| graph rebuild fails | Last ready projection remains queryable |
| synthesis LLM unavailable | Context bundles return without synthesized answer |
| disk pressure | Retention/alerts activate; policy governs admission |

## Recovery

Every worker uses idempotent state and durable leases/checkpoints. Restart recovery:

- expires stale claims;
- resumes queued/retry jobs;
- drains semantic outbox;
- verifies source generation ownership;
- restarts receivers from checkpoints;
- reports possible duplicate windows.

## Doctor

`context.doctor` offers fast and deep modes. Deep mode may perform test inserts/queries, citation hydration, and provider round trips without modifying user data.
