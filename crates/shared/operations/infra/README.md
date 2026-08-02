# soma-infra

`soma-infra` is the product-neutral infrastructure engine above `soma-fleet`. It provides typed read contracts plus verified mutation coordinators without owning Synapse routing, product authorization, configuration precedence, or surface formatting.

## Read surface

- host identity, uptime, resources, services, network, mounts, ports, filesystem usage, and doctor checks;
- Docker daemon, disk usage, containers, images, networks, volumes, logs, stats, and container process tables;
- local and strict-SSH Bollard clients bound to one exact host topology revision;
- Compose discovery, status, normalized configuration, and bounded logs;
- typed processes, system logs, and ZFS tables;
- descriptor-confined local and remote file, directory, tree, find, tail, stat, preview, and hash operations.

## Mutation surface

The first mutation slice adds:

- explicit `MutationFailure` values that retain `NotSent`, `Sent`, or `Unknown` backend send state;
- bounded postcondition verification policies;
- verified container `start`, `stop`, `restart`, `pause`, and `resume`;
- verified Compose `up -d` and `restart`;
- local and strict-SSH Docker mutation clients;
- process-backed Compose mutation commands with discrete argv.

The shared crate does not authorize mutations. Product runtimes must bind a deterministic plan, authorization evidence, exact target, and topology revision before invoking these drivers.

## Feature flags

- `process-driver`: command-backed Compose, process, log, ZFS, and Compose mutation support;
- `bollard-driver`: local Docker reads and container lifecycle mutations;
- `remote-bollard`: strict-SSH Docker Unix-socket forwarding and pooled remote clients;
- `linux-filesystem`: Linux `openat2` filesystem inspection.

The default build exposes neutral models, traits, coordinators, and deterministic validation without concrete drivers.

## Safety invariants

- all target-specific results carry host identity and exact topology revision;
- no shell command strings are constructed;
- filesystem reads remain descriptor-confined beneath explicit roots;
- Docker clients reject host or topology revision drift;
- mutation cancellation and timeout preserve uncertainty after the backend send boundary;
- a successful backend call is not reported as mutation success until an independent read verifies the postcondition;
- already-satisfied container states return verified no-op outcomes without a backend send;
- Compose success requires a nonempty service set with every reported service running, healthy, and exit code zero;
- SDK-specific Bollard types never cross the public API.

## Verification

```bash
cargo test -p soma-infra
cargo test -p soma-infra --all-features
cargo clippy -p soma-infra --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc -p soma-infra --all-features --no-deps
```
