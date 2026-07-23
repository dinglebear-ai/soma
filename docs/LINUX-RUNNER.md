---
title: "Linux CI Runner"
doc_type: "guide"
status: "active"
owner: "soma"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "soma"
source_of_truth: false
last_reviewed: "2026-06-27"
---

# Linux CI Runner

Linux GitHub Actions jobs run on a Dockerized self-hosted runner on TOOTIE:

```yaml
runs-on: [self-hosted, tootie, rmcp-template]
```

The runner is repo-scoped to `dinglebear-ai/soma` and mirrors the proven lab
runner layout.

## Live Layout

| Purpose | Path on TOOTIE |
|---|---|
| Compose project | `/mnt/cache/compose/actions-runner/soma` |
| Compose file | `/mnt/cache/compose/actions-runner/soma/docker-compose.yml` |
| Startup script | `/mnt/cache/compose/actions-runner/soma/start.sh` |
| GitHub PAT env file | `/mnt/cache/compose/actions-runner/soma/.env` |
| Persistent runner home | `/mnt/cache/appdata/actions-runner/soma/home` |
| Persistent work/cache root | `/mnt/cache/appdata/actions-runner/soma/work` |
| Persistent temp dir | `/mnt/cache/appdata/actions-runner/soma/tmp` (`1777`, sticky world-writable) |

Current runners:

| Runner | Labels |
|---|---|
| `tootie-soma-linux-a` | `self-hosted`, `Linux`, `X64`, `soma`, `tootie` |
| `tootie-soma-linux-b` | `self-hosted`, `Linux`, `X64`, `soma`, `tootie` |
| `tootie-soma-linux-c` | `self-hosted`, `Linux`, `X64`, `soma`, `tootie` |

Verify from this repo:

```bash
gh api repos/dinglebear-ai/soma/actions/runners \
  -q '.runners[] | [.name,.status,.busy,(.labels|map(.name)|join(","))] | @tsv'
```

## Docker Compose Pattern

The compose service uses `ghcr.io/actions/actions-runner:2.335.1`, JIT
registration, and a repo-scoped runner name:

```yaml
services:
  soma-linux-runner:
    image: ghcr.io/actions/actions-runner:2.335.1
    container_name: soma-linux-runner
    restart: unless-stopped
    group_add:
      - "281"
    working_dir: /home/runner
    environment:
      - RUNNER_REPO=dinglebear-ai/soma
      - RUNNER_NAME=tootie-soma-linux
      - RUNNER_LABELS=soma,tootie,self-hosted,linux,x64
      - RUNNER_WORKDIR=/home/runner/_work
      - RUNNER_URL=https://github.com/dinglebear-ai/soma
      - RUNNER_USE_JIT=1
      - TMPDIR=/tmp
      - TMP=/tmp
      - TEMP=/tmp
      - RUNNER_TEMP=/home/runner/_work/_temp
      - CARGO_HOME=/home/runner/.cargo
      - RUSTUP_HOME=/home/runner/.rustup
      - SCCACHE_DIR=/home/runner/_work/.sccache
      - SCCACHE_CACHE_SIZE=5G
    env_file:
      - .env
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - /mnt/cache/appdata/actions-runner/soma/home:/home/runner
      - /mnt/cache/appdata/actions-runner/soma/work:/home/runner/_work
      - /mnt/cache/appdata/actions-runner/soma/tmp:/tmp
      - /mnt/cache/compose/actions-runner/soma/start.sh:/start.sh:ro
    command: ["/start.sh"]
```

The real compose file defines three services using the same shape, with
runner-specific names and separate `home`, `work`, and `tmp` appdata directories.
`group_add: ["281"]` is the TOOTIE Docker socket group; keep it in parity with
`stat -c '%g' /var/run/docker.sock` so `container-smoke` and Docker publish jobs
can reach the host Docker daemon.

The persistent `/home/runner` volume must contain the runner distribution files
(`run.sh`, `config.sh`, `bin/`, `externals/`). If the volume is empty, it hides
the files baked into the image and the container loops with `./run.sh: No such
file or directory`. Seed it from an existing working runner home before the
first start, then let `start.sh` remove stale local registration files.

