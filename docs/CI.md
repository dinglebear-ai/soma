---
title: "CI"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
last_reviewed: "2026-06-27"
---

# CI

CI mirrors local quality gates so failures are reproducible before pushing.

## Local CI commands

```bash
just verify
just soma-check
cargo xtask pre-release-check
```

`just ci` delegates to `cargo xtask ci`, which runs formatting, clippy, tests, TOML checks, pattern checks, and audit when supporting tools are installed.

## GitHub workflows

The repository keeps separate workflows for fast PR feedback, release
automation, long-lived marketplace-variant maintenance, and scheduled drift
checks. Use the smallest workflow that proves the thing you changed; do not
turn release or sync workflows into general PR CI.

### `.github/workflows/ci.yml`

Use for: every PR, every push to `main`, and manual full verification.

Do not use for: tag-only packaging or marketplace-no-mcp branch maintenance.

CI is path-aware. The first job, `Changes`, runs
`cargo xtask changed-paths` and publishes routing booleans consumed by the
expensive jobs. Workflow changes fail safe to full CI; manual
`workflow_dispatch` runs full CI; `.agents/skills/**` and `docs/sessions/**`
changes intentionally skip heavyweight runtime, web, Docker, release, and
security jobs. Branch protection should require the stable aggregate `CI Gate`
status instead of individual path-skipped jobs.

`changed-paths` emits these routing keys: `all`, `docs`, `workflow`, `rust`,
`web`, `native`, `mcp`, `docker`, `toml`, `soma`, `security`, `secrets`,
and `release`. The routing is intentionally conservative:

- workflow changes (`.github/**`, `xtask/src/ci_paths.rs`,
  `xtask/src/main.rs`, or this doc) enable every key
- `workflow_dispatch` and an empty changed-file set enable every key
- Rust or web changes also enable native artifact checks
- Rust changes, Docker/config/script changes, and web changes enable container smoke
- low-risk Markdown docs, session notes, and agent-skill changes can skip secret scanning

For branch protection, require stable aggregate gates (`CI Gate` and `MSRV Gate`)
rather than individual path-skipped jobs. GitHub reports skipped jobs
inconsistently as required checks; the aggregate gates turn "passed or
intentionally skipped" into one predictable status. Branch-protection lookup is
not available for this private repo without GitHub Pro or making the repo
public, so treat the live repository settings as manual state and keep docs
focused on the required check names.

The jobs run on self-hosted runners: path classifiers, aggregate gates, and
Linux jobs use the TOOTIE Docker runner
(`runs-on: [self-hosted, tootie, rmcp-template]`, see `docs/LINUX-RUNNER.md`),
and Windows jobs use steamy (`runs-on: [self-hosted, Windows, rmcp-template, steamy]`,
see `docs/WINDOWS-RUNNER.md`). The Rust jobs force `RUSTC_WRAPPER=sccache`,
`CARGO_BUILD_RUSTC_WRAPPER=sccache`, and `CARGO_INCREMENTAL=0`; the local
`.github/actions/setup-rust-sccache` action installs Rust plus sccache and prints
the effective cache configuration. CI caches compilation only; binary artifact
sync is an explicit recipe such as `just sync-bin` or `just build-plugin`.

Self-hosted jobs, including `changes`, `ci-gate`, `MSRV Changes`, and
`MSRV Gate`, use a same-repository job guard. Pushes, schedules, manual runs,
and same-repo PRs can use the TOOTIE and steamy runners; fork PRs do not
allocate self-hosted runners. Add a GitHub-hosted fork fallback before accepting
outside PRs that need CI feedback.

