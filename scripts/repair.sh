#!/usr/bin/env bash
# Stop, rebuild, and restart the rtemplate-mcp service.
# Must be run from the repository root.
# Supports systemd user units and Docker Compose.
set -euo pipefail

echo "==> Stopping rtemplate-mcp..."
if systemctl --user is-active --quiet rtemplate-mcp.service 2>/dev/null; then
    systemctl --user stop rtemplate-mcp.service
    echo "    stopped systemd unit"
elif docker ps --filter 'name=^/rtemplate-mcp$' --quiet 2>/dev/null | grep -q .; then
    docker stop rtemplate-mcp >/dev/null 2>&1 || true
    echo "    stopped docker container"
else
    echo "    no running instance found"
fi

echo "==> Rebuilding release binary..."
cargo build --release --bin rtemplate-server --features full

echo "==> Restarting..."
if systemctl --user list-unit-files rtemplate-mcp.service 2>/dev/null | grep -q rtemplate-mcp; then
    mkdir -p "${HOME}/.local/bin"
    install -m 755 target/release/rtemplate-server "${HOME}/.local/bin/rtemplate-server"
    systemctl --user start rtemplate-mcp.service
    echo "    started systemd unit"
elif [ -f docker-compose.yml ]; then
    docker compose build
    docker compose up -d --force-recreate
    echo "    started docker compose service"
else
    echo "    no service manager detected; binary at target/release/rtemplate-server"
fi

echo "==> Done"
