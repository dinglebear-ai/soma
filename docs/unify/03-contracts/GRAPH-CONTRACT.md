# Evidence Graph Contract

## Purpose

The graph connects knowledge, deployed state, observations, sessions, tools, infrastructure, incidents, and memory. It is evidence-first, temporal, and rebuildable.

## Core objects

- **Entity:** a stable thing with kind, canonical name, aliases, and attributes.
- **Relationship:** a typed connection between two entities.
- **Claim:** a statement that may be confirmed, disputed, rejected, or superseded.
- **Evidence:** a reference to canonical material supporting or contradicting a relationship or claim.
- **Graph candidate:** an unresolved projection emitted by a parser, adapter, or correlation engine.
- **Projection:** a versioned materialization from canonical stores.
- **Community/report:** optional derived cluster and cited summary.

## Evidence invariant

Every persisted relationship and claim MUST have at least one evidence reference.

No naked model assertion may enter the authoritative graph projection.

## Authority classes

From strongest to weakest by default:

1. deterministic structured fact;
2. directly observed runtime fact;
3. official documentation;
4. deterministic/parser-derived fact;
5. authenticated user assertion;
6. historical narrative;
7. model-derived assertion.

Product policy MAY adjust precedence per relationship kind. Authority and relevance are separate.

## Trust and confidence

- Trust describes the source or observation channel.
- Confidence describes support for one resolved assertion.
- Confidence MUST be reproducible from evidence contributions and policy version.
- Contradicting evidence MUST be preserved.
- A confidence increase MUST NOT erase minority evidence.
- Model-derived edges MUST record model/provider and extractor version.

## Temporal semantics

Relationships and claims support:

- first/last observed;
- valid from/until;
- event time;
- projection time.

The graph MUST be able to answer what was believed or deployed at an incident time, not only current state.

## Entity resolution

Resolution proceeds through:

1. stable authoritative identifiers;
2. configured aliases;
3. deterministic composite keys;
4. parser-derived aliases;
5. semantic candidates;
6. review or explicit conflict when ambiguity remains.

Semantic similarity alone MUST NOT merge entities.

Every merge preserves provenance and can be reversed or rebuilt.

## Suggested v1 vocabulary families

```text
Knowledge:
project, repository, source, document, file, code_symbol, package, specification

Infrastructure:
host, device, service, service_instance, container, network, volume, domain, endpoint

Agentic:
agent_runtime, session, run, tool, mcp_server, prompt, skill, tool_execution

Operational:
deployment, configuration_snapshot, incident, error_signature, observation_window

Knowledge lifecycle:
memory, decision, claim, artifact
```

The shared crate provides a vocabulary trait. Soma owns the concrete product vocabulary.

## Query requirements

V1 MUST support:

- alias/entity lookup;
- one-hop and bounded multi-hop neighborhoods;
- incoming/outgoing relationship filters;
- type, trust, authority, and time filters;
- bounded evidence hydration;
- path explanation;
- deterministic ordering;
- truncation reports.

## Projection lifecycle

Canonical stores remain authoritative.

```text
canonical record
    ↓ versioned projector
GraphCandidate
    ↓ resolve/merge
graph projection
    ↓ query
```

A new projector version can rebuild affected graph data. Projection errors do not corrupt canonical records.

## Community support

Community detection and reports are stretch v1 capabilities. When enabled:

- algorithms and parameters are versioned;
- reports cite underlying entities/relationships/evidence;
- report text is derived and rebuildable;
- communities never become canonical entity truth.

## Required fixtures

- deterministic entity resolution;
- ambiguous aliases;
- conflicting claims;
- time-varying deployment;
- evidence expiry;
- path explanation;
- projection rebuild;
- cyclic graph traversal;
- authority precedence;
- model-derived low-trust edge.
