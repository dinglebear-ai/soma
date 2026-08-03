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
21. Destructive operations, arbitrary command execution, file transfer, image deletion, pruning, and Compose down remain outside this slice.

## Current Synapse adoption

The canonical Synapse runtime delegates all 35 read operations to `soma-fleet` and `soma-infra`. The first mutation slice additionally delegates seven of the 21 canonical mutations:

- `container.start`;
- `container.stop`;
- `container.restart`;
- `container.pause`;
- `container.resume`;
- `compose.up`;
- `compose.restart`.

Fourteen canonical mutations remain fail-closed until later slices add their operation-specific planning, send-state, verification, and recovery semantics.

## Verification

Required gates:

- default and all-feature unit tests;
- strict Clippy and warning-free rustdoc;
- lifecycle no-op, sent, unknown, cancelled, timeout, and failed-verification tests;
- Compose discrete-argv and nonzero-exit tests;
- stale-host and revision-bound client tests;
- filesystem traversal and symlink rejection;
- workspace sibling, architecture, pattern, and product-leakage checks.
