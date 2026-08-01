"""Generated typed provider catalog models. Do not edit by hand."""

from __future__ import annotations

from typing import Any, Literal, NotRequired, Required, TypedDict

__all__ = [
    "BrokerCapability",
    "BrowserCapability",
    "CliFlag",
    "CliOverlay",
    "CliPositional",
    "DocsOverlay",
    "Elicitation",
    "EnvCapability",
    "EnvRequirement",
    "Example",
    "FilesystemCapability",
    "GithubCapability",
    "HostCapabilities",
    "JsonSchema",
    "Limits",
    "McpPrimitiveOverlay",
    "McpToolOverlay",
    "Name",
    "NetworkCapability",
    "PaletteOverlay",
    "Parameter",
    "PluginOverlay",
    "Prompt",
    "Provider",
    "Resource",
    "RestOverlay",
    "Task",
    "TerminalCapability",
    "Tool",
    "UiOverlay",
]

class BrokerCapability(TypedDict, total=False):
    enabled: NotRequired[bool]
    logging: NotRequired[bool]
    metrics: NotRequired[bool]
    progress: NotRequired[bool]
    secret_names: NotRequired[list[str]]
    state_namespace: NotRequired[str | None]
    state_write: NotRequired[bool]

class BrowserCapability(TypedDict, total=False):
    allowed_origins: NotRequired[list[str]]
    enabled: NotRequired[bool]

class CliFlag(TypedDict, total=False):
    description: NotRequired[str]
    long: NotRequired[str]
    name: Required[str]
    required: NotRequired[bool]
    short: NotRequired[str]
    value_name: NotRequired[str]

class CliOverlay(TypedDict, total=False):
    about: NotRequired[str]
    aliases: NotRequired[list[Name]]
    command: NotRequired[Name]
    default_output: NotRequired[Literal['json', 'table', 'plain', 'yaml', 'markdown']]
    enabled: NotRequired[bool]
    flags: NotRequired[list[CliFlag]]
    hidden: NotRequired[bool]
    interactive: NotRequired[bool]
    long_about: NotRequired[str]

class CliPositional(TypedDict, total=False):
    description: NotRequired[str]
    name: Required[Name]
    required: NotRequired[bool]

class DocsOverlay(TypedDict, total=False):
    examples: NotRequired[list[Example]]
    troubleshooting: NotRequired[list[str]]
    when_to_use: NotRequired[str]

class Elicitation(TypedDict, total=False):
    description: Required[str]
    mcp: NotRequired[McpPrimitiveOverlay]
    name: Required[Name]
    schema: Required[JsonSchema]
    scope: NotRequired[str]

class EnvCapability(TypedDict, total=False):
    allowed: NotRequired[list[str]]
    enabled: NotRequired[bool]

class EnvRequirement(TypedDict, total=False):
    allow_unprefixed: NotRequired[bool]
    default: NotRequired[str | float | bool | list[Any] | dict[str, Any] | None]
    description: NotRequired[str]
    name: Required[str]
    required: NotRequired[bool]
    sensitive: NotRequired[bool]
    server_prefixed: NotRequired[bool]

class Example(TypedDict, total=False):
    cli: NotRequired[str]
    description: NotRequired[str]
    input: NotRequired[Any]
    mcp: NotRequired[dict[str, Any]]
    output: NotRequired[Any]
    rest: NotRequired[dict[str, Any]]
    title: NotRequired[str]

class FilesystemCapability(TypedDict, total=False):
    enabled: NotRequired[bool]
    read_roots: NotRequired[list[str]]
    write_roots: NotRequired[list[str]]

class GithubCapability(TypedDict, total=False):
    allowed_repos: NotRequired[list[str]]
    enabled: NotRequired[bool]
    read_only: NotRequired[bool]

class HostCapabilities(TypedDict, total=False):
    broker: NotRequired[BrokerCapability]
    browser: NotRequired[BrowserCapability]
    env: NotRequired[EnvCapability]
    filesystem: NotRequired[FilesystemCapability]
    github: NotRequired[GithubCapability]
    network: NotRequired[NetworkCapability]
    terminal: NotRequired[TerminalCapability]

JsonSchema = dict[str, Any]

class Limits(TypedDict, total=False):
    max_input_bytes: NotRequired[int]
    max_response_bytes: NotRequired[int]
    timeout_ms: NotRequired[int]

class McpPrimitiveOverlay(TypedDict, total=False):
    annotations: NotRequired[dict[str, Any]]
    enabled: NotRequired[bool]
    title: NotRequired[str]