The persistent temp mount must be sticky world-writable:

```bash
chmod 1777 /mnt/cache/appdata/actions-runner/soma/tmp
```

Without that, Linux package tools fail before the Rust setup step can install a
C linker.

## Manage The Runner

```bash
ssh tootie
cd /mnt/cache/compose/actions-runner/soma
docker compose ps
docker compose logs -f
docker compose up -d
docker compose restart
docker compose down
```

`start.sh` removes stale remote runner entries with the same name, requests a
fresh JIT config from the GitHub API, and starts `./run.sh --jitconfig ...`.
The `.env` file must provide `GITHUB_PAT` with permission to register repo
runners.

## Cache Model

Cargo and sccache data persist under the runner appdata volume:

- `CARGO_HOME=/home/runner/.cargo`
- `RUSTUP_HOME=/home/runner/.rustup`
- `SCCACHE_DIR=/home/runner/_work/.sccache`
- `SCCACHE_CACHE_SIZE=5G`

Workflow Rust jobs also set:

```yaml
CARGO_INCREMENTAL: "0"
RUSTC_WRAPPER: sccache
CARGO_BUILD_RUSTC_WRAPPER: sccache
SCCACHE_DIR: ${{ github.workspace }}/../.sccache
```

`CARGO_INCREMENTAL=0` is required because incremental compilation and sccache do
not compose cleanly. `CARGO_BUILD_RUSTC_WRAPPER=sccache` keeps CI focused on
cacheable compilation; artifact sync is explicit through recipes such as
`just sync-bin` or `just build-plugin`.

The persistent mounts are bounded by two guardrails:

- `SCCACHE_CACHE_SIZE=5G` caps each runner's local sccache store.
- `start.sh` prunes stale storage every time the JIT runner starts: `/tmp` and
  `_work/_temp` are cleared, old work directories are removed after 7 days,
  runner diagnostic logs after 14 days, and stale Cargo registry/git cache
  entries after 30 days.

These guards intentionally preserve warm caches for active work while preventing
old checkouts, temp files, and compile cache entries from growing without bound.

## Required Host Capabilities

The runner container needs:

- Docker socket access for `container-smoke` and Docker publish workflows.
- Network access to GitHub, crates.io, npm, and GHCR.
- A persistent `/mnt/cache/appdata/actions-runner/soma` tree.
- `GITHUB_PAT` in the compose `.env` file for runner registration.

Rust, sccache, and basic Linux build prerequisites (`build-essential`,
`pkg-config`, `libssl-dev`) are installed by
`.github/actions/setup-rust-sccache` inside the workflow, so the container does
not need them baked in.

## When To Use This Runner

Use the TOOTIE runner for trusted repo code: pushes, same-repo PRs, scheduled
maintenance, and release automation. `ci.yml` and `msrv.yml` first classify
changed paths with `cargo xtask changed-paths`; irrelevant heavyweight jobs are
allowed to skip and the stable aggregate `CI Gate` / `MSRV Gate` jobs convert
"passed or intentionally skipped" into the required status. Both workflows also
use same-repository job guards so fork PRs do not allocate self-hosted runners.

Do not run untrusted fork PR code on this runner. If the repo becomes public and
outside contributors need CI feedback, route fork PRs to GitHub-hosted runners.

## Troubleshooting

- **Runner appears offline immediately after start**: check
  `docker logs soma-linux-runner`; if `run.sh` is missing, seed the
  persistent home with the runner distribution files.
- **Job waits for a runner**: verify the workflow labels exactly match
  `self-hosted`, `tootie`, and `soma`.
- **Cargo ignores sccache**: check the setup action output for
  `RUSTC_WRAPPER=sccache` and `CARGO_BUILD_RUSTC_WRAPPER=sccache`.
- **Docker jobs fail**: confirm `/var/run/docker.sock` is mounted and the host
  Docker daemon is healthy.
- **Registration fails**: refresh the compose `.env` token and restart the
  container.
