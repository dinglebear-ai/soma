# soma-infra

`soma-infra` is the product-neutral, read-only infrastructure engine above `soma-fleet`. It provides typed host, Docker, Compose, and filesystem inspection contracts without owning Synapse routing, product authorization, configuration precedence, or surface formatting.

## Current read surface

- Linux host identity, uptime, memory, and load through a fleet command executor;
- Docker daemon, disk-usage, container, image, network, volume, logs, and one-shot stats reads through segregated traits;
- local and strict-SSH Bollard clients bound to one host topology revision;
- Compose project listing, status, normalized configuration, and bounded logs through discrete `docker compose` arguments;
- typed process snapshots with allowlisted sort fields and local filters;
- bounded syslog, journal, dmesg, and auth-log reads with validated journal filters;
- ZFS pool, dataset, and snapshot tables with validated targets and types;
- Linux filesystem stat, bounded preview, and SHA-256 hashing through descriptor-confined `openat2`;
- local and remote file, directory, tree, find, and tail queries through descriptor-walking helpers;
- host services, network, mounts, listening ports, filesystem usage, and doctor checks.

## Feature flags

- `process-driver`: command-backed Compose, process, log, and ZFS support;
- `bollard-driver`: local Docker API reads;
- `remote-bollard`: strict-SSH Docker Unix-socket forwarding and pooled remote clients;
- `linux-filesystem`: Linux `openat2` filesystem inspection.

The default build exposes only neutral models and traits.

## Safety invariants

- all results carry host identity and exact topology revision;
- no shell command strings are constructed;
- Compose config paths are absolute and normalized;
- filesystem reads are restricted to explicit roots and reject symlinks, magic links, and traversal;
- preview and hash byte ceilings are explicit;
- Docker clients reject host or topology revision drift;
- local Docker connections use the default daemon socket, so daemon identity cannot drift outside the host binding;
- Docker list results are capped at 10,000 items and 256 KiB of JSON per item;
- cancellation is propagated through fleet commands and Docker API calls;
- SDK-specific Bollard types never cross the public API.

Mutations, product policy, continuous streaming, and product surface wiring remain outside this shared crate. Canonical Synapse reads now consume this complete read surface directly.

## Verification

```bash
cargo test -p soma-infra
cargo test -p soma-infra --all-features
cargo clippy -p soma-infra --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc -p soma-infra --all-features --no-deps
```
