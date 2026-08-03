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

The mutation surface includes:

- explicit `MutationFailure` values that retain `NotSent`, `Sent`, or `Unknown` backend send state;
- bounded postcondition verification policies;
- verified container `start`, `stop`, `restart`, `pause`, and `resume`;
- verified Compose `up -d` and `restart`;
- verified Docker, container-image, and Compose image pulls;
- context-bound verified Docker and Compose image builds;
- configuration-bound verified container and Compose replacements;
- descriptor-confined context fingerprints with explicit root, file-count, and byte ceilings;
- canonical bounded phase progress and build logs whose delivery failures do not rewrite execution truth;
- OCI artifact references and local image-ID/digest verification;
- local and strict-SSH Docker mutation clients;
- process-backed Compose mutation commands with discrete argv.

The shared crate does not authorize mutations. Product runtimes must bind a deterministic plan, authorization evidence, exact target, and topology revision before invoking these drivers.

## Feature flags

- `process-driver`: command-backed Compose, process, log, ZFS, lifecycle mutation, artifact pull, context fingerprint, image build, and Compose replacement support;
- `bollard-driver`: local Docker reads, container lifecycle and replacement mutations, and image-pull streams;
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
- image pulls verify that requested references resolve to local content identities after stream completion;
- Compose pulls verify every selected configured service image;
- image builds re-fingerprint every context immediately before send and reject source drift as `NotSent`;
- build contexts reject symlinks and special files and bind relative paths, modes, sizes, and content into SHA-256;
- Docker and Compose builds verify each requested output tag through the local image store;
- container replacement captures and fingerprints image, env, command, entrypoint, labels, volumes, host config, and network attachments before removal;
- container replacement rechecks configuration immediately before destructive send and reports the furthest completed stage;
- Compose replacement binds normalized configuration and service pre-state, then verifies the exact healthy service set after force-recreate;
- progress sink failures remain bounded metadata and never change backend send or verification truth;
- SDK-specific Bollard types never cross the public API.

## Verification

```bash
cargo test -p soma-infra
cargo test -p soma-infra --all-features
cargo clippy -p soma-infra --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc -p soma-infra --all-features --no-deps
```
