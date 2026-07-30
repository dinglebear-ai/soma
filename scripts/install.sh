#!/usr/bin/env bash
set -euo pipefail

repo="${SOMA_MCP_REPO:-dinglebear-ai/soma}"
binary_name="soma"
install_dir="${INSTALL_DIR:-${SOMA_MCP_INSTALL_DIR:-${HOME}/.local/bin}}"
version="${SOMA_MCP_VERSION:-latest}"
release_base_url="${SOMA_MCP_RELEASE_BASE_URL:-}"

usage() {
  cat <<'USAGE'
Install soma from GitHub Releases.

Environment:
  INSTALL_DIR                   Destination directory (default: ~/.local/bin)
  SOMA_MCP_INSTALL_DIR          Legacy destination override
  SOMA_MCP_VERSION              Release tag such as v0.7.0 (default: latest)
  SOMA_MCP_REPO                 GitHub owner/repository
  SOMA_MCP_RELEASE_BASE_URL     Test or mirror release base URL
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

for command in curl install mktemp sha256sum tar; do
  command -v "$command" >/dev/null 2>&1 || {
    printf 'error: %s is required\n' "$command" >&2
    exit 1
  }
done

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  printf 'error: this installer supports Linux x86_64 only\n' >&2
  exit 1
fi

asset="${binary_name}-linux-x86_64.tar.gz"
if [[ -n "$release_base_url" ]]; then
  base="${release_base_url%/}/${version}"
elif [[ "$version" == "latest" ]]; then
  base="https://github.com/${repo}/releases/latest/download"
else
  base="https://github.com/${repo}/releases/download/${version}"
fi

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
curl --fail --location --silent --show-error --retry 3 \
  "$base/$asset" --output "$temporary/$asset"
curl --fail --location --silent --show-error --retry 3 \
  "$base/$asset.sha256" --output "$temporary/$asset.sha256"

(
  cd "$temporary"
  sha256sum --check "$asset.sha256"
)

members="$(tar -tzf "$temporary/$asset")"
if [[ "$members" != "$binary_name" ]]; then
  printf 'error: archive must contain exactly %s; got:\n%s\n' \
    "$binary_name" "$members" >&2
  exit 1
fi
tar -xzf "$temporary/$asset" -C "$temporary" "$binary_name"

mkdir -p "$install_dir"
if [[ ! -w "$install_dir" ]]; then
  printf 'error: install directory is not writable: %s\n' "$install_dir" >&2
  exit 1
fi
install -m 0755 "$temporary/$binary_name" "$install_dir/$binary_name"
printf 'Installed %s to %s/%s\n' "$binary_name" "$install_dir" "$binary_name"
