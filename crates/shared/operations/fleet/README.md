# soma-fleet

`soma-fleet` is the product-neutral connectivity layer for Soma's operations family. It owns validated host topology, revision-bound connection pooling, bounded command and transfer contracts, cancellation-aware fanout, strict OpenSSH execution, private Unix-socket forwarding, and lifecycle evidence.

It deliberately does not own product configuration precedence, authorization scopes, command allowlists, infrastructure semantics, CLI/MCP/REST presentation, or environment-variable loading.

## Features

- default: topology, ports, request/result models, events, cache/pool, fanout, and transfer lifecycle;
- `process-driver`: bounded local process execution for local hosts and conformance tests;
- `openssh-driver` on Unix: strict known-host OpenSSH connection planning, native multiplexing, pooled execution, and owner-only Unix-socket forwarding.

## Safety invariants

- every connection key binds host identity to a SHA-256 topology revision;
- endpoint or credential-path changes cannot reuse stale sessions;
- SSH host-key policy is always strict;
- command arguments remain discrete escaped argv values;
- SSH working directories fail closed instead of synthesizing shell commands;
- output, transfer, fanout, and deadline bounds are explicit;
- every fanout target receives a terminal outcome in input order;
- dropped transfer guards become `Abandoned`;
- post-spawn SSH cancellation or timeout becomes `RemoteCommandDetached`, because the remote process may still be running;
- forwarded sockets live in owner-only directories and are verified as owned Unix sockets before exposure.

## Verification

```bash
cargo test -p soma-fleet --all-features
cargo clippy -p soma-fleet --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -p soma-fleet --all-features --no-deps
```
