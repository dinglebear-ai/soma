---
title: "Windows CI Runner"
created: 2026-05-22
updated: 2026-07-30
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "soma"
source_of_truth: false
last_reviewed: "2026-07-26"
---

# Windows CI Runner

Soma runs its native Windows CI job on GitHub-hosted Windows:

```yaml
runs-on: windows-latest
```

The `build-windows` job in `.github/workflows/ci.yml` runs when the path-aware
`Changes` job marks native artifact checks as relevant. It builds and tests the
MSVC target, then uploads `soma-windows-x86_64` for PR-time smoke testing.
No self-hosted Windows runner or machine-specific labels are required.

Release packaging is separate: `.github/workflows/release.yml` cross-compiles
the Windows GNU artifact on the self-hosted Unraid Linux runners.

## Why Native Windows Builds

Native Windows CI catches behavior that Linux-to-Windows cross-compilation
cannot exercise:

- path parsing and drive-letter handling
- PowerShell quoting and process spawning
- Windows TLS, DNS, and socket behavior
- `windows-rs` and MSVC-specific dependency behavior
- Windows process-tree and Job Object cleanup

## Workflow Shape

The PR-time Windows path:

1. Runs the path classifier on the Unraid runner.
2. Builds the shared web export on Unraid.
3. Starts `build-windows` on `windows-latest`.
4. Downloads the web export.
5. Installs stable Rust with no compile cache.
6. Checks and tests the self-update surface.
7. Runs the native Windows workspace tests.
8. Builds the local-adapter and full release binaries.
9. Uploads `target/release/soma.exe`.

The native job intentionally runs Cargo without a compile wrapper. It still
installs no compile cache: the GitHub-hosted Windows runner has no route to the shared filesystem remote on tootie, so `setup-rust-kache` is called with `enable-cache: "false"`
and integrity checks.

## Portable Windows CPU Flags

The workflow sets portable CPU flags explicitly:

```yaml
WINDOWS_PORTABLE_RUSTFLAGS: >-
  -C target-cpu=x86-64
  -C target-feature=-avx512f,-avx512vl,-avx512bw,-avx512dq,-avx512cd,-avx512ifma,-avx512vbmi,-avx512vbmi2,-avx512vnni,-avx512bitalg,-avx512vpopcntdq
```

The Windows check, test, and build steps pass these flags through `RUSTFLAGS`.
Do not commit `target-cpu=native` or machine-specific SIMD flags for artifacts
that will be shared.

The long Cargo steps run through a PowerShell `Start-Process` wrapper that
prints a heartbeat every 60 seconds. This keeps long hosted-runner builds
observable without changing their exit status.

## GitHub-Hosted Environment

`windows-latest` provides Git, PowerShell, the MSVC toolchain, Windows SDK,
Python, and Node support. The workflow explicitly installs the Rust toolchain
and selects Python with `actions/setup-python`, so it does not depend on
machine-local state.

GitHub updates the image behind `windows-latest`. If an image change causes a
failure, inspect the run's `Set up job`, toolchain, and CPU-flag output before
pinning an older image. Keep `windows-latest` as the default so CI continues to
exercise a supported Windows environment.

## Artifact Smoke Test

After a workflow run:

```bash
gh run list --workflow CI --limit 5
gh run download <run-id> --name soma-windows-x86_64 --dir /tmp/soma-win
```

On Windows:

```powershell
.\soma.exe --version
.\soma.exe status
.\soma.exe doctor
```

For MCP stdio:

```powershell
.\soma.exe mcp
```

For HTTP:

```powershell
$env:SOMA_MCP_HOST = "127.0.0.1"
$env:SOMA_MCP_NO_AUTH = "true"
.\soma.exe serve
```

Then from another PowerShell:

```powershell
Invoke-WebRequest http://127.0.0.1:40060/health
```

## Troubleshooting

If the Windows artifact fails:

- inspect the `Show Windows Rust CPU flags` step
- confirm `RUSTFLAGS` reached every Cargo invocation
- check the GitHub runner image listed in `Set up job`
- reproduce with the same stable Rust toolchain on a local Windows host
- test `soma.exe --version` before MCP or HTTP behavior

If Cargo cannot find MSVC, first check the hosted image status and the
`windows-latest` software manifest. No repository-side runner installation or
service repair should be necessary.
