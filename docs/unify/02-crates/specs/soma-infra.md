---
title: "soma-infra"
created: 2026-08-01
updated: 2026-08-01
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
- Rustix `openat2` for Linux descriptor-confined filesystem access.

## Public contracts

### Host

- `HostInspectRequest`;
- `HostInspection`;
- `HostIdentity`;
- `HostMemory`;
- `HostLoadAverage`;
- `HostInspector`;
- `LinuxCommandHostInspector`.

### Docker

- `DockerSystemReader`;
- `ContainerReader`;
- `ImageReader`;
- `NetworkReader`;
- `VolumeReader`;
- `DockerReadClient`;
- neutral daemon, container, image, network, and volume models;
- optional `BollardReadClient`.

### Compose

- `ComposeProjectRef`;
- `ComposeProject`;
- `ComposeStatus`;
- `ComposeConfig`;
- `ComposeInspector`;
- optional `CommandComposeInspector`.

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
13. Journal unit and time values reject option smuggling before argv construction.
14. Process sorting and ZFS dataset types are allowlisted.
15. dmesg permission failures return structured operator guidance.
16. No mutation operation is exposed by this slice.

## Initial donor disposition

This slice begins extraction of:

- `flux_service/host*`;
- read-only `docker_client` and `flux_service` Docker paths;
- `flux_service/compose*` read paths;
- `secure_path.rs` and Scout filesystem reads.

The imported donor remains unchanged. Synapse does not cut over in this PR. Differential surface projection and remaining reads follow in later stacked slices.

## Verification

Required gates:

- default and all-feature unit tests;
- command-request and parser conformance through bounded mock executors;
- Docker JSON compatibility mapper fixtures;
- filesystem traversal and symlink rejection;
- preview truncation and hash ceiling tests;
- cancellation and non-zero command failure tests;
- strict Clippy and warning-free rustdoc;
- workspace sibling and architecture checks;
- no product or surface dependency leakage.
