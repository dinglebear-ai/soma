# GraphRAG Architecture

## V1 objective

Use deterministic topology and operational evidence to augment hybrid RAG. The graph connects what the environment is, what it contains, what it runs, what happened, and which documentation describes it.

## Graph inputs

### Axon-derived

- repositories, files, symbols, packages, issues, PRs, releases;
- documents and docsets;
- AI sessions, tools, skills, prompts;
- parser and adapter graph candidates.

### Cortex-derived

- hosts, service instances, containers, domains, networks, storage;
- logs, error signatures, incidents, deployments;
- sessions, tool calls, shell commands;
- inventory and heartbeat state;
- source and claimed identities.

## Authority

| Producer | Default authority |
|---|---|
| stable structured identifier | deterministic |
| configuration/parser | parser-derived |
| official documentation | official |
| operational observation | observed |
| LLM extraction | model-derived |
| unsupported synthesis | rejected |

## Required retrieval flow

```text
hybrid seed retrieval
  -> entity resolution
  -> bounded graph expansion
  -> temporal filtering
  -> evidence hydration
  -> relevant documentation and code retrieval
  -> reranking
  -> cited synthesis
```

## V1 graph scope

Required:

- aliases and canonical entity resolution;
- one-hop and bounded multi-hop traversal;
- evidence paths;
- temporal validity;
- relationship confidence;
- graph projection rebuild;
- entity/relationship semantic descriptions in Qdrant.

Designed but not required for core exit:

- hierarchical communities;
- community reports;
- global map-reduce search;
- graph embeddings beyond entity/report text descriptions.
