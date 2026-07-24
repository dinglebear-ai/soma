# ADR 0009: Use one typed AI-session model with dual projections

**Status:** Proposed  
**Date:** 2026-07-21

## Context

Axon and Cortex both parse Claude, Codex, and Gemini sessions but project them toward different outcomes.

## Decision

soma-transcript owns provider-neutral sessions, turns, content, tool calls, MCP calls, skills, hooks, and provenance. It projects to knowledge documents/chunks and operational observations/graph candidates.

## Consequences

- One parse path and stable session identity.
- Semantic and operational queries cite the same canonical session.
- Provider-specific dialects remain feature-gated.

## Rejected alternatives

- Keep separate Axon and Cortex session models.
- Normalize immediately to plain text and discard structure.

## Revisit when

This decision is revisited only when measured product requirements or a later versioned program invalidate its assumptions.
