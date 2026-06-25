# No-MCP Plugin Variant

`marketplace-no-mcp` is a long-lived alternate branch for installs that do not
want bundled MCP server registrations.

`main` is the full/default plugin source. The no-MCP branch keeps the same
plugin assets, hooks, monitors, skills, and scaffold content while removing
bundled MCP server registrations for users who rely on a separate gateway,
prefer CLI-only usage, or want skills to use their fallback paths.

The branch is maintained by `cargo xtask apply-no-mcp-marketplace`. The
transform removes local `mcp.json` / `.mcp.json` files and strips inline
`mcpServers` objects from Claude, Codex, and Gemini plugin manifests. The
current template plugin is stdio-first and does not ship bundled MCP
registrations, so the transform is an identity operation until such files are
introduced.

The branch is synchronized by
`.github/workflows/sync-marketplace-no-mcp.yml` after pushes to `main` and on a
daily schedule. Drift is checked by
`.github/workflows/check-no-mcp-drift.yml` and can be checked locally with:

```bash
cargo xtask check-no-mcp-drift --compare-ref
```

Humans should not casually merge, delete, or retire the branch. Direct writes
are release-maintenance work and must be followed by the drift check.
