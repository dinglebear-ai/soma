#!/usr/bin/env bash
# Compatibility wrapper. Canonical implementation: cargo xtask check-file-size.
set -euo pipefail

cargo xtask check-file-size "$@"
