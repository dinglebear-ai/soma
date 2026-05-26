---
title: "Windows CI Runner"
doc_type: "guide"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "template"
source_of_truth: false
last_reviewed: "2026-05-22"
---

# Windows CI Runner

This guide explains the Linux + Windows build workflow and the Windows runner
setup used by repos derived from `rmcp-template`.

The template can run on GitHub-hosted runners, but this repo's Windows job is
currently wired to the steamy self-hosted runner:

- `build-linux`: `ubuntu-latest`, builds `target/release/example` and `target/release/example-server`
- `build-windows`: `[self-hosted, Windows, rmcp-template, steamy]`, builds
  `target/release/example.exe` and `target/release/example-server.exe`

Both jobs run on push and pull request through `.github/workflows/ci.yml`. They
upload artifacts named `example-linux-x86_64` and `example-windows-x86_64` so PR
review can test the exact compiled binary before a release tag exists.

## Why Native Windows Builds

Rust MCP servers should prove Windows compatibility before release. Cross
compiling from Linux can work for simple crates, but native Windows catches
problems in:

- path parsing and drive-letter handling
- shell quoting and process spawning
- Windows TLS, DNS, and socket behavior
- `windows-rs` or MSVC-specific dependency behavior
- runner-level Cargo configuration that changes generated CPU instructions

`release.yml` is the tag-time packaging flow. The CI build jobs are earlier
feedback: they run on every PR and produce artifacts for smoke testing.

## Workflow Shape

The PR-time build path is:

1. Check out the repo.
2. Install Rust stable.
3. Restore Cargo cache.
4. Build `apps/web/out` with pnpm so the default `web` feature embeds real
   static assets.
5. Run Windows tests on the Windows job.
6. Build the release binary.
7. Upload the compiled binary as a workflow artifact.

The Windows job also prints Rust CPU flag evidence:

```powershell
rustc -vV
"RUSTFLAGS=$env:RUSTFLAGS"
rustc --print cfg | Select-String 'target_feature'
cargo config get build.rustflags --merged
cargo config get target.x86_64-pc-windows-msvc.rustflags --merged
```

This is intentional. On self-hosted runners, user-level Cargo config can silently
add `target-cpu=native` or SIMD flags that make the artifact crash on other
Windows machines.

## Portable Windows CPU Flags

`.github/workflows/ci.yml` sets:

```yaml
WINDOWS_PORTABLE_RUSTFLAGS: >-
  -C target-cpu=x86-64
  -C target-feature=-avx512f,-avx512vl,-avx512bw,-avx512dq,-avx512cd,-avx512ifma,-avx512vbmi,-avx512vbmi2,-avx512vnni,-avx512bitalg,-avx512vpopcntdq
```

The Windows `cargo test` and `cargo build --release` steps pass those flags via
`RUSTFLAGS`. Keep this override when switching from `windows-latest` to a
self-hosted Windows runner.

Do not put `target-cpu=native` in repo config. If a developer wants local native
optimizations, they belong in that developer's private environment, never in
committed `.cargo/config.toml`.

## Current Steamy Runner

The active runner is:

- GitHub repo: `jmagar/rmcp-template`
- Runner name: `rmcp-template-windows-1`
- Runner path: `C:\Users\jmaga\actions-runner\rmcp-template`
- Labels: `self-hosted`, `Windows`, `X64`, `rmcp-template`, `steamy`
- Startup file:
  `C:\Users\jmaga\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\rmcp-template-runner.vbs`

The runner is nested under the existing Axon runner directory so all GitHub
Actions runner state for steamy stays under `C:\Users\jmaga\actions-runner`.

## GitHub-Hosted Runner Setup

No repository configuration is required for `windows-latest`. GitHub provides
Windows, MSVC, PowerShell, Node, and Rust installation support.

Use the hosted runner when:

- build time is acceptable
- no private hardware, GPU, service, or desktop integration is required
- artifacts only need general x86_64 Windows compatibility

## Self-Hosted Windows Runner Setup

Use a self-hosted runner when the repo needs persistent caches, private network
access, specialized desktop testing, or a known Windows host.

1. In GitHub, open the repo or organization settings.
2. Go to `Actions` -> `Runners` -> `New self-hosted runner`.
3. Choose Windows x64 and follow GitHub's generated download/config commands.
4. Run the runner as a service so builds survive logouts.
5. Add stable labels such as `self-hosted`, `Windows`, and a repo-family label
   such as `rmcp-template`.

Then change the Windows job:

```yaml
runs-on: [self-hosted, Windows, rmcp-template]
```

If Linux should also use a self-hosted runner, change the Linux job similarly:

```yaml
runs-on: [self-hosted, Linux, rmcp-template]
```

Keep labels repo-family-specific. Avoid labels tied to one machine name unless
the workflow truly requires that exact machine.

## Required Windows Tools

Install these for a self-hosted Windows runner:

- Git for Windows
- Visual Studio Build Tools with the MSVC C++ toolchain
- Windows SDK
- Rustup and stable Rust
- Node.js LTS
- PowerShell 7 if the host does not already have it

Verify from the runner account, not from an administrator shell:

```powershell
git --version
rustup show
rustc -vV
cargo -V
node -v
npm -v
```

The runner service account is the effective build user. If the runner service
runs as `NETWORK SERVICE`, a local admin, or a named user, inspect that account's
Cargo home and PATH.

## Cargo Config Audit

Before trusting self-hosted artifacts, inspect merged Cargo config from a
workflow run or from the runner account:

```powershell
cargo config get build.rustflags --merged
cargo config get target.x86_64-pc-windows-msvc.rustflags --merged
cargo config get build.rustc-wrapper --merged
```

Also inspect likely config files:

```powershell
$env:USERPROFILE\.cargo\config.toml
$env:CARGO_HOME\config.toml
.\.cargo\config.toml
```

Remove or override anything like:

```toml
[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-cpu=native"]
```

Those flags can produce binaries that work on the runner and crash elsewhere.

## Artifact Smoke Test

After a workflow run:

```bash
gh run list --workflow CI --limit 5
gh run download <run-id> --name example-windows-x86_64 --dir /tmp/example-win
```

Then copy `example.exe` to a Windows host and run:

```powershell
.\example.exe --version
.\example.exe status
.\example.exe doctor
```

For MCP transport smoke testing:

```powershell
.\example.exe mcp
```

For HTTP smoke testing:

```powershell
$env:EXAMPLE_MCP_HOST = "127.0.0.1"
$env:EXAMPLE_MCP_NO_AUTH = "true"
.\example.exe serve
```

Then from another shell:

```powershell
Invoke-WebRequest http://127.0.0.1:40060/health
```

## Troubleshooting

If the Windows artifact crashes on another machine:

- Check the workflow's `Show Windows Rust CPU flags` step.
- Recheck the self-hosted runner user's Cargo config.
- Confirm `RUSTFLAGS` is set on both `cargo test` and `cargo build`.
- Rebuild with `windows-latest` to separate repo issues from host issues.
- Test `example.exe --version` before testing MCP or HTTP behavior.

If pnpm fails on Windows:

- Confirm `apps/web/package.json` has a valid `packageManager`.
- Confirm `node -p "require('./package.json').packageManager"` works in
  `apps/web`.
- Delete stale `apps/web/node_modules` and rerun the job.

If Cargo cannot find MSVC:

- Install Visual Studio Build Tools.
- Include the Windows SDK and MSVC C++ build tools workloads.
- Restart the runner service so PATH changes are visible.
