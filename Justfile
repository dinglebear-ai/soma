# =============================================================================
# Justfile — Development and deployment commands for Soma
#
# CUSTOMIZE: Soma is the product binary; internal crate names still use soma in this compatibility pass.
#           Replace port 40060 with your service's port if different.
#
# Usage: just <recipe>
# =============================================================================

# List all available recipes
default:
    @just --list

# ── Development ───────────────────────────────────────────────────────────────

# Install project toolchain and helper CLIs managed by mise
install-tools:
    mise install

# Bootstrap a local checkout for development
bootstrap: install-tools install-hooks

# Run the MCP server in development mode (HTTP transport 40060, no auth)
# WARNING: SOMA_MCP_NO_AUTH=true is safe only because HOST is 127.0.0.1 (loopback)
dev:
    SOMA_MCP_HOST=127.0.0.1 SOMA_MCP_NO_AUTH=true cargo run --bin soma -- serve

# Run in stdio MCP transport mode (for Claude Desktop or direct pipe)
mcp:
    cargo run --bin soma -- mcp

# Run a quick CLI greeting (smoke test without a running server)
greet:
    cargo run --bin soma -- greet --name "Developer"

# Run the doctor pre-flight check
doctor:
    cargo run --bin soma -- doctor

# ── Building ──────────────────────────────────────────────────────────────────

# Compile debug build (fast, includes debug symbols)
build:
    cargo build

# Compile the lightweight local/plugin binary only
build-local:
    cargo build --bin soma --no-default-features --features local-adapter

# Compile optimized release build (slower compile, much faster runtime)
build-release:
    cargo build --release

# Compile the lightweight local/plugin release binary only
build-local-release:
    cargo build --release --bin soma --no-default-features --features local-adapter

# Compile the full server release binary only
build-server-release:
    cargo build --release --bin soma --features full

# Build the Next.js web UI static export (required before cargo build embeds it)
# Output lands in apps/web/out/ and is baked into the binary via the `web` feature
build-web:
    cargo xtask build-web

# Watch apps/web for changes and rebuild on save (requires watchexec from mise)
web-watch:
    cargo xtask web-watch

# Run frontend lint, typecheck, tests, and static build
web-check:
    pnpm -C apps/web validate

# Build the full binary with embedded web assets (runs build-web first)
build-full: build-web build-server-release

# Compile optimized release build (short alias used across the Rust server repos)
release: build-release

# ── Code quality ──────────────────────────────────────────────────────────────

# Run cargo check (fast syntax/type check, no binary output)
check:
    cargo check

# Generate Rust API documentation (rustdoc) for all workspace crates (no deps)
doc:
    cargo xtask doc

# Generate Rust API docs and open them in a browser
doc-open:
    cargo xtask doc --open

# Build rustdoc with warnings as errors (mirrors CI; run before pushing)
doc-check:
    cargo xtask doc --strict

# Check Rust formatting without modifying files (used in CI + lefthook)
fmt-check:
    cargo fmt -- --check

# Run the full test suite using cargo-nextest (faster, better output than cargo test)
# nextest cannot execute doctests (cargo-nextest#16), so chase it with the
# dedicated doctest runner — same pair CI's test job runs.
test:
    cargo nextest run
    cargo test --doc --workspace

# Run tests with the CI profile (fail-fast, 2 retries — mirrors CI)
test-ci:
    cargo nextest run --profile ci

# Run clippy with warnings as errors (matches CI)
lint:
    cargo clippy --all-targets -- -D warnings

# Format all Rust source files
fmt:
    cargo fmt

# Auto-fix clippy warnings and format in one pass
fix:
    cargo fmt
    cargo clippy --fix --all-targets --allow-dirty --allow-staged

# Format all TOML files (requires taplo from mise)
fmt-toml:
    taplo format

# Check TOML format without modifying files (used in CI + lefthook)
check-toml:
    taplo check

# Run license, vulnerability, and source checks (requires cargo-deny from mise)
deny:
    cargo deny check

# Watch Rust checks interactively (requires bacon from mise)
watch:
    bacon

# Generate Rust coverage report (requires cargo-llvm-cov)
test-cov:
    cargo llvm-cov --html --workspace --all-features

# Report dependency updates without modifying Cargo.lock
deps-check:
    cargo xtask check-dependency-updates

# Fail if changed blobs exceed the repo size budget
blob-size-check:
    cargo xtask check-blob-size

