# Synapse

Standalone Synapse is the native product adapter over Soma's canonical operations engine. It links `synapse-application`, `soma-ops`, `soma-fleet`, and `soma-infra` directly and has no dependency on `crates/synapse/import`.

## Coverage

- 35 read operations
- 21 mutation operations
- 59 total canonical operations
- CLI, REST, HTTP MCP, and stdio MCP
- optional historical `flux` and `scout` request aliases
- canonical JSON results only

## Run

```bash
cargo run -p synapse -- operations
cargo run -p synapse -- run product.help --params '{}'
cargo run -p synapse -- plan container.restart \
  --params '{"host":"local","container_id":"api"}'
cargo run -p synapse -- run container.restart --yes \
  --params '{"host":"local","container_id":"api"}'
cargo run -p synapse -- mcp
cargo run -p synapse -- serve
```

Configuration is loaded from `--config`, `SYNAPSE_CONFIG`, or the platform config directory at `synapse/config.toml`. If no config exists, Synapse starts with one local host confined to the current working directory.

Start from [`config.example.toml`](config.example.toml). Every filesystem, build, execution, and transfer path requires an explicit absolute root for the target host.

## HTTP

Public routes: `GET /health`, `GET /ready`, and `GET /status`.

Protected routes when `server.api_token` is set:

- `GET /operations`
- `GET /activity`
- `GET /openapi.json`
- `POST /v1/operations/<name>/plan`
- `POST /v1/operations/<name>/execute`
- `/mcp`

```bash
curl -sS -H 'Authorization: Bearer replace-with-a-long-random-token' \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:40070/v1/operations/product.help/execute \
  -d '{"parameters":{}}'
```

## Mutation authorization

Mutations always build an exact target- and topology-bound plan before authorization.

- CLI requires `--yes`.
- REST requires `confirmed: true`.
- MCP asks the client to affirm both `confirm` and `understood` through elicitation.
- `server.allow_mutations = true` enables product-level automatic confirmation and should be used only on a deliberately trusted deployment.

Authorization evidence is bound to the exact plan fingerprint and expires after `authorization_ttl_secs`. Send-state uncertainty and independent postcondition verification remain part of the canonical result.

## Verification

```bash
just synapse-standalone-check
```
