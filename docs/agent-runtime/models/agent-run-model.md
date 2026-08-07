---
title: "Agent Run Model"
created: 2026-08-05
updated: 2026-08-05
doc_type: "model"
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

# Agent Run Model

## Aggregate root

<code>AgentRun</code> is the durable orchestration aggregate. It owns state transitions, attempt identity, external resource references, output requirements, terminal outcome, and cleanup state.

It does not embed full context, transcripts, logs, or artifacts. Those remain referenced evidence or observation records.

## Attempts and retries

A retry creates a child attempt or child run linked to the original. It records what changed:

- new resolved stack;
- new context generation;
- altered disclosure policy;
- refreshed loadout;
- new runtime instance;
- changed budgets or approval decisions.

Reusing the same run identity for materially different inputs is forbidden.

## Durable steps

Provisioning and execution use durable step identities:

- resolve package;
- compile context;
- resolve loadout;
- create/reconcile Incus instance;
- bootstrap runtime;
- start session;
- disclose bootstrap capsule;
- execute task;
- verify outputs;
- publish manifests;
- stop/snapshot/delete resources.

Completed steps record replayable outputs. Recovery uses those outputs or reconciles external state before continuing.

## External resources

The run stores canonical references for:

- LABBY loadout generation or physical gateway;
- Incus project, instance, operations, and snapshots;
- agent runtime process/session/thread/turn;
- compiled context generations;
- APM package resolution;
- artifacts and outputs.

## Cancellation

Cancellation is cooperative first and forced after policy timeout. It propagates to Code Mode, Axon jobs, Codex turns, runtime processes, and Incus actions where supported. Cancellation does not imply cleanup success.

## Terminal outcome

Terminal state records:

- category and stable code;
- safe message;
- verification status;
- retained runtime and artifact references;
- cleanup result;
- incomplete operations;
- retry eligibility.

## Worker engine

Axon's unified job engine is the preferred durable scheduler foundation because it already implements canonical job state, workers, leases, heartbeats, cancellation registry, watchdogs, recovery, artifacts, pagination, and retention. AgentRun adds product-specific phase and external-resource data.
