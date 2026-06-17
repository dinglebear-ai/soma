#!/usr/bin/env bash
# bump-version.sh — update version in all version-bearing files atomically.
#
# Usage:
#   ./scripts/bump-version.sh patch   # auto-increment patch
#   ./scripts/bump-version.sh minor   # auto-increment minor
#   ./scripts/bump-version.sh major   # auto-increment major

set -euo pipefail

REPO_ROOT="${CLAUDE_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
ARG="${1:-}"

case "$ARG" in
    major|minor|patch) ;;
    "") echo "Usage: $0 <major|minor|patch>"; exit 1 ;;
    *) echo "scripts/bump-version.sh now accepts only major, minor, or patch; use cargo xtask for component-aware bumps." >&2; exit 2 ;;
esac

cd "${REPO_ROOT}"
cargo xtask bump-version template "${ARG}"
echo "Done. Review CHANGELOG.md before tagging."
