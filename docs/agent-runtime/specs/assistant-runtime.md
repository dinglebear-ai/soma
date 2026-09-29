---
title: "Assistant Runtime Specification"
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

# Assistant Runtime Specification

## Initial adapter

The first supported agent adapter is Codex app-server using <code>crates/shared/codex-app-server-client</code>.

The adapter MUST use <code>CodexSession</code> and its existing process, stream, initialization, thread, turn, approval, event, and collection behavior. It must not create a competing JSON-RPC client.

## Container payload

A Codex assistant image SHOULD contain:

~~~text
/usr/local/bin/soma-agent-supervisor
/usr/local/bin/codex
/soma/bin/optional-helper-runtimes
/soma/package/          resolved APM package
/soma/docs/             global read-only documentation view
/soma/context/          compiled context projection
/workspace/             repository workspace
/run/                   run configuration and artifacts
~~~

The exact paths are runtime-contract fields, not assumptions embedded in prompts.

## Supervisor responsibilities

A small Soma-owned supervisor inside the instance MUST:

- validate bootstrap input and run identity;
- start Codex app-server with explicit arguments and config;
- connect the host-side adapter or expose a controlled Unix socket;
- forward process, stdout, stderr, and health telemetry;
- collect transcript and event data;
- enforce shutdown and timeout;
- write bounded terminal artifacts;
- avoid direct access to the host LABBY credential outside the scoped bootstrap.

The supervisor is not an autonomous agent.

## Session flow

~~~text
spawn/connect Codex app-server
-> initialize with client capabilities
-> create thread with repository working directory
-> provide bootstrap prompt and disclosed context handles
-> run turn
-> process approval requests through Soma policy
-> collect events, messages, diffs, and errors
-> request deeper context or tools through scoped interfaces
-> verify terminal output contract
-> close session
~~~

## Prompts

The adapter receives resolved prompts from the package and stack. It SHOULD separate role, task, research, challenge, and synthesis prompts rather than concatenate an unversioned monolith.

## Approvals

Approval requests MUST be mapped to the run's mutation policy and caller authorization. Denied or unavailable approval is a structured runtime result. The adapter must never auto-approve solely because the runtime is isolated.

## Global docs

The first implementation MAY mount the complete authoritative docs tree read-only at <code>/soma/docs</code> while disclosing only catalogs and selected summaries to the model. Later implementations MAY use lazy materialization or a virtual context filesystem.

## Other runtimes

Claude and Gemini adapters MAY be added behind the same application port after Codex proves the lifecycle. The shared model must not contain Codex-specific protocol types.
