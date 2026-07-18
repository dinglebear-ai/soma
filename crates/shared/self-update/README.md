# soma-self-update

`soma-self-update` is a standalone binary self-update transaction for Rust services. It has zero path-dependencies on the Soma workspace and can be copied into another repository wholesale.

## Scope

The crate owns artifact integrity checks, bounded staging, executable validation,
atomic Unix installation, durable confirmation state, and rollback.

The default policy streams at most 128 MiB and gives a staged executable 10
seconds to answer `--version`. Validation requires the advertised version as an
exact output token rather than a substring. Install captures the validated
artifact identity and rechecks its digest, device, and inode under the
transaction lock, including a final check immediately before replacement.

## Non-goals

Release discovery, HTTP and authentication, signature format and key
management, service orchestration, server-side artifact hosting, and automatic
Cortex or Soma integration remain adopter responsibilities.

## Safety boundary

Authenticate the directive independently or verify a detached signature before
starting an update. A same-channel SHA-256 digest does not authenticate a
hostile publisher. Verify any detached signature before calling `stage`.

Artifacts are same-origin by default and HTTPS-only. Loopback HTTP is available
only through explicit `HttpsOrLoopbackHttp` policy. URL references use URL
semantics, so endpoint `https://host/v1/heartbeats` plus
`/v1/agent/binary` resolves to `https://host/v1/agent/binary`, not a path under
the heartbeat endpoint. Transport adapters must disable automatic redirects or
call `validate_artifact_response_url` for every redirect target and the final
response URL; validating only the initial URL does not constrain a client's
redirect behavior.

## Platform support

Transport-neutral directive, staging, and validation APIs compile everywhere.
Validator timeouts terminate the full Unix process group or Windows Job Object,
including descendants that inherit the output pipes. Cancelling or dropping a
validation future triggers the same process-tree termination guard.
The included atomic installer and re-exec adapter support Unix only.
Non-Unix adopters can use directive, staging, and validation but the provided
installer reports `UnsupportedPlatform`; supply a platform-specific deployment
strategy there.

## API lifecycle

Construct an `UpdateDirective`, stage and validate its artifact, install it,
restart, recover pending state on startup, and confirm only after the new
service reports healthy.

The configured executable leaf must be a regular path, not a symlink. The
crate rejects leaf symlinks before staging or recovery so the staging grammar,
marker identity, backup identity, and installed target cannot diverge. Symlinked
parent directories are canonicalized consistently.

`recover_on_startup` also reclaims prior-process staging and orphan rollback
files only when their exact target-derived grammar, regular-file type,
directory, Unix owner, and dead creator PID prove they are stale crate-owned
artifacts. Live concurrent stages are preserved. Protected identities are
canonicalized, matching symlinks are never followed, and another executable's
files are untouched. Calling startup recovery before each service loop therefore
bounds crash leftovers across process restarts. Marker input is capped at 64
KiB. Failed staging explicitly reports both the operation and cleanup error;
automatic `Drop` cleanup is reserved as a best-effort cancellation fallback.

Installation derives its advisory lock from the canonical state identity. The
executable directory and state directory must not be writable by untrusted
principals; no pathname-based installer can close the final metadata-to-rename
race against an attacker who controls that directory. Installation writes and
syncs a durable marker with explicit `prepared`, `installed`, `rolling_back`,
and `rolled_back` phases. Marker replacement uses one deterministic
lock-protected `<state>.tmp` sibling; startup recovery validates and reclaims an
effective-user-owned regular-file leftover before reading transaction state.
Serialized state is capped at the same 64 KiB limit on both writes and reads.
Before backup creation or executable replacement, installation preflights the
largest reachable phase and attempt-count representation so later recovery
writes cannot outgrow that cap. Generated rollback paths are
checked against executable, state, lock, marker-temporary, and staged identities
before the backup is created.
The transaction retains a unique rollback backup, syncs the backup and its
directory before the marker may reference it, then atomically renames the
verified artifact. Unix
staging preserves the existing executable mode (falling back to restrictive
`0700` only when no target exists); copy-based rollback backups preserve that
same mode. `BackupStrategy::Copy` is available when an adopter cannot use hard
links or wants to exercise the copy path explicitly.
A process crash at any marker, swap, or rollback boundary is completed or
aborted idempotently by startup recovery. Each unconfirmed startup increments
the marker; after the configured threshold the digest-verified backup is
restored and the adopter must restart again. Successful
health confirmation durably removes the authoritative marker before cleaning
the backup, so a cleanup interruption can leave only a harmless orphan backup.
A running-version mismatch retains both marker and backup and returns a typed
error; an operator must inspect that identity mismatch before explicitly
removing recovery state. Corrupt markers, missing
backups, and cleanup failures are typed errors and retain diagnostic state where
possible. Operators should stop competing updater processes before repairing a
reported marker or backup path.

## Cortex extraction map

- Cortex `AgentUpdateDirective` maps to `UpdateDirective`.
- Cortex reqwest and bearer authentication stay in Cortex caller code.
- Cortex `maybe_update` splits into `stage` → `validate` → `install` → caller
  restart/re-exec.
- The first successful heartbeat calls `confirm_success`; startup calls
  `recover_on_startup` before entering the heartbeat loop.

See `examples/heartbeat_agent.rs` for a compile-checked lifecycle. The library
never reads global process arguments. Pass the arguments to preserve to
`restart_command` or `reexec`, or let a supervisor such as systemd restart the
service after `RestartRequired`.
