---
title: "soma-infra"
created: 2026-08-01
updated: 2026-08-03
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

### Artifact builds

- `BuildContextPolicy`, `BuildContextInspector`, and deterministic `BuildContextFingerprint`;
- descriptor-walking local or SSH context hashing with `O_NOFOLLOW` on every path segment;
- explicit context roots plus file-count and byte ceilings;
- `ImageBuildMutator`, `ImageBuildEngine`, `ComposeBuildMutator`, and `ComposeBuildEngine`;
- shell-free Docker and Compose build commands with bounded logs and phase progress;
- exact context-digest binding at plan and pre-send execution boundaries;
- independent output-tag verification with OCI artifact and source-context evidence.

### Replacement mutations

- `ContainerRecreateInspector`, `ContainerRecreateMutator`, and `ContainerRecreateEngine`;
- a driver-native SHA-256 over replacement-relevant Docker configuration without leaking Bollard models;
- optional image pull plus stop, remove, create, and start stages with conservative post-send uncertainty;
- recreation preserves image, environment, command, entrypoint, labels, working directory, user, volumes, host configuration, and network attachments;
- `ComposeRecreateMutator` and `ComposeRecreateEngine`;
- normalized Compose configuration plus service pre-state fingerprinting;
- shell-free `compose up -d --force-recreate` with bounded output;
- independent verification of the exact configured running, healthy, zero-exit service set.

### Bounded execution

- `ContainerExecRequest`, `ContainerExecMutator`, and host-bound `ContainerExecClientProvider`;
- direct non-TTY Docker exec argv with separate 96 KiB stdout and stderr ceilings plus inspected exit status;
- `HostExecCommand`, `HostExecPolicy`, and `CommandHostExec`;
- a fixed read-oriented command allowlist with typed option grammars and helper-executing option rejection;
- explicit per-host read roots, `O_NOFOLLOW` descriptor traversal, inherited `/proc/self/fd` operands, and descriptor-bound working directories;
- `HostExecManyEngine` with bounded concurrency, deterministic target order, conservative aggregate send state, partial results, and a bounded aggregate output envelope.

### Cleanup, teardown, and transfer

- `DockerCleanupEngine` binds one resolved image identity or one deterministic prune inventory before send;
- Bollard image removal and fixed-order container/image/volume/network/build-cache prune drivers retain conservative send state and exact deletion receipts;
- `ComposeDownEngine` binds normalized configuration plus the complete service pre-state and verifies no services remain after shell-free teardown;
- `FileTransferEngine` binds both host revisions, source bytes/SHA-256, destination pre-state, and destination path;
- `CommandFileTransfer` supports local or strict-SSH endpoints with descriptor-walking `O_NOFOLLOW` source reads and destination writes, a 16 MiB operation ceiling, lifecycle accounting, and independent destination digest verification.

## Security properties

