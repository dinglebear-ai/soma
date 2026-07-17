#!/usr/bin/env python3
"""Generate volatile docs and metadata from canonical Soma specs."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

from readme_related_servers import replace_related_servers_section


ROOT = Path(__file__).resolve().parents[1]
# actions.rs/config.rs/env_registry.rs moved from crates/soma/contracts to
# crates/soma/domain and crates/soma/config respectively (plan sections 3.18
# and 6.2 "From soma-contracts"; PR 13). crates/soma/contracts is now a
# deprecated re-export facade with no literal definitions to scan.
ACTION_RS = ROOT / "crates/soma/domain/src/actions.rs"
CONFIG_RS = ROOT / "crates/soma/config/src/config.rs"
ENV_REGISTRY_RS = ROOT / "crates/soma/config/src/env_registry.rs"
ENV_DOC = ROOT / "docs/ENV.md"
ENV_EXAMPLE = ROOT / ".env.example"
CONFIG_EXAMPLE = ROOT / "config.soma.toml"
WEB_ACTIONS = ROOT / "apps/web/lib/generated-actions.ts"
PLUGIN_SETTINGS_DOC = ROOT / "docs/generated/plugin-settings.md"
SCRIPTS_INDEX_DOC = ROOT / "docs/generated/scripts-index.md"
PLUGIN_SURFACE = ROOT / "plugins/soma/plugin.surface.json"
PLUGIN_CLAUDE_JSON = ROOT / "plugins/soma/.claude-plugin/plugin.json"
PLUGIN_CODEX_JSON = ROOT / "plugins/soma/.codex-plugin/plugin.json"
PLUGIN_GEMINI_JSON = ROOT / "plugins/soma/gemini-extension.json"
README = ROOT / "README.md"
CLAUDE = ROOT / "CLAUDE.md"
SKILL = ROOT / "plugins/soma/skills/soma/SKILL.md"


@dataclass(frozen=True)
class Param:
    name: str
    ty: str
    required: bool
    description: str


@dataclass(frozen=True)
class Action:
    name: str
    description: str
    scope: str | None
    transport: str
    rest_method: str | None
    rest_path: str | None
    cost: str
    params: list[Param]
    returns: str


@dataclass(frozen=True)
class EnvSpec:
    key: str
    classification: str
    placement: str
    toml_destination: str | None
    legacy_behavior: str
    secret: bool
    plugin_option: str | None


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def write_if_changed(path: Path, content: str) -> bool:
    path.parent.mkdir(parents=True, exist_ok=True)
    current = path.read_text(encoding="utf-8") if path.exists() else None
    if current == content:
        return False
    path.write_text(content, encoding="utf-8")
    return True


def string_field(block: str, field: str) -> str:
    match = re.search(rf'{field}:\s*"([^"]*)"', block)
    if not match:
        raise RuntimeError(f"missing string field {field} in block:\n{block}")
    return match.group(1)


def param_type_field(block: str, field: str) -> str:
    match = re.search(rf"{field}:\s*ParamType::(\w+)", block)
    if match:
        return match.group(1).lower()
    return string_field(block, field)


def bool_field(block: str, field: str) -> bool:
    match = re.search(rf"{field}:\s*(true|false)", block)
    if not match:
        raise RuntimeError(f"missing bool field {field} in block:\n{block}")
    return match.group(1) == "true"


def option_string_field(block: str, field: str) -> str | None:
    match = re.search(rf"{field}:\s*(None|Some\(\"[^\"]*\"\))", block)
    if not match:
        raise RuntimeError(f"missing option string field {field} in block:\n{block}")
    return rust_string(match.group(1))


def parse_params(text: str) -> dict[str, list[Param]]:
    groups: dict[str, list[Param]] = {"&[]": []}
    pattern = re.compile(r"const\s+(\w+_PARAMS):\s*&\[ParamSpec\]\s*=\s*&\[(.*?)\];", re.S)
    for name, body in pattern.findall(text):
        params = []
        for block in re.findall(r"ParamSpec\s*\{(.*?)\}", body, re.S):
            params.append(
                Param(
                    name=string_field(block, "name"),
                    ty=param_type_field(block, "ty"),
                    required=bool_field(block, "required"),
                    description=string_field(block, "description"),
                )
            )
        groups[name] = params
    return groups


def parse_actions() -> list[Action]:
    text = read(ACTION_RS)
    params = parse_params(text)
    action_block = text.split("pub const ACTION_SPECS", 1)[1].split("];", 1)[0]
    actions = []
    for block in re.findall(r"ActionSpec\s*\{(.*?)\}", action_block, re.S):
        name = string_field(block, "name")
        scope_expr = re.search(r"required_scope:\s*([^,\n]+)", block)
        transport_expr = re.search(r"transport:\s*ActionTransport::(\w+)", block)
        cost_expr = re.search(r"cost:\s*ActionCost::(\w+)", block)
        params_expr = re.search(r"params:\s*([^,\n]+)", block)
        if not (scope_expr and transport_expr and cost_expr and params_expr):
            raise RuntimeError(f"incomplete ActionSpec for {name}")
        scope_raw = scope_expr.group(1).strip()
        if scope_raw == "None":
            scope = None
        elif scope_raw == "Some(READ_SCOPE)":
            scope = "soma:read"
        elif scope_raw == "Some(WRITE_SCOPE)":
            scope = "soma:write"
        else:
            scope = "soma:__deny__"
        param_key = params_expr.group(1).strip().removeprefix("&")
        actions.append(
            Action(
                name=name,
                description=string_field(block, "description"),
                scope=scope,
                transport=transport_expr.group(1),
                rest_method=option_string_field(block, "rest_method"),
                rest_path=option_string_field(block, "rest_path"),
                cost=cost_expr.group(1).lower(),
                params=params.get(param_key, []),
                returns=string_field(block, "returns"),
            )
        )
    return actions


def split_args(call: str) -> list[str]:
    args: list[str] = []
    current: list[str] = []
    in_string = False
    escaped = False
    for ch in call:
        if escaped:
            current.append(ch)
            escaped = False
            continue
        if ch == "\\":
            current.append(ch)
            escaped = True
            continue
        if ch == '"':
            current.append(ch)
            in_string = not in_string
            continue
        if ch == "," and not in_string:
            args.append("".join(current).strip())
            current = []
            continue
        current.append(ch)
    if current:
        args.append("".join(current).strip())
    return args


def rust_string(value: str) -> str | None:
    value = value.strip()
    if value == "None":
        return None
    some = re.fullmatch(r'Some\("([^"]*)"\)', value)
    if some:
        return some.group(1)
    raw = re.fullmatch(r'"([^"]*)"', value)
    if raw:
        return raw.group(1)
    raise RuntimeError(f"unsupported string-ish Rust expression: {value}")


def parse_env_specs() -> list[EnvSpec]:
    text = read(ENV_REGISTRY_RS)
    specs = []
    for call in spec_calls(text):
        args = [arg for arg in split_args(call) if arg]
        if len(args) != 7:
            continue
        if not args[0].strip().startswith('"'):
            continue
        specs.append(
            EnvSpec(
                key=rust_string(args[0]) or "",
                classification=args[1].split("::")[-1],
                placement=args[2].split("::")[-1],
                toml_destination=rust_string(args[3]),
                legacy_behavior=args[4].split("::")[-1],
                secret=args[5] == "true",
                plugin_option=rust_string(args[6]),
            )
        )
    return specs


def spec_calls(text: str) -> list[str]:
    calls: list[str] = []
    index = 0
    while True:
        start = text.find("spec(", index)
        if start == -1:
            return calls
        pos = start + len("spec(")
        depth = 1
        in_string = False
        escaped = False
        body: list[str] = []
        while pos < len(text) and depth > 0:
            ch = text[pos]
            pos += 1
            if escaped:
                body.append(ch)
                escaped = False
                continue
            if ch == "\\":
                body.append(ch)
                escaped = True
                continue
            if ch == '"':
                body.append(ch)
                in_string = not in_string
                continue
            if not in_string and ch == "(":
                depth += 1
            elif not in_string and ch == ")":
                depth -= 1
                if depth == 0:
                    break
            body.append(ch)
        calls.append("".join(body))
        index = pos


def default_string(fn_name: str) -> str:
    text = read(CONFIG_RS)
    match = re.search(rf"fn {fn_name}\(\).*?\{{\s*(?:.*?\n)*?\s*\"([^\"]+)\"\.into\(\)\s*\}}", text)
    if not match:
        raise RuntimeError(f"could not find default string for {fn_name}")
    return match.group(1)


def default_int(fn_name: str) -> int:
    text = read(CONFIG_RS)
    match = re.search(rf"fn {fn_name}\(\).*?\{{\s*(\d+)\s*\}}", text, re.S)
    if not match:
        raise RuntimeError(f"could not find default integer for {fn_name}")
    return int(match.group(1))


def env_purpose(spec: EnvSpec) -> str:
    purposes = {
        "SOMA_API_URL": "Deployed platform API or upstream API base URL used by `SomaClient`. Empty selects offline stub mode.",
        "SOMA_API_KEY": "Bearer token or upstream API key. Keep secret. Required when the deployed API requires auth.",
        "SOMA_MCP_TOKEN": "Static bearer token. Required for bearer-only mounted HTTP.",
        "SOMA_SERVER_URL": "Optional remote/platform HTTP server URL used by plugin setup and health checks.",
        "SOMA_MCP_AUTH_MODE": "`bearer` or `oauth`.",
        "SOMA_MCP_NO_AUTH": "Disable local auth for loopback development only.",
        "SOMA_NOAUTH": "Trusted-gateway no-auth mode for non-loopback deployments where an upstream proxy enforces auth.",
        "SOMA_MCP_PUBLIC_URL": "Public URL used for OAuth metadata endpoints.",
        "SOMA_MCP_GOOGLE_CLIENT_ID": "Google OAuth client ID.",
        "SOMA_MCP_GOOGLE_CLIENT_SECRET": "Google OAuth client secret.",
        "SOMA_MCP_AUTH_ADMIN_EMAIL": "Initial/admin email allowed by the OAuth flow.",
        "SOMA_MCP_HOST": "Bind host for HTTP transport. Set `0.0.0.0` only with bearer, OAuth, or trusted-gateway auth configured.",
        "SOMA_MCP_PORT": "Bind port for HTTP transport.",
        "SOMA_MCP_SERVER_NAME": "MCP server name advertised to clients.",
        "SOMA_MCP_ALLOWED_HOSTS": "Extra accepted Host header values, comma-separated.",
        "SOMA_MCP_ALLOWED_ORIGINS": "Extra CORS origins, comma-separated.",
    }
    return purposes.get(spec.key, "CUSTOMIZE: document this environment variable.")


def env_default(spec: EnvSpec, host: str, port: int) -> str:
    defaults = {
        "SOMA_MCP_HOST": f"`{host}`",
        "SOMA_MCP_PORT": f"`{port}`",
        "SOMA_MCP_AUTH_MODE": "`bearer`",
        "SOMA_MCP_NO_AUTH": "`false`",
        "SOMA_NOAUTH": "`false`",
        "SOMA_MCP_SERVER_NAME": f"`{default_string('default_server_name')}`",
    }
    return defaults.get(spec.key, "unset")


def render_env_doc() -> str:
    specs = parse_env_specs()
    host = default_string("default_mcp_host")
    port = default_int("default_mcp_port")
    lines = [
        "---",
        'title: "Environment Variables"',
        'doc_type: "guide"',
        'status: "active"',
        'owner: "soma"',
        "audience:",
        '  - "contributors"',
        '  - "agents"',
        'scope: "soma"',
        "source_of_truth: false",
        "upstream_refs:",
        '  - "crates/soma/config/src/env_registry.rs"',
        '  - "crates/soma/config/src/config.rs"',
        'last_reviewed: "2026-06-19"',
        "---",
        "",
        "# Environment variables",
        "",
        "This file is generated from `ENV_KEY_SPECS` and typed config defaults. Run `cargo xtask generate-docs` after changing env/config metadata.",
        "",
        "## Runtime variables",
        "",
        "| Variable | Default | Secret | TOML destination | Plugin option | Purpose |",
        "|---|---:|---:|---|---|---|",
    ]
    for spec in specs:
        secret = "yes" if spec.secret else "no"
        toml = f"`{spec.toml_destination}`" if spec.toml_destination else "-"
        plugin = f"`{spec.plugin_option}`" if spec.plugin_option else "-"
        lines.append(
            f"| `{spec.key}` | {env_default(spec, host, port)} | {secret} | {toml} | {plugin} | {env_purpose(spec)} |"
        )
    lines.extend(
        [
            "",
            "## Docker runtime",
            "",
            "| Variable | Purpose |",
            "|---|---|",
            "| `PUID` | UID to run the container as (default: 1000). |",
            "| `PGID` | GID to run the container as (default: 1000). |",
            "| `DOCKER_NETWORK` | Docker network name (default: `mcp`). |",
            "| `VERSION` | Image tag to pull (default: `latest`). |",
            "",
            "## Logging",
            "",
            "| Variable | Example | Purpose |",
            "|---|---|---|",
            "| `RUST_LOG` | `info,rmcp=warn` | Tracing filter. |",
            "| `NO_COLOR` | `1` | Disable ANSI color in console logs. |",
            "| `FORCE_COLOR` | `1` | Force ANSI color even when stderr is not a TTY. |",
            "",
            "## Safety",
            "",
            "`.env` and `.env.*` are ignored by `.gitignore` and blocked by `scripts/block-env-commits.sh`. Only `.env.example` belongs in git.",
            "",
            "Non-secret settings go in `config.toml`; secrets and deployment URLs go in `.env`. See `docs/CONFIG.md` for the full split.",
            "",
            "Generate a bearer token:",
            "",
            "```bash",
            "just gen-token",
            "# or: openssl rand -hex 32",
            "```",
            "",
        ]
    )
    return "\n".join(lines)


def placeholder_for(spec: EnvSpec) -> str:
    placeholders = {
        "SOMA_API_URL": "https://api.example.com",
        "SOMA_API_KEY": "your-api-key-here",
        "SOMA_MCP_TOKEN": "",
        "SOMA_SERVER_URL": "http://localhost:40060",
        "SOMA_MCP_AUTH_MODE": "bearer",
        "SOMA_MCP_NO_AUTH": "false",
        "SOMA_NOAUTH": "false",
        "SOMA_MCP_PUBLIC_URL": "https://example.yourdomain.com",
        "SOMA_MCP_GOOGLE_CLIENT_ID": "123456789-abcdefg.apps.googleusercontent.com",
        "SOMA_MCP_GOOGLE_CLIENT_SECRET": "GOCSPX-your-secret-here",
        "SOMA_MCP_AUTH_ADMIN_EMAIL": "admin@example.com",
        "SOMA_MCP_HOST": default_string("default_mcp_host"),
        "SOMA_MCP_PORT": str(default_int("default_mcp_port")),
        "SOMA_MCP_SERVER_NAME": default_string("default_server_name"),
        "SOMA_MCP_ALLOWED_HOSTS": "example.yourdomain.com",
        "SOMA_MCP_ALLOWED_ORIGINS": "https://claude.ai",
    }
    return placeholders.get(spec.key, "")


def render_env_example() -> str:
    specs = parse_env_specs()
    lines = [
        "# =============================================================================",
        "# .env.example - generated secrets and URLs sample",
        "#",
        "# CUSTOMIZE: Rename SOMA_* throughout to your service's prefix.",
        "# Secrets and URLs go here; non-secret defaults belong in config.toml.",
        "# Regenerate with: cargo xtask generate-docs",
        "# =============================================================================",
        "",
        "# Upstream service",
    ]
    groups = [
        ("KeepEnv", "Shared runtime / plugin settings"),
        ("TrustedOperatorBootstrap", "MCP auth and trusted operator bootstrap"),
        ("ComposeEnv", "MCP HTTP server compose overrides"),
    ]
    emitted: set[str] = set()
    for classification, title in groups:
        group_specs = [spec for spec in specs if spec.classification == classification]
        if not group_specs:
            continue
        lines.extend(["", f"# {title}"])
        for spec in group_specs:
            if spec.key in emitted:
                continue
            emitted.add(spec.key)
            value = placeholder_for(spec)
            prefix = "" if spec.key in {"SOMA_API_URL", "SOMA_API_KEY"} else "# "
            if spec.secret and spec.key not in {"SOMA_API_KEY"}:
                prefix = "# "
            lines.append(f"# {env_purpose(spec)}")
            lines.append(f"{prefix}{spec.key}={value}")
            lines.append("")
    lines.extend(
        [
            "# Docker runtime",
            "# PUID=1000",
            "# PGID=1000",
            "# DOCKER_NETWORK=mcp",
            "# VERSION=latest",
            "",
            "# Logging",
            "# RUST_LOG=info",
            "# NO_COLOR=1",
            "# FORCE_COLOR=1",
            "",
        ]
    )
    return "\n".join(lines)


def render_config_example() -> str:
    host = default_string("default_mcp_host")
    port = default_int("default_mcp_port")
    server_name = default_string("default_server_name")
    sqlite_path = default_string("default_auth_sqlite_path")
    key_path = default_string("default_auth_key_path")
    return f"""# =============================================================================
