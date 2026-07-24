# Progress and Event Contract

## Purpose

Long-running refresh, crawl, indexing, observation maintenance, graph rebuild, and memory jobs emit one transport-neutral event stream.

## Event kinds

```text
started
progress
waiting
warning
completed
failed
cancelled
```

## Requirements

Every event has:

- stable event ID;
- operation/job ID;
- stage;
- occurrence time;
- optional bounded message;
- optional fraction;
- optional typed diagnostic;
- bounded numeric metrics.

## Ordering

Events from one operation have a monotonic sequence in durable storage. Delivery to clients may be at least once. Consumers deduplicate by event ID.

## Progress

Fractions are meaningful only within the named stage. Overall progress MAY be computed from a versioned stage-weight policy.

A job MUST NOT report 100 percent before durable completion.

## Surface behavior

The same event records feed:

- web progress views;
- CLI progress;
- API polling/streaming;
- MCP progress notifications;
- operator telemetry.

Surfaces may format differently but MUST NOT invent independent lifecycle state.

## Retention

Summary lifecycle events are durable with the job. High-volume debug progress MAY have shorter retention.
