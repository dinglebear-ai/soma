#!/usr/bin/env python3
"""Generate closed parameter schemas and diagnostic surface projections."""
from __future__ import annotations

import argparse, copy, hashlib, json, sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CLASSIFICATIONS = ROOT / "docs/unify/03-contracts/examples/synapse-canonical-operations.json"
PARAMETERS = ROOT / "docs/unify/03-contracts/examples/synapse-operation-parameters.json"
DIAGNOSTICS = ROOT / "docs/unify/03-contracts/examples/operation-diagnostic-projections.json"
EXPECTED_COUNT = 59
SURFACE_ONLY = {"action", "subaction", "response_format", "format"}

S = {"type": "string", "minLength": 1, "maxLength": 4096}
SHORT = {"type": "string", "minLength": 1, "maxLength": 256}
HOST = {"type": "string", "minLength": 1, "maxLength": 128, "pattern": r"^[^\u0000-\u001f\u007f]+$"}
PATH = {"type": "string", "minLength": 1, "maxLength": 4096, "pattern": r"^/"}
REL_PATH = {"type": "string", "minLength": 1, "maxLength": 4096, "allOf": [{"not": {"pattern": r"(^|/)\.\.(/|$)"}}, {"not": {"pattern": r"^/"}}]}
ARGV = {"type": "array", "minItems": 1, "maxItems": 256, "items": {"type": "string", "maxLength": 4096, "pattern": r"^[^\u0000]+$"}}
ARGS = {"type": "array", "maxItems": 256, "items": {"type": "string", "maxLength": 4096, "pattern": r"^[^\u0000]+$"}}
TARGETS = {"type": "array", "minItems": 1, "maxItems": 256, "items": {"type": "object", "additionalProperties": False, "required": ["host"], "properties": {"host": HOST, "path": PATH}}}
ALLOWED_COMMANDS = ["cat", "head", "tail", "grep", "rg", "ls", "tree", "wc", "uniq", "diff", "stat", "file", "du", "df", "pwd", "hostname", "uptime", "whoami"]

FIELDS: dict[str, dict[str, Any]] = {
    "topic": SHORT, "host": HOST, "source_host": HOST, "target_host": HOST, "dest_host": HOST,
    "path": PATH, "source_path": PATH, "target_path": PATH, "dest_path": PATH, "context": PATH,
    "exec_workdir": PATH, "container_id": SHORT, "project": SHORT, "service": SHORT, "image": SHORT,
    "tag": SHORT, "dockerfile": REL_PATH, "prune_target": {"enum": ["containers", "images", "volumes", "networks", "buildcache", "all"]},
    "state": {"enum": ["running", "exited", "paused", "restarting", "active", "inactive", "failed", "all"]},
    "name_filter": SHORT, "image_filter": SHORT, "label_filter": SHORT,
    "since": {"type": "string", "minLength": 1, "maxLength": 128}, "until": {"type": "string", "minLength": 1, "maxLength": 128},
    "grep": {"type": "string", "maxLength": 4096}, "stream": {"enum": ["stdout", "stderr", "both"]},
    "summary": {"type": "boolean"}, "query": {"type": "string", "minLength": 1, "maxLength": 4096},
    "exec_user": SHORT, "force": {"type": "boolean"}, "dangling_only": {"type": "boolean"},
    "no_cache": {"type": "boolean"}, "pull": {"type": "boolean"}, "remove_volumes": {"type": "boolean"},
    "tree": {"type": "boolean"}, "recursive": {"type": "boolean"},
    "lines": {"type": "integer", "minimum": 1, "maximum": 500}, "limit": {"type": "integer", "minimum": 1, "maximum": 10000},
    "offset": {"type": "integer", "minimum": 0, "maximum": 100000}, "depth": {"type": "integer", "minimum": 1, "maximum": 20},
    "exec_timeout_ms": {"type": "integer", "minimum": 1000, "maximum": 300000}, "timeout_secs": {"type": "integer", "minimum": 1, "maximum": 300},
    "protocol": {"enum": ["tcp", "udp"]}, "checks": {"type": "string", "minLength": 1, "maxLength": 1024},
    "pattern": {"type": "string", "minLength": 1, "maxLength": 1024, "not": {"pattern": r"^-"}},
    "sort": {"enum": ["cpu", "mem", "pid", "time"]}, "user": SHORT,
    "content": {"type": "string", "maxLength": 1048576}, "args": ARGS, "targets": TARGETS,
    "pool": SHORT, "dataset_type": {"enum": ["filesystem", "volume", "snapshot", "bookmark", "all"]},
    "dataset": SHORT, "unit": SHORT, "priority": {"type": "string", "minLength": 1, "maxLength": 32},
}

