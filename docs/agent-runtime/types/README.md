---
title: "Agent Runtime Type Blueprints"
created: 2026-08-05
updated: 2026-08-05
doc_type: "type-index"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Type Blueprints

These files provide proposed Rust-facing types aligned with Soma's current layering.

## Placement

Product invariants and transport-neutral value types begin in:

~~~text
crates/soma/domain/src/agent_runtime.rs
crates/soma/domain/src/agent_runtime/
  ids.rs
  capability.rs
  context.rs
  disclosure.rs
  run.rs
  snippet.rs
  stack.rs
  synthesis.rs
~~~

Application request/response DTOs, ports, orchestration records, and store traits begin in:

~~~text
crates/soma/application/src/agent_runtime.rs
crates/soma/application/src/agent_runtime/
  context.rs
  disclosure.rs
  package.rs
  run.rs
  runtime.rs
  snippet.rs
  stack.rs
  synthesis.rs
~~~

Concrete adapters and persistence remain in <code>crates/soma/integrations</code> and <code>crates/soma/runtime</code>. Surface-specific request types remain thin projections.

## Conventions

- New files follow the repository's sibling-file module rule; no <code>mod.rs</code>.
- Domain IDs are validated string newtypes. ID generation stays outside the domain crate so it does not require ULID or UUID dependencies.
- Transport timestamps are RFC 3339 strings initially, matching the proposed schemas. Internal stores may use integer epochs or typed timestamps.
- Enums serialize as lowercase kebab-case.
- Security-sensitive structs use <code>deny_unknown_fields</code>.
- Secret values are never represented, only <code>SecretRef</code>.
- Large evidence content uses canonical references and artifact handles.

## Files

- [ids-and-common.md](ids-and-common.md)
- [stack-and-capability-types.md](stack-and-capability-types.md)
- [context-and-disclosure-types.md](context-and-disclosure-types.md)
- [snippet-and-synthesis-types.md](snippet-and-synthesis-types.md)
- [run-and-lifecycle-types.md](run-and-lifecycle-types.md)
- [application-ports.md](application-ports.md)
