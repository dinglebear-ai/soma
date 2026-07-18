# scripts

Maintenance and automation scripts for Soma. Shell scripts target Bash and
generally use `set -euo pipefail`; Python scripts run with `python3`.

This README is the index for everything under `scripts/`. When a script is added,
renamed, or materially changed, update the quick index and the reference entry.

## Regenerating The Inventory

The committed generated script index lives at `docs/generated/scripts-index.md`
and is refreshed by:

```bash
cargo xtask generate-docs
```

The raw file list can also be generated directly from git:

```bash
rg --files scripts | sort
```

Useful starter for extracting script headers while updating this README:

```bash
for f in scripts/*.sh scripts/*.py; do
  printf '\n## %s\n' "$f"
  sed -n '1,40p' "$f"
done
```

The descriptions below are hand-curated from each script's current behavior,
usage text, Justfile wiring, CI references, and hook integration.

## Quick Index

### Release And Versioning

| File | Type | Entry points | What it does |
|---|---|---|---|
| `pre-release-check.sh` | Bash wrapper | `cargo xtask pre-release-check`, `just pre-release` | Delegates to xtask for the release-readiness gate: patterns, plugin layout, schema/OpenAPI docs, scaffold contract, Soma smoke tests, release version checks, blob size, ASCII hygiene, `just verify`, plugin build, and optional mcporter tests. |
| `bump-version.sh` | Bash wrapper | `cargo xtask bump-version soma <major|minor|patch>` | Thin wrapper for the xtask version bumper. |
| `check-version-sync.sh` | Bash wrapper | `cargo xtask check-version-sync` | Thin wrapper for the xtask manifest-backed version sync gate. |
| `check-dependency-updates.sh` | Bash wrapper | `cargo xtask check-dependency-updates`, `just deps-check` | Delegates to xtask for a read-only dependency drift report using `cargo update --dry-run` plus optional crates.io latest-version checks. |
| `check-blob-size.py` | Python wrapper | `cargo xtask check-blob-size`, `just blob-size-check`, CI | Delegates to xtask to block changed git blobs above the configured size budget unless allowlisted. |
| `blob-size-allowlist.txt` | Data | used by `check-blob-size.py` | Allowlist patterns for intentional large artifacts. |

### Generated Contracts And Docs

| File | Type | Entry points | What it does |
|---|---|---|---|
| `check-schema-docs.py` | Python wrapper | `cargo xtask check-schema-docs`, `just schema-docs`, `just schema-docs-check`, CI | Delegates to xtask to generate/check `docs/MCP_SCHEMA.md` and related action references from the canonical action specs. |
| `check-openapi.py` | Python wrapper | `cargo xtask check-openapi`, `just openapi`, `just openapi-check`, CI | Delegates to xtask to generate/check `docs/generated/openapi.json` for the REST API surface. |
| `generate-docs.py` | Python | `cargo xtask generate-docs`, `cargo xtask check-docs`, CI | Generates/checks volatile docs and metadata from the service-owned `ACTION_SPECS`, `ENV_KEY_SPECS`, and typed config defaults. |
| `check-stale-claims.py` | Python | `cargo xtask check-stale-claims`, CI | Fails when known stale hardcoded Soma claims reappear. |
| `check-readme-guide.py` | Python | `python3 scripts/check-readme-guide.py README.md` | Audits RMCP READMEs against `docs/RMCP_README_GUIDE.md` structural invariants before fleet alignment. |
| `check-scaffold-intent-contract.py` | Python wrapper | `cargo xtask check-scaffold-intent-contract`, `just scaffold-contract-check`, CI | Delegates to xtask to validate the scaffold intent JSON schema and checked-in examples without third-party packages. |
| `check-coupled-files.sh` | Bash wrapper | `cargo xtask check-coupled-files`, `just coupled-files-check`, CI | Delegates to xtask to warn when files that usually change together drift, such as script edits without `scripts/README.md` updates. |
| `refresh-docs.sh` | Bash wrapper | `cargo xtask refresh-docs`, `just refresh-docs*` | Delegates to xtask to refresh ignored protocol, SDK, Claude Code, and mcporter references under `docs/references/`. |

