---
title: "Rust Build Setup"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "family"
source_of_truth: true
last_reviewed: "2026-07-13"
---

# Rust Build Setup

This is the canonical build-setup reference for the rmcp server family:
`soma`, `labby`, `axon`, `cortex`, `unifi-rmcp`, `gotify-rmcp`,
`apprise-rmcp`, `tailscale-rmcp`, and `unraid-rmcp`.

All family repos share a common Cargo configuration model: heavy lifting lives
in `~/.cargo/config.toml` on the developer's machine; per-repo
`.cargo/config.toml` files are kept minimal and contain only what the global
config cannot express (xtask alias, repo-specific linker overrides).

---

## System prerequisites

| Tool | Purpose | Install |
|------|---------|---------|
| Rust stable ≥ 1.96 | Compiler | `rustup update stable` |
| `clang` | Linker driver for the mold integration | `apt install clang` |
| `mold` | High-speed linker; 5-10× faster than GNU `ld` on Linux | `apt install mold` |
| `mingw-w64` | Cross-compiler for `x86_64-pc-windows-gnu` targets | `apt install mingw-w64` |
| `just` | Command runner (optional, but used by all Justfile recipes) | `cargo install just` |

`clang` and `mold` are required for fast Linux incremental builds. Without
them the global config falls back to the system linker; builds still work but
link times are significantly slower on large dependency graphs.

`mingw-w64` is only needed for local Windows cross-compilation. CI installs
it automatically.

---

## Global Cargo config (`~/.cargo/config.toml`)

All family repos assume the following global configuration on the developer's
machine. **This file is not committed to any repo** — it lives only in
`~/.cargo/config.toml`.

```toml
# sccache is enabled globally. The user service owns the long-lived server; keep
# dev incremental disabled so Rust artifacts are cacheable across worktrees.

[build]
# Fallback for callers that bypass ~/.local/bin/cargo. The cargo wrapper computes
# CARGO_BUILD_JOBS dynamically from the active build count, so a solo wrapper-run
# build gets more parallelism while concurrent builds divide the CPU budget.
jobs = 8
# Wrapper, not sccache directly: resolves per-project rustup toolchains and
# passes -vV/--print probes straight to rustc.
rustc-wrapper = "/home/jmagar/.local/bin/sccache-wrapper"

[env]
SCCACHE_SERVER_UDS = "/home/jmagar/.local/state/sccache/sccache.sock"
SCCACHE_BASEDIRS = "/home/jmagar/workspace:/home/jmagar/.codex/worktrees"

[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"

[profile.dev]
debug = 1
codegen-units = 8
split-debuginfo = "unpacked"
incremental = false
opt-level = 0

[profile.test]
debug = 1
codegen-units = 8

[profile.dev.package."*"]
opt-level = 1
```

### Why mold?

`mold` replaces GNU `ld` as the linker for Linux builds. On large Rust
workspaces with many crates and dependencies, the link step dominates
incremental rebuild times. `mold` is typically 5–10× faster than `ld` and
2–3× faster than `lld`.

The global `[target.x86_64-unknown-linux-gnu]` block activates it via
`-fuse-ld=mold`. All family repos inherit this automatically — no per-repo
config is needed. **Do not add `rustflags` to `[target.x86_64-unknown-linux-gnu]`
in a per-repo config** — that would replace, not extend, the global rustflags
and silently drop the mold flag.

### Why sccache globally?

The host-level Cargo config enables `sccache-wrapper` once, and the user systemd
service owns the long-lived cache daemon. That keeps dependency compilation
cacheable across all worktrees without asking each repo to carry its own wrapper
hook. The local `/home/jmagar/.local/bin/cargo` shim computes
`CARGO_BUILD_JOBS` dynamically: a single build gets most cores, while concurrent
builds divide the CPU budget so the host stays usable.

### Profile settings rationale

| Setting | Value | Rationale |
|---------|-------|-----------|
| `profile.dev.debug` | `1` | Line tables only — enough for backtraces, without the 3× binary-size penalty of full DWARF |
| `profile.dev.codegen-units` | `8` | Parallelises compilation within a crate; 8 balances parallelism and optimisation quality |
| `profile.dev.split-debuginfo` | `"unpacked"` | Keeps debug info in separate `.dwo` files, reducing link-step memory pressure |
| `profile.dev.incremental` | `false` | Keeps dev artifacts cacheable by sccache across repos and worktrees |
| `profile.dev.opt-level` | `0` | No optimisation for the crate under active development |
| `profile.dev.package."*".opt-level` | `1` | Light optimisation for dependencies — prevents debug-only slowness in heavy crates like `serde` and `tokio` |
| `profile.test.debug` | `1` | Same as dev — enough for test failure backtraces |
| `profile.test.codegen-units` | `8` | Same rationale as dev |

