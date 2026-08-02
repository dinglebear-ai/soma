---
title: "Product Outcome"
created: 2026-07-24
updated: 2026-07-31
---

# Product Outcome

## Product family

The Soma repository produces five complete distributions from shared neutral engines:

| Distribution | Standalone responsibility |
|---|---|
| Labby | Full MCP gateway, provider catalog, Code Mode, routing, OAuth, and gateway administration. |
| Axon | Full source acquisition, research, RAG, retrieval, reranking, citations, synthesis, and research jobs. |
| Cortex | Full observability ingestion, canonical observation store, evidence graph, timelines, and investigation workflows. |
| Synapse | Full Docker, Compose, Incus, host, SSH, file, log, process, and ZFS operations. |
| Soma | Integrated superset product that composes gateway, knowledge, observations, operations, policy, workflows, audit, and Aurora UI. |

Each focused product remains installable, operable, testable, and releasable without Soma. Soma may embed the same neutral engines or connect to separately deployed products through stable remote contracts.

## Integrated Soma outcome

After v1, a single Soma deployment provides:

```text
MCP Gateway
Knowledge Base
Operational Log Store
Evidence Graph
Memory
Hybrid Search
GraphRAG
CLI
REST API
MCP
Aurora Web Application
OAuth
```

A query such as:

> Why does Labby work from Claude but fail when ChatGPT performs dynamic client registration?

can use:

- the exact deployed Labby commit;
- Labby's source, issues, PRs, reports, docs, and historical AI sessions;
- active Compose, SWAG, Authelia, and service configuration;
- correlated application, proxy, authentication, system, Docker, and network logs;
- official OpenAI, ChatGPT, MCP, OAuth, Google, RMCP, SWAG, Authelia, Docker, and Cloudflare documentation;
- graph paths connecting the project, service, host, domain, proxy, identity provider, repository, dependencies, sessions, and incident;
- prior verified memories.

The result is a cited diagnosis and actionable plan. V1 does not execute the fix autonomously. Later operations phases use Synapse-derived neutral engines for explicitly authorized execution and runtime verification.
