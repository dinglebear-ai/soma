---
title: "xtasks"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
last_reviewed: "2026-05-15"
---

# xtasks

The `xtask/` crate contains typed repo automation invoked as `cargo xtask <command>`. Shell scripts are good for glue; xtasks are better when the check needs structured parsing, walking the repo, or cross-platform behavior.

## Repository layout

```
xtask/
  Cargo.toml    # name = "xtask", path dep on main crate
  src/
    main.rs     # cargo xtask <command>
```

## Commands

| Command | Purpose |
|---|---|
| `cargo xtask dist` | Build the release binary into the Cargo target directory. |
| `cargo xtask ci` | Run local CI sequence: fmt, clippy, tests, taplo, patterns, audit when tools exist. |
| `cargo xtask symlink-docs` | Create `AGENTS.md` and `GEMINI.md` symlinks next to each `CLAUDE.md`. |
| `cargo xtask check-env` | Validate required environment before server start. |
| `cargo xtask patterns` | Check static contracts derived from `docs/PATTERNS.md`. |
| `cargo xtask generate-docs` | Regenerate volatile docs and metadata from canonical Rust specs. |
| `cargo xtask check-docs` | Fail when generated docs or metadata drift from canonical Rust specs. |
| `cargo xtask check-stale-claims` | Fail when known stale hardcoded Soma claims reappear. |
| `cargo xtask sync-web-source` | Copy editable `apps/web` source into `crates/soma/web/assets/source` with generated artifacts excluded. |
| `cargo xtask check-web-source-sync` | Fail if the bundled web source has drifted from `apps/web`. |
| `cargo xtask update-aurora-web` | Refresh the known Aurora registry components, validate `apps/web`, then sync the bundle. |
| `cargo xtask changed-paths` | Classify changed files into CI routing categories consumed by path-aware GitHub workflow gates. |
| `cargo xtask codex-schema regen <dir>` | Regenerate `crates/shared/codex-app-server-client/schema/{protocol.schema.json,methods.json,CODEX_VERSION.txt}` from a `codex app-server generate-json-schema` output directory. |
| `cargo xtask codex-schema bisect <dir>` | Binary-search a fresh schema dump for the minimal definition(s) that panic typify's schema-merge logic, when `codex-app-server-client` fails to build after a `codex` CLI upgrade. |
| `cargo xtask codex-schema drift [--dir <dir>] [--json] [--strict]` | Diff the vendored `schema/methods.json` against a fresh (or `--dir`'d) `codex app-server generate-json-schema` dump; reports added/removed/changed methods per section so a `codex` upgrade that changes the app-server protocol surface can't silently slip past `build.rs`'s version-string-only staleness warning. Missing `codex` on PATH (with no `--dir`) is a graceful skip (exit 0), never a failure; `--strict` exits non-zero when drift is found. See `.github/workflows/codex-schema-drift-monitor.yml` for the scheduled CI job. |
| `cargo xtask check-ts-client [--write\|--check]` | Regenerate (`--write`) or verify (`--check`, the default) `crates/shared/codex-app-server-client/clients/typescript/src/generated/openapi-types.ts` against that crate's checked-in `openapi.json`, via the package's own `pnpm run generate`/`check-sync`/`typecheck` scripts - proof the REST adapter's spec is consumable by a real TypeScript toolchain, not just Rust. Missing `node`/`pnpm` on PATH is a graceful skip (exit 0), never a failure. Wired into `.github/workflows/ci.yml`'s `soma` job. See `crates/shared/codex-app-server-client/clients/typescript/README.md` for the package itself. |

## Justfile delegates to xtask

```just
dist:
    cargo xtask dist
symlink-docs:
    cargo xtask symlink-docs
```

## Pattern checks

`cargo xtask patterns` verifies important architecture contracts:

- required Soma files exist
- no `mod.rs` files
- file size warnings and hard limits
- MCP/CLI shims remain thin (no business logic)
- action surfaces stay represented in schemas, help text, tests, and CLI
- routes, plugin manifests, auth config, and tooling hooks exist

`cargo xtask patterns --strict` treats warnings as failures.

### What the pattern checker catches

```
WARN  crates/soma/mcp/src/tools.rs  line 42: potential business logic in MCP shim
WARN  crates/soma/cli/src/lib.rs  line 87: potential business logic in CLI shim
ERROR crates/soma/application/src/service/mod.rs: mod.rs files are banned
ERROR crates/soma/mcp/src/tools.rs: action "new_action" in ACTION_SPECS missing from dispatch
ERROR apps/soma/tests/tool_dispatch.rs: action "new_action" has no test
## Web Source Sync

`soma-web` bundles editable Aurora frontend source for generated projects.
The source of truth is `apps/web`; the bundled copy lives at
`crates/soma/web/assets/source`.

```bash
cargo xtask sync-web-source
cargo xtask check-web-source-sync
```

The sync excludes generated artifacts: `.next`, `node_modules`, `out`,
`tsconfig.tsbuildinfo`, and `.DS_Store`. `cargo xtask ci` runs
`check-web-source-sync` so drift is caught before merge.

To pull the current Aurora registry versions into Soma web app:

```bash
cargo xtask update-aurora-web
```

That command refreshes the Aurora tokens plus the Aurora UI components currently
used by Soma, runs `pnpm --dir apps/web validate`, then syncs the bundled
source.

## Generated Docs

`cargo xtask generate-docs` delegates to `scripts/generate-docs.py --write`.
It renders volatile tables and metadata from `ACTION_SPECS`,
`ENV_KEY_SPECS`, and typed config defaults:

- `docs/ENV.md`
- `.env.example`
- `config.soma.toml`
- `plugins/soma/.claude-plugin/plugin.json`
- `plugins/soma/.codex-plugin/plugin.json`
- `plugins/soma/gemini-extension.json`
- `apps/web/lib/generated-actions.ts`
- README and skill action tables
- `docs/generated/plugin-settings.md`
- `docs/generated/scripts-index.md`

`cargo xtask check-docs` runs the same renderer in drift-check mode and is part
of local CI, contract audit, and release checks.

## symlink-docs

`cargo xtask symlink-docs` keeps `AGENTS.md` and `GEMINI.md` in sync with `CLAUDE.md` across every directory that has a `CLAUDE.md`:

```bash
find . -name "CLAUDE.md" -not -path "./.git/*" -not -path "./target/*" | while read f; do
    dir=$(dirname "$f")
    ln -sf "CLAUDE.md" "${dir}/AGENTS.md"
    ln -sf "CLAUDE.md" "${dir}/GEMINI.md"
done
```

Run `just symlink-docs` after adding any new `CLAUDE.md` file.

## check-env

`cargo xtask check-env` reports missing or misconfigured environment before startup:

```
✓ SOMA_API_URL:   https://example.internal/api (set)
✗ SOMA_API_KEY:   not set
  → Set SOMA_API_KEY in ~/.soma/.env or your environment
```

See `docs/PATTERNS.md` §24 and §48 for the xtask and doctor patterns.

## changed-paths

`cargo xtask changed-paths` is the source of truth for path-aware CI routing.
`ci.yml` and `msrv.yml` call it before expensive jobs and write the booleans to
`GITHUB_OUTPUT`.

```bash
cargo xtask changed-paths --event pull_request --output /tmp/ci-paths.env
cargo xtask changed-paths \
  --event pull_request \
  --changed-files /tmp/changed-files.txt \
  --output /tmp/ci-paths.env \
  --write-changed-files /tmp/seen-files.txt
```

Outputs: `all`, `docs`, `workflow`, `rust`, `web`, `native`, `mcp`, `docker`,
`toml`, `soma`, `security`, `secrets`, and `release`. Workflow changes,
manual dispatch, and empty changed-file sets fail safe to full CI.

## codex-schema

`cargo xtask codex-schema` regenerates and troubleshoots the vendored JSON
Schema `codex-app-server-client` generates its protocol types from. See
`crates/shared/codex-app-server-client/README.md`'s "Regenerating the schema"
section for the full workflow:

```bash
codex app-server generate-json-schema --out /tmp/codex-schema --experimental
cargo xtask codex-schema regen /tmp/codex-schema
cargo xtask codex-schema bisect /tmp/codex-schema   # only if regen breaks the build
```

`build.rs` also does a best-effort, non-fatal staleness check on every build:
if a `codex` binary is on `PATH` and its version doesn't match
`schema/CODEX_VERSION.txt` (stamped by the last `regen` run), it emits a
`cargo:warning` pointing back at the regen workflow.
