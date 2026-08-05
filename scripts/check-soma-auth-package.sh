#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

version="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; data=json.load(sys.stdin); print(next(p["version"] for p in data["packages"] if p["name"] == "soma-auth"))')"
package="target/package/soma-auth-${version}.crate"

cargo package -p soma-auth --allow-dirty

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/soma-auth-package.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT
mkdir -p "$tmp_dir/unpacked" "$tmp_dir/consumer/src"
tar -xzf "$package" -C "$tmp_dir/unpacked"
crate_root="$tmp_dir/unpacked/soma-auth-${version}"

cat >"$tmp_dir/consumer/Cargo.toml" <<EOF
[package]
name = "soma-auth-external-consumer"
version = "0.0.0"
edition = "2024"
rust-version = "1.97.1"
publish = false

[features]
auth-http = ["soma-auth/http-axum"]
auth-upstream = ["soma-auth/upstream-oauth-rmcp"]
auth-all = ["auth-http", "auth-upstream"]

[dependencies]
soma-auth = { path = "$crate_root", default-features = false }
EOF

cat >"$tmp_dir/consumer/src/main.rs" <<'EOF'
use soma_auth::config::{AuthConfigBuilder, AuthProfile};

fn main() -> Result<(), soma_auth::error::AuthError> {
    let config = AuthConfigBuilder::from_profile(AuthProfile {
        env_prefix: "CONSUMER".into(),
        upstream_client_name: "consumer".into(),
        ..AuthProfile::default()
    })
    .build()?;
    assert_eq!(config.env_prefix, "CONSUMER");
    Ok(())
}
EOF

consumer_manifest="$tmp_dir/consumer/Cargo.toml"
cargo check --manifest-path "$consumer_manifest"
cargo check --manifest-path "$consumer_manifest" --features auth-http
cargo check --manifest-path "$consumer_manifest" --features auth-upstream
cargo check --manifest-path "$consumer_manifest" --features auth-all

echo "soma-auth ${version} packaged and compiled from a blank external consumer"
