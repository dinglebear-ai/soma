# Plugin Settings

Generated from `crates/soma/config/src/env_registry.rs`.

| Plugin option env | Runtime env | Secret | TOML destination |
|---|---|---:|---|
| `CLAUDE_PLUGIN_OPTION_SOMA_API_URL` | `SOMA_API_URL` | no | `soma.api_url` |
| `CLAUDE_PLUGIN_OPTION_SOMA_API_KEY` | `SOMA_API_KEY` | yes | `soma.api_key` |
| `CLAUDE_PLUGIN_OPTION_API_TOKEN` | `SOMA_MCP_TOKEN` | yes | `mcp.api_token` |
| `CLAUDE_PLUGIN_OPTION_SERVER_URL` | `SOMA_SERVER_URL` | no | - |
| `CLAUDE_PLUGIN_OPTION_AUTH_MODE` | `SOMA_MCP_AUTH_MODE` | no | `mcp.auth.mode` |
| `CLAUDE_PLUGIN_OPTION_NO_AUTH` | `SOMA_MCP_NO_AUTH` | no | `mcp.no_auth` |
| `CLAUDE_PLUGIN_OPTION_PUBLIC_URL` | `SOMA_MCP_PUBLIC_URL` | no | `mcp.auth.public_url` |
| `CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_ID` | `SOMA_MCP_GOOGLE_CLIENT_ID` | yes | `mcp.auth.google_client_id` |
| `CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_SECRET` | `SOMA_MCP_GOOGLE_CLIENT_SECRET` | yes | `mcp.auth.google_client_secret` |
| `CLAUDE_PLUGIN_OPTION_AUTH_ADMIN_EMAIL` | `SOMA_MCP_AUTH_ADMIN_EMAIL` | no | `mcp.auth.admin_email` |