# config.soma.toml - generated non-secret config sample
#
# Copy to config.toml and adjust for your deployment.
# Env vars override these values. Secrets and URLs belong in .env.
# Regenerate with: cargo xtask generate-docs
# =============================================================================

[soma]
# Set SOMA_API_URL and SOMA_API_KEY in .env instead of committing them.
# api_url = "https://api.example.com"
# api_key = ""

[mcp]
host = "{host}"
port = {port}
server_name = "{server_name}"
no_auth = false
trusted_gateway = false
allowed_hosts = []
allowed_origins = []

# Set SOMA_MCP_TOKEN in .env instead of committing it.
# api_token = ""

[mcp.auth]
mode = "bearer"
# public_url = "https://example.yourdomain.com"
# google_client_id = ""
# google_client_secret = ""
admin_email = ""
allowed_emails = []
sqlite_path = "{sqlite_path}"
key_path = "{key_path}"
access_token_ttl_secs = 3600
refresh_token_ttl_secs = 2592000
auth_code_ttl_secs = 300
register_rpm = 10
authorize_rpm = 60
allowed_client_redirect_uris = []
"""


TS_IDENTIFIER = re.compile(r"^[A-Za-z_$][A-Za-z0-9_$]*$")


def ts_key(key: str) -> str:
    if TS_IDENTIFIER.fullmatch(key):
        return key
    return json.dumps(key)


def ts_value(value: object, indent: int = 0, key: str | None = None) -> str:
    pad = " " * indent
    nested = " " * (indent + 2)
    if isinstance(value, str):
        encoded = json.dumps(value)
        if key and len(f"{pad}{ts_key(key)}: {encoded},") > 100:
            return f"{pad}{ts_key(key)}:\n{nested}{encoded},"
        if key:
            return f"{pad}{ts_key(key)}: {encoded},"
        return f"{pad}{encoded}"
    if isinstance(value, bool):
        encoded = "true" if value else "false"
        if key:
            return f"{pad}{ts_key(key)}: {encoded},"
        return f"{pad}{encoded}"
    if value is None:
        if key:
            return f"{pad}{ts_key(key)}: null,"
        return f"{pad}null"
    if isinstance(value, int | float):
        if key:
            return f"{pad}{ts_key(key)}: {value},"
        return f"{pad}{value}"
    if isinstance(value, list):
        if not value:
            encoded = "[]"
            if key:
                return f"{pad}{ts_key(key)}: {encoded},"
            return f"{pad}{encoded}"
        if all(not isinstance(item, list | dict) for item in value):
            encoded = f"[{', '.join(ts_value(item).strip() for item in value)}]"
            if len(f"{pad}{ts_key(key) if key else ''}: {encoded},") <= 100:
                if key:
                    return f"{pad}{ts_key(key)}: {encoded},"
                return f"{pad}{encoded}"
        lines = [f"{pad}{ts_key(key)}: [" if key else f"{pad}["]
        for item in value:
            rendered = ts_value(item, indent + 2)
            if not isinstance(item, list | dict):
                rendered = f"{rendered},"
            lines.append(rendered)
        lines.append(f"{pad}],")
        return "\n".join(lines)
    if isinstance(value, dict):
        if not value:
            encoded = "{}"
            if key:
                return f"{pad}{ts_key(key)}: {encoded},"
            return f"{pad}{encoded}"
        lines = [f"{pad}{ts_key(key)}: {{" if key else f"{pad}{{"]
        for child_key, child_value in value.items():
            lines.append(ts_value(child_value, indent + 2, str(child_key)))
        lines.append(f"{pad}}},")
        return "\n".join(lines)
    raise TypeError(f"unsupported TypeScript value: {value!r}")


def action_example(action: Action) -> dict[str, object]:
    params: dict[str, object] = {}
    for param in action.params:
        params[param.name] = "Alice" if param.name == "name" else "Hello!"
    return {"action": action.name, "params": params}


def action_response(action: Action) -> dict[str, object]:
    examples = {
        "greet": {"greeting": "Hello, Alice!", "target": "Alice"},
        "echo": {"echo": "Hello!"},
        "status": {"status": "ok", "note": "stub"},
        "help": {"actions": ["greet", "echo", "status", "help"]},
        "elicit_name": {"greeting": "Hello, Alice!", "target": "Alice", "elicited": True},
        "scaffold_intent": {"kind": "soma_scaffold_intent", "schema_version": 1},
    }
    return examples.get(action.name, {})


def render_web_actions() -> str:
    actions = parse_actions()
    rendered = []
    for action in actions:
        transport = "rest" if action.transport == "Any" else "mcp-only"
        rendered.append(
            {
                "id": action.name,
                "label": action.name,
                "description": action.description,
                "scope": action.scope or "public",
                "transport": transport,
                **(
                    {"method": action.rest_method, "path": action.rest_path}
                    if action.rest_method and action.rest_path
                    else {}
                ),
                "params": [
                    {
                        "name": param.name,
                        "label": param.name.replace("_", " ").title(),
                        "type": "text",
                        "placeholder": "Alice" if param.name == "name" else "Hello!",
                        "required": param.required,
                        "description": param.description,
                    }
                    for param in action.params
                ],
                "example": action_example(action),
                "response": action_response(action),
            }
        )
    return (
        "// Generated by scripts/generate-docs.py. Do not edit by hand.\n"
        "import type { ActionSpec } from \"./soma\";\n\n"
        f"export const ACTIONS = {ts_value(rendered).strip().removesuffix(',')} as const satisfies readonly ActionSpec[];\n"
    )


def load_plugin_surface() -> dict:
    return json.loads(read(PLUGIN_SURFACE))


def plugin_setting_key(plugin_option: str) -> str:
    raw = plugin_option.removeprefix("CLAUDE_PLUGIN_OPTION_").lower()
    if raw == "api_token":
        return "api_token"
    return raw


def plugin_setting_title(key: str) -> str:
    return {
        "server_url": "Server URL",
        "api_token": "API token",
        "auth_mode": "Auth mode",
        "no_auth": "Disable service auth",
        "public_url": "Public URL",
        "google_client_id": "Google OAuth client ID",
        "google_client_secret": "Google OAuth client secret",
        "auth_admin_email": "OAuth admin email",
        "soma_api_url": "Service API URL",
        "soma_api_key": "Service API key",
    }.get(key, key.replace("_", " ").title())


def plugin_setting_description(key: str, env_key: str) -> str:
    descriptions = {
        "server_url": "Optional HTTP server base URL for remote/platform fallback and health monitoring. The default MCP connection uses local stdio instead.",
        "api_token": "Optional bearer token for HTTP MCP fallback. Not used by the default stdio connection.",
        "auth_mode": "Server auth mode. 'bearer' uses only the static API token. 'oauth' enables Google OAuth/JWT.",
        "no_auth": "Run the MCP server without authentication. ONLY safe when the server is bound to 127.x loopback.",
        "public_url": "Public base URL for OAuth issuer metadata, e.g. https://example.yourdomain.com. Required when auth_mode=oauth.",
        "google_client_id": "Google OAuth client ID used when auth_mode=oauth.",
        "google_client_secret": "Google OAuth client secret from the same Google Cloud Console credential.",
        "auth_admin_email": "Bootstrap allowed Google account for OAuth mode.",
        "soma_api_url": "Optional upstream API URL for dropped-in tools that use the bundled SomaClient compatibility layer. Maps to SOMA_API_URL.",
        "soma_api_key": "Optional upstream API key for dropped-in tools that use the bundled SomaClient compatibility layer. Maps to SOMA_API_KEY.",
    }
    return descriptions.get(key, f"Maps to {env_key}.")


def plugin_settings() -> list[dict]:
    settings = []
    for spec in parse_env_specs():
        if not spec.plugin_option:
            continue
        key = plugin_setting_key(spec.plugin_option)
        default: object = ""
        kind = "string"
        if key == "no_auth":
            kind = "boolean"
            default = False
        if key == "auth_mode":
            default = "bearer"
        item = {
            "key": key,
            "env": spec.key,
            "type": kind,
            "title": plugin_setting_title(key),
            "description": plugin_setting_description(key, spec.key),
            "required": key in {"use_docker", "no_auth", "auth_mode"},
            "default": default,
            "sensitive": spec.secret,
        }
        settings.append(item)
    return settings


def json_file(value: object) -> str:
    return json.dumps(value, indent=2) + "\n"


def render_claude_plugin_json() -> str:
    surface = load_plugin_surface()
    user_config = {
        "use_docker": {
            "type": "boolean",
            "title": "Deploy with Docker",
            "description": "True uses docker compose; false uses a systemd user service.",
            "required": True,
            "default": False,
        }
    }
    for setting in plugin_settings():
        user_config[setting["key"]] = {
            "type": setting["type"],
            "title": setting["title"],
            "description": setting["description"],
            "required": setting["required"],
            "default": setting["default"],
            **({"sensitive": True} if setting["sensitive"] else {}),
        }
    return json_file(
        {
            "name": surface["name"],
            "description": surface["claudeDescription"],
            "author": {"name": surface["author"]["name"]},
            "homepage": surface["homepage"],
            "repository": surface["repository"],
            "license": surface["license"],
            "keywords": surface["keywords"],
            "hooks": "./hooks/hooks.json",
            "skills": "./skills",
            "userConfig": user_config,
        }
    )


def render_codex_plugin_json() -> str:
    surface = load_plugin_surface()
    interface = dict(surface["codexInterface"])
    interface["websiteURL"] = surface["homepage"]
    interface["composerIcon"] = "./assets/icon.png"
    interface["logo"] = "./assets/logo.svg"
    return json_file(
        {
            "name": surface["name"],
            "description": surface["codexDescription"],
            "homepage": surface["homepage"],
            "repository": surface["repository"],
            "license": surface["license"],
            "keywords": surface["keywords"],
            "skills": "./skills/",
            "interface": interface,
            "author": surface["author"],
        }
    )


def render_gemini_extension_json() -> str:
    surface = load_plugin_surface()
    settings = []
    for setting in plugin_settings():
        settings.append(
            {
                "name": setting["key"],
                "description": setting["description"],
                "required": False,
                **({"sensitive": True} if setting["sensitive"] else {}),
            }
        )
    return json_file(
        {
            "name": surface["name"],
            "description": surface["geminiDescription"],
            "author": surface["author"]["name"],
            "homepage": surface["homepage"],
            "repository": surface["repository"],
            "license": surface["license"],
            "keywords": surface["keywords"],
            "contextFileName": "GEMINI.md",
            "skills": "./skills",
            "hooks": "./hooks/hooks.json",
            "settings": settings,
        }
    )


def render_plugin_settings_doc() -> str:
    specs = parse_env_specs()
    lines = [
        "# Plugin Settings",
        "",
        "Generated from `crates/soma/config/src/env_registry.rs`.",
        "",
        "| Plugin option env | Runtime env | Secret | TOML destination |",
        "|---|---|---:|---|",
    ]
    for spec in specs:
        if not spec.plugin_option:
            continue
        secret = "yes" if spec.secret else "no"
        toml = f"`{spec.toml_destination}`" if spec.toml_destination else "-"
        lines.append(f"| `{spec.plugin_option}` | `{spec.key}` | {secret} | {toml} |")
    lines.append("")
    return "\n".join(lines)


def params_summary(action: Action) -> str:
    if not action.params:
        return "none"
    return ", ".join(
        f"`{param.name}` ({'required' if param.required else 'optional'} {param.ty})"
        for param in action.params
    )


def cli_command(action: Action) -> str:
    commands = {
        "greet": "soma greet [--name N]",
        "echo": "soma echo --message <msg>",
        "status": "soma status",
        "help": "soma --help",
    }
    if action.transport != "Any":
        return "_MCP-only_"
    return commands.get(action.name, f"soma {action.name.replace('_', '-')}")


def action_table_markdown() -> str:
    lines = [
        "| Action | Scope | Cost | Transport | REST route | CLI | Parameters | Description |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for action in parse_actions():
        scope = f"`{action.scope}`" if action.scope else "public"
        transport = "MCP + CLI + REST" if action.transport == "Any" else "MCP-only"
        route = (
            f"`{action.rest_method} {action.rest_path}`"
            if action.rest_method and action.rest_path
            else "-"
        )
        lines.append(
            f"| `{action.name}` | {scope} | `{action.cost}` | {transport} | {route} | `{cli_command(action)}` | {params_summary(action)} | {action.description} |"
        )
    return "\n".join(lines)


def parity_table_markdown() -> str:
    lines = [
        "| Service Method | MCP Action | CLI Command | REST Route | Notes |",
        "|---|---|---|---|---|",
    ]
    methods = {
        "greet": "service.greet(name)",
        "echo": "service.echo(message)",
        "status": "service.status()",
        "help": "built-in help",
        "elicit_name": "MCP client interaction",
        "scaffold_intent": "MCP elicitation wizard",
    }
    for action in parse_actions():
        route = (
            f"`{action.rest_method} {action.rest_path}`"
            if action.rest_method and action.rest_path
            else "_MCP-only_"
        )
        notes = "MCP-only; requires elicitation-capable client" if action.transport != "Any" else ""
        lines.append(
            f"| `{methods.get(action.name, 'service.' + action.name + '()')}` | `soma(action=\"{action.name}\")` | `{cli_command(action)}` | {route} | {notes} |"
        )
    return "\n".join(lines)


def skill_action_table() -> str:
    lines = [
        "| action | purpose | parameters |",
        "|---|---|---|",
    ]
    for action in parse_actions():
        lines.append(f"| `{action.name}` | {action.description} | {params_summary(action)} |")
    return "\n".join(lines)


def generated_block(name: str, body: str) -> str:
    return (
        f"<!-- BEGIN GENERATED {name} -->\n"
        "<!-- Generated by scripts/generate-docs.py; do not edit by hand. -->\n"
        f"{body.rstrip()}\n"
        f"<!-- END GENERATED {name} -->"
    )


def replace_or_insert(text: str, name: str, body: str, anchor: str) -> str:
    block = generated_block(name, body)
    pattern = re.compile(
        rf"<!-- BEGIN GENERATED {re.escape(name)} -->.*?<!-- END GENERATED {re.escape(name)} -->",
        re.S,
    )
    if pattern.search(text):
        return pattern.sub(block, text)
    if anchor not in text:
        raise RuntimeError(f"anchor not found for generated block {name}: {anchor}")
    return text.replace(anchor, f"{anchor}\n\n{block}", 1)


def render_readme() -> str:
    text = read(README)
    text = replace_related_servers_section(text, README, self_name="soma")
    text = replace_or_insert(
        text,
        "README_ACTION_TABLE",
        action_table_markdown(),
        "The single `soma` tool dispatches on the `action` parameter:",
    )
    return text


def render_claude() -> str:
    text = read(CLAUDE)
    return replace_or_insert(
        text,
        "CLAUDE_PARITY_TABLE",
        parity_table_markdown(),
        "with no CLI analogue.",
    )


def render_skill() -> str:
    text = read(SKILL)
    return replace_or_insert(
        text,
        "SKILL_ACTION_TABLE",
        skill_action_table(),
        "A single MCP tool, `mcp__soma__soma`, dispatches on a required `action` argument:",
    )


def script_summary(path: Path) -> str:
    text = path.read_text(encoding="utf-8", errors="ignore")
    docstring = re.search(r'\A#![^\n]*\n"""(.*?)"""', text, re.S)
    if docstring:
        first = docstring.group(1).strip().splitlines()[0].strip()
        if first:
            return first
    for line in text.splitlines()[1:12]:
        stripped = line.strip()
        if stripped.startswith('"""') and stripped.endswith('"""') and len(stripped) > 6:
            return stripped.strip('"')
        if stripped.startswith("#") and not stripped.startswith("#!"):
            summary = stripped.lstrip("#").strip()
            if summary and not set(summary) <= {"="}:
                return summary
    return "CUSTOMIZE: add a header comment describing this script."


def render_scripts_index() -> str:
    lines = [
        "# Scripts Index",
        "",
        "Generated from script header comments.",
        "",
        "| File | Summary |",
        "|---|---|",
    ]
    for path in sorted((ROOT / "scripts").glob("*")):
        if path.suffix not in {".sh", ".py"}:
            continue
        rel = path.relative_to(ROOT)
        lines.append(f"| `{rel}` | {script_summary(path)} |")
    lines.append("")
    return "\n".join(lines)


GENERATED_FILES = {
    ENV_DOC: render_env_doc,
    ENV_EXAMPLE: render_env_example,
    CONFIG_EXAMPLE: render_config_example,
    WEB_ACTIONS: render_web_actions,
    PLUGIN_CLAUDE_JSON: render_claude_plugin_json,
    PLUGIN_CODEX_JSON: render_codex_plugin_json,
    PLUGIN_GEMINI_JSON: render_gemini_extension_json,
    PLUGIN_SETTINGS_DOC: render_plugin_settings_doc,
    SCRIPTS_INDEX_DOC: render_scripts_index,
    README: render_readme,
    CLAUDE: render_claude,
    SKILL: render_skill,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="Rewrite generated docs and metadata.")
    parser.add_argument("--check", action="store_true", help="Fail if generated docs drift.")
    args = parser.parse_args()
    if not args.write and not args.check:
        args.check = True

    stale: list[Path] = []
    written: list[Path] = []
    for path, renderer in GENERATED_FILES.items():
        content = renderer()
        if args.write:
            if write_if_changed(path, content):
                written.append(path)
        if args.check:
            if not path.exists() or path.read_text(encoding="utf-8") != content:
                stale.append(path)

    for path in written:
        print(f"wrote {path.relative_to(ROOT)}")
    if stale:
        for path in stale:
            print(f"FAIL: {path.relative_to(ROOT)} is stale; run cargo xtask generate-docs", file=sys.stderr)
        return 1
    if args.check:
        print("generated docs are current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
