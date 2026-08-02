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

`soma-infra` owns typed infrastructure semantics above `soma-fleet`. It converts bounded host and Docker transports into stable host, Docker, Compose, and filesystem read models.

It does not own product configuration, environment loading, authorization scopes, Flux/Scout routing, CLI/MCP/REST formatting, or operation policy.

## Dependencies

Required internal dependencies:

- `soma-ops` for cross-process timestamps and operation vocabulary;
- `soma-fleet` for host identity, topology revision, cancellation, and command execution.

Optional external drivers:

- Bollard for local Docker reads;
- strict OpenSSH Unix-socket forwarding for remote Docker reads;
- Rustix `openat2` for Linux descriptor-confined filesystem access.

## Public contracts

### Host

- `HostInspectRequest`;
- `HostInspection`;
- `HostIdentity`;
- `HostMemory`;
- `HostLoadAverage`;
- `HostInspector`;
- `CommandHostInspector`.

### Docker

- `DockerSystemReader`;
- `ContainerReader`;
- `ImageReader`;
- `NetworkReader`;
- `VolumeReader`;
- `DockerReadClient`;
- `DockerTelemetryReader`;
- neutral daemon, disk-usage, container, image, network, volume, log, and one-shot stats models;
- optional `BollardReadClient`.

### Compose

- `ComposeProjectRef`;
- `ComposeProject`;
- `ComposeStatus`;
- `ComposeConfig`;
- `ComposeLogRequest`;
- `ComposeLogs`;
- `ComposeInspector`;
- optional `CommandComposeInspector`.

### Process, logs, and ZFS

- `ProcessListRequest`, `ProcessSnapshot`, and `ProcessInspector`;
- `LogReadRequest`, `JournalFilters`, `LogRead`, and `LogReader`;
- `ZfsPoolRequest`, `ZfsDatasetRequest`, `ZfsSnapshotRequest`, `ZfsTable`, and `ZfsInspector`;
- optional fleet-backed command drivers for each domain.

### Filesystem

- `FileReadPolicy`;
- `FileMetadata`;
- `FilePreview`;
- `FileHash`;
- `FilesystemInspector`;
- optional `LinuxFilesystemInspector`.

## Security properties

1. Every result is bound to a host and topology revision.
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
18. No mutation operation is exposed by this slice.

## Initial donor disposition

This slice begins extraction of:

- `flux_service/host*`;
- read-only `docker_client` and `flux_service` Docker paths;
- `flux_service/compose*` read paths;
- `secure_path.rs` and Scout filesystem reads;
- container logs/stats and Docker data-usage reads;
- Compose log reads;
- Scout process, operating-system log, and ZFS reads.

The imported donor remains unchanged as historical source material. The canonical Synapse read runtime now delegates all 35 read operations to `soma-fleet` and `soma-infra`; no legacy result projection is retained. Mutations remain the next execution layer.

## Verification

Required gates:

- default and all-feature unit tests;
- process-backed host and Compose conformance;
- Docker SDK-shaped mapper fixtures;
- filesystem traversal and symlink rejection;
- preview truncation and hash ceiling tests;
- cancellation and non-zero command failure tests;
- process/ZFS parser and discrete-argv tests;
- journal option-smuggling and file-fallback tests;
- Docker usage/stat mapper and one-shot telemetry tests;
- strict Clippy and warning-free rustdoc;
- workspace sibling and architecture checks;
- no product or surface dependency leakage.