# Check coupled files such as Justfile/lefthook and scripts/docs
coupled-files-check:
    cargo xtask check-coupled-files

# Check tracked source/config/docs for non-ASCII characters
ascii-check:
    cargo xtask run-ascii-check

# Replace common smart punctuation with ASCII in tracked source/config/docs
ascii-fix:
    cargo xtask run-ascii-check --fix

# Check staged source files against line-count budgets
file-size-check:
    cargo xtask check-file-size

# Regenerate MCP schema contract docs from crates/soma/mcp/src/schemas.rs
schema-docs:
    cargo xtask check-schema-docs --write

# Verify MCP schema contract docs and action surfaces are in sync
schema-docs-check:
    cargo xtask check-schema-docs --check

# Regenerate volatile docs and metadata from canonical Rust specs
generate-docs:
    cargo xtask generate-docs

# Verify generated docs and metadata are current
check-docs:
    cargo xtask check-docs

# Verify stale hardcoded Soma claims have not reappeared
check-stale-claims:
    cargo xtask check-stale-claims

# Regenerate OpenAPI docs for the REST API surface
openapi:
    cargo xtask check-openapi --write

# Verify generated OpenAPI docs are current
openapi-check:
    cargo xtask check-openapi --check

# Validate scaffold intent JSON Schema and checked-in examples
scaffold-contract-check:
    cargo xtask check-scaffold-intent-contract

# Check static contracts from docs/PATTERNS.md
patterns-check:
    cargo xtask patterns

# Check PATTERNS.md contracts and fail on warnings
patterns-strict:
    cargo xtask patterns --strict

# Emit PATTERNS.md contract findings as JSON
patterns-json:
    cargo xtask patterns --json

# Run local static/spec contract checks without contacting live upstream services
contract-audit:
    cargo xtask contract-audit

# Run shell/Rust-adjacent Soma invariant smoke tests
soma-features:
    cargo xtask test-soma-features

# Run fast Soma-specific checks
soma-check:
    just contract-audit
    just validate-plugin

# Check fleet plugin hooks, manifests, and required operator recipes
fleet-alignment-check:
    cargo xtask check-plugin-hook-contract

# Run all local quality checks in sequence: fmt-check → lint → check → test → doc-check
verify:
    just fmt-check
    just lint
    just check
    just test
    just doc-check

# Preview the path-aware local pre-push plan without running checks
pre-push-plan:
    python3 scripts/ci/pre_push.py --dry-run

# Run the same path-aware local pre-push checks as lefthook
pre-push:
    python3 scripts/ci/pre_push.py

# Run the full local pre-push validation suite
pre-push-full:
    SOMA_FULL_PRE_PUSH=1 python3 scripts/ci/pre_push.py

# Ensure pre-commit stays limited to fast staged-file checks
lefthook-speed-check:
    python3 scripts/check_lefthook_pre_commit_speed.py

# Run all quality checks in sequence (mirrors CI pipeline)
# Delegates to cargo xtask ci for the full suite (fmt, clippy, nextest, taplo, audit)
ci:
    cargo xtask ci

# Remove build artifacts and generated files
clean:
    cargo clean
    rm -rf .cache/ dist/

# ── xtask automation ─────────────────────────────────────────────────────────

# Local operator convenience: build the release binary and copy it to dist/.
# GitHub releases publish binaries as artifacts; this recipe does not update main.
dist:
    cargo xtask dist

# Create AGENTS.md and GEMINI.md symlinks next to every CLAUDE.md in the repo.
# Pattern §32: CLAUDE.md is the single source of truth for project instructions.
# Run after adding any new CLAUDE.md file.
symlink-docs:
    cargo xtask symlink-docs

# Inline version of symlink-docs — no xtask required.
# CUSTOMIZE: Use this if xtask is unavailable (e.g. before first cargo build).
symlink-docs-inline:
    find . -name "CLAUDE.md" -not -path "./.git/*" -not -path "./target/*" \
        -exec sh -c 'dir=$(dirname "$1"); ln -sf CLAUDE.md "${dir}/AGENTS.md"; ln -sf CLAUDE.md "${dir}/GEMINI.md"; echo "  link ${dir}/AGENTS.md + ${dir}/GEMINI.md"' _ {} \;

# Validate required environment variables are set before starting the server.
check-env:
    cargo xtask check-env

