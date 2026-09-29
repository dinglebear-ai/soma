---
title: "Agent Package Manager Integration Specification"
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

# APM Integration Specification

## Boundary

APM is the package manager. Soma is the execution harness.

APM owns:

- manifest and dependency resolution;
- transitive package installation;
- primitive deployment across supported clients;
- lockfiles and integrity hashes;
- package audit, policy, drift detection, pack, and distribution;
- MCP dependency consent and installation.

Soma owns:

- stack and context resolution;
- runtime capability authorization;
- LABBY loadouts;
- Incus provisioning;
- progressive disclosure;
- Code Mode and snippet execution;
- lifecycle telemetry;
- outputs, verification, and retention.

## Inputs

A Soma stack MAY reference:

- <code>apm.yml</code>;
- <code>apm.lock.yaml</code>;
- an installed package root;
- selected agents, prompts, skills, hooks, plugins, instructions, or MCP dependencies from the resolved package.

## Resolution strategy

The first implementation MUST invoke the installed APM CLI through a bounded process adapter. It MUST NOT port APM's Python resolver into Rust.

Required process operations:

- inspect version and capabilities;
- validate manifest;
- install or verify against lock;
- audit policy and integrity;
- produce a machine-readable resolved inventory;
- compile a selected target when needed.

If APM lacks a required machine-readable command, Soma SHOULD initially read documented manifest and lockfile shapes and record the limitation. Any new APM integration contract should be contributed upstream rather than inferred from human CLI output indefinitely.

## Locking

A resolved run MUST record:

- APM CLI version;
- manifest and lock paths;
- manifest and lock SHA-256 digests;
- resolved package identities, versions, sources, and content hashes;
- selected primitive identities;
- audit and policy result;
- installation or compilation target.

The run MUST fail when required lock integrity cannot be verified.

## Primitive mapping

Initial mapping:

| APM primitive | Soma use |
|---|---|
| instructions | orientation or policy prompt inputs |
| skills | declared agent and snippet skill dependencies |
| prompts | role, task, research, challenge, synthesis, and verification prompts |
| agents | runtime role definitions and adapter defaults |
| hooks | package-installed lifecycle hooks, disabled unless explicitly supported by Soma policy |
| plugins | packaged capability collections and client exports |
| MCP servers | candidates for LABBY installation and loadout exposure, never automatic execution authority |

## Security

- APM policy and audit MUST complete before run provisioning.
- Package-installed MCP servers require LABBY installation and explicit loadout exposure.
- Package hooks do not execute merely because APM installed them.
- Package content is executable in effect and must retain source and integrity metadata.
- Soma policy can only narrow package capabilities.

## Caching

Installed packages MAY be cached by lock digest. A run receives a read-only package projection and immutable receipt. Cache mutation during a run is forbidden.

## Future integration

Soma MAY generate or update APM manifests for agent stacks, but generated changes require explicit user action and remain package-plane operations separate from running an agent.
