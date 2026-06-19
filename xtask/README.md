# xtask

Repo automation commands, invoked via `cargo xtask <command>`. Replaces ad-hoc shell scripts with cross-platform Rust.

## Commands

### `cargo xtask ci`

Run all CI checks locally in the same order as `.github/workflows/ci.yml`. Stops on first failure.

| Step | Tool | Skipped if absent |
|---|---|---|
| 1/7 | `cargo fmt --check` | — |
| 2/7 | `cargo clippy -- -D warnings` | — |
| 3/7 | `cargo nextest run --profile ci` | falls back to `cargo test` |
| 4/7 | `taplo check` | yes |
| 5/7 | `cargo xtask patterns` | — |
| 6/7 | `cargo xtask check-test-siblings` | — |
| 7/7 | `cargo audit` | yes |

```bash
cargo xtask ci
# or via Justfile:
just ci
```

---

### `cargo xtask dist`

Build the release binary into the Cargo target directory.

1. Runs `cargo build --release --locked`
2. Copies the binary to `bin/<binary-name>` (creating `bin/` if needed)
3. Sets executable permissions on Unix
4. Prints the `git add` / `git commit` / `git push` instructions

```bash
cargo xtask dist
# or:
just dist
```

Respects `CARGO_TARGET_DIR` if set. After running, commit the updated `bin/` pointer and push to update LFS.

**TEMPLATE**: Update `BINARY_NAME` in `xtask/src/main.rs` to match the `[[bin]] name` in your `Cargo.toml`.

---

### `cargo xtask symlink-docs`

Create `AGENTS.md` and `GEMINI.md` symlinks next to every `CLAUDE.md` in the repo (Pattern §32: single source of truth for AI documentation).

- Walks the entire repo, skipping `.git/` and `target/`
- For each `CLAUDE.md` found, creates two relative symlinks in the same directory:
  - `AGENTS.md → CLAUDE.md` (Codex / OpenAI agents)
  - `GEMINI.md → CLAUDE.md` (Google Gemini)
- Skips entries that already exist or are dangling symlinks
- Prints a created/skipped summary

```bash
cargo xtask symlink-docs
# or:
just symlink-docs
```

Symlinks use relative targets so they remain valid after `git clone`. Run this after adding any new `CLAUDE.md` file to the repo.

---

### `cargo xtask patterns`

Check high-signal static contracts from `docs/PATTERNS.md`.

```bash
cargo xtask patterns
cargo xtask patterns --strict
cargo xtask patterns --json
# or:
just patterns-check
just patterns-strict
just patterns-json
```

The checker enforces required files, modern Rust module layout (`no mod.rs`), thin MCP/CLI shims, CLI/API/MCP/web surface-thinness heuristics, action schema/help/test/CLI surface drift, plugin manifest version rules, binary-owned plugin hook constraints, auth/config basics, route presence, and tooling hooks.

File-size target overages are warnings until they exceed a hard limit, so existing borderline modules do not block unrelated work. Use `--strict` to fail on warnings for newly adapted servers or cleanup branches. Use `--json` for machine-readable output in dashboards or CI annotations.

---

### `cargo xtask check-test-siblings`

Verify every `src/*.rs` implementation file has a sibling `*_tests.rs` file where expected.

```bash
cargo xtask check-test-siblings
```

---

### `cargo xtask block-env-commits`

Block staged `.env*` files before they can be committed. `.env.example` is
allowed.

```bash
cargo xtask block-env-commits
# compatibility wrapper:
bash scripts/block-env-commits.sh
```

Used by the `env_guard` pre-commit hook through the wrapper script.

---

### `cargo xtask check-coupled-files`

Check a git diff for common companion-file drift, such as script changes without
`scripts/README.md` or schema changes without generated MCP schema docs.

```bash
cargo xtask check-coupled-files
cargo xtask check-coupled-files origin/main HEAD
# compatibility wrapper:
bash scripts/check-coupled-files.sh origin/main HEAD
```

The default range is `origin/main..HEAD`, falling back to `HEAD~1..HEAD` when
`origin/main` is unavailable.

---

### `cargo xtask check-file-size`

Check staged `.rs`, `.ts`, and `.tsx` source files against line-count budgets.
Test files are exempt, and Rust trailing `#[cfg(test)] mod ...` blocks do not
count against production lines.

```bash
cargo xtask check-file-size
MAX_RS=450 MAX_TS=350 cargo xtask check-file-size
# compatibility wrapper:
bash scripts/check-file-size.sh
```

Defaults are `MAX_RS=350` and `MAX_TS=300`.

---

### `cargo xtask sync-cargo`

Copy `Cargo.lock` from `CLAUDE_PLUGIN_ROOT` to `CLAUDE_PLUGIN_DATA` when the
plugin data copy is missing or stale. If the copy fails, the command falls back
to `cargo fetch --manifest-path <repo>/Cargo.toml`.

