---
title: "Code Mode Synthesis Specification"
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

# Code Mode Synthesis Specification

## Purpose

Soma synthesis is a bounded investigation over typed context, not a single prompt stuffed with every retrieved record.

The synthesis runtime can:

- query and traverse the compiled context;
- filter, join, group, count, compare, and correlate records;
- execute read-only snippets;
- formulate evidence-dependent research questions;
- create bounded Axon jobs;
- ingest and attach research results;
- challenge hypotheses;
- emit structured claims with evidence and uncertainty.

## Pipeline

~~~text
compile context
-> orient
-> inspect structured and graph evidence
-> derive hypotheses
-> run calculations and snippets
-> identify missing evidence
-> create dependent Axon research jobs
-> wait or poll through durable job state
-> compile child context with new evidence
-> challenge hypotheses and preserve conflicts
-> produce structured synthesis
~~~

The first implementation MAY cap dependent research depth at one.

## Code Mode dataset

A compiled context MUST be addressable through a run-scoped Code Mode host. The host SHOULD expose typed actions equivalent to:

- context metadata and catalog;
- entity lookup and graph neighborhood;
- bounded timeline;
- canonical record hydration;
- evidence evaluation for a claim;
- context comparison;
- disclosure request;
- Axon research creation and status;
- artifact writing.

The host MUST enforce authorization before returning catalog entries or results.

## Dependent research

A research question MUST include:

- question text;
- parent run and context IDs;
- evidence IDs that caused the question;
- source policy and preferred authority classes;
- repository, dependency, version, or time constraints;
- budget and deadline;
- maximum dependent depth;
- expected output contract.

Research questions and jobs become graph entities. The answer must retain canonical sources and citations.

## Axon reuse

The implementation SHOULD wrap or transplant the existing Axon boundaries:

- <code>RetrievalRequest</code> and <code>RetrievalPlan</code>;
- <code>ContextBundle</code> and <code>AskContext</code>;
- query service;
- synthesis pipeline;
- unified durable jobs;
- graph candidates and evidence;
- Codex-backed or other LLM runtime adapters.

Soma adds task/run identity, dependent-question edges, context generation, authorization, and lifecycle observation.

## Stopping conditions

A synthesis policy MAY stop when:

- required output fields are supported;
- all material claims have evidence;
- no material open question remains;
- confidence or coverage threshold is met;
- source diversity requirement is met;
- dependent-depth, job, time, or token budget is exhausted;
- approval is required.

Budget exhaustion is a classified terminal condition, not a fabricated conclusion.

## Claims

Each claim MUST include:

- stable claim ID;
- text and normalized subject/predicate when available;
- status and confidence;
- supporting and contradicting evidence;
- source diversity;
- freshness;
- inference explanation when inferred;
- dependent questions;
- supersession or invalidation links.

Allowed statuses include observed, verified, documented, implemented, historical, claimed, inferred, correlated, contradicted, unknown, and superseded.

## Output

The final <code>SynthesisResult</code> contains:

- summary;
- findings and claims;
- evidence index;
- rejected hypotheses;
- conflicts;
- open questions;
- recommended actions;
- research jobs and context generations;
- budget and timing report;
- verification status.

Narrative Markdown is a projection of this structured result.

## Safety

Synthesis does not grant mutation authority. A recommended action is data. Executing it requires a separately authorized action or snippet and, where required, approval.
