---
title: "Scripts"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "agents"
scope: "soma"
source_of_truth: false
upstream_refs:
  - "scripts/README.md"
last_reviewed: "2026-06-18"
---

# Scripts

Maintenance automation is owned by `cargo xtask`. The files in `scripts/` are thin wrappers for older docs, hooks, and operator muscle memory, except for generated-doc helper scripts that have not been migrated yet. The authoritative per-command usage reference is `scripts/README.md`.
The generated quick index is committed at
[`docs/generated/scripts-index.md`](generated/scripts-index.md) and is refreshed
by `cargo xtask generate-docs`.

## Categories

| Category | Scripts |
|---|---|
| Release gates | `cargo xtask pre-release-check`, `check-version-sync`, `check-blob-size`, `check-coupled-files` |
| Hygiene | `cargo xtask asciicheck`, `check-file-size`, `block-env-commits`, `run-ascii-check`, `check-stale-claims`, `check-readme-guide` |
| MCP/plugin validation | `cargo xtask check-schema-docs`, `validate-plugin-layout`, `check-plugin-hook-contract`, `test-mcp-auth` |
| Runtime/deploy | `cargo xtask check-runtime-current`, `sync-cargo`, `bump-soma-version` |
| Reference docs | `cargo xtask refresh-docs`, `generate-docs`, `check-docs` |

## Important commands

```bash
cargo xtask pre-release-check
cargo xtask pre-release-check --mcporter   # include live MCP tests
cargo xtask refresh-docs --dry-run
cargo xtask test-mcp-auth --url http://localhost:40060/mcp --token <token>
python3 scripts/check-readme-guide.py README.md
```

## pre-release-check

The full release gate. Runs:
1. `cargo xtask patterns`
2. plugin layout validation
3. schema docs validation
4. Soma feature smoke tests
5. release version gate
6. blob-size check
7. ASCII hygiene
8. `just verify`
9. `just build-plugin`

## refresh-docs

Fetches fresh reference material into `docs/references/`:

- **Axon crawls** — `axon crawl <url> --wait --yes` → markdown into `docs/references/<target>/`
- **Repomix packs** — `repomix --remote <repo> --style xml` → XML snapshot
- **Change tracking** — sha256 checksums before/after; appends diff summary to `docs/references/CHANGES.md`

```bash
just refresh-docs              # full refresh
just refresh-docs-repomix      # skip crawl
just refresh-docs-crawl        # skip repomix
just refresh-docs-dry          # dry run (no mutations)
```

`docs/references/` is gitignored — content is large, auto-generated, and should be fetched fresh. Run when starting development on a new feature, when the service releases a new API version, or monthly.

## install.sh pattern

The install script validates the environment before installing:

```bash
preflight() {
    local errors=0

    # 1. OS / arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    arch="$(uname -m)"
    [[ "${os}" == "linux" ]] || { echo "✗ Only Linux is supported"; (( errors++ )); }

    # 2. Required tools
    for cmd in curl tar grep; do
        command -v "${cmd}" >/dev/null || { echo "✗ ${cmd}: not found"; (( errors++ )); }
    done

    # 3. Disk space (need at least 50MB)
    free_mb="$(df -k "${HOME}" | awk 'NR==2{printf "%d", $4/1024}')"
    (( free_mb >= 50 )) || { echo "✗ Only ${free_mb}MB free (need 50MB)"; (( errors++ )); }

    return "${errors}"
}
```

One-line install:
```bash
curl -fsSL https://raw.githubusercontent.com/dinglebear-ai/soma/main/install.sh | bash
```

After install: `soma doctor` to validate the environment.

## block-env-commits

Prevents accidentally committing `.env` files with secrets. Allows only `.env.example`. Called by lefthook on every commit through `cargo xtask block-env-commits`.

## Contract

- `cargo xtask` owns script behavior; `scripts/` files should stay thin thin wrappers.
- Mutating scripts must be explicit about what they write.
- Release checks must be repeatable; generated plugin binaries are allowlisted in `scripts/blob-size-allowlist.txt`.
- Keep `scripts/README.md` current when adding, renaming, or changing xtask commands or wrappers.

See `docs/PATTERNS.md` §38 and §49 for the refresh-docs and install.sh patterns.
