#!/usr/bin/env bash
# Compatibility wrapper. Canonical implementation: cargo xtask check-plugin-stdio-smoke.
set -euo pipefail

cargo xtask check-plugin-stdio-smoke "$@"
