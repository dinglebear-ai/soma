---
title: "Progressive Disclosure Specification"
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

# Progressive Disclosure Specification

## Purpose

Progressive disclosure prevents an agent from receiving the complete context and capability universe before it is relevant. It is both a context-budget mechanism and a security boundary.

The controller MUST distinguish:

- eligible context;
- materialized or mounted context;
- disclosed context;
- evidence cited by a claim or action;
- tools and skills visible in the catalog;
- tools and skills actually invoked.

## Disclosure levels

### Level 0: bootstrap

Required bootstrap information:

- agent identity and role;
- task and acceptance criteria;
- stack and run IDs;
- repository/project summary and revision;
- active constraints and risk class;
- context-domain catalog;
- snippet and skill catalogs;
- scoped LABBY tool catalog;
- output and verification requirements.

### Level 1: orientation

Repository architecture, recent work, relevant services, active issues, source freshness, and operational health summaries.

### Level 2: focused neighborhoods

Task-specific graph neighborhoods, retrieval results, timelines, source summaries, and named context views.

### Level 3: evidence bundles

Snippet outputs, selected records, correlations, conflicts, primary-source findings, and structured hypothesis evaluations.

### Level 4: raw evidence

Selected logs, traces, transcript segments, files, commands, configurations, and full source documents. Raw disclosure MUST require justification and bounded selection.

### Level 5: expanded scope

Cross-repository, global documentation, broader time windows, unrelated devices, restricted logs, or other high-cost or sensitive context. This level may require approval.

## Requests

An agent requests disclosure by naming:

- purpose or question;
- requested context domain or finding;
- desired representation;
- maximum records, bytes, or tokens;
- whether raw evidence is required;
- parent claim, snippet, or research question.

The controller returns a decision with allowed, narrowed, denied, or approval-required status.

## Decision inputs

The controller MUST evaluate:

- effective capabilities;
- context-manifest policy;
- stack disclosure policy;
- caller and agent identity;
- source sensitivity;
- current task and graph scope;
- prior disclosures;
- cost and remaining budgets;
- freshness and trust;
- whether a derived summary can satisfy the request.

## Derived before raw

The default order is:

~~~text
catalog -> summary -> graph neighborhood -> evidence bundle -> raw records
~~~

Raw evidence is not forbidden. It is delayed until needed and must remain inspectable.

## Tool and skill disclosure

The same controller governs catalogs. An agent MAY know that a capability class exists without receiving its exact tool schema. Mutation tools SHOULD remain undisclosed until a permitted workflow reaches an approval boundary.

## Trust disclosure

Every disclosed item MUST expose or inherit one of:

- observed;
- verified;
- documented;
- implemented;
- historical;
- claimed;
- inferred;
- correlated;
- contradicted;
- stale;
- unknown.

The model must not infer trust from prose formatting.

## Telemetry

Every decision MUST emit a lifecycle event containing:

- run, agent, and context IDs;
- request and decision IDs;
- requested and granted level;
- reason code;
- source classes and canonical references;
- size and token estimates;
- policy and authorization references;
- parent claim, snippet, or question;
- timestamp and outcome.

Cortex can then compare successful and failed runs by disclosure history.

## Replay

A run replay SHOULD support:

- original disclosures only;
- original disclosures plus newly available evidence;
- alternate disclosure policy for evaluation.

The original record remains immutable.
