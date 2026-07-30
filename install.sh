#!/usr/bin/env bash
# Published compatibility entrypoint. New documentation should link directly to
# scripts/install.sh; this root path remains functional for existing callers.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "${script_dir}/scripts/install.sh" ]]; then
  exec bash "${script_dir}/scripts/install.sh" "$@"
fi

command -v curl >/dev/null 2>&1 || {
  printf 'error: curl is required\n' >&2
  exit 1
}
curl --fail --location --silent --show-error --retry 3 \
  "https://raw.githubusercontent.com/dinglebear-ai/soma/main/scripts/install.sh" |
  bash -s -- "$@"
