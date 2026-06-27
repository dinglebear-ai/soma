---
title: "Linux CI Runner"
doc_type: "guide"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "operators"
  - "agents"
scope: "template"
source_of_truth: false
last_reviewed: "2026-06-27"
---

# Linux CI Runner

Linux GitHub Actions jobs run on a Dockerized self-hosted runner on TOOTIE:

```yaml
runs-on: [self-hosted, linux-lab, rmcp-template]
```

The runner is repo-scoped to `jmagar/template-rmcp` and mirrors the proven lab
runner layout.

## Live Layout

| Purpose | Path on TOOTIE |
|---|---|
| Compose project | `/mnt/cache/compose/actions-runner/rmcp-template` |
| Compose file | `/mnt/cache/compose/actions-runner/rmcp-template/docker-compose.yml` |
| Startup script | `/mnt/cache/compose/actions-runner/rmcp-template/start.sh` |
| GitHub PAT env file | `/mnt/cache/compose/actions-runner/rmcp-template/.env` |
| Persistent runner home | `/mnt/cache/appdata/actions-runner/rmcp-template/home` |
| Persistent work/cache root | `/mnt/cache/appdata/actions-runner/rmcp-template/work` |
| Persistent temp dir | `/mnt/cache/appdata/actions-runner/rmcp-template/tmp` (`1777`, sticky world-writable) |

Current runners:

| Runner | Labels |
|---|---|
| `tootie-rmcp-template-linux-a` | `self-hosted`, `Linux`, `X64`, `linux-lab`, `rmcp-template`, `tootie` |
| `tootie-rmcp-template-linux-b` | `self-hosted`, `Linux`, `X64`, `linux-lab`, `rmcp-template`, `tootie` |
| `tootie-rmcp-template-linux-c` | `self-hosted`, `Linux`, `X64`, `linux-lab`, `rmcp-template`, `tootie` |

Verify from this repo:

```bash
gh api repos/jmagar/template-rmcp/actions/runners \
  -q '.runners[] | [.name,.status,.busy,(.labels|map(.name)|join(","))] | @tsv'
```

## Docker Compose Pattern

The compose service uses `ghcr.io/actions/actions-runner:2.335.1`, JIT
registration, and a repo-scoped runner name:

```yaml
services:
  rmcp-template-linux-runner:
    image: ghcr.io/actions/actions-runner:2.335.1
    container_name: rmcp-template-linux-runner
    restart: unless-stopped
    group_add:
      - "281"
    working_dir: /home/runner
    environment:
      - RUNNER_REPO=jmagar/template-rmcp
      - RUNNER_NAME=tootie-rmcp-template-linux
      - RUNNER_LABELS=linux-lab,rmcp-template,tootie,self-hosted,linux,x64
      - RUNNER_WORKDIR=/home/runner/_work
      - RUNNER_URL=https://github.com/jmagar/template-rmcp
      - RUNNER_USE_JIT=1
      - TMPDIR=/tmp
      - TMP=/tmp
      - TEMP=/tmp
      - RUNNER_TEMP=/home/runner/_work/_temp
      - CARGO_HOME=/home/runner/.cargo
      - RUSTUP_HOME=/home/runner/.rustup
      - SCCACHE_DIR=/home/runner/_work/.sccache
    env_file:
      - .env
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - /mnt/cache/appdata/actions-runner/rmcp-template/home:/home/runner
      - /mnt/cache/appdata/actions-runner/rmcp-template/work:/home/runner/_work
      - /mnt/cache/appdata/actions-runner/rmcp-template/tmp:/tmp
      - /mnt/cache/compose/actions-runner/rmcp-template/start.sh:/start.sh:ro
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
chmod 1777 /mnt/cache/appdata/actions-runner/rmcp-template/tmp
```

Without that, Linux package tools fail before the Rust setup step can install a
C linker.

## Manage The Runner

```bash
ssh tootie
cd /mnt/cache/compose/actions-runner/rmcp-template
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

Workflow Rust jobs also set:

```yaml
CARGO_INCREMENTAL: "0"
RUSTC_WRAPPER: sccache
CARGO_BUILD_RUSTC_WRAPPER: sccache
SCCACHE_DIR: ${{ github.workspace }}/../.sccache
```

`CARGO_INCREMENTAL=0` is required because incremental compilation and sccache do
not compose cleanly. `CARGO_BUILD_RUSTC_WRAPPER=sccache` intentionally overrides
the repo's local `scripts/cargo-rustc-wrapper`; CI should cache compilation, not
sync freshly built binaries into `./bin`.

## Required Host Capabilities

The runner container needs:

- Docker socket access for `container-smoke` and Docker publish workflows.
- Network access to GitHub, crates.io, npm, and GHCR.
- A persistent `/mnt/cache/appdata/actions-runner/rmcp-template` tree.
- `GITHUB_PAT` in the compose `.env` file for runner registration.

Rust, sccache, and basic Linux build prerequisites (`build-essential`,
`pkg-config`, `libssl-dev`) are installed by
`.github/actions/setup-rust-sccache` inside the workflow, so the container does
not need them baked in.

## When To Use This Runner

Use the TOOTIE runner for trusted repo code: pushes, same-repo PRs, scheduled
maintenance, and release automation.

Do not run untrusted fork PR code on this runner. If the repo becomes public,
add same-repo PR guards to self-hosted jobs or route fork PRs to GitHub-hosted
runners.

## Troubleshooting

- **Runner appears offline immediately after start**: check
  `docker logs rmcp-template-linux-runner`; if `run.sh` is missing, seed the
  persistent home with the runner distribution files.
- **Job waits for a runner**: verify the workflow labels exactly match
  `self-hosted`, `linux-lab`, and `rmcp-template`.
- **Cargo ignores sccache**: check the setup action output for
  `RUSTC_WRAPPER=sccache` and `CARGO_BUILD_RUSTC_WRAPPER=sccache`.
- **Docker jobs fail**: confirm `/var/run/docker.sock` is mounted and the host
  Docker daemon is healthy.
- **Registration fails**: refresh the compose `.env` token and restart the
  container.
