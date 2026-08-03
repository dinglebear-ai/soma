---
title: "soma-infra"
created: 2026-08-01
updated: 2026-08-02
status: implemented
---

# soma-infra

**Path:** `crates/shared/operations/infra`
**Layer:** shared
**Package status:** private during extraction

## Purpose

`soma-infra` owns typed infrastructure semantics above `soma-fleet`. It converts bounded local, SSH, Docker, Compose, filesystem, process, log, and ZFS transports into stable models and verified mutation outcomes.

It does not own product configuration, environment loading, authorization scopes, Flux/Scout routing, confirmation UX, or CLI/MCP/REST formatting.

## Dependencies

Required internal dependencies:

- `soma-ops` for timestamps, mutation send state, and verification vocabulary;
- `soma-fleet` for host identity, topology revision, cancellation, command execution, strict SSH, and forwarding.

Optional external drivers:

- Bollard for local and forwarded Docker access;
- strict OpenSSH Unix-socket forwarding for remote Docker access;
- Rustix `openat2` for Linux descriptor-confined filesystem access.

## Read contracts

### Host and operating system

- `HostInspector` and `HostSystemInspector`;
- identity, uptime, memory, load, services, interfaces, mounts, ports, filesystem usage, and doctor reports.

### Docker and containers

- segregated system, container, image, network, volume, and telemetry readers;
- local and strict-SSH `BollardReadClient` instances bound to exact host revisions;
- bounded logs, one-shot statistics, disk usage, inspection, and process tables.

### Compose

- project discovery, status, configuration, and bounded logs;
- `CommandComposeInspector` with discrete `docker compose` arguments.

### Filesystem, processes, logs, and ZFS

- explicit read-root policies;
- descriptor-confined stat, preview, hash, file, directory, tree, find, and tail operations;
- typed process snapshots;
- validated syslog, journal, kernel, and authentication-log reads;
- ZFS pool, dataset, and snapshot tables.

## Mutation contracts

### Common mutation semantics

- `MutationFailure` preserves `MutationSendState` and the underlying infrastructure error;
- `MutationVerificationPolicy` bounds verification attempts and delays;
- cancellation or timeout before a backend call is `NotSent`;
- cancellation, timeout, or connection failure after the send boundary is conservatively `Unknown` unless the backend response proves `Sent`;
- backend acceptance and postcondition verification remain separate facts.

### Container lifecycle

- `ContainerLifecycleMutator` and `ContainerLifecycleEngine`;
- start, stop, restart, pause, and resume;
- exact host/revision-bound local or remote Bollard clients;
- already-satisfied states produce verified no-op outcomes;
- independent `container.inspect` reads verify the requested runtime state.

### Compose lifecycle

- `ComposeMutator` and `ComposeMutationEngine`;
- `compose up -d` and `compose restart`;
- shell-free process-backed commands;
- independent Compose status reads verify a nonempty service set in running, healthy, zero-exit state.

### Artifact pulls

- `ImagePullMutator`, `ImagePullEngine`, and host-bound `DockerArtifactClientProvider`;
- `ComposePullMutator` and `ComposePullEngine`;
- canonical `ProgressEvent` delivery through an object-safe reporter;
- bounded retained progress and delivery-error metadata;
- independent Docker image-store verification of IDs, tags, and digests;
- OCI artifact references and runtime-state evidence at the product result boundary.

## Security properties

1. Every target-specific model is bound to a host and exact topology revision.
2. Command-backed operations use discrete argv, never synthesized shell strings.
3. Docker clients reject host or topology drift after construction.
4. Remote Docker clients own private forwarded Unix sockets and revision-keyed SSH sessions.
5. Filesystem access remains beneath explicit absolute roots and rejects symlinks, magic links, and traversal.
6. All previews, hashes, command output, logs, traversal, and verification loops are bounded.
7. Product authorization is deliberately absent from this shared crate.
8. Mutation drivers preserve whether a backend call was not sent, sent, or uncertain.
9. A successful backend response is not promoted to mutation success until a separate read verifies the postcondition.
10. Pull streams emit canonical bounded progress while retaining progress-delivery failures separately from execution truth.
11. Docker and container pulls verify local image IDs, tags, and digests after stream completion.
12. Compose pulls resolve the configured service-image set before send and verify every selected image afterward.
13. Destructive operations, arbitrary command execution, file transfer, image deletion, pruning, builds, and Compose down remain outside this slice.

## Current Synapse adoption

The canonical Synapse runtime delegates all 35 read operations to `soma-fleet` and `soma-infra`. Ten of the 21 canonical mutations are now delegated:

- `container.start`, `container.stop`, `container.restart`, `container.pause`, and `container.resume`;
- `compose.up` and `compose.restart`;
- `docker.pull`, `container.pull`, and `compose.pull`.

Pull plans bind the exact authorized image artifact set. Pull execution emits canonical progress, verifies local content identities, and returns OCI artifact references plus runtime-state evidence. Eleven canonical mutations remain fail-closed.

## Verification

Required gates:

- default and all-feature unit tests;
- strict Clippy and warning-free rustdoc;
- lifecycle no-op, sent, unknown, cancelled, timeout, and failed-verification tests;
- Compose discrete-argv and nonzero-exit tests;
- pull progress, delivery-failure, image-reference drift, and artifact-verification tests;
- stale-host and revision-bound client tests;
- filesystem traversal and symlink rejection;
- workspace sibling, architecture, pattern, and product-leakage checks.
