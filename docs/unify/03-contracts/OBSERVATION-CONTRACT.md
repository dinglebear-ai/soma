# Observation Contract

## Purpose

This contract governs continuing, periodic, or append-oriented operational records such as logs, Docker events, OTLP logs, heartbeats, inventory snapshots, shell history, tool calls, and agent activity.

## Canonical flow

```text
wire record / collected snapshot
    ↓ adapter parse
ObservationRecord
    ↓ bounded queue
batch
    ↓ canonical SQLite transaction
observation rows + FTS rows + semantic outbox
    ↓ acknowledgement/checkpoint
    ↓ asynchronous graph and semantic projection
```

## Required fields

Every observation MUST have:

- stable `observationId`;
- `kind`;
- event time and receive time;
- normalized severity;
- explicit source adapter and instance;
- bounded message and attributes;
- retention class;
- sensitivity classification.

## Identity and deduplication

Adapters MUST define their identity policy.

Examples:

- syslog: receiver instance + peer + timestamp + message fingerprint + sequence discriminator;
- Docker event: daemon identity + event time + container ID + action + event ID;
- OTLP: resource/scope/trace/span/log identity where present, otherwise deterministic record fingerprint;
- file tail: source ID + file identity + byte range;
- heartbeat: collector instance + host identity + sample time;
- shell history: source file/database identity + stable source row or byte range.

At-least-once delivery is permitted. Stores MUST support idempotent insert or duplicate classification.

## Observed versus claimed identity

Transport-observed identity and payload-claimed identity MUST remain separate.

For syslog:

- the peer address is observed by the receiver;
- the message hostname is sender-claimed;
- vendor fields are untrusted content.

Entity resolution may connect them later with evidence. Parsing MUST NOT silently treat claimed identity as authoritative.

## Boundedness

Before persistence:

- messages and attributes are bounded;
- recursive metadata is depth and byte limited;
- secret-shaped fields are redacted;
- invalid UTF-8 has an explicit handling policy;
- control characters are normalized or rejected;
- parser diagnostics are bounded.

The original transport payload MAY be retained as a restricted artifact when policy permits, but it MUST NOT leak through ordinary query results.

## Acknowledgement

A source checkpoint advances only after canonical persistence succeeds.

- UDP cannot provide sender acknowledgement, but internal durability status is still tracked.
- TCP/file-tail/collector sources MUST not acknowledge or checkpoint records that failed canonical persistence.
- Semantic or graph projection failure MUST NOT block canonical acknowledgement.

## Semantic outbox

The canonical SQLite transaction MAY enqueue a `SemanticProjectionTask`. It MUST NOT synchronously require TEI or Qdrant availability.

The outbox task includes:

- canonical reference;
- projection kind/version;
- deterministic task ID;
- state/attempts;
- safe bounded failure diagnostics.

## Retention

Canonical retention is determined by observation kind, severity, importance, and promotion state.

- Routine high-volume observations MAY have short retention.
- Incident evidence MAY be pinned.
- A semantic point MUST be deleted when all canonical evidence expires unless the projection was promoted into a durable incident, memory, or document.
- Retention deletion MUST create derived-index cleanup work.

## Health

Every ingest adapter/runtime MUST expose:

- running/degraded/stopped state;
- accepted/dropped/retried counts;
- queue depth;
- oldest unpersisted age;
- last success/error;
- checkpoint lag when applicable;
- projection backlog;
- bounded safe diagnostics.

## Required fixtures

- malformed wire records;
- backpressure;
- storage outage;
- duplicate delivery;
- file rotation/truncation;
- out-of-order events;
- clock skew;
- projection outage;
- retention cleanup;
- secret-bearing metadata.
