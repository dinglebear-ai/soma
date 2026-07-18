# Quickstart — 5 minutes to a working MCP server

## Prerequisites

- Rust 1.96+ (`rustup update stable`)
- `clang` and `mold` for fast Linux builds: `apt install clang mold`
- `just` command runner: `cargo install just` (optional but convenient)

> See [docs/RUST.md](RUST.md) for the full system setup including the expected
> `~/.cargo/config.toml`, the mold linker rationale, and Windows cross-compilation.

## 1. Run Soma

```bash
git clone https://github.com/jmagar/soma
cd soma
cargo run --bin soma -- serve
```

The server starts on `http://localhost:40060`. In another terminal:

```bash
# Health check (no auth required)
curl http://localhost:40060/health
# {"status":"ok"}

# Call the greet action
curl -s -X POST http://localhost:40060/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"soma","arguments":{"action":"greet","name":"Alice"}}}'

# List available tools
curl -s -X POST http://localhost:40060/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
```

## 2. Try the CLI

```bash
cargo run -- greet --name Alice
cargo run -- echo --message "Hello, MCP!"
cargo run -- status
cargo run -- --help
```

## 3. Try stdio transport

```bash
cargo run -- mcp
# Server reads JSON-RPC from stdin. Send:
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

## 4. Run the tests

```bash
cargo test
```

All tests pass with no credentials needed — the stubs return hardcoded JSON.

## 5. Add bearer auth

Generate a token:

```bash
openssl rand -hex 32
# → e.g. a3f2c1...
```

Start with auth:

```bash
SOMA_MCP_TOKEN=a3f2c1... cargo run --bin soma -- serve
```

Now all `/mcp` calls require `Authorization: Bearer a3f2c1...`:

```bash
curl -s -X POST http://localhost:40060/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "Authorization: Bearer a3f2c1..." \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"soma","arguments":{"action":"status"}}}'
```

## 6. Connect Claude Desktop

Add to your Claude Desktop MCP config (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "soma": {
      "command": "/path/to/soma/target/debug/soma",
      "args": ["mcp"],
      "env": { "RUST_LOG": "warn" }
    }
  }
}
```

Or use Streamable HTTP (server must be running):

```json
{
  "mcpServers": {
    "soma": {
      "url": "http://localhost:40060/mcp"
    }
  }
}
```

## Next steps

- Read the [README](../README.md) for the step-by-step guide to adapting Soma for your own API.
- Use [docs/SCAFFOLD.md](SCAFFOLD.md) for the one-command scaffold plan/apply/verify workflow.
- Read [CLAUDE.md](../CLAUDE.md) for the thin-shim rule and how to add actions.
- For OAuth setup, set `SOMA_MCP_AUTH_MODE=oauth` and the `SOMA_MCP_GOOGLE_*` env vars — see the env var table in the README.

## Checklist for adapting Soma

Use this when creating a real service from soma:

- [ ] Replace every occurrence of `soma`/`Example`/`EXAMPLE` with your service name
- [ ] Implement API client in `crates/soma/client/src/client.rs` (transport only — no logic)
- [ ] Add service methods to `crates/soma/application/src/service.rs` (ALL logic here)
- [ ] Add native actions to `crates/soma/domain/src/actions.rs` and keep MCP/CLI/REST shims registry-driven
- [ ] Regenerate schema docs and OpenAPI after changing the service registry
- [ ] Update `crates/soma/config/src/config.rs` with service-specific config fields
- [ ] Add elicitation to destructive actions (or `confirm=true` flag fallback)
- [ ] Set port in `config.toml`, `docker-compose.yml`, and Dockerfile `EXPOSE`
- [ ] Implement central auth policy resolution in library code
- [ ] Implement `default_data_dir()` with container detection
- [ ] Write `entrypoint.sh` with permission setup and required-var validation
- [ ] Set up xtask crate with `dist`, `ci`, `symlink-docs`, `check-env`
- [ ] Configure nextest (`.config/nextest.toml`)
- [ ] Configure taplo (`taplo.toml`)
- [ ] Configure lefthook (`lefthook.yml`) — minimal hooks only
- [ ] Write `.github/workflows/ci.yml`, `docker-publish.yml`, `release.yml`
- [ ] Write tests in `*_tests.rs` sidecars + `apps/soma/tests/` integration tests
- [ ] Write `apps/soma/tests/mcporter/test-mcp.sh` with semantic validation
- [ ] Update `plugins/<service>/skills/<service>/SKILL.md` with real API details
- [ ] Write `install.sh` matching the GitHub release tarball names
- [ ] Copy `.gitignore` and `.dockerignore` from cortex
- [ ] Write `CHANGELOG.md`
- [ ] Run `just symlink-docs` to create `AGENTS.md` and `GEMINI.md` symlinks
- [ ] Write `server.json` for MCP registry
- [ ] Write `.codex-plugin/plugin.json` next to `.claude-plugin/plugin.json`
- [ ] Add `.worktreeinclude` at the repo root with `.env` and `config.toml`
- [ ] Run `cargo check` — must compile clean, zero warnings
- [ ] Run `cargo nextest run` — all tests pass
- [ ] Run `./apps/soma/tests/mcporter/test-mcp.sh` against a live server instance

For new projects, replace the manual rename checklist with:

```bash
cargo xtask scaffold --name myservice --category upstream-client --port auto --plan
cargo xtask scaffold --intent scaffold-intent.json --apply ../generated
cargo xtask scaffold --verify ../generated/myservice-mcp
```
