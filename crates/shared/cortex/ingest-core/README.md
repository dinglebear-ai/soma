# cortex-ingest-core

Reusable hot-path ingest primitives extracted from Cortex for Soma-derived and
standalone Rust services. The crate has no dependency on Cortex storage,
runtime, transport, authentication, CLI, or deployment code.

## What it owns

- deterministic log-message normalization for error signature grouping;
- stable SHA-256 signature hashing;
- bounded JSON metadata encoding;
- recursive sensitive-key redaction and string/key/object limits;
- canonical ingest `SourceKind` wire vocabulary and the agent-Docker source marker.

It does not parse syslog or OTLP, open SQLite, start background tasks, or know
about Cortex product configuration. Those concerns belong in higher extraction
layers.

## Example

```rust
use cortex_ingest_core::{metadata, normalize};
use serde_json::json;

let template = normalize::normalize_template(
    "Failed password for alice from 10.0.0.1 port 2222 ssh2",
);
let signature = normalize::signature_hash(&template);
assert!(!signature.is_empty());

let metadata = metadata::bounded_metadata_json(json!({
    "source_type": "example",
    "authorization": "Bearer do-not-store",
}));
assert!(metadata.contains("[REDACTED]"));
```

## Compatibility contract

`normalize::NORMALIZER_VERSION` identifies the normalization output contract.
Any change that can alter normalized output for an existing input must bump the
version and include migration notes for persisted signatures. Metadata limits
and sensitive-key matching are likewise treated as behavior, not incidental
implementation details.

The initial implementation is behavior-preserving source extraction from Cortex
commit `7edf23fadb94650c2d2a2f9c80111fb44319eea8`. Donor tests were moved with
the implementation, and public-API tests exercise the crate from an external
consumer boundary.

The crate is intentionally `publish = false` during boundary stabilization,
matching Soma ADR 0002. Publishing can begin after the extraction tracker marks
its API, parity suite, and independent-consumer proof stable.
