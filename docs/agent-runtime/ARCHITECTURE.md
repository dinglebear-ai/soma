---
title: "Soma Agent Runtime Architecture"
created: 2026-08-05
updated: 2026-08-05
doc_type: "architecture"
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

# Architecture

## Control flow

~~~text
APM package + lock
        |
        v
AgentStack + ContextManifest + policies
        |
        v
Soma resolver and validator
        |
        +----> Axon knowledge and research
        +----> Cortex observations and graph evidence
        +----> LABBY gateway catalog and snippets
        +----> Incus local runtime
        +----> Codex or other agent runtime adapter
        |
        v
ResolvedAgentRun
        |
        v
provision -> bootstrap -> disclose -> execute -> observe -> verify -> finalize
        |
        v
CompiledContext + RunManifest + SynthesisResult + artifacts
~~~

## Architectural planes

### Package plane

APM resolves prompts, skills, agents, hooks, plugins, and MCP dependencies into a lockfile-backed installation. Soma records the resolved package identity and hashes but does not reimplement APM dependency resolution.

### Context plane

Axon and Cortex remain separate ingestion systems. Soma's context broker queries canonical records, FTS, vectors, graph projections, and memory. A context manifest defines eligibility and policy. A compiled context records the exact selected evidence for one task and revision.

### Computation plane

Code Mode executes bounded JavaScript against a host-supplied catalog. Snippets are named, versioned Code Mode programs. The effective host catalog is scoped to the run's capability intersection.

### Capability plane

LABBY remains the gateway and upstream catalog authority. A loadout selects upstream namespaces, tools, virtual-server surfaces, rate limits, credentials, and mutation permissions. The normal implementation is a logical capability token against a shared gateway. A physically dedicated gateway is optional for stronger isolation.

### Runtime plane

Incus creates the isolation boundary. The current client supports the local Unix socket and must remain the only v1 transport. A runtime adapter starts Codex app-server or another supported agent process inside the instance. Mounts, devices, networks, resources, snapshots, and lifecycle actions are declared by the stack.

### Disclosure plane

The disclosure controller tracks four distinct sets:

1. eligible context;
2. materialized or mounted context;
3. context disclosed to the agent;
4. evidence cited by a claim or action.

Disclosure may progress from bootstrap metadata to summaries, graph neighborhoods, evidence bundles, raw records, and cross-repository expansion.

### Observation plane

Cortex records agent-run events, commands, tool calls, transcript segments, resource samples, process events, Incus events, context disclosures, claims, artifacts, and terminal outcomes. Existing OTLP, log, heartbeat, session, shell-history, Docker, and graph capabilities remain authoritative building blocks.

## Aggregate ownership

- <code>AgentStack</code> owns declarative desired state.
- <code>ResolvedAgentRun</code> owns immutable resolution results.
- <code>CompiledContext</code> owns selected evidence and provenance.
- <code>DisclosureSession</code> owns what was revealed and why.
- <code>SnippetDefinition</code> owns reusable investigation logic and requirements.
- <code>LabbyLoadout</code> owns requested gateway exposure.
- <code>EffectiveCapabilities</code> owns the final intersection after authorization.
- <code>RuntimeInstance</code> owns Incus identity and lifecycle state.
- <code>AgentRun</code> owns orchestration state and terminal result.
- <code>SynthesisResult</code> owns claims, evidence, conflicts, uncertainty, and actions.

## Persistence

Canonical run data belongs under the Soma data root and canonical context stores. Proposed runtime layout:

~~~text
<SOMA_HOME>/
  stacks/
  contexts/
    manifests/
    compiled/
  snippets/
  loadouts/
  runs/
    <run-id>/
      resolved-stack.json
      compiled-context.json
      disclosure-log.jsonl
      run-manifest.json
      synthesis-result.json
      artifacts/
  cache/
  logs/
~~~

This package does not override context-v1's canonical SQLite, artifact, FTS, Qdrant, graph, or memory authority model.

## Deployment modes

- **Local one-shot:** local Soma, local LABBY, local Incus, one task.
- **Resident assistant:** long-lived Incus instance with durable workspace and repeated runs.
- **Remote-client mode:** CLI or web calls a running Soma service; Soma still provisions only against its local Incus socket in the first implementation.
- **Dedicated gateway mode:** the agent instance receives its own gateway process or sidecar when logical scoping is insufficient.
