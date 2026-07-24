# Jobs and Progress Product Contract

## Durable operations

The following are durable jobs:

- source discovery/refresh;
- crawl;
- document preparation;
- embedding/indexing;
- generation publication/cleanup;
- semantic observation projection;
- retention cleanup;
- FTS/vector reindex;
- graph projection/rebuild;
- memory extraction/review where enabled;
- backup integrity checks.

## Job payloads

Product job payloads reference canonical IDs and versioned configuration. They do not embed unbounded documents, logs, or secrets.

## Runner separation

`soma-jobs` owns runtime mechanics. Soma product runners own domain behavior:

```text
SourceRefreshRunner
CrawlRunner
SemanticProjectionRunner
GraphRebuildRunner
RetentionRunner
ReindexRunner
```

## Progress

Each runner emits the shared `ProgressEvent` contract. The existing surfaces render the same durable stream.

## Cancellation

- cancellation is cooperative first;
- process/network adapters receive cancellation;
- canonical transactions complete or roll back;
- source publication never leaves a half-committed pointer;
- incomplete derived data becomes cleanup debt;
- the job reports what remains.

## Retry

Retries follow typed error classification. Permanent input/policy failures are not automatically retried. Backoff includes jitter and maximum attempts.

## Operator controls

- list/filter;
- inspect payload/result;
- cancel;
- retry from safe stage;
- view events;
- open linked source/observation/projection;
- view dead-letter diagnostics.
