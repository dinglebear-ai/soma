#!/usr/bin/env bash
# check-version-sync.sh — compatibility wrapper for the manifest-backed gate.
set -euo pipefail

PROJECT_DIR="${1:-.}"
cd "$PROJECT_DIR"

cargo xtask check-version-sync
