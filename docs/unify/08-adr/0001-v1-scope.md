# ADR 0001: V1 merges context planes, not agent orchestration

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Soma needs a bounded first integration of Axon and Cortex. Agent Package Manager, worker-agent dispatch, custom Incus images, and autonomous remediation would multiply runtime, security, and verification scope before the context substrate exists.

## Decision

V1 delivers knowledge ingestion, observations, GraphRAG, memory, and existing surface integration. It explicitly excludes APM, Orchestrator/worker workflows, agent Incus containers, custom image compilation, autonomous PR/deploy, chat bridges, and harness self-improvement.

## Consequences

- A shippable diagnostic context layer precedes action automation.
- V1 schemas do not require speculative agent fields.
- Future agent work becomes a separate versioned program.

## Rejected alternatives

- Build the full autonomous harness immediately.
- Add placeholder public APIs for future agent systems.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
