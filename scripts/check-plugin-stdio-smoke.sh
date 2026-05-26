#!/usr/bin/env bash
# Smoke-test the plugin-packaged stdio MCP binary.
set -euo pipefail

PLUGIN_ROOT="${PLUGIN_ROOT:-plugins/example}"
BIN="${PLUGIN_ROOT}/bin/example"
TIMEOUT_SECS="${TIMEOUT_SECS:-5}"

if [[ ! -x "${BIN}" ]]; then
  echo "plugin stdio smoke: missing executable ${BIN}" >&2
  echo "run: just build-plugin" >&2
  exit 1
fi

response="$(
  printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"plugin-stdio-smoke","version":"0.0.0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"example","arguments":{"action":"status"}}}' \
    | CLAUDE_PLUGIN_ROOT="${PLUGIN_ROOT}" EXAMPLE_API_URL="" RUST_LOG=warn timeout "${TIMEOUT_SECS}s" "${BIN}" mcp
)"

printf '%s\n' "${response}" \
  | jq -es 'map(select(.id == 2))[0].result.structuredContent.status == "ok"' >/dev/null

echo "plugin stdio smoke passed"
