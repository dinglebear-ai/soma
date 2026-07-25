# ADR 0006: Extend Soma xtask as the contract control plane

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Axon has strong schema/reference generation. Soma already has architecture checks, generated docs, release tooling, and contract conventions.

## Decision

Port runtime-neutral schemas and drift machinery into Soma's existing xtask. Do not publish a contract-tool crate in v1. Generate combined schemas, references, capability matrices, input hashes, dependency graphs, and fixture checks.

## Consequences

- One developer command center.
- Contracts are executable and CI-enforced.
- A reusable contract-kit can be extracted later if another repository genuinely consumes it.

## Rejected alternatives

- Keep separate Axon and Soma contract generators.
- Publish a new tool crate before the product needs it.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
