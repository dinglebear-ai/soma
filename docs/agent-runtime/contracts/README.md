---
title: "Agent Runtime Contracts"
created: 2026-08-05
updated: 2026-08-05
doc_type: "contract-index"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Contracts

Contracts define transport-neutral boundaries that application use cases, stores, adapters, CLI, API, MCP, web, and Code Mode must share.

| Contract | Purpose |
|---|---|
| [identity-and-uri-contract.md](identity-and-uri-contract.md) | Stable IDs and canonical references |
| [context-compile-contract.md](context-compile-contract.md) | Context compilation operations and invariants |
| [snippet-contract.md](snippet-contract.md) | Snippet discovery, resolution, execution, and outputs |
| [disclosure-contract.md](disclosure-contract.md) | Disclosure requests, decisions, and receipts |
| [gateway-loadout-contract.md](gateway-loadout-contract.md) | LABBY loadout resolution and effective capabilities |
| [agent-run-contract.md](agent-run-contract.md) | Agent stack resolution and run state machine |
| [lifecycle-event-contract.md](lifecycle-event-contract.md) | Canonical event envelope and event families |
| [synthesis-contract.md](synthesis-contract.md) | Structured claims, evidence, research, and results |
| [security-contract.md](security-contract.md) | Authorization, redaction, secrets, mounts, and approvals |

## Contract rules

- All public surfaces call the same application use case.
- IDs and serialized enums use lowercase kebab-case unless an existing contract requires otherwise.
- Timestamps use RFC 3339 UTC strings at transport boundaries.
- Durations use integer milliseconds in serialized runtime contracts.
- Secrets are references, never returned values.
- Unknown fields are rejected for security-sensitive manifests unless a schema explicitly permits extensions.
- Errors use Soma's existing structured application error contract and stable codes.
- Every result reports truncation and warnings explicitly.
