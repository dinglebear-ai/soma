---
title: "Agent Runtime Security Contract"
created: 2026-08-05
updated: 2026-08-05
doc_type: "contract"
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

# Security Contract

## Capability intersection

Actual capability is the intersection of package, stack, context, snippet, LABBY, runtime, and caller policy. No layer may broaden a stricter parent.

## Secrets

- Manifests contain secret references, never secret values.
- Resolved manifests contain reference identities and injection receipts only.
- Secrets are scoped to run, service, tool, and lifetime.
- Secret-bearing files are mode 0600 where supported and must reject symlink targets.
- Secrets do not enter transcripts, Code Mode traces, lifecycle attributes, artifacts, or error messages.

## Filesystem

- Host paths are canonicalized and checked against allowed roots.
- Symlink escapes are rejected.
- Context and package mounts are read-only.
- Writable repository mounts require explicit policy.
- Host root, Docker socket, Incus socket, device nodes, SSH material, and cloud credentials are denied unless a dedicated trusted profile explicitly grants them.

## Network

- Runtime egress is denied or bounded by profile and snippet capability.
- LABBY is the preferred route for external tools and services.
- Direct network access from provider/snippet runtimes follows existing broker and allow-host patterns.
- Remote Incus is unavailable until secure mTLS support exists.

## Tools and mutation

- Tool discovery is authorization-filtered.
- Read-only is the default mutation class.
- Repository, runtime, and infrastructure mutations are separate classes.
- Broadly disruptive, credential, security, or destructive actions require explicit authorization and often approval.
- Recommendations never execute automatically.

## Package security

APM audit, lock integrity, policy, and drift checks run before provisioning. Package-installed hooks and MCP servers are inert until separately allowed by Soma and LABBY policy.

## Context security

Authorization filters SQL, FTS, vector, graph, memory, source, observation, materialization, and disclosure lanes. Visible graph entities cannot leak protected evidence. Raw auth logs and transcripts are restricted source classes.

## Runtime isolation

Incus is defense in depth, not the sole authorization mechanism. An isolated agent still receives only scoped tools, mounts, credentials, network, and context.

## Audit

Every policy decision records policy version, actor, requested capability, effective decision, reason code, and related run/event IDs. Sensitive denials avoid confirming protected resource existence.

## Fail-closed requirements

The run fails or pauses when required authorization, package integrity, context policy, loadout scoping, secret injection, mount safety, or approval cannot be established. It must not silently continue with broader ambient access.
