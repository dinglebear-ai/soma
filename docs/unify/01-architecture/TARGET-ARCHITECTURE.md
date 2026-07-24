# Target Architecture

## Product layers

```text
External callers
  CLI | REST | MCP | Web
          |
          v
Existing Soma surface adapters
          |
          v
Soma application use cases
  sources | observations | context | graph | memory | jobs
          |
          +--------------------+
          |                    |
          v                    v
Knowledge subsystem      Observation subsystem
          |                    |
          +---------+----------+
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
- source and observation protocols;
- adapters and parsers;
- storage traits and optional backend implementations;
- RAG engines;
- graph kernel;
- memory lifecycle;
- jobs runtime;
- bounded safety behavior.

### Soma product modules own

- which adapters are enabled;
- configuration defaults;
- authorization;
- database paths and migration order;
- Qdrant collection naming;
- semantic projection policy;
- graph vocabulary;
- query planning policy;
- web/API/MCP/CLI use cases;
- health and operations.

## Required dependency direction

```text
leaf primitives
    |
domain records
    |
pure engines
    |
ports and protocols
    |
infrastructure adapters
    |
Soma product domains
    |
surface adapters
    |
apps/soma composition root
```

No lower layer may import a higher layer.
