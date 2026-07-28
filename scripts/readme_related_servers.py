"""Render and validate canonical RMCP README related-server sections."""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class RelatedServer:
    name: str
    url: str
    description: str

    @property
    def link(self) -> str:
        return f"[{self.name}]({self.url})"


RELATED_SERVERS = (
    RelatedServer("soma", "https://github.com/dinglebear-ai/soma", "RMCP runtime for provider-backed MCP servers."),
    RelatedServer("runifi", "https://github.com/dinglebear-ai/runifi", "UniFi controller REST API bridge."),
    RelatedServer(
        "rtailscale",
        "https://github.com/dinglebear-ai/rtailscale",
        "Tailscale API bridge for devices, users, and tailnet operations.",
    ),
    RelatedServer(
        "unraid",
        "https://github.com/dinglebear-ai/unraid",
        "Unraid monorepo: Python and Rust GraphQL MCP servers plus Unraid plugins.",
    ),
    RelatedServer(
        "rapprise",
        "https://github.com/dinglebear-ai/rapprise",
        "Apprise notification fan-out bridge for many delivery backends.",
    ),
    RelatedServer(
        "rgotify",
        "https://github.com/dinglebear-ai/rgotify",
        "Gotify push notification bridge for sends, messages, apps, and clients.",
    ),
    RelatedServer(
        "rarcane",
        "https://github.com/dinglebear-ai/rarcane",
        "Arcane Docker management bridge for containers and related resources.",
    ),
    RelatedServer(
        "yarr",
        "https://github.com/dinglebear-ai/yarr",
        "Media-stack bridge for Sonarr, Radarr, Prowlarr, Plex, and related services.",
    ),
    RelatedServer(
        "rytdl", "https://github.com/dinglebear-ai/rytdl", "Media download and metadata workflow server."
    ),
    RelatedServer(
        "synapse",
        "https://github.com/dinglebear-ai/synapse",
        "Local Synapse workflow server for scout and flux actions.",
    ),
    RelatedServer(
        "cortex", "https://github.com/dinglebear-ai/cortex", "Syslog and homelab log aggregation MCP server."
    ),
    RelatedServer(
        "axon",
        "https://github.com/dinglebear-ai/axon",
        "RAG, crawl, scrape, extract, and semantic search project.",
    ),
    RelatedServer(
        "labby", "https://github.com/dinglebear-ai/labby", "Homelab control plane and MCP gateway project."
    ),
    RelatedServer("lumen", "https://github.com/jmagar/lumen", "Local semantic code search MCP server."),
)

# Historical directory / repo names that still resolve to a canonical entry.
# The fleet moved to the dinglebear-ai org and dropped the `-rmcp` suffix;
# GitHub redirects keep the old names working, so keep mapping them.
RELATED_SERVER_SELF_ALIASES = {
    "apprise-rmcp": "rapprise",
    "arcane-rmcp": "rarcane",
    "cortex-rmcp": "cortex",
    "gotify-rmcp": "rgotify",
    "lab": "labby",
    "labby-mcp": "labby",
    "rmcp-template": "soma",
    "rtemplate-mcp": "soma",
    "runraid": "unraid",
    "soma-rmcp": "soma",
    "synapse-rmcp": "synapse",
    "tailscale-rmcp": "rtailscale",
    "unifi-rmcp": "runifi",
    "unraid-mcp": "unraid",
    "unraid-rmcp": "unraid",
    "yarr-mcp": "yarr",
    "ytdl-rmcp": "rytdl",
}

STALE_RELATED_SERVER_NAMES = (
    "rustarr",
    "rustcane",
    "rustifi",
    "rustscale",
    "synapse2",
    "unrust",
)


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def first_heading(text: str) -> str | None:
    match = re.search(r"^#\s+(.+?)\s*$", text, flags=re.MULTILINE)
    if not match:
        return None
    return match.group(1).strip()


def normalize_self_name(value: str) -> str:
    normalized = value.strip().lower()
    return RELATED_SERVER_SELF_ALIASES.get(normalized, normalized)


def known_server_names() -> set[str]:
    return {server.name for server in RELATED_SERVERS}


def infer_self_name(text: str, path: Path) -> str | None:
    candidates = [first_heading(text), path.parent.name, path.parent.parent.name]
    if path.parent.name == "packages":
        candidates.append(path.stem)
    for candidate in candidates:
        if not candidate:
            continue
        normalized = normalize_self_name(candidate)
        if normalized in known_server_names():
            return normalized
    return None


def related_servers_section_bounds(text: str) -> tuple[int, int, int, str] | None:
    match = re.search(r"^## Related Servers\s*$", text, flags=re.MULTILINE)
    if not match:
        return None
    next_heading = re.search(r"^##\s+", text[match.end() :], flags=re.MULTILINE)
    end = match.end() + next_heading.start() if next_heading else len(text)
    section = text[match.end() : end]
    return match.start(), end, line_number(text, match.start()), section


def render_related_servers_section(self_name: str | None) -> str:
    lines = ["## Related Servers", ""]
    normalized_self = normalize_self_name(self_name) if self_name else None
    for server in RELATED_SERVERS:
        if server.name == normalized_self:
            continue
        lines.append(f"- {server.link} - {server.description}")
    return "\n".join(lines).rstrip() + "\n\n"


def replace_related_servers_section(text: str, path: Path, self_name: str | None = None) -> str:
    bounds = related_servers_section_bounds(text)
    if not bounds:
        raise RuntimeError("missing ## Related Servers section")
    start, end, _, _ = bounds
    resolved_self = normalize_self_name(self_name) if self_name else infer_self_name(text, path)
    return text[:start] + render_related_servers_section(resolved_self) + text[end:]


def check_related_servers(text: str, path: Path) -> list[str]:
    bounds = related_servers_section_bounds(text)
    if not bounds:
        return ["missing ## Related Servers section"]

    _, _, start_line, section = bounds
    failures: list[str] = []
    lowered = section.lower()
    self_name = infer_self_name(text, path)

    for stale in STALE_RELATED_SERVER_NAMES:
        if re.search(rf"\b{re.escape(stale)}\b", lowered):
            failures.append(
                f"line {start_line}: Related Servers uses stale implementation name `{stale}`"
            )

    for server in RELATED_SERVERS:
        if server.name == self_name:
            if server.link in section:
                failures.append(
                    f"line {start_line}: Related Servers must omit current repo `{server.name}`"
                )
            continue
        if server.link not in section:
            failures.append(
                f"line {start_line}: Related Servers missing linked entry `{server.link}`"
            )

    for match in re.finditer(r"^-\s+(.+)$", section, flags=re.MULTILINE):
        line = match.group(1).strip()
        linked_known = any(server.link in line for server in RELATED_SERVERS)
        if not linked_known:
            entry_line = start_line + section.count("\n", 0, match.start())
            failures.append(
                f"line {entry_line}: Related Servers entry is not a recognized linked repo"
            )

    return failures
