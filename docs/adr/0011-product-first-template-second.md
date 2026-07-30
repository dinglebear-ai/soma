---
title: "ADR 0011: Keep Soma product-first and template-second"
created: 2026-07-13
updated: 2026-07-30
doc_type: "adr"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: true
last_reviewed: "2026-07-13"
---

# ADR 0011: Keep Soma product-first and template-second

## Status

Accepted, 2026-07-13.

## Context

Soma is now a shipped runtime product with its own binary, provider registry,
HTTP MCP transport, REST API, auth, web surface, health checks, plugins, and
release metadata. At the same time, the repository still contains scaffold and
generation machinery for creating Soma-shaped projects.

Those two roles are compatible, but only if their priority is clear. Runtime
work should not be blocked by template-only checks unless the change directly
alters template output. Conversely, template output should not preserve legacy
runtime assumptions that the product has already retired.

The single-binary runtime decision sharpened this boundary. `soma` is the
canonical product binary, and explicit subcommands select runtime mode:

| Command | Role |
|---|---|
| `soma serve` | Owns the HTTP runtime, provider registry, REST API, HTTP MCP, auth, web, and health |
| `soma mcp` | Starts the stdio MCP adapter |
| `soma <command>` | Runs CLI actions, using local or remote adapter mode |

## Decision

Soma is a real product first and a template source second.

- Product behavior, product docs, product metadata, product commands, and
  product release surfaces are the source of truth.
- The canonical binary is `soma`; the product must not carry a separate
  `soma-server` identity.
- Scaffold or template output is a derivative export of the product shape, not
  a peer source of truth.
- Generated projects should inherit Soma's current runtime shape by default,
  including the single explicit-mode binary, unless an ADR records a deliberate
  divergence.
- Template verification remains valuable, but it belongs in a separate
  scaffold/generation lane. It is not part of product runtime acceptance unless
  the change intentionally modifies scaffold behavior.
- Product runtime acceptance should focus on the shipped surfaces: Cargo binary
  targets, `soma serve`, `soma mcp`, CLI remote/local behavior, REST/MCP
  forwarding, Docker, installers, CI/release metadata, docs, plugins, and
  generated product metadata.

## Consequences

Positive:

- Soma can remain a useful seed for new Soma-shaped projects without making the
  product read like a generic placeholder template.
- Runtime work has a crisp acceptance target: the shipped product behavior.
- Template breakage cannot quietly reintroduce retired product concepts such as
  split `soma` and `soma-server` binaries.
- Scaffold verification can be slower and broader without making every product
  change wait for generated-project builds.

Tradeoffs:

- The scaffold lane needs explicit ownership. If no one maintains it, it should
  be deprecated or removed rather than allowed to drift.
- Changes that touch shared product and scaffold rewrite code need two stated
  verification scopes: product acceptance and scaffold acceptance.
- String-rewrite based generation remains fragile. If the template role stays
  important, it should move toward a more declarative export model.

## Verification Policy

For product runtime changes, require product-focused checks such as:

```bash
cargo fmt --all --check
cargo test -p soma --bin soma
cargo build -p soma --bin soma --no-default-features --features server
cargo build -p soma --bin soma --features full
cargo xtask generate-provider-surfaces --check
cargo xtask check-docs
cargo xtask check-version-sync
```

For scaffold/template changes, use a separate lane such as:

```bash
cargo xtask check-cargo-generate
```

That lane proves generated projects still build, but it should be invoked
because scaffold behavior is in scope, not because Soma product runtime changes
need template proof by default.

## Alternatives Considered

### Product-only, remove all scaffold machinery

Viable if the scaffold lane has no owner. Rejected for now because a derivative
template remains useful for creating new Soma-shaped projects, and keeping it
does not conflict with product-first semantics when the priority is explicit.

### Template-first, keep Soma generic

Rejected. Generic template language and split placeholder identities make the
shipped product harder to reason about and invite drift in docs, releases, and
operator commands.

### Product and template as equal sources of truth

Rejected. Equal authority makes acceptance ambiguous. When product behavior and
template output disagree, the product should win and the template should be
updated, deprecated, or explicitly forked.

## References

- [`README.md`](../../README.md)
- [`docs/DEPLOYMENT.md`](../DEPLOYMENT.md)
- [`docs/adr/0001-stdio-first-plugin-adapter.md`](./0001-stdio-first-plugin-adapter.md)
- [`docs/adr/0010-extraction-verification-gates.md`](./0010-extraction-verification-gates.md)
