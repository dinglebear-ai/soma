# plugins

Claude Code, Codex, and Gemini plugin packages for the MCP server. Marketplace
manifests intentionally do not bundle an MCP server registration; the server is
expected to be connected through the user's existing gateway or local MCP setup.
All platforms share the same skills and lifecycle hooks.

## Structure

```
plugins/soma/
├── .claude-plugin/
│   └── plugin.json       # Claude Code manifest
├── .codex-plugin/
│   ├── plugin.json       # Codex manifest
│   └── README.md         # Codex manifest field reference
├── gemini-extension.json # Gemini extension manifest
├── hooks/
│   └── hooks.json        # Lifecycle hook definitions (call the binary directly)
└── skills/
    ├── soma/
    │   └── SKILL.md      # Tool documentation for Claude and Codex
    └── scaffold-project/
        └── SKILL.md      # Turns scaffold_intent JSON into an approval-first plan
```

---

## Manifests

### `.claude-plugin/plugin.json`

Claude Code plugin manifest. Defines the plugin identity, lifecycle hooks, and user-configurable options.

**User config fields** (set via Claude Code plugin settings):

| Field | Type | Description |
|---|---|---|
| `server_url` | string | Optional MCP HTTP server base URL for fallback/monitoring |
| `api_token` | string (sensitive) | Optional bearer token for HTTP fallback auth |
| `no_auth` | boolean | Disable auth (loopback dev only; non-loopback requires an upstream gateway) |
| `auth_mode` | string | `bearer` or `oauth` |
| `public_url` | string | Public URL for OAuth callbacks |
| `google_client_id` | string (sensitive) | Google OAuth client ID |
| `google_client_secret` | string (sensitive) | Google OAuth client secret |
| `auth_admin_email` | string | OAuth admin email |
| `soma_api_url` | string | Deployed platform API or upstream service URL used by stdio adapter |
| `soma_api_key` | string (sensitive) | Deployed API bearer token or upstream service API key |

Soma maps these plugin options into the current `SOMA_API_URL` /
`SOMA_API_KEY` runtime env names for compatibility.

The plugin settings surface currently exposes Google credentials only. The
server runtime also supports Authelia and GitHub, but those providers must be
configured with the direct `SOMA_MCP_AUTHELIA_*`, `SOMA_MCP_GITHUB_*`, and
`SOMA_MCP_AUTH_DEFAULT_PROVIDER` environment variables documented in
[`docs/ENV.md`](../docs/ENV.md).

### `.codex-plugin/plugin.json`

Codex equivalent of the Claude Code manifest. Shares `skills/` with the Claude plugin. Adds Codex-specific UI fields under `interface`:

- `displayName`, `shortDescription`, `longDescription` — registry presentation
- `defaultPrompt` — three sample prompts shown in the Codex UI
- `brandColor` — hex color for the plugin icon (e.g., `#6366F1`)
- `composerIcon`, `logo` — asset paths (512×512 PNG, SVG)

See `.codex-plugin/README.md` for a full field reference and `brandColor` guide.

## Hooks

### `hooks/hooks.json`

Defines two lifecycle hooks:

| Hook | Trigger | Command |
|---|---|---|
| `SessionStart` | Every Claude Code session start | `soma setup plugin-hook` |
| `ConfigChange` | User updates plugin settings | `soma setup plugin-hook` |

Timeout: 300 seconds.

### `soma setup plugin-hook`

The lifecycle command. Runs on every session start and config change, called directly by `hooks.json` (no shell wrapper).

- Reads `CLAUDE_PLUGIN_OPTION_*` env vars from plugin `userConfig` and maps them to the binary's `SOMA_*` runtime env vars (`apply_plugin_options()` in `crates/soma/cli/src/setup.rs`)
- Runs from the binary already installed on `PATH`
- Prepares the plugin appdata directory
- Checks/repairs setup and emits the JSON hook contract

Deployment policy, repair behavior, env-var mapping, and failure classification all live in the Rust binary. The former `plugin-setup.sh` wrapper was a pure env-mapping middleman and has been removed.

---

## Skills

### `skills/soma/SKILL.md`

Three-tier structured documentation for the `soma` MCP tool, used by both Claude Code and Codex to understand when and how to invoke the tool.

**Tier 1** (above the fold): tool name, quick action table, most common usage.  
**Tier 2**: full action reference — parameters, types, example calls, response shapes.  
**Tier 3**: multi-step workflows demonstrating real-world use.

Also includes HTTP fallback examples using `CLAUDE_PLUGIN_OPTION_SERVER_URL` and `CLAUDE_PLUGIN_OPTION_API_TOKEN` env vars for when the MCP connection isn't available.

Soma ships with the action table and examples generated from its canonical action registry.

---

## Version sync

`Cargo.toml` is the canonical package version. Version-bearing files must stay in sync when you bump it:

| File | Field |
|---|---|
| `Cargo.toml` | `version` |
| `Cargo.lock` | package version |
| `server.json` | MCP Registry version |

Claude, Codex, and Gemini plugin manifests intentionally do not contain a `version` field. Marketplace/plugin versioning is derived from git metadata.

Use `scripts/bump-version.sh patch` (or `minor`/`major`) to update version-bearing files atomically.

---

## Scaffold Checklist

When adapting this plugin for a real service:

1. Replace Soma identifiers and `SOMA_` env vars with your service name and env prefix.
2. Update `userConfig` in both `plugin.json` files to match your service's credential fields.
3. Update `skills/soma/SKILL.md` with your actual actions, parameters, and examples.
4. Set `brandColor` in `.codex-plugin/plugin.json` to your service's color.
5. Replace `defaultPrompt` entries in the Codex manifest with realistic prompts for your service.
6. Run `scripts/bump-version.sh` after any version change.