```bash
cargo xtask sync-cargo
CLAUDE_PLUGIN_ROOT=/repo CLAUDE_PLUGIN_DATA=/tmp/plugin-data cargo xtask sync-cargo
# compatibility wrapper:
bash scripts/sync-cargo.sh
```

---

### `cargo xtask run-ascii-check`

Collect tracked source/config/docs files and run the repo ASCII checker.

```bash
cargo xtask run-ascii-check
cargo xtask run-ascii-check --fix
# compatibility wrapper:
bash scripts/run-ascii-check.sh --fix
```

The command delegates character replacement rules to `scripts/asciicheck.py`
but owns the tracked-file selection previously implemented in shell.

---

### `cargo xtask check-plugin-stdio-smoke`

Smoke-test the installed stdio MCP binary used by plugin manifests.

```bash
cargo xtask check-plugin-stdio-smoke
BIN=rtemplate TIMEOUT_SECS=10 cargo xtask check-plugin-stdio-smoke
# compatibility wrapper:
bash scripts/check-plugin-stdio-smoke.sh
```

The command sends a minimal JSON-RPC initialize plus `example(status)` tool call
and verifies the `id=2` response reports `structuredContent.status == "ok"`.

---

### `cargo xtask contract-audit`

Run local static/spec checks without contacting live upstream services. This wraps the high-signal template contract checks used before release.

```bash
cargo xtask contract-audit
# or:
just contract-audit
```

---

### `cargo xtask cargo-generate`

Smoke-test real `cargo-generate` output plus the Rust post-generation rewrite.
The command stages a clean template copy, generates both a simple server and a
hyphenated-package server, lets the native Rhai hook record selected values,
runs `cargo xtask cargo-generate-post` against each generated project, checks
plugin/repository metadata, verifies template-only files were removed, and runs
`cargo check --workspace --all-targets` inside each generated project.

```bash
cargo xtask cargo-generate
cargo xtask cargo-generate --no-cargo-check
```

Use `--no-cargo-check` for a faster shape-only check while iterating on the
generator.

### `cargo xtask cargo-generate-post`

Internal post-processor for generated projects. It replaces the old Python
rewrite hook and rewrites packages, crates, binaries, env prefixes, scopes,
ports, repository metadata, and generated paths in the final output directory.
When called with only `<generated-root>`, it reads `.cargo-generate-values.toml`
from the generated project and removes that temporary file before returning.

---

### `cargo xtask sync-web-source`

Copy editable `apps/web` source into `crates/rtemplate-web/assets/source`.
Generated artifacts are excluded: `.next`, `node_modules`, `out`,
`tsconfig.tsbuildinfo`, and `.DS_Store`.

```bash
cargo xtask sync-web-source
```

---

### `cargo xtask check-web-source-sync`

Validate that the bundled source in `rtemplate-web` matches `apps/web`.
This runs inside `cargo xtask ci`, so generated projects do not accidentally
receive stale Aurora frontend source.

```bash
cargo xtask check-web-source-sync
```

---

### `cargo xtask update-aurora-web`

Refresh Aurora tokens/components in `apps/web`, run the frontend validation
script, then sync the bundled source.

```bash
cargo xtask update-aurora-web
```

---

### `cargo xtask check-env`

Validate environment variables before starting the server. Prints the status of every required and optional variable, then exits non-zero if any required variable is missing.

```bash
cargo xtask check-env
# or:
just check-env
```

Example output:

```
[OK]      RTEMPLATE_MCP_TOKEN   — Static bearer token for MCP auth
[MISSING] RTEMPLATE_API_KEY     — Upstream service API key (required)

Error: 1 required variable(s) missing. Copy .env.example to .env and fill in the values.
```

**TEMPLATE**: Update `REQUIRED_VARS` and `OPTIONAL_VARS` in `xtask/src/main.rs` for your service. The template ships with no required variables (the stub works without credentials).

---

## Design notes

- **Minimal dependencies**: only `anyhow` and `walkdir` — keeps xtask compile time fast.
- **Workspace root awareness**: all commands `cd` to the workspace root via `CARGO_MANIFEST_DIR` before running, so they work from any subdirectory.
- **Delegation pattern**: shells out to external tools when useful (`cargo`, `taplo`, etc.) and implements repo-specific contract checks directly in Rust.
- **Optional tools**: `ci` gracefully skips `nextest`, `taplo`, and `cargo-audit` if they are not installed, so the command is always runnable on a fresh checkout.

## Adding a new command

1. Add a new function `fn your_command() -> anyhow::Result<()>` in `xtask/src/main.rs`.
2. Add a match arm in `main()`:
   ```rust
   Some("your-command") => your_command(),
   ```
3. Add it to the `print_help()` output.
4. Optionally add a `just` recipe in the root `Justfile`.