OPTIONAL: dict[str, list[str]] = {
    "product.help": ["topic"], "docker.info": ["host"], "docker.df": ["host"],
    "docker.images": ["host", "dangling_only"], "docker.networks": ["host"], "docker.volumes": ["host"],
    "docker.pull": [], "docker.build": ["dockerfile", "no_cache"], "docker.rmi": [], "docker.prune": [],
    "container.list": ["host", "state", "name_filter", "image_filter", "label_filter"],
    "container.inspect": ["host", "summary"], "container.logs": ["host", "lines", "since", "until", "grep", "stream"],
    "container.stats": ["host", "container_id"], "container.top": ["host"], "container.search": ["host"],
    "container.start": [], "container.stop": [], "container.restart": [], "container.pause": [], "container.resume": [], "container.pull": [],
    "container.recreate": ["pull"], "container.exec": ["exec_user", "exec_workdir", "exec_timeout_ms"],
    "host.status": ["host"], "host.info": ["host"], "host.uptime": ["host"], "host.resources": ["host"],
    "host.services": ["state", "service"], "host.network": ["host"], "host.mounts": [],
    "host.ports": ["protocol", "limit", "offset"], "host.doctor": ["checks"],
    "compose.list": [], "compose.status": ["service"], "compose.up": [], "compose.down": ["remove_volumes", "force"],
    "compose.restart": [], "compose.recreate": [], "compose.logs": ["lines", "since", "service"],
    "compose.build": ["service"], "compose.pull": ["service"], "compose.refresh": [], "fleet.nodes": [],
    "files.read": ["tree", "depth"], "files.find": ["depth", "limit"],
    "processes.list": ["sort", "grep", "user", "limit"], "filesystem.usage": ["path"],
    "files.compare": ["target_host", "target_path", "content"], "host.exec": ["path", "args", "timeout_secs"],
    "host.exec_many": ["args", "timeout_secs"], "files.transfer": [], "zfs.pools": ["pool"],
    "zfs.datasets": ["pool", "dataset_type", "recursive"], "zfs.snapshots": ["pool", "dataset", "limit"],
    "logs.syslog": ["lines", "grep"], "logs.journal": ["lines", "grep", "unit", "priority", "since", "until"],
    "logs.kernel": ["lines", "grep"], "logs.auth": ["lines", "grep"],
}

@dataclass(frozen=True)
class Projection:
    category: str; cli_exit_code: int; http_status: int; mcp_error_code: int
    event_severity: str; retry: str; terminal: bool

P = Projection
PROJECTIONS = {
    "authorization.denied": P("authorization",3,403,-32001,"error","never",True),
    "authorization.expired": P("authorization",3,401,-32001,"error","never",True),
    "authorization.required": P("authorization",3,401,-32001,"error","never",True),
    "backend.unavailable": P("unavailable",6,503,-32003,"error","conditional",True),
    "capability.unsupported": P("unsupported",2,422,-32002,"error","never",True),
    "command.failed": P("backend",9,500,-32603,"error","conditional",True),
    "command.rejected": P("input",2,400,-32602,"error","never",True),
    "docker.conflict": P("conflict",5,409,-32009,"error","conditional",True),
    "docker.not_found": P("not_found",4,404,-32004,"error","never",True),
    "docker.unavailable": P("unavailable",6,503,-32003,"error","conditional",True),
    "filesystem.not_found": P("not_found",4,404,-32004,"error","never",True),
    "filesystem.path_denied": P("authorization",3,403,-32001,"error","never",True),
    "filesystem.too_large": P("limit",2,413,-32012,"error","never",True),
    "fleet.empty": P("empty",0,200,0,"info","never",False),
    "fleet.unavailable": P("unavailable",6,503,-32003,"error","conditional",True),
    "host.not_found": P("not_found",4,404,-32004,"error","never",True),
    "host.unreachable": P("unavailable",6,503,-32003,"error","conditional",True),
    "internal.failure": P("internal",9,500,-32603,"error","never",True),
    "logs.truncated": P("truncated",0,206,0,"warning","never",False),
    "logs.unavailable": P("unavailable",6,503,-32003,"error","conditional",True),
    "mutation.uncertain": P("uncertain",10,202,-32010,"warning","never",True),
    "operation.cancelled": P("cancelled",8,499,-32800,"warning","never",True),
    "operation.timeout": P("timeout",7,504,-32008,"error","conditional",True),
    "output.truncated": P("truncated",0,206,0,"warning","never",False),
    "plan.required": P("input",2,400,-32602,"error","never",True),
    "plan.stale": P("conflict",5,409,-32009,"error","never",True),
    "product.unavailable": P("unavailable",6,503,-32003,"error","conditional",True),
    "request.invalid": P("input",2,400,-32602,"error","never",True),
    "target.not_found": P("not_found",4,404,-32004,"error","never",True),
    "verification.failed": P("verification",11,422,-32011,"error","never",True),
    "verification.inconclusive": P("verification",0,202,0,"warning","conditional",False),
    "zfs.not_found": P("not_found",4,404,-32004,"error","never",True),
    "zfs.unavailable": P("unavailable",6,503,-32003,"error","conditional",True),
}

