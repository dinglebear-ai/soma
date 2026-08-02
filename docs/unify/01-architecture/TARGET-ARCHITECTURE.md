---
title: "Target Architecture"
created: 2026-07-24
updated: 2026-07-31
---

# Target Architecture

## Product family

```text
                           shared neutral engines
        gateway | knowledge | observations | operations | runtime
             |          |             |             |
             v          v             v             v
          Labby       Axon          Cortex        Synapse
             |          |             |             |
             +----------+-------------+-------------+
                                |
                                v
                              Soma
                     integrated superset product
```

Labby, Axon, Cortex, and Synapse are complete standalone products. Soma is not their runtime prerequisite. Soma composes the same engines in-process or through stable remote adapters.

## Integrated Soma layers

```text
External callers
  CLI | REST | MCP | Web
          |
          v
Soma surface adapters
          |
          v
Soma application use cases
  gateway | sources | observations | operations | context | graph | memory | jobs
          |
          +--------------------+--------------------+
          |                    |                    |
          v                    v                    v
Knowledge subsystem      Observation subsystem   Operations subsystem
          |                    |                    |
          +--------------------+--------------------+
                               |
                               v
                         Context plane
                SQL + FTS5 + Qdrant + graph + memory
                               |
                               v
                         Context broker
```

## Shared mechanisms versus product policy

### Shared crates own

- stable types and algorithms;
- gateway, source, observation, and operation protocols;
- adapters and parsers;
- storage traits and optional backend implementations;
- RAG, graph, memory, ingestion, fleet, and infrastructure engines;
- jobs runtime;
- bounded safety behavior;
- transport-neutral plans, progress, verification, and events.

### Product modules own

Each product owns its configuration defaults, authorization, storage layout, migrations, enabled adapters, surfaces, health, operations, and release behavior.

Soma additionally owns cross-domain query planning, integrated workflows, workspace policy, unified audit, semantic projection policy, graph vocabulary, Aurora UI, and selection between embedded and remote product adapters.

## Required dependency direction

```text
leaf primitives and protocol clients
    |
domain records and pure engines
    |
ports and protocols
    |
infrastructure adapters
    |
product domain and application crates
    |
product surface crates
    |
apps/<product> composition root
```

No lower layer may import a higher layer. In particular:

- `crates/shared/*` must not depend on `crates/soma/*`, `crates/labby/*`, `crates/axon/*`, `crates/cortex/*`, `crates/synapse/*`, or `apps/*`;
- Soma integrations consume neutral engines or stable remote clients, never another product's surface internals;
- product environment variables and defaults are translated at product boundaries, not read by shared crates.
