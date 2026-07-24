# North-Star Scenario: Labby OAuth Connector Failure

## Purpose

This scenario is the v1 architectural north star.

It proves that Soma can combine:

- Labby source code and deployed revision;
- Labby docs, issues, PRs, releases, reports, plans, and agent sessions;
- OpenAI/ChatGPT, Claude, MCP, RMCP, OAuth, Google, SWAG, Authelia and related official documentation;
- Docker Compose and reverse-proxy configuration;
- Labby, SWAG, Authelia, Docker, syslog, journald, kernel, network and identity logs;
- shell/agent command history;
- heartbeat and inventory snapshots;
- graph topology and temporal relationships;
- prior verified memories.

## User question

```text
Labby works with Google OAuth when connected to Claude.ai.
ChatGPT web fails while adding the connector:

Dynamic client registration failed:
registration endpoint returned 403

Help me identify the exact cause and explain the verified fix.
```

## V1 expected behavior

V1 is diagnostic and advisory. It does not create `apm.yaml`, spawn an Incus worker, implement a patch, create a PR, merge, or deploy.

Soma MUST produce an `InvestigationBundle` containing:

1. resolved Labby project/repository/service/domain/host/proxy/auth entities;
2. exact deployed commit/artifact and active configuration when available;
3. correlated failed request and surrounding incident timeline;
4. attribution of the 403 to a specific layer when evidence permits;
5. version-matched relevant implementation excerpts;
6. relevant official documentation/specification excerpts;
7. prior session/history evidence;
8. ranked hypotheses with supporting and contradicting evidence;
9. a root-cause conclusion when established;
10. explicit unknowns/evidence gaps;
11. an actionable, bounded remediation and verification plan;
12. citations for every material claim.

## Required evidence classes

```text
[ ] runtime observation
[ ] active/deployed configuration
[ ] deployed source or artifact identity
[ ] official normative documentation
[ ] relevant project implementation
[ ] topology/graph path
[ ] prior history when relevant
```

## Required query behavior

- exact FTS finds the error/request/log records;
- structured SQL applies service/time/status filters;
- vector retrieval finds semantically relevant docs and prior sessions;
- graph traversal joins Labby to host, domain, SWAG, Authelia, OAuth, RMCP and documentation;
- temporal investigation selects state active during the incident;
- reranking prioritizes evidence relevant to the actual failing layer;
- hydration returns canonical citations.

## Claim discipline

The answer distinguishes:

```text
OBSERVED
DOCUMENTED
IMPLEMENTED
HISTORICAL
INFERRED
UNKNOWN
```

It MUST NOT claim the root cause when evidence only supports possibilities.

## Evaluation

The fixture specifies:

- required entities;
- required evidence IDs;
- required graph paths;
- forbidden unrelated evidence;
- known failing layer and causal chain;
- accepted remediation elements;
- unsupported claims that cause failure.

## Future extension

A later Orchestrator/APM program may consume this bundle to implement and verify the fix through a real ChatGPT browser flow. That work is explicitly outside context-layer v1.