def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def digest(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def field_schema(operation: str, field: str) -> dict[str, Any]:
    if field == "command":
        return copy.deepcopy(ARGV if operation == "container.exec" else {"enum": ALLOWED_COMMANDS})
    schema = copy.deepcopy(FIELDS[field])
    if field == "force" and operation in {"docker.rmi", "docker.prune"}:
        return {"const": True}
    if field == "pull" and operation == "container.recreate":
        schema["default"] = True
    if field == "tree" and operation == "files.read":
        schema["default"] = False
    if field == "recursive":
        schema["default"] = False
    if field == "depth":
        schema["default"] = 3 if operation == "files.read" else 10
    if field == "limit":
        if operation == "files.find":
            schema["default"] = 500
        elif operation == "processes.list":
            schema["default"] = 50
    if field == "lines":
        if operation == "container.logs":
            schema["default"] = 50
        elif operation.startswith("logs."):
            schema["default"] = 100
    if field == "timeout_secs":
        schema["default"] = 30
    if field == "exec_timeout_ms":
        schema["default"] = 30000
    return schema


def parameter_schema(operation: dict[str, Any]) -> dict[str, Any]:
    name = operation["name"]
    required = list(operation["required"]["fields"])
    required_any = [list(group["fields"]) for group in operation["required_any"]]
    try:
        optional = OPTIONAL[name]
    except KeyError as exc:
        raise ValueError(f"missing optional-field decision for {name}") from exc
    fields = sorted(set(required + optional + [field for group in required_any for field in group]))
    if SURFACE_ONLY.intersection(fields):
        raise ValueError(f"surface-only fields leaked into {name}")
    missing = [field for field in fields if field not in FIELDS and field != "command"]
    if missing:
        raise ValueError(f"missing field schemas for {name}: {missing}")
    schema: dict[str, Any] = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": operation["parameter_schema"],
        "type": "object",
        "additionalProperties": False,
        "properties": {field: field_schema(name, field) for field in fields},
        "required": sorted(required),
    }
    if required_any:
        branches = []
        for group in required_any:
            prohibited = sorted({field for other in required_any if other != group for field in other})
            branch: dict[str, Any] = {"required": sorted(group)}
            if prohibited:
                branch["not"] = {"anyOf": [{"required": [field]} for field in prohibited]}
            branches.append(branch)
        schema["oneOf"] = branches
    if name == "compose.down":
        schema["allOf"] = [{
            "if": {"properties": {"remove_volumes": {"const": True}}, "required": ["remove_volumes"]},
            "then": {"properties": {"force": {"const": True}}, "required": ["force"]},
        }]
    return schema


def build_parameter_bundle(classifications: dict[str, Any]) -> dict[str, Any]:
    schemas = [{
        "operation_name": operation["name"],
        "schema_id": operation["parameter_schema"],
        "schema": parameter_schema(operation),
    } for operation in classifications["operations"]]
    bundle = {
        "format_version": 1,
        "classification_sha256": classifications["classification_sha256"],
        "excluded_surface_fields": sorted(SURFACE_ONLY),
        "schema_count": len(schemas),
        "schemas": schemas,
    }
    bundle["parameter_schema_sha256"] = digest(schemas)
    validate_parameters(bundle, classifications)
    return bundle


def validate_parameters(bundle: dict[str, Any], classifications: dict[str, Any]) -> None:
    schemas = bundle.get("schemas")
    if not isinstance(schemas, list) or len(schemas) != EXPECTED_COUNT:
        raise ValueError(f"expected {EXPECTED_COUNT} parameter schemas")
    if bundle.get("schema_count") != len(schemas):
        raise ValueError("schema_count does not match schema length")
    if bundle.get("classification_sha256") != classifications.get("classification_sha256"):
        raise ValueError("parameter schemas target the wrong classification digest")
    if bundle.get("parameter_schema_sha256") != digest(schemas):
        raise ValueError("parameter schema digest is stale")
    operations = {operation["name"]: operation for operation in classifications["operations"]}
    if set(OPTIONAL) != set(operations):
        raise ValueError("optional-field decisions do not cover exactly the canonical operations")
    names: set[str] = set()
    ids: set[str] = set()
    for record in schemas:
        name = record["operation_name"]
        if name not in operations:
            raise ValueError(f"unknown operation schema {name}")
        if name in names:
            raise ValueError(f"duplicate parameter schema for {name}")
        names.add(name)
        schema_id = record["schema_id"]
        if schema_id in ids:
            raise ValueError(f"duplicate schema id {schema_id}")
        ids.add(schema_id)
        operation = operations[name]
        schema = record["schema"]
        if schema_id != operation["parameter_schema"] or schema.get("$id") != schema_id:
            raise ValueError(f"schema identity drift for {name}")
        if schema.get("additionalProperties") is not False:
            raise ValueError(f"parameter schema is not closed for {name}")
        properties = set(schema.get("properties", {}))
        if properties.intersection(SURFACE_ONLY):
            raise ValueError(f"surface-only property leaked into {name}")
        required = set(operation["required"]["fields"])
        if set(schema.get("required", [])) != required or not required.issubset(properties):
            raise ValueError(f"required fields drift for {name}")
    if names != set(operations):
        raise ValueError("parameter schema coverage mismatch")


