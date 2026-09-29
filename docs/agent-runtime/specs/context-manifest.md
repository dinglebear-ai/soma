---
title: "Context Manifest Specification"
created: 2026-08-05
updated: 2026-08-05
doc_type: "spec"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Context Manifest Specification

## Purpose

A context manifest is the versioned recipe for the evidence universe surrounding a repository, project, service, device, incident, or agent. It defines what may be queried and materialized, not what must be placed in every prompt.

The default file name is <code>soma.context.yaml</code>.

## Source classes

A manifest MAY select:

### Axon knowledge

- local repositories and directories;
- upstream repositories;
- documentation collections;
- web sources and crawls;
- GitHub repositories, issues, pull requests, and releases;
- package registries;
- transcripts and durable research results;
- memory.

### Cortex observations

- AI sessions and transcript events;
- agent and user commands;
- tool and MCP events;
- OTLP logs, spans, and metrics;
- syslog, journald, dmesg, and managed file tails;
- Docker logs and lifecycle events;
- nginx/SWAG and authentication events;
- UniFi, inventory, configuration, process, and device heartbeat observations;
- incidents and previous snippet runs.

### Soma-native sources

- repository status and current revision;
- provider catalogs;
- gateway configuration and catalog state;
- Code Mode state and artifacts;
- run manifests and compiled contexts.

## Graph scope

The manifest MUST identify one or more roots and MAY define bounded traversal rules:

- entity kinds and canonical IDs;
- incoming or outgoing relationship types;
- maximum depth and entity count;
- time and revision constraints;
- trust, authority, sensitivity, and confidence filters;
- evidence hydration rules.

The graph query must remain bounded and deterministic after tie-breaking.

## Views

A manifest MAY declare named views such as <code>repository-maintainer</code>, <code>incident</code>, <code>pr-review</code>, or <code>release</code>. A view can:

- add task-specific roots;
- narrow source classes;
- define retrieval lanes;
- add saved snippets;
- set budgets and time windows;
- set disclosure defaults;
- require primary or directly observed evidence.

A view MUST NOT broaden a parent security policy.

## Prompt and skill references

A manifest MAY reference orientation, investigation, research, challenge, verification, and synthesis prompts. It MAY request skills. A context manifest does not install those primitives; APM or another package source supplies them.

## Freshness

Every source class MUST have a freshness policy. Supported forms include:

- maximum age;
- required generation or commit;
- required successful crawl after a timestamp;
- live or latest observation window;
- stale-allowed with explicit result classification.

Compilation MUST report stale, unavailable, conflicting, and excluded sources.

## Policies

The manifest MUST support:

- repository and project scope;
- sensitivity and redaction;
- raw transcript restrictions;
- raw authentication-log restrictions;
- secret and credential exclusion;
- caller authorization requirements;
- retention;
- source diversity;
- citation requirements;
- inference classification;
- conflict preservation;
- maximum graph and retrieval budgets.

## Storage and symlinks

A manifest MAY request filesystem materialization. Durable Axon-managed documents may be mounted or symlinked from an authoritative global tree. Every materialized entry MUST retain a canonical reference and content digest. Symlink escapes are forbidden.

## Compilation interface

~~~bash
soma context validate
soma context compile --view incident --task "investigate gateway 502s" --at HEAD --since 24h
soma context inspect CONTEXT_ID
soma context materialize CONTEXT_ID --format filesystem
~~~

Compilation returns an immutable context ID and generated manifest. Query and materialization are separate operations.

## Compatibility with context v1

This specification extends the existing <code>Context Query Contract</code>. It does not replace hybrid SQL, FTS, Qdrant, graph, memory, citations, or claim classifications. It adds a durable declarative input and reproducible compiled result around those use cases.
