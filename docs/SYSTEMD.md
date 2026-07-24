---
title: "systemd Deployment"
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

# systemd

Soma supports user-level systemd deployments when a unit named `soma-mcp.service` is installed by the derived service.

## Install the binary

```bash
cargo build --release --bin soma --features full
install -m 755 target/release/soma ~/.local/bin/soma
```

Or use the install script:

```bash
curl -fsSL https://raw.githubusercontent.com/dinglebear-ai/soma/main/install.sh | bash
```

The binary installs to `~/.local/bin/`. Verify it's in `$PATH`:

```bash
soma --version
soma doctor
```

## Unit file pattern

```ini
[Unit]
Description=Soma MCP server
After=network.target

[Service]
Type=simple
ExecStart=%h/.local/bin/soma serve
Restart=on-failure
RestartSec=5
EnvironmentFile=%h/.example/.env

[Install]
WantedBy=default.target
```

Key points:
- Use `EnvironmentFile` pointing at `~/.soma/.env` — never hardcode tokens in unit files.
- `%h` expands to the user home directory.
- `soma serve` is the canonical HTTP runtime mode. It owns the provider
  registry, REST API, web fallback, health/status routes, and Streamable HTTP
  MCP endpoint (see `docs/DEPLOYMENT.md`).

## Restart flow

```bash
systemctl --user daemon-reload
systemctl --user restart soma-mcp.service
systemctl --user status soma-mcp.service
```

## Runtime verification

`just runtime-current` detects stale running processes. The checker compares:

- `/proc/<pid>/exe` for the running service process
- the unit `ExecStart` binary
- optional `--expected-binary`

```bash
scripts/check-runtime-current.sh --mode systemd --expected-binary target/release/soma
just runtime-current
```

If hashes differ, install the new binary and restart the unit.

## Logging

With systemd, logs go to the journal:

```bash
journalctl --user -u soma-mcp.service -f
journalctl --user -u soma-mcp.service --since "1h ago"
```

The binary also writes structured JSON logs to `~/.soma/logs/example.log` regardless of deployment mode (see `docs/OBSERVABILITY.md`).

## Doctor pre-flight

Run `soma doctor` before starting the unit to validate the environment:

```bash
soma doctor
```

Exit code 0 = ready to start. Exit code 1 = one or more issues found.

## Environment

Prefer an `EnvironmentFile` that points at the repo or appdata `.env`. See `docs/ENV.md` for variable meanings.

See `docs/PATTERNS.md` §46, §47, §48 for binary commands, installation, and the doctor command patterns.