1. Every target-specific model is bound to a host and exact topology revision.
2. Host and Compose commands use discrete argv values through `soma-fleet`.
3. Compose project and service identifiers are bounded and validated before spawn.
4. Linux filesystem reads use `openat2` with `BENEATH`, `NO_SYMLINKS`, and `NO_MAGICLINKS`.
5. Filesystem access requires explicit absolute read roots.
6. File preview and hash byte limits are enforced.
7. Docker clients reject a host or revision different from their construction binding.
8. Bollard-generated models remain private to the adapter.
9. The local Bollard adapter uses only the default daemon socket and cannot override the daemon independently of the host binding.
10. Docker list results reject more than 10,000 items or an item larger than 256 KiB of JSON.
11. Cancellation is accepted at every asynchronous driver boundary.
12. Docker log reads are one-shot, byte-bounded, and filter locally.
13. Remote Docker clients own private Unix-socket forwards and exact-revision pooled SSH connections.
14. Remote filesystem queries open every path segment with `O_NOFOLLOW` and receive user values only through argv.
15. Journal unit and time values reject option smuggling before argv construction.
16. Process sorting and ZFS dataset types are allowlisted.
17. dmesg permission failures return structured operator guidance.
18. Product authorization is deliberately absent from this shared crate.
19. Mutation drivers preserve whether a backend call was not sent, sent, or uncertain.
20. A successful backend response is not promoted to mutation success until a separate read verifies the postcondition.
21. Pull streams emit canonical bounded progress while retaining progress-delivery failures separately from execution truth.
22. Docker and container pulls verify local image IDs, tags, and digests after stream completion.
23. Compose pulls resolve the configured service-image set before send and verify every selected image afterward.
24. Build contexts reject symlinks, special files, root escapes, and configured file or byte ceiling violations.
25. Build plans bind the exact source-context SHA-256 and output image artifact set.
26. Build execution re-fingerprints contexts before send and verifies every output tag afterward.
27. Container recreate plans bind the exact replacement configuration digest and image-pull choice.
28. Container recreate rechecks the digest immediately before removal and records the furthest completed destructive stage.
29. Compose recreate plans bind normalized configuration and service pre-state, then verify the exact healthy post-state.
30. Container exec uses direct argv without a shell or TTY, crosses the uncertain send boundary only at `start_exec`, and inspects the final exit status.
31. Host exec admits only the fixed read-oriented command set and typed options; filesystem operands and working directories are descriptor-bound beneath explicit roots.
32. Host fanout preserves deterministic target order, retains every partial result, bounds concurrency and aggregate output, and requires replanning unresolved targets rather than blind batch retry.
33. Image removal plans bind the requested reference plus resolved local ID, tags, and digests; verification requires both reference and content identity absence.
34. Prune plans bind exact candidate identities and build-cache usage; `all` executes a fixed five-scope order and verification rejects any reported identity that remains.
35. Compose down binds normalized configuration plus the complete service set, rejects volume removal without explicit force, and verifies no services remain.
36. File transfer binds both host revisions, source content identity, destination pre-state, and destination path before send.
37. Transfer paths remain beneath explicit source/destination roots, reject symlink escapes with `O_NOFOLLOW`, cap content at 16 MiB, and require matching source/destination SHA-256 evidence.

## Current Synapse adoption

The canonical Synapse runtime delegates all 35 read operations and all 21 canonical mutations to `soma-fleet` and `soma-infra`:

- `container.start`, `container.stop`, `container.restart`, `container.pause`, and `container.resume`;
- `compose.up`, `compose.down`, `compose.restart`, and `compose.recreate`;
- `docker.pull`, `docker.build`, `docker.rmi`, and `docker.prune`;
- `container.pull`, `container.recreate`, and `container.exec`;
- `compose.pull` and `compose.build`;
- `host.exec` and `host.exec_many`;
- `files.transfer`.

Pull plans bind exact image artifacts. Build plans bind exact context SHA-256 values and output tags. Replacement and teardown plans bind exact configuration and service pre-state digests. Execution plans bind direct argv, users, paths, timeouts, topology revisions, and normalized fanout target order. Cleanup plans bind exact image or prune inventories. Transfer plans bind both endpoints and content identities. The canonical runtime now covers all 59 operations with no fail-closed catalog gaps.

## Verification

Required gates:

- default and all-feature unit tests;
- strict Clippy and warning-free rustdoc;
- lifecycle no-op, sent, unknown, cancelled, timeout, and failed-verification tests;
- Compose discrete-argv and nonzero-exit tests;
- pull progress, delivery-failure, image-reference drift, and artifact-verification tests;
- build-context determinism, symlink rejection, context drift, bounded argv/logs, and output-verification tests;
- replacement fingerprint drift, pull-choice binding, partial-stage, force-recreate argv, and post-state verification tests;
- direct container argv, pre/post-start send-state, descriptor-bound host operands, symlink escape rejection, nonzero exits, bounded output, and stable partial fanout tests;
- exact image-removal identity, fixed-order prune receipts, candidate drift, Compose down service-set verification, force-gated volume removal, bounded command stdin, real local file copy, digest parity, and destination symlink-escape tests;
- stale-host and revision-bound client tests;
- filesystem traversal and symlink rejection;
- workspace sibling, architecture, pattern, and product-leakage checks.
