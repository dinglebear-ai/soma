---
title: "ADR 0007: Package reusable admin UI as a web boundary"
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

# ADR 0007: Package reusable admin UI as a web boundary

## Status

Accepted, 2026-05-26.

## Context

Admin apps across the server family share shell, auth bootstrap, protected
route, API provider, loading, error, toast, and navigation behavior. Future
products should not copy this code or import product-specific app routes.

The design system and frontend registry stay separate from backend
authorization. Backend authorization remains a Rust/server concern.

## Decision

Create reusable TypeScript/React package boundaries for admin-product UI.

Expected exports for a reusable admin UI package are:

```text
<package>
<package>/auth
<package>/shell
<package>/next
```

The web boundary owns frontend auth UX, protected route wrappers, session hooks,
admin shell primitives, common loading/error/toast primitives, and API provider
wiring.

It must not own product pages, backend authorization, or a full Next.js app
scaffold. Starters/templates own full app files and consume the reusable web
package.

Design-system packages remain frontend package/registry boundaries, not Rust
crates. If Rust binaries need compiled web assets, use a separate web-assets
helper boundary that contains built assets, not React source.

## Consequences

- Frontend auth improves UX but is never the authorization source of truth.
- Reusable web packages may depend on design-system packages and generated API
  clients.
- Design-system packages must not depend on product web packages.
- API client packages must remain UI-framework-free.
- Shared async UI utilities need abort/race cleanup tests when they cross await
  boundaries.

## References

- Source decision ported from Lab ADR
  `docs/adr/0006-lab-web-frontend-package-boundary.md`.
- [`docs/adr/0006-typescript-client-generation-from-openapi.md`](./0006-typescript-client-generation-from-openapi.md)
- [`docs/adr/0010-extraction-verification-gates.md`](./0010-extraction-verification-gates.md)

