# Product Outcome

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

The result is a cited diagnosis and actionable plan. V1 does not execute the fix autonomously.
