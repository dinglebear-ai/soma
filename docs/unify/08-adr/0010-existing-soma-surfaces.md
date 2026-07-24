# ADR 0010: Keep existing Soma gateway, auth, provider catalog, and surfaces authoritative

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Soma already has the desired gateway, OAuth, provider catalog, Code Mode, CLI/API/MCP projection, and Aurora web shell. Re-converging donor surfaces would undo completed architecture work.

## Decision

New context capabilities enter through Soma's existing domain/application/runtime and surface adapters. Axon/Cortex CLI, API, MCP, web, auth, and observability code are not migrated as competing foundations.

## Consequences

- One product contract.
- No duplicate gateways/auth/surfaces.
- The web shell grows into the full product surface.

## Rejected alternatives

- Merge all donor surfaces and reconcile later.
- Create context-specific auth or MCP servers.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
