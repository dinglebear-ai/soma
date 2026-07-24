# Diagnostic and Error Contract

## Principle

Each crate owns a typed, `#[non_exhaustive]` error. Cross-domain and product surfaces project crate errors into `PipelineDiagnostic`.

A universal error enum MUST NOT become a dependency hub.

## Required classifications

```text
invalid_input
not_found
conflict
unauthorized
forbidden
rate_limited
unavailable
timeout
cancelled
storage
provider
internal
```

A diagnostic records:

- stable machine code;
- bounded user/operator-safe message;
- category;
- retryability;
- visibility;
- stage;
- bounded safe context.

## Rules

- Internal causes may be preserved in tracing, but public messages are safe.
- `Debug` MUST not leak secrets.
- Retryability is determined by the owning adapter/store, not string matching in callers.
- Cancellation is not logged as an internal failure.
- Partial-success operations return explicit result and diagnostics rather than hiding failures.
- A parser error MUST preserve the original record when policy permits fallback.
- Batch writers identify permanent poison records without retrying the whole batch forever.

## Surface projection

Soma's existing API/MCP/CLI/web layers translate diagnostics consistently. Shared crates MUST NOT depend on HTTP status codes or MCP error envelopes.

## Stability

Machine error codes are part of the public compatibility contract. Human messages may improve without a breaking release.
