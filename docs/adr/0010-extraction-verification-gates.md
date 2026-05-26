---
title: "ADR 0010: Require boundary and generated-client verification"
doc_type: "adr"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-05-26"
---

# ADR 0010: Require boundary and generated-client verification

## Status

Accepted, 2026-05-26.

## Context

Extraction changes dependency direction, public APIs, generated clients, and
runtime composition. A successful narrow build can miss all-features coupling,
frontend package drift, or generated-client breakage.

## Decision

Require verification gates for backend composition, product boundaries,
REST/OpenAPI, generated clients, frontend packages, standalone binaries, and
boundary rules.

Minimum backend verification:

```bash
cargo check --workspace --all-features
cargo nextest run --workspace --all-features
```

Generated-client verification must export product OpenAPI, regenerate the
client, typecheck the client, and typecheck at least one consumer fixture before
the client is considered reusable.

Boundary checks should be added as soon as practical:

- shared crates do not import product crates;
- product crates do not import sibling product crates;
- frontend packages respect allowed dependency direction;
- design-system packages do not import product web code;
- API client packages do not import React or reusable web-shell packages.

Standalone binaries must build and expose `--help`; product smoke tests are
added as their runtime builders mature.

## Consequences

- All-features workspace verification remains the backend truth.
- Generated files cannot silently drift in CI.
- Boundary enforcement can start with simple scripts and become stricter over
  time.
- Live homelab services are not required for extraction unit tests.

## References

- Source decision ported from Lab ADR
  `docs/adr/0009-extraction-verification-gates.md`.
- [`docs/adr/0002-extract-reusable-platform-and-product-packages.md`](./0002-extract-reusable-platform-and-product-packages.md)
- [`docs/adr/0009-extraction-execution-lanes.md`](./0009-extraction-execution-lanes.md)