### Plugin And MCP Validation

| File | Type | Entry points | What it does |
|---|---|---|---|
| `conformance_report.py` | Python | `just conformance-report` | Summarizes official MCP conformance `checks.json` result files under `results/`, with optional JSON output for audits. |
| `validate-plugin-layout.sh` | Bash wrapper | `cargo xtask validate-plugin-layout`, `just validate-plugin`, CI | Delegates to xtask to validate Claude, Codex, and Gemini plugin packaging conventions. |
| `check-plugin-hook-contract.py` | Python wrapper | `cargo xtask check-plugin-hook-contract` | Delegates to xtask to audit cross-repo plugin setup hook JSON contracts, optionally executing setup commands. |
| `check-plugin-stdio-smoke.sh` | Bash wrapper | `cargo xtask check-plugin-stdio-smoke`, docs/contracts | Delegates to xtask to smoke-test the installed stdio plugin binary with JSON-RPC initialize plus `status`. |
| `test-mcp-auth.sh` | Bash wrapper | `cargo xtask test-mcp-auth`, `just test-mcp-auth` | Delegates to xtask to smoke-test HTTP MCP bearer-auth behavior. |
| `generate-cli.sh` | Bash wrapper | `cargo xtask generate-cli`, `just generate-cli` | Delegates to xtask to use mcporter to generate a standalone CLI from a running MCP server schema. |
| `sync-cargo.sh` | Bash wrapper | `cargo xtask sync-cargo`, plugin hook/runtime support | Delegates to xtask to copy `Cargo.lock` into plugin data directories, falling back to `cargo fetch` if needed. |

### Soma And Local Runtime Checks

| File | Type | Entry points | What it does |
|---|---|---|---|
| `test-soma-features.sh` | Bash wrapper | `cargo xtask test-soma-features`, `just soma-features`, CI | Delegates to xtask for fast Soma invariant smoke tests. |
| `check-cargo-generate.py` | Python wrapper | `cargo xtask cargo-generate`, docs | Thin wrapper for the xtask-owned cargo-generate smoke test. |
| `check-runtime-current.sh` | Bash wrapper | `cargo xtask check-runtime-current`, `just runtime-current` | Delegates to xtask to check whether the running systemd unit or Docker container uses the expected/current artifact. |
| `repair.sh` | Bash wrapper | `cargo xtask repair`, `just repair` | Delegates to xtask to stop, rebuild, and restart the local `soma-mcp` service through systemd or Docker Compose. |

### Hygiene And Developer Workflow

| File | Type | Entry points | What it does |
|---|---|---|---|
| `ci/changed_paths.py` | Python | `scripts/ci/pre_push.py`, future CI routing | Classifies changed paths into coarse categories such as rust, web, docker, MCP, release, security, and Soma; keep its path taxonomy in parity with `cargo xtask changed-paths`. |
| `ci/pre_push.py` | Python | `lefthook` pre-push, `just pre-push`, `just pre-push-plan` | Runs a path-aware local pre-push plan. Full local validation is opt-in with `SOMA_FULL_PRE_PUSH=1` or `just pre-push-full`. |
| `with_timeout.sh` | Bash | `lefthook.yml` | Applies a wall-clock budget to local hook commands so one check cannot stall commits indefinitely. |
| `check_lefthook_pre_commit_speed.py` | Python | `lefthook.yml`, `just lefthook-speed-check`, CI | Fails if the pre-commit stage grows workspace-scale cargo/test/build commands. |
| `block-env-commits.sh` | Bash wrapper | `cargo xtask block-env-commits`, lefthook pre-commit | Delegates to xtask to prevent staged `.env*` secret files from being committed, except `.env.example`. |
| `check-file-size.sh` | Bash wrapper | `cargo xtask check-file-size`, `just file-size-check`, lefthook pre-commit | Delegates to xtask to enforce staged source-file size budgets. |
| `asciicheck.py` | Python wrapper | `cargo xtask asciicheck`, through `run-ascii-check.sh` | Delegates to xtask to check files for unexpected non-ASCII characters and optionally fix common smart punctuation. |
| `run-ascii-check.sh` | Bash wrapper | `cargo xtask run-ascii-check`, `just ascii-check`, `just ascii-fix`, CI | Delegates to xtask to collect tracked text-like files and run `asciicheck.py`. |
| `build-web.sh` | Bash wrapper | `cargo xtask build-web`, `just build-web` | Delegates to xtask to build the optional Next.js static web UI export. |
| `web-watch.sh` | Bash wrapper | `cargo xtask web-watch`, `just web-watch` | Delegates to xtask to rebuild the optional web UI on changes using `watchexec`. |

