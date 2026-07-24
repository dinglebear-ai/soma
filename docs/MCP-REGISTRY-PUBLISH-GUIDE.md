# MCP Registry Publishing Guide

This guide explains how to publish your MCP server to the
[official MCP registry](https://modelcontextprotocol.io/registry/quickstart)
using the `server.json` manifest at the repo root.

<!-- CUSTOMIZE: This entire guide is reusable as-is. The only values you need to
     change are your domain name, GitHub org/username, and service name.
     Search for "CUSTOMIZE:" in server.json to find the fields that need updating. -->

---

## Prerequisites

- You own the domain represented by the reverse-DNS namespace in `name`
  (for example, `ai.dinglebear` represents `dinglebear.ai` in `ai.dinglebear/soma`)
- Your Docker image is published to a container registry (e.g. `ghcr.io`)
- Your GitHub repo is public

---

## Step 1 — Update server.json

Edit `server.json` in the repo root. Every field marked `CUSTOMIZE:` must be replaced:

| Field | Replace with |
|---|---|
| `name` | Reverse-DNS namespace plus service, such as `com.example/your-service-mcp` |
| `title` | Human-readable display name, e.g. "My Service MCP" |
| `description` | One sentence about what your server does |
| `repository.url` | Your GitHub repo URL |
| `packages[].identifier` | Your package identifier, for example `ghcr.io/org/repo:version` |
| `environmentVariables[].name` | Your service's actual env var names |
| `environmentVariables[].description` | User-visible descriptions for registry UI |
| `remotes[0].url` | Your hosted `/mcp` endpoint (or remove `remotes` if not hosting publicly) |

---

## Step 2 — Install mcp-publisher

```bash
# Linux amd64
curl -fsSL \
  "https://github.com/modelcontextprotocol/registry/releases/latest/download/mcp-publisher_linux_amd64.tar.gz" \
  | tar xz mcp-publisher
chmod +x mcp-publisher
```

For other platforms, check the
[releases page](https://github.com/modelcontextprotocol/registry/releases).

---

## Step 3 — Authenticate

### Option A: DNS-based (domain ownership proof — preferred for named namespaces)

```bash
./mcp-publisher login dns \
  --domain yourdomain.com \
  --private-key "$MCP_PRIVATE_KEY"
```

The private key must correspond to an MCP DNS TXT record published at the
domain apex (`yourdomain.com`), not under an `_mcp` selector. See the registry
docs for the exact TXT record format.

### Option B: GitHub OAuth

```bash
./mcp-publisher login github
```

This grants you the `io.github.<your-username>/` namespace automatically,
for example `io.github.jmagar/soma-mcp`.

---

## Step 4 — Publish

```bash
./mcp-publisher publish
```

This reads `server.json` from the current directory and submits it to the registry.
On success, your server appears at:
`https://registry.modelcontextprotocol.io/servers/<your-name>`

---

## Step 5 — Automate via CI (recommended)

The `docker-publish.yml` workflow in Soma already includes a publish step
that runs automatically when you push a version tag (e.g. `v1.2.3`).

The relevant workflow snippet:

```yaml
- name: Set version in server.json
  run: |
    VERSION="${GITHUB_REF_NAME#v}"
    jq --arg v "$VERSION" \
       --arg img "ghcr.io/dinglebear-ai/soma:${VERSION}" \
       '.version = $v | (.packages[] | select(.registryType == "oci")) |= (.identifier = $img | del(.version, .registryBaseUrl))' \
       server.json > server.tmp && mv server.tmp server.json

- name: Publish to MCP registry
  env:
    MCP_PRIVATE_KEY: ${{ secrets.MCP_PRIVATE_KEY }}
  run: |
    ./mcp-publisher login dns --domain yourdomain.com --private-key "$MCP_PRIVATE_KEY"
    ./mcp-publisher publish
```

Add `MCP_PRIVATE_KEY` as a GitHub repository secret.

---

## Version management

`server.json` always reflects the **currently released version**. Do not manually
edit the `version` or `packages[0].identifier` fields — the `release.yml` workflow
updates them automatically when you push a version tag.

To release a new version:

```bash
git tag v1.2.3
git push origin v1.2.3
```

The `release.yml` workflow builds binaries, updates `server.json`, and triggers
`docker-publish.yml` which publishes the new Docker image and re-publishes to
the MCP registry.

---

## Troubleshooting

### "Name not in your namespace"

You must authenticate for the domain or GitHub user that prefixes your server name.
If your `name` is `ai.dinglebear/soma`, you must authenticate with DNS for
`dinglebear.ai`. If your `name` is `io.github.dinglebear-ai/soma`, use GitHub OAuth.

### "Invalid schema"

Run the JSON through the schema validator:

```bash
npx @modelcontextprotocol/registry-validator server.json
```

### "Image not found"

The `packages[0].identifier` OCI image must be publicly pullable before you publish.
Push to GHCR first, then publish to the registry.

---

## Registry namespace summary

| Namespace format | Auth method |
|---|---|
| `com.example/<name>` | DNS TXT record proof for `example.com` |
| `io.github.<org>/<name>` | GitHub OAuth |
