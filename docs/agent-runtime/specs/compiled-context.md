---
title: "Compiled Context Specification"
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

# Compiled Context Specification

## Purpose

A compiled context is an immutable, task-scoped evidence snapshot produced from a context manifest, current canonical stores, graph projections, and caller authorization.

It answers:

- what question or task was compiled;
- which repository revision and time window were used;
- which sources were eligible and available;
- which query plan and graph traversal were executed;
- which evidence, entities, records, and summaries were selected;
- what was stale, excluded, conflicting, or unavailable;
- which materializations were created;
- what content was later disclosed to the agent.

## Identity

Every compiled context MUST have a stable unique ID. The ID is not itself a content hash. The manifest MUST also include hashes for the request, source manifests, query plan, selected evidence index, and serialized result.

## Required contents

A compiled context MUST record:

- context ID and schema version;
- source context-manifest digest;
- repository/project/service roots;
- task and parameters;
- caller and authorization decision reference;
- revision, dirty-state policy, and time window;
- selected query mode and retrieval lanes;
- graph projection revision;
- entity, relationship, evidence, document, observation, session, command, trace, metric, and artifact counts;
- canonical references for every selected item;
- citations and bounded excerpts;
- classified freshness and trust;
- conflicts and unknowns;
- budget use and truncation;
- materialization receipts.

## Canonical data

The compiled context MUST NOT become the sole copy of source documents or observations. It stores references, selected excerpts, derived summaries, and reproducibility metadata. Large raw data remains in canonical stores or durable artifacts.

## Snapshot modes

- <code>reference</code>: references canonical stores and requires them to remain available.
- <code>portable</code>: copies bounded evidence and content required for replay.
- <code>forensic</code>: captures selected raw evidence, runtime metadata, and stronger chain-of-custody fields.

Reference mode is the default.

## Enrichment

A synthesis run MAY enrich a compiled context with dependent Axon research or newly arrived Cortex observations. Enrichment MUST create a new context generation or child context. The original snapshot remains immutable.

~~~text
context A
  -> research job R1
  -> observations window O2
  -> context B, parent=A, additions=[R1,O2]
~~~

## Comparison

Soma SHOULD support comparing two compiled contexts across:

- repository and configuration changes;
- selected entities and evidence;
- telemetry and incidents;
- source freshness;
- claims and conflicts;
- disclosed context.

## Materializations

Supported projections SHOULD include:

- generated manifest JSON;
- Markdown briefing;
- filesystem view;
- MCP resource tree;
- graph subgraph JSON;
- event JSONL;
- Code Mode dataset handle.

Every materialization MUST include a digest, size, content type, and source context ID.

## Retention

Context retention is independent from source retention. Deleting a compiled context does not delete canonical evidence. A run manifest MUST remain able to state that a referenced context expired or was removed.
