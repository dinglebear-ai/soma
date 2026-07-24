# State Machine Contracts

## Source generation

```text
queued
  → discovering
  → acquiring
  → preparing
  → staging
  → complete
  → published

Any pre-published active state:
  → failed
  → cancelled

published:
  terminal, followed by independent cleanup jobs
```

## Semantic projection task

```text
pending → claimed → complete
              ↘ retry → claimed
              ↘ dead_letter
pending/claimed/retry → cancelled
```

Claims expire and are recoverable. Completion is idempotent.

## Job

```text
queued → claimed → running → succeeded
                    ├→ waiting → running
                    ├→ failed
                    ├→ cancelled
                    └→ dead_letter after policy exhaustion
```

## Memory

```text
candidate → active → needs_review
              ├→ superseded
              ├→ contradicted
              └→ archived

needs_review → active/superseded/contradicted/archived
```

## Graph projection

```text
absent → building → ready
             ├→ failed
ready → rebuilding → ready
                ├→ failed (old ready projection remains queryable)
ready → stale
```

## Adapter/receiver health

```text
starting → healthy ↔ degraded
    ├→ failed
healthy/degraded → stopping → stopped
failed → starting when restart policy permits
```

## Invalid transitions

Invalid transitions return typed conflicts and never silently coerce state. Every durable transition records actor, time, prior state, next state, and reason.
