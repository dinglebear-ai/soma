# Donor Parity Plan

## Purpose

Axon and Cortex will not be refactored to consume the new crates. Parity therefore uses pinned donor snapshots and shared fixtures.

## Fixture classes

### Axon

- source routing;
- source manifests/diffs;
- local/web/registry/session normalization;
- chunk boundaries and locators;
- vector payloads;
- hybrid retrieval;
- source publication and cleanup;
- jobs;
- graph candidates;
- memory lifecycle.

### Cortex

- syslog parsing;
- metadata redaction;
- file rotation/checkpoints;
- Docker normalization;
- OTLP conversion;
- heartbeat/inventory;
- shell/session ingestion;
- FTS queries;
- retention;
- graph evidence and explanations.

## Comparison types

- exact serialized equality;
- set equality;
- normalized semantic equality;
- state-transition trace equality;
- ranked-result quality threshold;
- behavioral invariant rather than byte equality.

Every non-exact comparison documents why.

## CI layout

```text
checkout soma
checkout pinned donors under target/donors/
run donor fixture exporters or use committed golden fixtures
run Soma implementations
compare
```

No Cargo dependency on donor repositories enters publishable crates.

## Intentional divergence

Every divergence needs:

- reason;
- old behavior;
- new behavior;
- compatibility impact;
- migration;
- test;
- ADR when architectural.