# Install lefthook git hooks
install-hooks:
    lefthook install

# Uninstall lefthook git hooks
uninstall-hooks:
    lefthook uninstall

# ── Utilities ─────────────────────────────────────────────────────────────────

# Generate a cryptographically random bearer token for SOMA_MCP_TOKEN
# Copy the output into your .env file
gen-token:
    openssl rand -hex 32

# Copy .env.example to .env (safe — won't overwrite an existing .env)
setup:
    cp -n .env.example .env || echo ".env already exists — skipping"
    @echo "Edit .env and fill in your credentials"

# ── Docker ────────────────────────────────────────────────────────────────────

# Build the Docker image from source (does not start the container)
docker-build:
    docker build -f config/Dockerfile -t soma .

# Start the Docker Compose stack in detached mode
# CUSTOMIZE: The compose file references the "jakenet" external network.
#           Create it first if it doesn't exist: docker network create jakenet
docker-up:
    docker compose up -d

# Stop and remove the Docker Compose stack (data volume persists)
docker-down:
    docker compose down

# Short alias for docker-up
up: docker-up

# Short alias for docker-down
down: docker-down

# Restart the running container (faster than down+up; no image rebuild)
restart:
    docker compose restart

# Rebuild the Docker image from source and restart the stack
docker-rebuild:
    docker compose build --no-cache
    docker compose up -d --force-recreate

# Compile an optimized-but-fast host binary for local iteration.
# Uses the `release-fast` profile (release opts, no LTO, many codegen units) so
# the binary behaves like release while compiling in a fraction of the time.
build-fast:
    cargo build --profile release-fast --bin soma --features full

# Fast "edit → rebuild image → check in browser" loop.
# Unlike docker-rebuild's --no-cache full build, this reuses BuildKit layer and
# cargo cache mounts so only changed crates recompile, then recreates just this
# service. Reach for this during active container dev; use docker-rebuild only
# when you need a guaranteed-clean image.
sync-container:
    DOCKER_BUILDKIT=1 docker compose build
    docker compose up -d --force-recreate

# Follow Docker container logs
docker-logs:
    docker compose logs -f

# Short alias for docker-logs
logs:
    docker compose logs -f

# ── Health & diagnostics ──────────────────────────────────────────────────────

# Check the MCP server health endpoint (no auth required)
# CUSTOMIZE: Change port 40060 if you use a different port
health:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v jq >/dev/null 2>&1; then
        curl -sf http://localhost:40060/health | jq .
    else
        curl -sf http://localhost:40060/health | python3 -m json.tool
    fi

# Verify that the running Docker/systemd service matches the current artifact
runtime-current:
    cargo xtask check-runtime-current --expected-binary target/release/soma

# Smoke-test the protected MCP HTTP auth path (requires running bearer-auth server)
auth-smoke:
    cargo xtask test-mcp-auth

# Call the status action through the protected MCP HTTP path (requires SOMA_MCP_TOKEN in env)
status:
    #!/usr/bin/env bash
    set -euo pipefail
    TOKEN="${SOMA_MCP_TOKEN:-}"
    if [[ -z "${TOKEN}" ]]; then
        echo "Set SOMA_MCP_TOKEN or use 'just dev' (no-auth mode)"
        exit 1
    fi
    curl -sf http://localhost:40060/mcp \
        -H "Authorization: Bearer ${TOKEN}" \
        -H "Content-Type: application/json" \
        -H "Accept: application/json, text/event-stream" \
        -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"soma","arguments":{"action":"status"}}}' \
        | { if command -v jq >/dev/null 2>&1; then jq .; else python3 -m json.tool; fi; }

# ── Plugin ────────────────────────────────────────────────────────────────────

# Repair: stop, rebuild, and restart via systemd user unit or Docker Compose
repair:
    cargo xtask repair

# Validate plugin metadata. Plugins launch the installed PATH binary and do not
# bundle a release artifact.
build-plugin: validate-plugin

# Explicit local binary sync. Soma plugins launch the installed PATH binary instead of bundling bin/.
sync-bin: install-local

# Install the release binary on the local PATH.
install: install-local

# Install the release binary on the local PATH for runtime smoke testing
install-local: build-local-release
    mkdir -p "${HOME}/.local/bin"
    install -m 755 target/release/soma "${HOME}/.local/bin/soma"
    @echo "Installed ${HOME}/.local/bin/soma"

