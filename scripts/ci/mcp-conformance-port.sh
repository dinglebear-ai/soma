#!/usr/bin/env bash

# Cooperative port allocation for mcp-conformance.sh. The caller keeps the
# lock until its Soma child exits; non-cooperating bind races are handled by
# retrying the child launch in the main script.

PORT_LOCK="${PORT_LOCK:-}"

release_conformance_port() {
  if [[ -n "${PORT_LOCK:-}" ]]; then
    rmdir "$PORT_LOCK" 2>/dev/null || true
    PORT_LOCK=""
  fi
}

reserve_conformance_port() {
  local start candidate candidate_lock offset
  if [[ -n "${PORT:-}" ]]; then
    start="$PORT"
  else
    start=$((20000 + ($$ % 30000)))
  fi
  for offset in $(seq 0 999); do
    if [[ -n "${MCP_CONFORMANCE_PORT:-}" ]]; then
      candidate="$start"
    else
      candidate=$((20000 + ((start - 20000 + offset) % 30000)))
    fi
    candidate_lock="${TMPDIR:-/tmp}/soma-mcp-conformance-port-${candidate}.lock"
    if mkdir "$candidate_lock" 2>/dev/null; then
      if ! (exec 3<>"/dev/tcp/127.0.0.1/${candidate}") 2>/dev/null; then
        PORT="$candidate"
        PORT_LOCK="$candidate_lock"
        return 0
      fi
      rmdir "$candidate_lock" 2>/dev/null || true
    fi
    if [[ -n "${MCP_CONFORMANCE_PORT:-}" ]]; then
      break
    fi
  done
  echo "unable to reserve a free conformance port${MCP_CONFORMANCE_PORT:+: ${MCP_CONFORMANCE_PORT}}" >&2
  return 1
}
