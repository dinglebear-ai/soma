#!/usr/bin/env bash
set -euo pipefail

RMCP_VERSION="${RMCP_VERSION:-3.0.0-beta.2}"
RMCP_TAG="${RMCP_TAG:-rmcp-v${RMCP_VERSION}}"
RMCP_COMMIT="${RMCP_COMMIT:-14298b72e0b25473ea79d5465fe186e22eb86397}"
CONF_VERSION="${MCP_CONFORMANCE_VERSION:-0.2.0-alpha.9}"
SPEC_VERSION="${MCP_SPEC_VERSION:-2026-07-28}"
PORT="${MCP_CONFORMANCE_PORT:-18002}"
OUT="${MCP_CONFORMANCE_OUTPUT_DIR:-target/mcp-conformance}"
ROOT="$(git rev-parse --show-toplevel)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/soma-mcp-conf.XXXXXX")"
PID=""
cleanup() {
  if [[ -n "$PID" ]]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

for cmd in git npm cargo curl; do
  command -v "$cmd" >/dev/null || { echo "missing $cmd" >&2; exit 1; }
done
grep -Fq "rmcp = { version = \"=${RMCP_VERSION}\"" "$ROOT/Cargo.toml" || {
  echo "Cargo.toml must pin rmcp to =${RMCP_VERSION}" >&2
  exit 1
}
if ss -tlnH 2>/dev/null | grep -q ":${PORT} "; then
  echo "port ${PORT} is in use" >&2
  exit 1
fi
mkdir -p "$ROOT/$OUT"

git clone --quiet --depth 1 --branch "$RMCP_TAG" \
  https://github.com/modelcontextprotocol/rust-sdk.git "$WORK/rust-sdk"
ACTUAL="$(git -C "$WORK/rust-sdk" rev-parse HEAD)"
[[ "$ACTUAL" == "$RMCP_COMMIT" ]] || {
  echo "rmcp tag commit is $ACTUAL, expected $RMCP_COMMIT" >&2
  exit 1
}
npm install --prefix "$WORK/js" --ignore-scripts --no-audit --no-fund \
  "@modelcontextprotocol/conformance@${CONF_VERSION}"
CONF="$WORK/js/node_modules/.bin/conformance"
RUSTFLAGS="" cargo build --manifest-path "$WORK/rust-sdk/Cargo.toml" -p mcp-conformance
RUSTFLAGS="" cargo build --bin soma --locked

SOMA_MCP_HOST=127.0.0.1 SOMA_MCP_PORT="$PORT" SOMA_MCP_NO_AUTH=true \
SOMA_MCP_CONFORMANCE_FIXTURES=true \
  "$ROOT/target/debug/soma" serve >"$ROOT/$OUT/soma-server.log" 2>&1 &
PID="$!"
READY=false
for _ in $(seq 1 50); do
  if curl -fs "http://127.0.0.1:${PORT}/health" >/dev/null; then
    READY=true
    break
  fi
  sleep 0.2
done
if [[ "$READY" != true ]] || ! kill -0 "$PID" 2>/dev/null; then
  echo "Soma conformance server did not become ready" >&2
  tail -100 "$ROOT/$OUT/soma-server.log" >&2 || true
  exit 1
fi

URL="http://127.0.0.1:${PORT}/mcp"
"$CONF" server --url "$URL" --suite all --spec-version "$SPEC_VERSION" \
  --expected-failures "$ROOT/conformance-baseline.yml" \
  -o "$ROOT/$OUT/server-dated"

TASKS=(
  tasks-lifecycle tasks-capability-negotiation tasks-wire-fields
  tasks-request-state-removal tasks-mrtr-input tasks-request-headers
  tasks-dispatch-and-envelope tasks-status-notifications
  tasks-required-task-error tasks-mrtr-composition
)
for SCENARIO in "${TASKS[@]}"; do
  "$CONF" server --url "$URL" --scenario "$SCENARIO" \
    --expected-failures "$ROOT/conformance/expected-failures-extensions.yaml" \
    -o "$ROOT/$OUT/server-extensions"
done

CLIENT="$WORK/rust-sdk/target/debug/conformance-client"
"$CONF" client --command "$CLIENT" --suite all --spec-version "$SPEC_VERSION" \
  -o "$ROOT/$OUT/client-dated"
"$CONF" client --command "$CLIENT" --suite extensions \
  --expected-failures "$ROOT/conformance/expected-failures-extensions.yaml" \
  -o "$ROOT/$OUT/client-extensions"
echo "MCP conformance matrix written to $ROOT/$OUT"
