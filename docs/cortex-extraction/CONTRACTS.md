---
title: "Cortex Extraction Contracts"
created: 2026-08-17
updated: 2026-08-17
doc_type: "contract"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-08-17"
---

# Cortex Extraction Contracts

These rules are normative for every Cortex crate extracted into Soma. A lane is
not complete merely because Cargo can compile it.

## C1: Shared dependency direction

A package under `crates/shared/cortex/**` may depend on external crates and
other packages under `crates/shared/**`. It must not depend on
`crates/soma/**`, `apps/**`, or another product runtime/surface.

`cargo xtask check-architecture` is the executable enforcement for this rule.
Do not add a Cortex-specific exception simply to make a migration compile.

## C2: Product preservation

Every extraction wave keeps Cortex buildable or records a narrowly scoped
integration lane that restores the product before the wave can merge. The final
Cortex binary is composed from the reusable crates. No crate extraction is
allowed to turn Cortex into a dead source snapshot.

## C3: Narrow public APIs

Public APIs expose semantic contracts, not implementation storage. In
particular, public domain/application types must not contain raw SQLite row
types, Axum extractors/responses, RMCP protocol request types, or process-global
handles unless the crate itself is the adapter that owns that concern.

Broad glob re-exports are discouraged. Prefer module-scoped APIs whose owner is
obvious to downstream consumers.

## C4: Conversion ownership

When a storage adapter has a private persistence type and the domain has a
public semantic type, the conversion is implemented in a layer that may legally
see both. Do not make the domain crate depend on the storage crate to host an
`From<DbRow>` implementation.

## C5: Thin surfaces

REST, MCP, CLI, and executable layers may parse, authenticate/authorize for the
transport, dispatch to the application facade, and format output. Business
validation, filtering, correlation, limits, enrichment, and stateful use-case
policy belong below the surface.

## C6: Explicit runtime composition

The product runtime exposes a library-level builder/equivalent composition API.
Configuration and replaceable dependencies enter explicitly. Process-global
singletons may survive only as temporary compatibility shims with a removal
entry in the progress tracker.

## C7: Authentication convergence

New Cortex shared crates do not add direct dependencies on the Labby repository
or `lab-auth`. Runtime/auth extraction migrates to `soma-auth` or a narrowly
defined shared adapter over it. OAuth/resource/scope behavior must remain
covered by the Cortex auth parity tests.

## C8: Feature discipline

Every extracted crate declares explicit default features. Heavy optional stacks
such as Axum, RMCP transports, Docker, OTLP, SSH, or service-specific collectors
are feature-gated when a meaningful smaller consumer profile exists. A feature
may add capability; it must not silently change the semantics of an unrelated
core API.

## C9: Documentation and metadata

Each crate has:

- package description, license, repository/homepage inheritance, categories and
  useful keywords where applicable;
- `[package.metadata.soma-architecture] layer = "shared"`;
- `[lints] workspace = true`;
- a README with scope, non-goals, example, and compatibility notes;
- crate/module/item rustdoc sufficient for warning-free public docs;
- docs.rs metadata when the crate has public API intended for eventual reuse.

## C10: Donor provenance and behavior parity

Every lane records the Cortex donor commit and source paths it claims. Existing
unit tests move with the implementation or are replaced by stronger equivalent
tests. Intentional behavior changes must be isolated and called out in the PR;
source movement is not a license to quietly change semantics.

Persisted or externally observed contracts receive exact parity checks where
practical. Examples include normalization output/version, migrations, JSON
shapes, route names, action schemas, auth scopes, config keys, and CLI exit
behavior.

## C11: Independent consumer proof

A reusable crate has at least one integration test or fixture compiled from the
external-crate perspective. The test imports only public API. It must not reach
through `pub(crate)`, test-only product state, or workspace-relative source
paths.

## C12: Error contracts

Reusable crates expose typed errors when downstream callers need to react to a
condition. `anyhow` is acceptable at final composition/CLI boundaries, but it
should not erase a stable capability error taxonomy solely for extraction
convenience. Errors must not leak secrets or unbounded untrusted payloads.

## C13: Concurrency and lifecycle ownership

A library that starts tasks, listeners, watchers, or worker loops must expose
shutdown/lifecycle ownership explicitly. Dropping a handle should have defined
behavior. Reusable libraries must not assume they own process signals or
`std::process::exit`.

## C14: Configuration ownership

Crate-local configuration structs describe capability settings. Environment
variable loading and Cortex-specific precedence stay at product/runtime
composition unless another consumer genuinely needs that loader. Shared crates
should be constructible from typed configuration without mutating global env.

## C15: Security and data safety

Redaction, metadata bounds, request/body limits, path validation, auth scope
checks, and other existing safety controls are behavior that must survive the
move. Extracting a lower-level crate must not create an unbounded bypass that
the monolithic call path previously prevented.

## C16: Publication gate

Extracted packages remain `publish = false` until all of the following are
true:

1. public API and feature set have an explicit stability review;
2. independent consumer tests exist;
3. package README/rustdoc are complete;
4. architecture and all-features tests pass;
5. versioning/release ownership is added to Soma's release machinery;
6. at least one real consumer or maintained fixture validates the package shape.

Opening publication is a deliberate later change, not a side effect of moving
source.

## C17: Review gate

Before a lane is marked complete, perform separate architecture/API and
behavior/safety review passes. Findings and their resolutions are recorded in
`REVIEW.md` or the lane PR, and there must be no known unresolved P0/P1
extraction defect at merge.