Jobs:
- `changes`: classifies changed files into CI routing categories
- `actionlint`: validates workflow syntax and self-hosted labels
- `fmt`: `cargo fmt -- --check`
- `clippy`: `cargo clippy -- -D warnings`
- `test`: builds the stdio binary, runs `cargo nextest run --profile ci`, and uploads the JUnit report
- `frontend-assets`: `pnpm install --frozen-lockfile`, `pnpm audit`, `pnpm lint`, `pnpm typecheck`, `pnpm build`
- `build-linux`: native Linux release build, uploads `soma-linux-x86_64`
- `build-windows`: native Windows release build and test on steamy, uploads `soma-windows-x86_64`
- `mcp-smoke`: starts the HTTP MCP server and runs the mcporter smoke suite
- `container-smoke`: validates compose files and builds the Docker image
- `toml`: `taplo check`
- `lefthook-speed`: keeps pre-commit hooks staged-only and fast
- `soma`: generated docs, plugin layout, scaffold, release-version, blob, coupled-file, and ASCII gates
- `deny`: `cargo deny check`
- `gitleaks`: secret scanning
- `ci-gate`: single aggregate status for branch protection

The Linux and Windows build jobs are PR-time artifact checks. They prove the
binary compiles natively before a release tag exists and give reviewers a
downloadable artifact for manual smoke testing.

The Windows job follows the Axon CI pattern: it builds on native Windows and
sets explicit portable CPU flags so self-hosted runner config cannot leak
`target-cpu=native` into published artifacts. See `docs/WINDOWS-RUNNER.md` for
the full runner setup and audit process.

### `.github/workflows/msrv.yml`

Use for: proving the declared `rust-version` remains honest.

Do not use for: full behavior testing; it only checks that the workspace still
builds on the minimum supported toolchain.

Runs on PRs and pushes to `main` with Rust 1.96.0 and sccache. It is also
path-aware: `MSRV Changes` skips the self-hosted MSRV build unless Rust, native,
TOML, or workflow files changed. Require `MSRV Gate` if this workflow is part of
branch protection.

### `.github/workflows/release-please.yml`

Use for: automatic release PRs, changelog updates, version bumps, release tags,
and GitHub Releases after CI succeeds on `main`.

Do not use for: binary packaging, npm publishing, Docker publishing, or registry
manifest publishing. Those are downstream artifact workflows triggered by the
release/tag that release-please creates.

It runs after a successful `CI` workflow on `main`, uses
`RELEASE_PLEASE_TOKEN`, and opens or updates the release PR from conventional
commits. When a release PR is created, a fixup job runs
`cargo xtask sync-release-please-version`, regenerates provider surfaces, and
commits derived version files back to the release PR branch.

### `.github/workflows/release.yml`

Use for: release-time binary packaging, npm package publishing, and attaching
artifacts to the GitHub Release.

Do not use for: PR validation or release version decisions. PRs should use
`ci.yml`; release versioning is owned by release-please.

It runs when release-please publishes a GitHub Release, or by manual dispatch
with an existing tag. It checks out the release tag, builds Linux and Windows
release artifacts, writes SHA256 sums, publishes the `soma-rmcp` npm launcher
package with provenance/trusted publishing support, and uploads artifacts to the
existing GitHub Release. Release Cargo builds use sccache through the same
wrapper environment as CI. Linux release jobs cross-compile the Windows GNU
target from the TOOTIE runner; the native Windows build is a PR-time `ci.yml`
check, not the release packaging path. The LFS write-back job is intentionally
isolated here because it can push to `main`; audit that behavior before reusing
it in a derived repo.

### `.github/workflows/docker-publish.yml`

Use for: publishing container images after code has landed.

Do not use for: PR smoke tests. `ci.yml` has a non-pushing `container-smoke` job
for that.

Runs only on `v*` tags. Do not path-gate this workflow: a release tag is already
an explicit publish action, and the image plus MCP registry manifest should stay
coupled to the tag. Release-please creates the tag after the release PR merges.

Tag jobs:
- Docker build and push
- Trivy vulnerability scan
- MCP Registry manifest publish when credentials are configured

### `.github/workflows/scheduled.yml`

Use for: surfacing new RUSTSEC advisories after code has already merged, plus
manual full dependency audits.

Do not use for: replacing the PR-time `audit` job. This is a periodic safety
net, not the merge gate.

Do not path-gate scheduled runs: the point is to catch advisory database changes
that happen when no repository paths changed. Scheduled runs check advisories
only; manual dispatch can run the full `cargo-deny` suite.

### `.github/workflows/check-no-mcp-drift.yml`

