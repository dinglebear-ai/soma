---
title: "Operation Event Contract"
created: 2026-07-31
updated: 2026-07-31
---

# Operation Event Contract

Operation events connect Synapse and Soma execution to Cortex observations without coupling the operations engine to Cortex storage or graph internals.

## Event sequence

The canonical lifecycle is:

```text
OperationRequested
OperationPlanned
OperationAuthorized
OperationStarted
OperationProgressed*
OperationSucceeded | OperationFailed | OperationCancelled
OperationVerified
```

Events MAY be omitted only when the corresponding lifecycle stage did not occur. Terminal execution events are mandatory once `OperationStarted` has been emitted.

## Common envelope

Every event MUST include:

- `event_id`;
- `event_version`;
- `event_type`;
- `occurred_at`;
- `operation_id`;
- `operation_name`;
- `correlation_id`;
- optional `causation_id`;
- actor reference when known;
- resolved target reference when known;
- producer identity and version;
- trace context;
- payload;
- redaction metadata.

Event IDs MUST be stable for retried delivery. Consumers MUST be idempotent by `event_id`.

## Event semantics

### OperationRequested

Records intent before target resolution. It MUST NOT imply authorization or execution.

### OperationPlanned

Records the resolved plan, topology revision, risk, reversibility, expected effects, verification strategy, and plan fingerprint.

### OperationAuthorized

Records an opaque authorization reference, issuer, approved scope, and expiry. It MUST NOT include raw tokens, cookies, keys, or secrets.

### OperationStarted

Records the concrete target and the point after which execution may have affected external state.

### OperationProgressed

Records bounded monotonic progress where available. Progress events SHOULD include current, total, unit, phase, and a human-readable summary. Unknown totals are represented explicitly rather than fabricated.

### OperationSucceeded

Records successful execution. It does not imply verification.

### OperationFailed

Records structured diagnostics, retry classification, whether a mutation may have been sent, and any partial results.

### OperationCancelled

Records cancellation origin and whether external work may continue despite local cancellation.

### OperationVerified

Records `verified`, `failed`, `inconclusive`, `not_supported`, or `not_requested`, plus evidence references.

## Delivery

- Embedded mode MAY deliver events through an in-process sink.
- Remote mode MAY deliver through an API, message transport, or Cortex ingest protocol.
- Delivery failure MUST NOT silently change operation success into failure after an external mutation has completed.
- Failed delivery MUST be retried from a bounded durable outbox when durability is configured.
- Standalone Synapse MUST function when no observation sink is configured.

## Cortex projection

Cortex MAY project events into relationships such as:

```text
actor -> requested -> operation
operation -> targeted -> host
operation -> affected -> runtime resource
operation -> used -> approval
operation -> produced -> artifact
operation -> verified -> runtime state
incident -> resolved_by -> operation
```

Cortex MUST preserve the original event and provenance needed to explain each projection.

## Security

Events MUST exclude raw credentials, authorization tokens, private keys, environment secrets, and unrestricted command output. Sensitive outputs become protected artifacts with references and retention policy.