---

## Per-repo `.cargo/config.toml`

Each family repo has a minimal `.cargo/config.toml`. The rule is: **only put
settings here that the global config cannot provide**.

### What belongs here

```toml
[alias]
# Required if the repo has an xtask/ crate.
xtask = "run --package xtask --"

[target.x86_64-pc-windows-gnu]
# Only if the repo cross-compiles for Windows and the global config may not
# be present (e.g. in CI without the standard global config).
linker = "x86_64-w64-mingw32-gcc"
```

### What does NOT belong here

| Setting | Reason to keep it in the global config |
|---------|---------------------------------------|
| Profile settings (`debug`, `codegen-units`, etc.) | Already set globally; duplicating causes confusion when the global changes |
| `build.jobs` | Machine-specific; the global config tunes it per host |
| `[target.x86_64-unknown-linux-gnu].rustflags` | Overriding this drops the mold flag from the global config |
| `build.rustc-wrapper` for generic artifact sync | Generic repos must use explicit sync commands; hidden post-compile copies do not belong in Cargo config |

### Repos without an xtask crate

Repos without an `xtask/` crate either omit `.cargo/config.toml` entirely or
keep only documented repo-specific overrides.

---

## Repo-specific overrides

Some repos intentionally diverge from the global config for documented reasons:

| Repo | Override | Reason |
|------|----------|--------|
| `axon` | `build.rustc-wrapper = "scripts/cargo-rustc-wrapper"` | Automatically refreshes the actively used local `axon` binary and named repo artifacts after successful bin builds |
| `cortex` | `build.rustc-wrapper = "scripts/cargo-rustc-wrapper"` and `build.target-dir = ".cache/cargo"` | Release-only local `~/.local/bin/cortex` refresh plus a non-root target directory for Docker bind mounts |
| `lab` | `build.rustc-wrapper = "scripts/cargo-rustc-wrapper"` | Keeps the active `labby` binary fresh for the local gateway/operator workflow |

If you add a new legitimate per-repo override, document it in the repo's
`docs/RUST.md` and add a row to this table in soma's `docs/RUST.md`.

---

## Explicit artifact sync

Generic rmcp repos do not use repo-local `rustc-wrapper` hooks for artifact
sync. Build normally with Cargo, then run an explicit recipe when you want to
refresh checked-in or plugin-local binaries:

```bash
just sync-bin
```

For repos with bundled plugin binaries, `sync-bin` delegates to
`just build-plugin`. In Soma itself, plugins launch the installed PATH binary,
so `sync-bin` delegates to `just install-local`.

The global mise config also exposes a cross-repo dispatcher:

```bash
mise run cargo:sync-bin
```

That task calls `just sync-bin` when present, falls back to `just build-plugin`,
and fails loudly in repos with no explicit artifact-sync recipe.

---

## Windows cross-compilation

Repos that publish Windows binaries configure the mingw linker. The global
`~/.cargo/config.toml` already sets this; per-repo configs set it as a
fallback for CI environments that may not have the standard global config.

```toml
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
```

Add the Windows target if it is not already installed:

```bash
rustup target add x86_64-pc-windows-gnu
```

Install the cross-compiler on Debian/Ubuntu build hosts:

```bash
apt install mingw-w64
```

## Native Windows CI builds

PR CI also builds on native Windows through `.github/workflows/ci.yml`. This is
separate from Linux-to-Windows cross-compilation:

- cross-compilation is useful for tag-time packaging when dependencies support it
- native Windows CI catches Windows runtime, path, shell, and MSVC issues earlier
- self-hosted Windows runners must be audited for user-level Cargo config

The Windows CI job sets explicit portable CPU flags:

```powershell
$env:RUSTFLAGS = "-C target-cpu=x86-64 -C target-feature=-avx512f,-avx512vl,-avx512bw,-avx512dq,-avx512cd,-avx512ifma,-avx512vbmi,-avx512vbmi2,-avx512vnni,-avx512bitalg,-avx512vpopcntdq"
```

Keep machine-specific optimization out of committed config. In particular, do
not add this to repo or runner-wide config for artifacts that will be shared:

```toml
[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-cpu=native"]
```

For self-hosted runner setup, labels, required tools, artifact smoke testing,
and Cargo config audits, see `docs/WINDOWS-RUNNER.md`.

---

## Quick verification

Run these after cloning to confirm the build environment is correctly wired:

```bash
# Verify mold is in use (should show "mold" in the link invocation)
cargo build -v 2>&1 | grep "link-arg"

# Verify the xtask alias works (if the repo has xtask/)
cargo xtask --help

# Verify Windows cross-compile target is installed
rustup target list --installed | grep windows-gnu
```