Use for: detecting drift between `main` and the protected `marketplace-no-mcp`
variant.

Do not use for: syncing or modifying the branch. This workflow is read-only.

### `.github/workflows/sync-marketplace-no-mcp.yml`

Use for: keeping the protected `marketplace-no-mcp` branch current with `main`
while applying the no-MCP variant rules.

Do not use for: branch cleanup. `marketplace-no-mcp` is a long-lived protected
variant branch and must not be deleted, squashed away, or folded into `main`
unless Jacob explicitly asks for that exact branch to be retired.

### `.github/workflows/dependabot-auto-merge.yml`

Use for: auto-merging eligible Dependabot updates after required checks pass.

Do not use for: human-authored dependency migrations, major upgrades, or changes
that alter public behavior. Those need normal review.

### `.github/workflows/rmcp-release-monitor.yml`

Use for: watching upstream `rmcp`, MCP schema, and conformance movement and
opening/updating an issue when there is drift.

Do not use for: automatically bumping protocol dependencies. It reports; humans
decide the migration.

## nextest configuration

CI uses `cargo nextest` with a dedicated profile in `.config/nextest.toml`:

```toml
[profile.default]
fail-fast = false

[profile.ci]
fail-fast = true
retries = 2
```

## Release gate

`cargo xtask pre-release-check` runs:

1. `cargo xtask patterns`
2. plugin layout validation
3. schema docs validation
4. Soma feature smoke tests
5. release version gate
6. blob-size check
7. ASCII hygiene
8. `just verify`
9. `just build-plugin`

Use `--mcporter` when a server is running and live MCP integration should be included.

## TOML formatting

All repos require `taplo` for TOML formatting:

```bash
taplo format     # format
taplo check      # CI check
```

Install: `cargo install taplo-cli` or `mise use taplo`.

`taplo.toml`:
```toml
[formatting]
align_entries = false
array_trailing_comma = true
array_auto_expand = true
array_auto_collapse = true
compact_arrays = true
compact_inline_tables = false
column_width = 100
indent_string = "  "
trailing_newline = true
allowed_blank_lines = 1
```

## Blob policy

Large artifacts are blocked unless allowlisted in `scripts/blob-size-allowlist.txt`. Plugin binaries are expected artifacts and are allowlisted.

## Release artifact distribution

Release-please is the source of truth for release PRs, changelog updates,
version bumps, `v*` tags, and GitHub Releases. `release/components.toml` remains
the local inventory of version-bearing files and distribution metadata that
must stay synchronized. PR CI runs `cargo xtask check-version-sync`; release PR
fixups run `cargo xtask sync-release-please-version` and regenerated metadata
checks so derived files follow `.release-please-manifest.json`.

Release-please-created version tags (`v*`) trigger the artifact workflows. The
release workflow builds release binaries and attaches them to the GitHub
Release. Soma still includes an
explicit Git LFS write-back job that commits binary pointers to `bin/` on
`main` for plugin install compatibility. Treat that as an auditable release-only
exception: disable it in derived repos that distribute solely through GitHub
Release assets. Local `just dist` / `cargo xtask dist` recipes are operator
conveniences for preparing artifacts and should not become a separate CI
write-back path.

CI artifact naming convention:

- `soma-linux-x86_64`
- `soma-windows-x86_64`

Release tarball naming convention:

- `soma-x86_64.tar.gz`
- `soma-windows-x86_64.tar.gz`

## CHANGELOG.md

Every repo keeps a `CHANGELOG.md` following [Keep a Changelog](https://keepachangelog.com/):

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] — 2026-05-13

### Added
- Initial release
- MCP server with action-based tool dispatch
- CLI thin shim
- Bearer token + Google OAuth authentication
- Streamable HTTP + stdio transport
- Thin plugin setup hook plus binary-owned setup/repair
- Claude Code plugin with userConfig
```

Update `[Unreleased]` with every meaningful change. The release workflow promotes it to a versioned section on tag.

See `docs/PATTERNS.md` §21, §24, §29, §31, §34 for release artifacts, nextest, taplo, GitHub workflow, and changelog patterns.
