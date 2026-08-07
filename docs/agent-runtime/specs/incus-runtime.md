---
title: "Incus Agent Runtime Specification"
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

# Incus Agent Runtime Specification

## Purpose

Incus supplies the isolation and lifecycle boundary for Soma agent services. The current <code>soma-incus-client</code> is the required foundation.

## Transport boundary

The initial implementation MUST connect through the local Incus Unix socket. Remote HTTPS/mTLS transport is explicitly unsupported. Soma MUST reject remote endpoint configuration rather than silently falling back to shell commands or SSH.

## Runtime modes

### One-shot worker

Soma creates or restores an instance for one run, applies configuration, starts the runtime, collects outputs, and stops, snapshots, or deletes it according to policy.

### Resident assistant

Soma manages a durable instance and workspace that accepts multiple runs. Each run still receives distinct context, capability, disclosure, and output identities.

## Instance declaration

The stack MAY declare:

- project;
- image alias or fingerprint;
- instance type;
- profiles;
- config keys;
- devices and mounts;
- networks;
- resource limits;
- environment references;
- initialization command;
- health probe;
- snapshot and retention policy.

Soma MUST render deterministic instance configuration from the resolved stack.

## Mount policy

Each mount declares source, target, mode, purpose, sensitivity, and lifetime. Defaults:

- repository workspace: read-only for investigators, read-write only when authorized;
- global docs: read-only;
- compiled context: read-only;
- run artifacts: write-only or read-write within the run directory;
- secrets: ephemeral and never copied into run manifests;
- host root and arbitrary devices: denied.

## Lifecycle operations

The current client already supports list, get, create, update, patch, delete, start, stop, restart, pause, snapshots, operations, and event subscription. The implementation MUST use those APIs rather than shelling out to <code>incus</code> for covered operations.

Missing runtime needs must be added to the shared client with tests before product use. Likely additions include:

- instance exec with bounded I/O and cancellation;
- file push/pull or agent bootstrap transfer;
- state and resource inspection;
- project/profile/device validation helpers;
- operation wait helpers with run correlation.

## Provisioning sequence

~~~text
validate local Incus availability
-> resolve project/image/profiles/networks/storage
-> reserve deterministic instance name
-> create or reconcile instance
-> attach mounts and limits
-> install or verify runtime payload
-> issue bootstrap configuration and credentials
-> subscribe to Incus lifecycle events
-> start instance
-> wait for health
-> hand control to agent runtime adapter
~~~

All steps MUST be idempotent or protected by durable run state.

## Naming

Instance names SHOULD include stack, service, and short run identity while respecting Incus limits. Durable resident instances use stable names; one-shot workers include a run suffix.

## Failure and recovery

Provisioning failure MUST retain enough state to inspect the instance. Cleanup policy determines automatic deletion. A failed cleanup becomes a distinct terminal condition and remains visible to operators.

## Observation

Soma MUST correlate Incus operations and events with Cortex run events. Resource samples SHOULD include CPU, memory, disk, network, process, and instance state at the configured heartbeat interval.
