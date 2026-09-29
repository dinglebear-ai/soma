---
title: "Synthesis and Research Model"
created: 2026-08-05
updated: 2026-08-05
doc_type: "model"
status: "proposed"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "agent-runtime"
source_of_truth: true
last_reviewed: "2026-08-05"
---

# Synthesis and Research Model

## Investigation, not completion

Synthesis is modeled as an investigation aggregate containing the current context generation, hypotheses, calculations, snippet executions, dependent questions, jobs, claims, conflicts, and stopping state.

The model output is one contributor to the aggregate. It is not the aggregate itself.

## Research DAG

~~~text
Task
  -> local evidence question
      -> Axon research question
          -> sources and findings
              -> child context generation
                  -> refined or new question
~~~

Each question records <code>derivedFrom</code> evidence and parent depth. Cycles are prevented by canonical normalized-question digest plus ancestry checks. Duplicate questions may attach to the same durable job.

## Hypotheses

A hypothesis has status:

- proposed;
- supported;
- weakened;
- contradicted;
- rejected;
- unresolved.

Hypotheses are working objects. Only final claims enter the synthesis result, and their status reflects evidence class rather than rhetorical confidence.

## Claims and evidence

A claim is linked to supporting and contradicting evidence. Confidence is a bounded product policy derived from authority, directness, diversity, freshness, agreement, and known limitations. The raw model confidence is not authoritative.

## Context enrichment

Completed research or newly arrived observations create a child compiled-context generation. The synthesis aggregate records which generation supported each claim. This prevents later evidence from being retroactively attributed to an earlier decision.

## Stopping state

The investigation stops because requirements are met, evidence is insufficient, an approval is required, or a budget is exhausted. “No more ideas” is not a valid durable reason.

## Persistence

Store structured investigation state separately from large model transcripts and raw evidence. Axon's job and synthesis modules provide the donor behavior for durable work and output assembly. Cortex receives lifecycle events and graph projections from questions, claims, and evidence links.