def build_diagnostic_bundle(classifications: dict[str, Any]) -> dict[str, Any]:
    vocabulary = sorted({code for operation in classifications["operations"] for code in operation["diagnostic_codes"]})
    if set(vocabulary) != set(PROJECTIONS):
        missing = sorted(set(vocabulary) - set(PROJECTIONS))
        extra = sorted(set(PROJECTIONS) - set(vocabulary))
        raise ValueError(f"diagnostic projection mismatch: missing={missing}, extra={extra}")
    mappings = [{"code": code, **asdict(PROJECTIONS[code])} for code in vocabulary]
    bundle = {
        "format_version": 1,
        "classification_sha256": classifications["classification_sha256"],
        "mapping_count": len(mappings),
        "mappings": mappings,
    }
    bundle["projection_sha256"] = digest(mappings)
    validate_diagnostics(bundle, classifications)
    return bundle


def validate_diagnostics(bundle: dict[str, Any], classifications: dict[str, Any]) -> None:
    mappings = bundle.get("mappings")
    if not isinstance(mappings, list) or len(mappings) != len(PROJECTIONS):
        raise ValueError("diagnostic mapping count is invalid")
    if bundle.get("mapping_count") != len(mappings):
        raise ValueError("mapping_count does not match mappings")
    if bundle.get("classification_sha256") != classifications.get("classification_sha256"):
        raise ValueError("diagnostic mappings target the wrong classification digest")
    if bundle.get("projection_sha256") != digest(mappings):
        raise ValueError("diagnostic projection digest is stale")
    codes = [mapping["code"] for mapping in mappings]
    if codes != sorted(set(codes)):
        raise ValueError("diagnostic mappings are not sorted and unique")
    for mapping in mappings:
        if not (0 <= mapping["cli_exit_code"] <= 125):
            raise ValueError(f"invalid CLI exit code for {mapping['code']}")
        if not (100 <= mapping["http_status"] <= 599):
            raise ValueError(f"invalid HTTP status for {mapping['code']}")
        if mapping["event_severity"] not in {"info", "warning", "error"}:
            raise ValueError(f"invalid event severity for {mapping['code']}")
        if mapping["retry"] not in {"never", "safe", "conditional"}:
            raise ValueError(f"invalid retry projection for {mapping['code']}")


def load(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def write(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=True) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("generate", "check"))
    parser.add_argument("--classifications", type=Path, default=CLASSIFICATIONS)
    parser.add_argument("--parameters", type=Path, default=PARAMETERS)
    parser.add_argument("--diagnostics", type=Path, default=DIAGNOSTICS)
    args = parser.parse_args()
    try:
        classifications = load(args.classifications)
        parameters = build_parameter_bundle(classifications)
        diagnostics = build_diagnostic_bundle(classifications)
        if args.action == "generate":
            write(args.parameters, parameters)
            write(args.diagnostics, diagnostics)
            print(
                f"wrote {parameters['schema_count']} parameter schemas "
                f"({parameters['parameter_schema_sha256'][:12]}) and "
                f"{diagnostics['mapping_count']} diagnostic projections "
                f"({diagnostics['projection_sha256'][:12]})"
            )
            return 0
        committed_parameters = load(args.parameters)
        committed_diagnostics = load(args.diagnostics)
        validate_parameters(committed_parameters, classifications)
        validate_diagnostics(committed_diagnostics, classifications)
        if committed_parameters != parameters:
            raise ValueError("committed parameter schemas are stale")
        if committed_diagnostics != diagnostics:
            raise ValueError("committed diagnostic projections are stale")
        print(f"operation surface contracts are valid ({len(parameters['schemas'])} schemas, {len(diagnostics['mappings'])} diagnostics)")
        return 0
    except (OSError, ValueError, json.JSONDecodeError, KeyError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