# Validate all plugin manifests, MCP config, hooks, and skills
validate-plugin:
    cargo xtask validate-plugin-layout

# Validate all plugin skills have required SKILL.md fields
validate-skills: validate-plugin

# ── mcporter ─────────────────────────────────────────────────────────────────

# Run mcporter-based integration tests (requires running server + mcporter CLI)
# CUSTOMIZE: Ensure the server is running first: just dev   or   just docker-up
test-mcporter:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v mcporter &>/dev/null; then
        echo "mcporter not found. Install it first."
        exit 1
    fi
    bash apps/soma/tests/mcporter/test-mcp.sh

# ── MCP conformance ────────────────────────────────────────────────────────────

# Run the official MCP conformance suite against a locally booted server.
# Boots a loopback no-auth server, waits for /health, runs the suite, tears down.
# Requires npx (Node). Suite: active (latest dated spec, default) | draft | all | pending
# Defaults to port 41060 to avoid colliding with a live server on the default 40060.
# CUSTOMIZE: adjust the default port and binary name if you renamed them.
conformance suite="active" port="41060":
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v npx >/dev/null 2>&1; then
        echo "npx (Node.js) not found — install Node to run the conformance suite."
        exit 1
    fi
    PORT={{port}}
    URL="http://127.0.0.1:${PORT}/mcp"
    # Pre-flight: refuse to run if the port is already taken — otherwise we would
    # silently test whatever server already owns it (e.g. a live deployment).
    if ss -tlnH 2>/dev/null | grep -qE "[^0-9]${PORT}\b"; then
        echo "Port ${PORT} is already in use. Pick a free port: just conformance {{suite}} <port>"
        exit 1
    fi
    echo "Building server (default features)..."
    cargo build --bin soma
    echo "Starting loopback no-auth server on ${PORT}..."
    SOMA_MCP_HOST=127.0.0.1 SOMA_MCP_PORT=${PORT} SOMA_MCP_NO_AUTH=true \
        SOMA_MCP_CONFORMANCE_FIXTURES=true \
        ./target/debug/soma serve >/tmp/soma-conformance-server.log 2>&1 &
    SERVER_PID=$!
    trap 'kill ${SERVER_PID} 2>/dev/null || true' EXIT
    echo "Waiting for /health on ${PORT}..."
    for _ in $(seq 1 50); do
        if curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then break; fi
        sleep 0.2
    done
    # Hard guard: ensure OUR server is the one answering, not a pre-existing one.
    if ! kill -0 ${SERVER_PID} 2>/dev/null; then
        echo "Server failed to start (likely bind error). Log:"
        cat /tmp/soma-conformance-server.log
        exit 1
    fi
    echo "Running MCP conformance suite '{{suite}}' against ${URL}..."
    # --expected-failures fences known gaps so this exits non-zero only on a NEW
    # regression (or flags a baselined scenario that started passing as stale).
    npx -y @modelcontextprotocol/conformance server --url "${URL}" --suite {{suite}} \
        --expected-failures conformance-baseline.yml

# Summarize official conformance results/checks.json output.
conformance-report:
    python3 scripts/conformance_report.py --results results

# Run the release-readiness gate
pre-release:
    cargo xtask pre-release-check

# Generate a standalone CLI for this server via mcporter (requires running server)
# CUSTOMIZE: Update port and token env var name in scripts/generate-cli.sh
generate-cli:
    cargo xtask generate-cli

# ── Publishing ────────────────────────────────────────────────────────────────

# Local emergency helper only. Normal releases are managed by release-please.
bump-version bump="patch":
    cargo xtask bump-version soma {{bump}}

# Releases are created by release-please after a Conventional Commit lands on main.
publish:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo xtask check-version-sync
    cargo xtask generate-provider-surfaces --check
    echo "Release flow: merge a feat:/fix:/deps: commit, then merge the release-please PR."

# ── Reference docs ────────────────────────────────────────────────────────────

# Refresh local reference documentation (crawls + repomix)
refresh-docs:
    cargo xtask refresh-docs

# Refresh docs — repomix packs only (no crawl)
refresh-docs-repomix:
    cargo xtask refresh-docs --skip-crawl

# Refresh docs — crawl only (no repomix)
refresh-docs-crawl:
    cargo xtask refresh-docs --skip-repomix

# Dry-run: print what would be refreshed
refresh-docs-dry:
    cargo xtask refresh-docs --dry-run
