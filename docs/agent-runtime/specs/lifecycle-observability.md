---
title: "Agent Lifecycle Observability Specification"
created: 2026-08-05
updated: 2026-08-05
doc_type: "spec"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Agent Lifecycle Observability Specification

## Purpose

Cortex must observe the complete cognitive and operational lifecycle of every Soma agent run, not only its final answer.

## Required event families

### Orchestration

- run created, resolved, provisioned, started, stopped, finalized;
- state transitions, retries, cancellation, recovery, and cleanup;
- Incus operations, events, snapshots, and instance health;
- gateway loadout resolution and catalog generation;
- package and context resolution.

### Context and disclosure

- context compilation and generation;
- source availability and freshness;
- disclosure requests and decisions;
- context items and evidence bundles disclosed;
- raw-record access;
- materialization creation and reads;
- context comparison and enrichment.

### Agent activity

- runtime process and thread/turn identity;
- prompts and skill identities by hash, never expanded secrets;
- tool catalog listings;
- tool and snippet calls;
- shell commands and working directories;
- repository status, commit, and file changes;
- approval requests and decisions;
- transcript segments and model/provider metadata;
- claims, hypotheses, conflicts, and recommendations.

### Runtime health

- process lifecycle;
- CPU, memory, disk, network, open files, and process counts;
- stdout and stderr;
- OTLP spans, logs, and metrics;
- host and device heartbeat data;
- Docker, syslog, journald, dmesg, nginx, Authelia, UniFi, and other related observations already available through Cortex.

### Outputs

- artifacts and digests;
- output-schema validation;
- verification results;
- terminal status and failure taxonomy;
- retention and cleanup results.

## Correlation keys

Every event SHOULD carry:

- run ID;
- stack and service ID;
- agent and runtime instance ID;
- context and context-generation ID;
- disclosure decision ID;
- snippet execution ID;
- Code Mode execution and step IDs;
- trace and span IDs;
- Incus operation and instance IDs;
- repository/project/host/device entity IDs;
- event and ingestion timestamps.

## Canonical authority

Cortex canonical stores remain authoritative for observations. Agent-run events must enter the same evidence and retention model rather than a private orchestration log database with no graph projection.

## Existing reuse

The implementation must reuse or transplant current Cortex behavior for:

- AI transcript forwarding;
- shell-history forwarding;
- syslog and journald;
- OTLP;
- Docker ingestion;
- heartbeat windows and host state;
- inventory and raw configuration collection;
- incident evidence and graph projection;
- multi-hop graph queries;
- storage budgets and retention.

## Heartbeat policy

Thirty-second samples MAY be retained raw for a bounded window. Longer retention SHOULD aggregate to one-minute, five-minute, and hourly summaries while preserving full-resolution windows around incidents, deployments, failures, and agent runs.

## Privacy and redaction

Events MUST NOT persist secrets, bearer tokens, cookies, private keys, unredacted authorization headers, or unrestricted raw authentication data. Transcript and command capture follows explicit stack policy and source sensitivity.

## Query outcomes

The system must support questions such as:

- What context did the agent have before this edit?
- Which tool or snippet produced this claim?
- Which source contradicted the conclusion?
- What changed between two runs?
- Did resource pressure or an external service failure correlate with the result?
- Which disclosures and skills improved success rates?
