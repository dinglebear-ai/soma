#!/usr/bin/env bash
set -euo pipefail

RMCP_VERSION="${RMCP_VERSION:-3.1.0}"
RMCP_TAG="${RMCP_TAG:-rmcp-v${RMCP_VERSION}}"
RMCP_COMMIT="${RMCP_COMMIT:-1f9358eddca42d3a510c70ae6446dd6548c7c856}"
CONF_VERSION="${MCP_CONFORMANCE_VERSION:-0.2.0-alpha.9}"
SPEC_VERSION="${MCP_SPEC_VERSION:-2026-07-28}"
PORT="${MCP_CONFORMANCE_PORT:-}"
PORT_LOCK=""
ROOT="$(git rev-parse --show-toplevel)"
RUN_KEY="${RMCP_COMMIT:0:12}-$$"
OUT="${MCP_CONFORMANCE_OUTPUT_DIR:-target/mcp-conformance/run-${RUN_KEY}}"
if [[ "$OUT" = /* ]]; then
  OUTPUT_DIR="$OUT"
else
  OUTPUT_DIR="$ROOT/$OUT"
fi
UPSTREAM_TARGET="${MCP_CONFORMANCE_UPSTREAM_TARGET_DIR:-$ROOT/target/mcp-conformance-upstream/$RMCP_COMMIT}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/soma-mcp-conf.XXXXXX")"
PID=""
# shellcheck source=scripts/ci/mcp-conformance-port.sh
source "$ROOT/scripts/ci/mcp-conformance-port.sh"
cleanup() {
  if [[ -n "$PID" ]]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  release_conformance_port
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
mkdir -p "$OUTPUT_DIR"

git clone --quiet --depth 1 --branch "$RMCP_TAG" \
  https://github.com/modelcontextprotocol/rust-sdk.git "$WORK/rust-sdk"
ACTUAL="$(git -C "$WORK/rust-sdk" rev-parse HEAD)"
[[ "$ACTUAL" == "$RMCP_COMMIT" ]] || {
  echo "rmcp tag commit is $ACTUAL, expected $RMCP_COMMIT" >&2
  exit 1
}
npm_config_cache="$WORK/npm-cache" npm install --prefix "$WORK/js" \
  --ignore-scripts --no-audit --no-fund \
  "@modelcontextprotocol/conformance@${CONF_VERSION}"
CONF="$WORK/js/node_modules/.bin/conformance"
CARGO_TARGET_DIR="$UPSTREAM_TARGET" RUSTFLAGS="" \
  cargo build --manifest-path "$WORK/rust-sdk/Cargo.toml" -p mcp-conformance
RUSTFLAGS="" cargo build --bin soma --locked

start_soma() {
  local attempt ready
  for attempt in $(seq 1 5); do
    PORT="${MCP_CONFORMANCE_PORT:-}"
    reserve_conformance_port
    echo "Reserved conformance port ${PORT} (attempt ${attempt})"
    SOMA_MCP_HOST=127.0.0.1 SOMA_MCP_PORT="$PORT" SOMA_MCP_NO_AUTH=true \
    SOMA_MCP_CONFORMANCE_FIXTURES=true \
      "$ROOT/target/debug/soma" serve >"$OUTPUT_DIR/soma-server.log" 2>&1 &
    PID="$!"
    ready=false
    for _ in $(seq 1 50); do
      if curl -fs "http://127.0.0.1:${PORT}/health" >/dev/null \
        && kill -0 "$PID" 2>/dev/null; then
        sleep 0.1
        if kill -0 "$PID" 2>/dev/null; then
          ready=true
          break
        fi
      fi
      if ! kill -0 "$PID" 2>/dev/null; then
        break
      fi
      sleep 0.2
    done
    if [[ "$ready" == true ]]; then
      return 0
    fi
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
    PID=""
    release_conformance_port
    if [[ -n "${MCP_CONFORMANCE_PORT:-}" ]]; then
      break
    fi
  done
  echo "Soma conformance server did not become ready" >&2
  tail -100 "$OUTPUT_DIR/soma-server.log" >&2 || true
  return 1
}
start_soma

URL="http://127.0.0.1:${PORT}/mcp"
"$CONF" server --url "$URL" --suite all --spec-version "$SPEC_VERSION" \
  --expected-failures "$ROOT/conformance-baseline.yml" \
  -o "$OUTPUT_DIR/server-dated"

TASKS=(
  tasks-lifecycle tasks-capability-negotiation tasks-wire-fields
  tasks-request-state-removal tasks-mrtr-input tasks-request-headers
  tasks-dispatch-and-envelope tasks-status-notifications
  tasks-required-task-error tasks-mrtr-composition
)
for SCENARIO in "${TASKS[@]}"; do
  "$CONF" server --url "$URL" --scenario "$SCENARIO" \
    --expected-failures "$ROOT/conformance/expected-failures-extensions.yaml" \
    -o "$OUTPUT_DIR/server-extensions"
done

CLIENT="$UPSTREAM_TARGET/debug/conformance-client"
"$CONF" client --command "$CLIENT" --suite all --spec-version "$SPEC_VERSION" \
  -o "$OUTPUT_DIR/client-dated"
"$CONF" client --command "$CLIENT" --suite extensions \
  --expected-failures "$ROOT/conformance/expected-failures-extensions.yaml" \
  -o "$OUTPUT_DIR/client-extensions"
echo "MCP conformance matrix written to $OUTPUT_DIR"
