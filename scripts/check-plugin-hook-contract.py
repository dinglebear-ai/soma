#!/usr/bin/env python3
"""Audit binary-owned plugin hook setup contracts across Rust MCP servers."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = ROOT.parent
REQUIRED_FIELDS = {
    "exit_policy",
    "ran_repair",
    "no_repair",
    "blocking_failures",
    "advisory_failures",
}
EXIT_POLICIES = {"success", "advisory_failure", "blocking_failure"}


@dataclass(frozen=True)
class Server:
    name: str
    repo: Path
    binary: str
    hook: str | None
    plugin_root: str | None = None
    check_plugin_layout: bool = True
    package_args: tuple[str, ...] = ()
    setup_args: tuple[str, ...] = ("setup", "plugin-hook", "--no-repair")
    env: tuple[tuple[str, str], ...] = ()
    appdata_env: str = "CLAUDE_PLUGIN_DATA"
    make_appdata: bool = True


SERVERS = [
    Server(
        "syslog",
        WORKSPACE / "syslog-mcp",
        "syslog",
        "scripts/plugin-setup.sh",
        plugin_root=".",
        setup_args=("setup", "plugin-hook", "--no-repair", "--json"),
        env=(("SYSLOG_MCP_TOKEN", "test-token"),),
    ),
    Server(
        "gotify",
        WORKSPACE / "rustify",
        "gotify",
        "plugins/gotify/hooks/plugin-setup.sh",
        setup_args=("--json", "setup", "plugin-hook", "--no-repair"),
        appdata_env="GOTIFY_MCP_HOME",
    ),
    Server(
        "unifi",
        WORKSPACE / "rustifi",
        "unifi",
        "plugins/unifi/hooks/plugin-setup.sh",
        setup_args=("--json", "setup", "plugin-hook", "--no-repair"),
        appdata_env="UNIFI_MCP_HOME",
    ),
    Server(
        "tailscale",
        WORKSPACE / "rustscale",
        "tailscale",
        "plugins/tailscale/hooks/plugin-setup.sh",
        setup_args=("--json", "setup", "plugin-hook", "--no-repair"),
        appdata_env="TAILSCALE_MCP_HOME",
    ),
    Server(
        "apprise",
        WORKSPACE / "apprise-mcp",
        "apprise",
        "plugins/apprise/hooks/plugin-setup.sh",
        env=(("APPRISE_URL", "http://apprise.example:8000"), ("APPRISE_MCP_TOKEN", "test-token")),
    ),
    Server(
        "unraid",
        WORKSPACE / "unrust",
        "unraid",
        "plugins/unraid/hooks/plugin-setup.sh",
        env=(
            ("UNRAID_API_URL", "https://tower.example/graphql"),
            ("UNRAID_API_KEY", "test-key"),
            ("UNRAID_MCP_TOKEN", "test-token"),
        ),
        appdata_env="UNRAID_HOME",
    ),
    Server(
        "example",
        ROOT,
        "example",
        # Hook calls the binary directly now (no plugin-setup.sh wrapper); the
        # env-var mapping lives in apply_plugin_options() in src/cli/setup.rs.
        None,
        env=(
            ("EXAMPLE_API_URL", "https://api.example.test"),
            ("EXAMPLE_API_KEY", "test-key"),
            ("EXAMPLE_MCP_TOKEN", "test-token"),
        ),
        appdata_env="EXAMPLE_HOME",
    ),
    Server(
        "lab",
        WORKSPACE / "lab",
        "labby",
        None,
        check_plugin_layout=False,
        package_args=("-p", "labby", "--all-features"),
        setup_args=("setup", "plugin-hook", "--no-repair", "--json"),
        appdata_env="LAB_HOME",
    ),
]


def fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def check_hook(server: Server) -> None:
    if server.hook is None:
        return
    hook = server.repo / server.hook
    if not hook.is_file():
        fail(f"{server.name}: missing hook {hook}")
    text = hook.read_text()
    expected = f"{server.binary} setup plugin-hook \"$@\""
    if expected not in text:
        fail(f"{server.name}: hook must delegate with `{expected}`")
    forbidden = [
        "cargo build",
        "cargo install",
        "cargo run",
        "docker compose",
        "systemctl",
    ]
    found = [token for token in forbidden if token in text]
    if "curl" in text and "| sh" in text:
        found.append("curl | sh")
    if found:
        fail(f"{server.name}: hook contains forbidden bootstrap tokens: {', '.join(found)}")
    subprocess.run(["bash", "-n", str(hook)], check=True)


def check_plugin_layout(server: Server) -> None:
    if not server.check_plugin_layout:
        return
    plugin_root = server.repo / (server.plugin_root or f"plugins/{server.name}")
    if not plugin_root.is_dir():
        fail(f"{server.name}: missing plugin root {plugin_root}")

    manifest_relatives = (".claude-plugin/plugin.json", ".codex-plugin/plugin.json")
    found_manifest = False
    for relative in manifest_relatives:
        manifest = plugin_root / relative
        if not manifest.is_file():
            continue
        found_manifest = True
        try:
            payload = json.loads(manifest.read_text())
        except json.JSONDecodeError as error:
            fail(f"{server.name}: invalid JSON in {manifest}: {error}")
        if "version" in payload:
            fail(f"{server.name}: plugin manifest must not contain version: {manifest}")
    if not found_manifest:
        fail(f"{server.name}: missing plugin manifests under {plugin_root}")

    required = []
    if (plugin_root / ".mcp.json").exists():
        required.append(plugin_root / ".mcp.json")
    if server.hook is not None:
        hooks_json = plugin_root / "hooks/hooks.json"
        if hooks_json.exists():
            required.append(hooks_json)
    for path in required:
        if not path.is_file():
            fail(f"{server.name}: missing plugin file {path}")
        try:
            json.loads(path.read_text())
        except json.JSONDecodeError as error:
            fail(f"{server.name}: invalid JSON in {path}: {error}")


def check_required_recipes(server: Server) -> None:
    justfile = server.repo / "Justfile"
    if not justfile.is_file():
        fail(f"{server.name}: missing Justfile")
    output = subprocess.run(
        ["just", "--list"],
        cwd=server.repo,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if output.returncode != 0:
        fail(f"{server.name}: just --list failed: {output.stderr.strip()}")
    recipes = output.stdout
    missing = [
        recipe
        for recipe in ("validate-plugin", "runtime-current")
        if f"    {recipe}" not in recipes and f"{recipe}\n" not in recipes
    ]
    if missing:
        fail(f"{server.name}: missing Justfile recipes: {', '.join(missing)}")


def check_binary(server: Server) -> None:
    with tempfile.TemporaryDirectory(prefix=f"{server.name}-plugin-contract-") as temp:
        appdata = Path(temp) / "appdata"
        log_dir = Path(temp) / "logs"
        if server.make_appdata:
            appdata.mkdir()
        log_dir.mkdir()
        env = {
            "PATH": f"{server.repo / 'target' / 'debug'}:{os.environ.get('PATH', '')}",
            "RUST_LOG": "warn",
            "LAB_LOG_DIR": str(log_dir),
            server.appdata_env: str(appdata),
            "CLAUDE_PLUGIN_DATA": str(appdata),
            **dict(server.env),
        }
        command = ["cargo", "run", "--locked", "--quiet", *server.package_args, "--", *server.setup_args]
        output = subprocess.run(
            command,
            cwd=server.repo,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    stdout = output.stdout.strip()
    if not stdout.startswith("{"):
        stderr = output.stderr.strip()
        fail(
            f"{server.name}: setup command did not emit clean JSON on stdout: "
            f"{stdout[:120]!r}; stderr: {stderr[:240]!r}"
        )
    try:
        payload = json.loads(stdout)
    except json.JSONDecodeError as error:
        fail(f"{server.name}: setup stdout is not JSON: {error}")
    missing = REQUIRED_FIELDS.difference(payload)
    if missing:
        fail(f"{server.name}: JSON missing fields: {', '.join(sorted(missing))}")
    if payload["exit_policy"] not in EXIT_POLICIES:
        fail(f"{server.name}: invalid exit_policy {payload['exit_policy']!r}")
    if not isinstance(payload["blocking_failures"], list):
        fail(f"{server.name}: blocking_failures must be an array")
    if not isinstance(payload["advisory_failures"], list):
        fail(f"{server.name}: advisory_failures must be an array")
    if output.returncode != 0 and payload["exit_policy"] != "blocking_failure":
        fail(f"{server.name}: nonzero exit with non-blocking policy")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--execute", action="store_true", help="run each binary setup command via cargo run")
    args = parser.parse_args()

    for server in SERVERS:
        check_plugin_layout(server)
        check_required_recipes(server)
        check_hook(server)
        if args.execute:
            check_binary(server)
        print(f"ok {server.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
