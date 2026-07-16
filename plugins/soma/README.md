# Soma Plugin

Multi-platform plugin package that documents and configures Claude Code, Codex,
and Gemini CLI access to the Soma MCP server.

The default MCP connection is the installed local stdio adapter:
`soma mcp`. For platform deployments, set
`soma_api_url` to the deployed `soma` REST API base URL so the local
adapter forwards business actions to that API. HTTP MCP remains available as a
manual fallback for remote/gateway deployments.

## Structure

```
plugins/soma/
├── .claude-plugin/
│   └── plugin.json         # Claude Code manifest
├── .codex-plugin/
│   ├── plugin.json         # Codex manifest
│   └── README.md           # Codex manifest field reference
├── gemini-extension.json   # Gemini CLI extension manifest
├── hooks/
│   └── hooks.json          # SessionStart + ConfigChange hook definitions (call the binary directly)
├── monitors/
│   └── monitors.json       # Background health monitor (requires Claude Code v2.1.105+)
└── skills/
    └── soma/
        └── SKILL.md        # Tool documentation (shared by Claude and Codex)
```

## Platform manifests

Claude Code and Codex package metadata lives in their platform manifests. Gemini
CLI embeds its `mcpServers` config inline in `gemini-extension.json` (its own
format). All three share the same `skills/` directory.

| File | Platform | MCP config | Variable syntax |
|---|---|---|---|
| `.claude-plugin/plugin.json` | Claude Code | external stdio config | `${user_config.*}` |
| `.codex-plugin/plugin.json` | Codex | external stdio config | `${user_config.*}` |
| `gemini-extension.json` | Gemini CLI | inline `mcpServers` | `${settings.*}` |

**No `version` field in any manifest.** The marketplace assigns version from the git commit SHA. Adding an explicit version creates duplicate entries on every push.

## MCP connection

When registering Soma with an MCP client, launch the installed binary in stdio
mode and pass the user-configured API target into the child process:

```json
{
  "mcpServers": {
    "soma": {
      "type": "stdio",
      "command": "soma",
      "args": ["mcp"],
      "env": {
        "SOMA_API_URL": "${user_config.soma_api_url}",
        "SOMA_API_KEY": "${user_config.soma_api_key}",
        "RUST_LOG": "warn"
      }
    }
  }
}
```

Gemini CLI uses the same shape inline in `gemini-extension.json` with
`${extensionPath}` for the installed extension directory and `${settings.*}` for
user settings. The `${user_config.*}` / `${settings.*}` variables are populated
from each platform's user-configurable settings at runtime.

## Hooks

`hooks/hooks.json` runs `soma setup plugin-hook` directly on `SessionStart` and `ConfigChange` (no shell wrapper).

The binary maps plugin settings (`CLAUDE_PLUGIN_OPTION_*`) to its `SOMA_*` environment variables via `apply_plugin_options()` (`crates/soma/cli/src/setup.rs`), self-installs into `~/.local/bin`, prepares appdata, and runs setup checks/repair.

## Monitors

**Requires Claude Code v2.1.105+.**

`monitors/monitors.json` declares an optional background `server-health` monitor.
It is not registered by default because the plugin's default MCP path is stdio
and does not require a local HTTP server. Projects that ship the full
`soma` HTTP profile can opt into this monitor from the Claude manifest.

When enabled, it runs `soma watch` from `PATH` and delivers each
stdout line to Claude as a notification whenever the HTTP server changes state.

The monitor emits only on state transitions — Claude is not notified while the server is stable. Three states:

- `UP` — `/health` returned 2xx
- `DOWN` — connection refused / timeout
- `DEGRADED(HTTP N)` — non-2xx HTTP response

The command requires `soma` to be installed on `PATH` before enabling the monitor:

```bash
just install-local
```

Disabling the plugin mid-session does not stop an already-running monitor; it stops when the session ends.

## Skills

`skills/soma/SKILL.md` is the three-tier structured documentation for the `soma` MCP tool. The AI reads Tier 1 for quick lookups, Tier 2 for parameter details, Tier 3 for multi-step workflows.

## Scaffold/Export Plugin Checklist

1. Replace every Soma identifier with your service name
2. Update `userConfig` / `settings` in all three manifests to match your service's credentials
3. Update `skills/soma/SKILL.md` — action table, parameters, response shapes, workflows
4. Set `brandColor` and `defaultPrompt` in `.codex-plugin/plugin.json`
5. Keep MCP registration stdio-first (`soma mcp`) unless your service must be remote HTTP only
6. Update `apply_plugin_options()` in `crates/soma/cli/src/setup.rs` to map your service's plugin options to its `SOMA_*` vars
7. Run `cargo xtask symlink-docs` after adding any new `CLAUDE.md`
