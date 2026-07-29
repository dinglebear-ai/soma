# plugins/soma — Soma plugin instructions

## What this directory is

Multi-platform plugin package for the Soma MCP server. Contains manifests for
Claude Code, Codex, and Gemini CLI plus shared skills. MCP connection
registration is supplied by the client or gateway; the default command is
`soma mcp`.

## File map

| File | Role |
|---|---|
| `.claude-plugin/plugin.json` | Claude Code manifest — identity, skills, monitors, `userConfig` |
| `.codex-plugin/plugin.json` | Codex manifest — same data + Codex UI fields (`interface`) |
| `gemini-extension.json` | Gemini CLI manifest — uses `settings` array instead of `userConfig` |
| `monitors/monitors.json` | Background health monitor config (requires Claude Code v2.1.105+) |
| `skills/soma/SKILL.md` | Three-tier tool documentation shared by Claude and Codex |

## Versioning rule

**Do not add a `version` field to any manifest.** The marketplace derives version from the git commit SHA. An explicit `version` field causes every push to register as a new version and creates duplicate marketplace entries.

## Updating a manifest

When changing connection guidance (URL, auth headers, stdio args), update this
README, the platform manifests/settings, and `gemini-extension.json` examples
together. Do not assume a shared `.mcp.json` exists in this plugin.

When changing user-configurable settings, update all three manifests: `userConfig` in the Claude and Codex `plugin.json` files, and `settings` in `gemini-extension.json`. Keep field names and descriptions consistent across all three.

## Monitors (Claude Code v2.1.105+)

The default stdio MCP registration runs `soma mcp`. The binary must be installed
on `PATH` before the plugin is installed. Install it with:

```bash
just install-local
```

`monitors/monitors.json` is optional and only useful for HTTP deployments. Its
command must **not** reference `${user_config.*}`: Claude Code v2.1.207+ rejects
that in monitor commands and the monitor silently never starts. Monitor processes
also receive no `CLAUDE_PLUGIN_OPTION_*`. `soma watch` therefore takes no `--url`
and resolves its own default from config; if a monitor ever needs a non-default
URL, read it from a config file inside the script — the documented alternative.
Do not hardcode URLs in `monitors.json` either.

**This is scaffold text.** Whatever shape this takes is inherited by every server
generated from soma, so keep it correct.

When adding a new monitor: add an entry to `monitors.json` and reference the installed `soma` binary or scripts under `${CLAUDE_PLUGIN_ROOT}/scripts/`.

## Updating the skill

`skills/soma/SKILL.md` is shared by Claude Code and Codex. Gemini reads it via the `skills` path in `gemini-extension.json`. Edit it once — all platforms see the change.

The three-tier structure must be preserved:
- **Tier 1** (above fold): tool name, quick action table, critical gotchas
- **Tier 2** (middle): full action reference with parameters and response shapes
- **Tier 3** (bottom): workflows, HTTP fallback, error handling

## Updating the plugin option mapping

`apply_plugin_options()` in `crates/soma/cli/src/setup.rs` reads `CLAUDE_PLUGIN_OPTION_*` env vars that map to the `userConfig` fields in `plugin.json`, translating them into the binary's `SOMA_*` vars before `Config::load()`. When you add or rename a `userConfig` field, update the mapping table in that function to match.

This package ships **no** lifecycle hooks, so nothing runs `soma setup
plugin-hook` automatically — run it manually after install or a settings
change. Do not add a `hooks` key or a `hooks/hooks.json`: `just
validate-plugin`, `cargo xtask patterns`, and
`apps/soma/tests/plugin_contract.rs` all assert their absence.

Sensitive fields declared `"sensitive": true` in `plugin.json` are **never** substituted into skill content.

## Soma adaptation

When renaming `soma` → your service:

1. Replace Soma identifiers and `SOMA_` env vars in every file in this directory.
2. Rename `skills/soma/` to `skills/<your-service>/`.
3. Update `apply_plugin_options()` in `crates/soma/cli/src/setup.rs` — its mapping table maps `CLAUDE_PLUGIN_OPTION_*` to your service's actual `SOMA_*` vars.
4. Keep the no-version rule: do not add `"version"` to any manifest.
