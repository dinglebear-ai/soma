# soma-infra

`soma-infra` is the product-neutral infrastructure engine above `soma-fleet`. It provides typed read contracts plus verified mutation coordinators without owning Synapse routing, product authorization, configuration precedence, or surface formatting.

## Read surface

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

## Mutation surface

The first mutation slice adds:

- explicit `MutationFailure` values that retain `NotSent`, `Sent`, or `Unknown` backend send state;
- bounded postcondition verification policies;
- verified container `start`, `stop`, `restart`, `pause`, and `resume`;
- verified Compose `up -d` and `restart`;
- verified Docker, container-image, and Compose image pulls;
- canonical bounded progress events whose delivery failures do not rewrite execution truth;
- OCI artifact references and local image-ID/digest verification;
- local and strict-SSH Docker mutation clients;
- process-backed Compose mutation commands with discrete argv.

The shared crate does not authorize mutations. Product runtimes must bind a deterministic plan, authorization evidence, exact target, and topology revision before invoking these drivers.

## Feature flags

- `process-driver`: command-backed Compose, process, log, ZFS, lifecycle mutation, and Compose pull support;
- `bollard-driver`: local Docker reads, container lifecycle mutations, and image-pull streams;
- `remote-bollard`: strict-SSH Docker Unix-socket forwarding and pooled remote clients;
- `linux-filesystem`: Linux `openat2` filesystem inspection.

The default build exposes neutral models, traits, coordinators, and deterministic validation without concrete drivers.

## Safety invariants

- all target-specific results carry host identity and exact topology revision;
- no shell command strings are constructed;
- filesystem reads remain descriptor-confined beneath explicit roots;
- Docker clients reject host or topology revision drift;
- local Docker connections use the default daemon socket, so daemon identity cannot drift outside the host binding;
- Docker list results are capped at 10,000 items and 256 KiB of JSON per item;
- cancellation is propagated through fleet commands and Docker API calls;
- mutation cancellation and timeout preserve uncertainty after the backend send boundary;
- a successful backend call is not reported as mutation success until an independent read verifies the postcondition;
- already-satisfied container states return verified no-op outcomes without a backend send;
- Compose success requires a nonempty service set with every reported service running, healthy, and exit code zero;
- image pulls verify that requested references resolve to local content identities after stream completion;
- Compose pulls verify every selected configured service image;
- progress sink failures remain bounded metadata and never change backend send or verification truth;
- SDK-specific Bollard types never cross the public API.

## Verification

```bash
cargo test -p soma-infra
cargo test -p soma-infra --all-features
cargo clippy -p soma-infra --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc -p soma-infra --all-features --no-deps
```
