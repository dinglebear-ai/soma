---
title: "soma-fleet"
created: 2026-08-01
updated: 2026-08-01
status: implemented
---

# soma-fleet

**Path:** `crates/shared/operations/fleet`
**Layer:** shared
**Package status:** private during extraction

## Purpose

`soma-fleet` is the neutral connectivity and topology engine below infrastructure operations. It resolves no product configuration and grants no authority. It accepts validated host records and bounded requests, then provides local, SSH, forwarding, transfer-lifecycle, connection-pool, and fanout mechanics.

```text
soma-ops <- soma-fleet <- soma-infra <- product adapters
```

## Owned models

- `HostId`, `CapabilityName`, `TopologyRevision`, and `PoolKey`;
- `HostEndpoint`, `SshEndpoint`, `HttpEndpoint`, `HostRecord`, and `TopologySnapshot`;
- bounded `CommandRequest`, `CommandOutput`, `TransferRequest`, and `TransferReceipt`;
- `FleetError`, lifecycle events, and event-sink port;
- stable-order fanout reports with success, failure, cancellation, and timeout states, including host/payload pairs and duplicate-host requests distinguished by input index;
- observable `TransferLifecycle` and RAII `TransferGuard` states.

## Ports

- `HostRepository` returns immutable snapshots;
- `ConnectionFactory` opens and explicitly closes revision-bound connections;
- `CommandExecutor` executes discrete-argument requests;
- `FileTransfer` transfers descriptor-confined path pairs;
- `FleetClock` supplies deterministic admission time;
- `FleetEventSink` receives product-neutral lifecycle events.

## Connection semantics

`HostRecord` derives its revision from serialized transport-affecting endpoint material. Deserialization recomputes that revision and rejects forged values. `ConnectionPool` keys cells by `HostId + TopologyRevision`, shares one cold-cache initialization among concurrent callers, removes failed initialization cells, evicts keys absent from a new snapshot, invalidates every revision for a host, and explicitly closes initialized connections at shutdown.

No pool key includes secrets. Identity, config, and known-host file paths are topology material; credential contents remain outside the contract.

## Command semantics

`CommandRequest` contains an executable, discrete arguments, optional absolute local working directory, optional bounded stdin bytes, an absolute deadline, and stdout/stderr byte ceilings. Stdin is capped at 64 MiB and never interpreted as shell syntax. Product command allowlists remain above this crate.

`LocalProcessDriver` uses `tokio::process::Command` without a shell, writes optional stdin concurrently with draining both output streams, retains bounded prefixes, kills local children on cancellation or timeout, and distinguishes pre-spawn cancellation from in-flight deadline expiry.

`OpenSshDriver` uses native multiplexing, always configures `KnownHosts::Strict`, supports explicit port/user/identity/config/known-host paths, uses owner-only control directories, passes arguments through escaped `Command::arg` semantics, writes optional bounded stdin concurrently with output draining, rejects remote working directories instead of synthesizing shell commands, bounds output and execution permits, and invalidates sessions after transport failures.

OpenSSH cannot guarantee termination of a remote process when its local child handle is dropped. Cancellation or timeout after spawn therefore returns `FleetError::RemoteCommandDetached`; callers must treat the remote process as potentially still running.

## Forwarding semantics

`ForwardedUnixSocket` is a Unix-only RAII guard over one local-to-remote socket forward. Local paths derive from topology revision material plus a process-local sequence, never from untrusted aliases. Runtime directories are owner-only mode `0700`. Before exposure, the endpoint must be a real owned Unix socket and is changed to mode `0600`. Explicit close and drop both remove the path and request teardown.

## Transfer semantics

`TransferRequest` uses absolute normalized source and destination paths, a hard byte ceiling, and an absolute deadline. `TransferLifecycle` exposes a cloneable observer while `TransferGuard` records chunks and terminal state. It rejects overflow, overrun, receipt mismatch, duplicate terminal transitions, and invalid failure detail. Dropping a nonterminal guard records `Abandoned`.

Concrete infrastructure-aware file semantics remain in `soma-infra`; implementations consume the `FileTransfer` port and lifecycle guard. The final Synapse transfer driver uses bounded command stdin to deliver destination bytes without ambient `scp`, shell strings, or untracked temporary files.

## Fanout semantics

`FanoutScheduler` enforces nonzero concurrency and per-target timeout bounds. It supports both one operation per host and distinct host/payload pairs, including repeated host identities whose requests remain separated by their original index. It uses bounded unordered execution internally but restores original target order. Every admitted target produces exactly one terminal classification, reports can be consumed without cloning opaque failures, and shared cancellation accounts for running and queued targets rather than dropping them.

## Forbidden responsibilities

`soma-fleet` must not read product environment variables or configuration files, choose discovery precedence or default targets, define scopes/confirmations/allowlists, depend on product or surface crates, implement infrastructure-domain semantics, or convert transport success into operation verification.

## Verification evidence

The deterministic suite covers identity and forged revisions, endpoint changes, stale pooled connections, concurrent single-connect initialization, shutdown, argument injection, bounded stdin/output, local cancellation/timeout, strict OpenSSH plans, fail-closed SSH working directories, runtime/socket ownership and permissions, file/symlink rejection, transfer digest exposure, overrun/mismatch/cancellation/failure/abandonment, and bounded fanout with partial success, timeout, cancellation, overload prevention, stable order, duplicate-host payloads, and consuming reports.

Live SSH and host-key mismatch tests remain environment-gated product verification; deterministic tests prove the driver can construct only strict-host-key plans.