McpToolOverlay = McpPrimitiveOverlay

Name = str

class NetworkCapability(TypedDict, total=False):
    allowed_hosts: NotRequired[list[str]]
    enabled: NotRequired[bool]

class PaletteOverlay(TypedDict, total=False):
    arg_mode: NotRequired[Literal['none', 'optionalSingle', 'single', 'split', 'schema']]
    aurora_blocks: NotRequired[list[str]]
    category: NotRequired[str]
    enabled: NotRequired[bool]
    icon: NotRequired[str]
    result_view: NotRequired[Literal['auto', 'json', 'markdown', 'table', 'code', 'artifact']]
    tone: NotRequired[Literal['info', 'success', 'warn', 'neutral', 'rose', 'orange']]

class Parameter(TypedDict, total=False):
    description: NotRequired[str]
    name: Required[Name]
    required: NotRequired[bool]
    schema: NotRequired[JsonSchema]

class PluginOverlay(TypedDict, total=False):
    generate_claude: NotRequired[bool]
    generate_codex: NotRequired[bool]
    generate_gemini: NotRequired[bool]
    generate_marketplace: NotRequired[bool]
    generate_skill: NotRequired[bool]
    mcp_registration: NotRequired[Literal['none', 'plugin-root', 'gemini-inline', 'both']]

class Prompt(TypedDict, total=False):
    arguments_schema: NotRequired[JsonSchema]
    description: Required[str]
    examples: NotRequired[list[Example]]
    mcp: NotRequired[McpPrimitiveOverlay]
    name: Required[Name]
    scope: NotRequired[str]
    template: NotRequired[str]

class Provider(TypedDict, total=False):
    description: NotRequired[str]
    enabled: NotRequired[bool]
    homepage: NotRequired[str]
    kind: Required[Literal['static-rust', 'openapi', 'ai-sdk', 'wasm', 'mcp', 'python', 'langchain', 'llamaindex']]
    name: Required[Name]
    source: NotRequired[str]
    title: NotRequired[str]
    version: NotRequired[str]

class Resource(TypedDict, total=False):
    annotations: NotRequired[dict[str, Any]]
    description: Required[str]
    mcp: NotRequired[McpPrimitiveOverlay]
    mime_type: NotRequired[str]
    name: Required[str]
    scope: NotRequired[str]
    uri_template: Required[str]

class RestOverlay(TypedDict, total=False):
    deprecated: NotRequired[bool]
    description: NotRequired[str]
    enabled: NotRequired[bool]
    method: NotRequired[Literal['GET', 'POST', 'PUT', 'PATCH', 'DELETE']]
    path: NotRequired[str]
    path_params: NotRequired[Any]
    query_params: NotRequired[Any]
    request_body_schema: NotRequired[Any]
    summary: NotRequired[str]
    tags: NotRequired[list[str]]

class Task(TypedDict, total=False):
    description: Required[str]
    input_schema: Required[JsonSchema]
    limits: NotRequired[Limits]
    mcp: NotRequired[McpPrimitiveOverlay]
    name: Required[Name]
    output_schema: NotRequired[JsonSchema]
    scope: NotRequired[str]

class TerminalCapability(TypedDict, total=False):
    allowlist: NotRequired[list[str]]
    enabled: NotRequired[bool]
    working_dir: NotRequired[str]

class Tool(TypedDict, total=False):
    cli: NotRequired[CliOverlay]
    cost: NotRequired[Literal['cheap', 'moderate', 'expensive', 'write']]
    description: Required[str]
    destructive: NotRequired[bool]
    env: NotRequired[list[EnvRequirement]]
    examples: NotRequired[list[Example]]
    input_schema: Required[JsonSchema]
    limits: NotRequired[Limits]
    mcp: NotRequired[McpToolOverlay]
    meta: NotRequired[dict[str, Any]]
    name: Required[Name]
    output_schema: NotRequired[JsonSchema]
    palette: NotRequired[PaletteOverlay]
    requires_admin: NotRequired[bool]
    rest: NotRequired[RestOverlay]
    scope: NotRequired[str]
    title: NotRequired[str]
    ui: NotRequired[UiOverlay]

class UiOverlay(TypedDict, total=False):
    aurora_registry_dependencies: NotRequired[list[str]]
    categories: NotRequired[list[str]]
    enabled: NotRequired[bool]
    meta: NotRequired[dict[str, Any]]
    shadcn_items: NotRequired[list[str]]