## Script Reference

### `asciicheck.py`

```bash
cargo xtask asciicheck README.md Justfile
cargo xtask asciicheck --fix README.md
just ascii-check
just ascii-fix
```

Checks files for unexpected non-ASCII characters. `--fix` replaces common smart
punctuation with ASCII equivalents. A small allowlist permits intentional
documentation glyphs such as section signs, arrows, and box-drawing characters.

Usually run through `cargo xtask run-ascii-check`, which provides the repo's
tracked-file selection.

### `blob-size-allowlist.txt`

Data file for `scripts/check-blob-size.py`.

Each non-comment line is a glob pattern for an intentional large artifact. The
checker strips comments and blank lines, then treats matching paths as
allowlisted instead of failing the size budget.

Currently allowlists `crates/shared/codex-app-server-client/schema/protocol.schema.json`
(a vendored JSON Schema `build.rs` reads directly to generate protocol types -
see that crate's README).

### `block-env-commits.sh`

```bash
cargo xtask block-env-commits
```

Thin wrapper for `cargo xtask block-env-commits`.

The xtask command inspects the git staging area and rejects staged `.env`,
`.env.local`, `.env.prod`, `.env.staging`, or other `.env*` files. `.env.example`
is explicitly allowed.

Used by `lefthook.yml`.

### `build-web.sh`

```bash
cargo xtask build-web
just build-web
```

Builds the optional Next.js web UI static export from `apps/web/`. If
`apps/web/` is absent, the script exits successfully without doing anything. If
`node_modules/` is missing, it runs `pnpm install --frozen-lockfile`, then
`pnpm build`.

Output lands in `apps/web/out/` and is embedded into the binary by the `web`
feature.

### `bump-version.sh`

```bash
scripts/bump-version.sh patch
scripts/bump-version.sh minor
scripts/bump-version.sh major
```

Thin wrapper for:

```bash
cargo xtask bump-version soma <major|minor|patch>
```

It updates every version-bearing file declared for the `soma` component in
`release/components.toml`. Plugin manifests intentionally remain versionless.

### `check-blob-size.py`

```bash
cargo xtask check-blob-size
cargo xtask check-blob-size --base origin/main --head HEAD --max-bytes 512000
just blob-size-check
```

Checks changed git blobs between a base and head ref. Defaults to `origin/main`,
then `main`, then `HEAD~1` if needed. Files over the byte budget fail unless a
matching pattern is present in `scripts/blob-size-allowlist.txt`.

Binary changes are reported as binary so reviewers can distinguish large text
files from generated artifacts.

### `check-cargo-generate.py`

```bash
cargo xtask check-cargo-generate
cargo xtask check-cargo-generate --help
```

Thin wrapper for `cargo xtask cargo-generate`. It runs:

```bash
cargo xtask cargo-generate <args>
```

from the repository root and returns the xtask exit code. The real implementation
and maintained usage live in `xtask`.

### `check-coupled-files.sh`

```bash
cargo xtask check-coupled-files
cargo xtask check-coupled-files origin/main HEAD
just coupled-files-check
```

Thin wrapper for `cargo xtask check-coupled-files`.

The xtask command checks changed paths and reports likely documentation or
automation drift:

- `Justfile` without `lefthook.yml`, or vice versa.
- `scripts/*` without `scripts/README.md`.
- `crates/soma/mcp/src/schemas.rs` without `docs/MCP_SCHEMA.md`.
- plugin package changes without `docs/PLUGINS.md`.

Used in CI as a guardrail. It intentionally reports coupled-file concerns rather
than trying to infer every valid exception.

### `ci/pre_push.py`

```bash
python3 scripts/ci/pre_push.py --dry-run
SOMA_FULL_PRE_PUSH=1 python3 scripts/ci/pre_push.py
```

Runs the path-aware local pre-push plan used by `lefthook`, `just pre-push`,
and `just pre-push-plan`. Rust-category changes run version sync, script syntax
checks, workflow linting, coupled-file checks, `cargo xtask check-architecture`,
clippy, focused nextest, schema docs, and release version gates. Set
`SOMA_FULL_PRE_PUSH=1` or use `just pre-push-full` for the full local suite.

### `conformance_report.py`

```bash
just conformance-report
python3 scripts/conformance_report.py --results results
python3 scripts/conformance_report.py --results results --json
```

Summarizes `checks.json` files emitted by the official MCP conformance suite.
The text output is intended for quick local audits; `--json` emits a stable
machine-readable summary with total checks, pass rate, status counts,
per-scenario counts, and non-success failures.

### `check-dependency-updates.sh`

```bash
cargo xtask check-dependency-updates
cargo xtask check-dependency-updates --skip-search
cargo xtask check-dependency-updates --fail-on-updates
just deps-check
```

Read-only dependency update report. It runs `cargo update --dry-run` for
lockfile-compatible updates, then checks direct root dependencies against
crates.io unless `--skip-search` is used.

Options:

| Option | Effect |
|---|---|
| `--skip-search` | Skip crates.io latest-version checks. |
| `--fail-on-updates` | Exit 1 when possible updates are detected. |
| `-h`, `--help` | Show help. |

### `check-file-size.sh`

```bash
cargo xtask check-file-size
scripts/check-file-size.sh
MAX_RS=450 MAX_TS=350 cargo xtask check-file-size
just file-size-check
```

Thin wrapper for `cargo xtask check-file-size`.

The xtask command checks staged `.rs`, `.ts`, and `.tsx` files against effective
production-line budgets. Test files are exempt. Rust trailing inline
`#[cfg(test)] mod ...` blocks are excluded from the production count.

Defaults:

| Variable | Default | Meaning |
|---|---:|---|
| `MAX_RS` | `350` | Maximum effective production lines for Rust files. |
| `MAX_TS` | `300` | Maximum effective production lines for TypeScript/TSX files. |

Used by `lefthook.yml`.

### `check-openapi.py`

```bash
cargo xtask check-openapi --write
cargo xtask check-openapi --check
just openapi
just openapi-check
```

Generates `docs/generated/openapi.json` for Soma REST API surface:

- public `/health` and `/status`
- direct `/v1/*` business routes
- `/v1/capabilities`
- deprecated `retired REST action-envelope route` compatibility envelope

The version comes from `Cargo.toml`. The REST action enum is derived from
`crates/soma/domain/src/actions.rs`, excluding MCP-only actions.

### `check-readme-guide.py`

```bash
python3 scripts/check-readme-guide.py README.md
python3 scripts/check-readme-guide.py /home/jmagar/workspace/gotify-rmcp/README.md
```

Audits one or more README files against the high-signal invariants in
`docs/RMCP_README_GUIDE.md`: first-screen value prop, product boundary,
installation/client paths, runtime surfaces, MCP/CLI reference, credential
boundaries, generated-vs-curated docs ownership, distribution contracts, and
verification sections, plus the short related-server family section.

This is a fleet-alignment helper, not a full prose linter. It intentionally
checks structure and obvious credential-in-arguments mistakes before a human
does the final README pass.

### `generate-docs.py`

```bash
python3 scripts/generate-docs.py --write
python3 scripts/generate-docs.py --check
cargo xtask generate-docs
cargo xtask check-docs
```

Generates volatile docs and metadata from canonical Rust specs:

- `docs/ENV.md`
- `.env.example`
- `config.soma.toml`
- `apps/web/lib/generated-actions.ts`
- `docs/generated/plugin-settings.md`
- `docs/generated/scripts-index.md`

The checker fails when any generated file drifts.

### `check-stale-claims.py`

```bash
python3 scripts/check-stale-claims.py
cargo xtask check-stale-claims
```

Scans non-generated source/docs for Soma claims that should not reappear,
such as stale old local-port examples, old MCP port defaults, or explicit
plugin manifest `version` fields.

### `check-plugin-hook-contract.py`

```bash
cargo xtask check-plugin-hook-contract
cargo xtask check-plugin-hook-contract --execute
```

Audits plugin setup hooks across known Rust MCP server repositories in the
workspace. Static mode checks expected files and JSON contract shape. `--execute`
runs each binary setup command in an isolated temporary data directory and
validates the emitted contract JSON.

This is an operator/release audit tool, not a normal per-commit check.

### `check-plugin-stdio-smoke.sh`

```bash
cargo xtask check-plugin-stdio-smoke
BIN=soma TIMEOUT_SECS=10 cargo xtask check-plugin-stdio-smoke
```

Thin wrapper for `cargo xtask check-plugin-stdio-smoke`.

The xtask command smoke-tests the installed stdio MCP binary used by plugin
manifests. It sends a
minimal JSON-RPC sequence:

1. `initialize`
2. `notifications/initialized`
3. `tools/call` for the `soma` tool with `action=status`

The response is parsed in Rust; the command passes only when the status result is
`ok`.

Environment:

| Variable | Default | Meaning |
|---|---|---|
| `BIN` | `soma` | Binary to execute from `PATH`. |
| `TIMEOUT_SECS` | `5` | Timeout for the stdio exchange. |

### `check-runtime-current.sh`

```bash
scripts/check-runtime-current.sh
scripts/check-runtime-current.sh --mode systemd --expected-binary target/release/soma
scripts/check-runtime-current.sh --mode docker --pull --compose-dir .
just runtime-current
```

Checks whether the live runtime is using the expected/current artifact.

Systemd mode compares the running process hash from `/proc/<pid>/exe` against
the unit `ExecStart` binary and, when supplied, `--expected-binary`.

Docker mode compares the running container image ID with the local Docker Compose
image ID. `--pull` refreshes the Compose image before comparison.

Options:

| Option | Meaning |
|---|---|
| `--mode auto|systemd|docker` | Runtime to inspect. Default: `auto`. |
| `--pull` | Docker mode only: pull before comparing. |
| `--unit NAME` | Systemd user unit. Default: `soma-mcp.service`. |
| `--service NAME` | Docker Compose service/container. Default: `soma-mcp`. |
| `--compose-dir DIR` | Docker Compose project directory. Default: current directory. |
| `--expected-binary PATH` | Systemd mode: also compare against this binary. |

Soma adopters should rename `SOMA_*`, service, and binary defaults.

### `check-scaffold-intent-contract.py`

```bash
cargo xtask check-scaffold-intent-contract
just scaffold-contract-check
```

Validates `docs/contracts/scaffold-intent.schema.json` plus JSON examples under
`docs/contracts/examples/`.

The validator intentionally avoids third-party dependencies. It checks the
schema shape and the specific semantic constraints the scaffold handoff relies
on; it is not a full JSON Schema implementation.

### `check-schema-docs.py`

```bash
cargo xtask check-schema-docs --write
cargo xtask check-schema-docs --check
just schema-docs
just schema-docs-check
```

Treats `crates/soma/domain/src/actions.rs::ACTION_SPECS` as canonical
and generates/checks `docs/MCP_SCHEMA.md`.

It also checks that action docs stay mentioned in key user-facing surfaces such
as the README and plugin skill text. Action descriptions are maintained in this
script, so new actions usually require a script update plus a generated docs
refresh.

### `check-version-sync.sh`

```bash
cargo xtask check-version-sync
cargo xtask check-version-sync /path/to/project
```

Thin wrapper for:

```bash
cargo xtask check-version-sync
```

The xtask validates `release/components.toml`, exact JSON pointers, Cargo and
Cargo.lock parity, MCP registry metadata, OpenAPI version, changelog heading,
and plugin-manifest versionlessness.

### `generate-cli.sh`

```bash
SOMA_MCP_TOKEN=... cargo xtask generate-cli
just generate-cli
```

Generates a standalone CLI binary for this server through:

```bash
mcporter generate-cli
```

Requirements:

- a running MCP server on `http://localhost:40060/mcp`
- `mcporter` available on `PATH`
- optional `SOMA_MCP_TOKEN` for bearer-authenticated schema fetches

The script fetches `/mcp/tools/list`, hashes the schema, and skips regeneration
when `dist/.cache/soma-cli.schema_hash` already matches and `dist/soma-cli`
exists. The generated CLI embeds the token; do not commit or share it.

Soma adopters must update the port, generated binary name, and token env var.

### `pre-release-check.sh`

```bash
cargo xtask pre-release-check
cargo xtask pre-release-check --skip-verify --skip-build-plugin
cargo xtask pre-release-check --mcporter
just pre-release
```

Runs the release-readiness gate.

Always runs:

- `cargo xtask patterns`
- `just validate-plugin`
- `cargo xtask check-schema-docs --check`
- `cargo xtask check-openapi --check`
- `cargo xtask check-scaffold-intent-contract`
- `cargo xtask test-soma-features`
- `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`
- `cargo xtask check-blob-size`
- `just ascii-check`

By default it also runs `just verify` and `just build-plugin`. `--mcporter` adds
`just test-mcporter`, which requires a running server.

### `refresh-docs.sh`

```bash
cargo xtask refresh-docs
cargo xtask refresh-docs --dry-run
cargo xtask refresh-docs --skip-crawl
cargo xtask refresh-docs --skip-repomix
just refresh-docs
just refresh-docs-dry
```

Refreshes ignored reference docs under `docs/references/`.

Current inputs:

- crawled docs from `https://modelcontextprotocol.io`
- crawled docs from `https://code.claude.com`
- Repomix packs for `modelcontextprotocol/rust-sdk`
- Repomix packs for `modelcontextprotocol/modelcontextprotocol`
- Repomix packs for `modelcontextprotocol/registry`
- mcporter docs/source references

Environment:

| Variable | Default | Meaning |
|---|---|---|
| `AXON_OUTPUT_DIR` | `~/.axon/output` | Axon host output directory. |
| `REPOMIX_BIN` | auto-detected | Repomix executable; falls back to `npx --yes repomix`. |

Soma adopters should add service-specific docs and repos in the marked
`CUSTOMIZE:` sections.

### `repair.sh`

```bash
cargo xtask repair
just repair
```

Stops, rebuilds, and restarts the local `soma-mcp` service.

Flow:

1. Stop `soma-mcp.service` if active.
2. Otherwise stop a Docker container named `soma-mcp` if active.
3. Build `target/release/soma` with `--features full`.
4. If the systemd unit exists, install the binary into `~/.local/bin/` and start
   the unit.
5. Otherwise, if `docker-compose.yml` exists, rebuild and recreate with Docker
   Compose.
6. If no manager is detected, leave the rebuilt binary in `target/release/`.

### `run-ascii-check.sh`

```bash
cargo xtask run-ascii-check
cargo xtask run-ascii-check --fix
just ascii-check
just ascii-fix
```

Thin wrapper for `cargo xtask run-ascii-check`.

The xtask command collects tracked text-like files and runs
`scripts/asciicheck.py`.

Included extensions:

- `*.md`
- `*.rs`
- `*.toml`
- `*.json`
- `*.yml`
- `*.yaml`
- `*.sh`
- `*.py`

Excluded paths:

- `docs/references/**`
- `docs/sessions/**`

`--fix` rewrites files in place using `asciicheck.py --fix`.

### `sync-cargo.sh`

```bash
cargo xtask sync-cargo
CLAUDE_PLUGIN_ROOT=/path/to/repo CLAUDE_PLUGIN_DATA=/path/to/data cargo xtask sync-cargo
```

Thin wrapper for `cargo xtask sync-cargo`.

The xtask command copies `Cargo.lock` from `CLAUDE_PLUGIN_ROOT` to
`CLAUDE_PLUGIN_DATA` when the destination is missing or stale. If the copy fails,
it runs `cargo fetch` against the source manifest. If both fail, it removes the
destination lockfile and exits non-zero.

Used by plugin/runtime setup paths that need Cargo metadata in a plugin data
directory.

### `test-mcp-auth.sh`

```bash
SOMA_MCP_TOKEN=... cargo xtask test-mcp-auth
cargo xtask test-mcp-auth --url http://localhost:40060/mcp --token ...
cargo xtask test-mcp-auth --check-x-api-key
just test-mcp-auth
```

Smoke-tests HTTP MCP bearer auth:

- `/health` is public.
- `/mcp` rejects missing bearer tokens.
- `/mcp` rejects bad bearer tokens.
- `/mcp` accepts the configured bearer token.
- `--check-x-api-key` optionally checks `x-api-key` behavior.

The default URL is Soma's local MCP endpoint. Soma adopters should
update examples and env var names.

### `test-soma-features.sh`

```bash
cargo xtask test-soma-features
just soma-features
```

Fast shell smoke tests for Soma invariants that are awkward to express as
Rust tests.

Current checks:

- `.env` guard blocks staged secrets.
- `.env.example` remains allowed.
- the inline agent-doc symlink pattern creates `AGENTS.md` and `GEMINI.md`
  symlinks pointing at `CLAUDE.md`.
- `cargo xtask validate-plugin-layout` passes.
- `cargo xtask check-schema-docs --check` passes.
- `cargo xtask run-ascii-check` accepts the tracked repo file set.

### `validate-plugin-layout.sh`

```bash
cargo xtask validate-plugin-layout
PLUGIN_ROOT=plugins/soma cargo xtask validate-plugin-layout
just validate-plugin
```

Validates the Claude, Codex, and Gemini plugin package layout.

It checks, among other things:

- manifests exist and are valid JSON
- plugin names match expectations
- plugin manifests have no `version` field
- MCP config paths are correct
- hooks and skills are wired
- sensitive user config fields are marked sensitive
- stdio command/args use the expected PATH binary
- Gemini settings map into MCP environment variables

### `web-watch.sh`

```bash
cargo xtask web-watch
just web-watch
```

Runs one initial `cargo xtask build-web`, then watches `apps/web/` and rebuilds on
changes using `watchexec`.

Ignored paths:

- `apps/web/.next/**`
- `apps/web/out/**`
- `apps/web/node_modules/**`

Requires:

```bash
mise install watchexec
```

## Hook And CI Integration

Pre-commit hook scripts:

- `block-env-commits.sh`
- `check-file-size.sh`

CI-facing checks:

- `validate-plugin-layout.sh`
- `check-schema-docs.py --check`
- `check-openapi.py --check`
- `check-scaffold-intent-contract.py`
- `test-soma-features.sh`
- `check-blob-size.py`
- `check-coupled-files.sh`
- `run-ascii-check.sh`

Release-facing checks:

- `pre-release-check.sh`
- `check-version-sync.sh`
- `bump-version.sh`
- `check-dependency-updates.sh`
- `check-plugin-hook-contract.py`
- `check-runtime-current.sh`

Install hooks with:

```bash
just install-hooks
```

## Maintenance Rule

When adding, renaming, or changing a script:

1. Update this README.
2. Update any Justfile recipe that calls it.
3. Update CI or hook wiring if the script is part of a gate.
4. If the script changes generated docs or contracts, run the matching `--check`
   command before release.
