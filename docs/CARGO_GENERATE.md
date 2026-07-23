# cargo-generate

Soma's scaffold/export lane can use `cargo-generate` while the repository stays
a normal compileable Rust product.

Soma avoids Liquid placeholders in live Rust and TOML files. Instead,
`cargo-generate` copies the compileable repository, then the Rust `xtask`
post-processor renames packages, binaries, env prefixes, scopes, type names,
and plugin paths in the generated copy.

## Install

```bash
cargo install cargo-generate
```

## Recommended: xtask scaffold

For new projects, prefer the higher-level scaffold command:

```bash
cargo xtask scaffold --name myservice --category upstream-client --port auto --plan
cargo xtask scaffold --intent scaffold-intent.json --apply ../generated
cargo xtask scaffold --verify ../generated/myservice-mcp
```

See [docs/SCAFFOLD.md](SCAFFOLD.md) for the intent JSON bridge, action starter
manifest, generated report, and verification workflow.

## Lower-level cargo-generate

```bash
cargo generate \
  --git https://github.com/dinglebear-ai/soma \
  --name myservice-mcp
cd myservice-mcp
cargo run --quiet -p xtask -- cargo-generate-post "$PWD"
```

Useful non-interactive form:

```bash
cargo generate \
  --git https://github.com/dinglebear-ai/soma \
  --name myservice-mcp \
  --define package_name=myservice-mcp \
  --define crate_prefix=myservice \
  --define binary_name=myservice \
  --define service_slug=myservice \
  --define type_prefix=MyService \
  --define env_prefix=MYSERVICE \
  --define scope_prefix=myservice \
  --define default_port=40060 \
  --define github_owner=jmagar \
  --define github_repo=myservice-mcp \
  --define default_features=full
cd myservice-mcp
cargo run --quiet -p xtask -- cargo-generate-post "$PWD"
```

The `cargo-generate` hook writes the selected values into a temporary
`.cargo-generate-values.toml` file. `cargo-generate-post` consumes that file,
rewrites the generated project, then removes generation-only files:
`.cargo-generate-values.toml`, `docs/CARGO_GENERATE.md`, and any copied
`cargo-generate.toml` or `scaffold/` files. It also best-effort removes the
`target/` directory created by the post step.

`package_name`, `crate_prefix`, and binary names may use hyphens because Cargo
package names and executable names support them. `service_slug` is also used as
a Rust config field/module identifier, so keep it snake_case:
`^[a-z][a-z0-9_]*$`.

`default_features` controls the generated repo's default Cargo feature set.
Use `full` for the platform superset, `local-adapter` for CLI + stdio MCP, or a
comma-separated custom set such as `server,web,oauth,observability`.

## After Generation

Run:

```bash
cargo fmt
cargo test
cargo clippy -- -D warnings
```

Then replace the stub transport client and service actions with the real
upstream or platform implementation.

## Scaffold Verification

When changing scaffold/export behavior, run the cargo-generate lane smoke test:

```bash
cargo xtask cargo-generate
```

For a faster shape-only check while iterating:

```bash
cargo xtask cargo-generate --no-cargo-check
```

The smoke test generates both a simple project and a project with hyphenated
Cargo package names, plus an upstream-client `local-adapter` project. It runs
the Rust `cargo-generate-post` rewrite command, checks plugin/repository
metadata, verifies scaffold-only files were removed, and runs
`cargo check --workspace --all-targets` inside each generated project.
`scripts/check-cargo-generate.py` is only a compatibility wrapper for the xtask
command.
