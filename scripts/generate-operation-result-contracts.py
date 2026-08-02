#!/usr/bin/env python3
"""Generate canonical closed result payload schemas for all Synapse operations."""
from __future__ import annotations

import argparse, copy, hashlib, json, sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CLASSIFICATIONS = ROOT / "docs/unify/03-contracts/examples/synapse-canonical-operations.json"
OUTPUT = ROOT / "docs/unify/03-contracts/examples/synapse-operation-results.json"
EXPECTED_COUNT = 59

TEXT = {"type": "string", "maxLength": 1048576}
SHORT = {"type": "string", "minLength": 1, "maxLength": 4096}
DIGEST = {"type": "string", "pattern": "^(?:sha256:)?[0-9a-f]{64}$"}
ARTIFACT = {
    "type": "object", "additionalProperties": False,
    "required": ["uri", "media_type", "bytes"],
    "properties": {
        "uri": {"type": "string", "minLength": 1, "maxLength": 4096},
        "media_type": {"type": "string", "minLength": 1, "maxLength": 256},
        "bytes": {"type": "integer", "minimum": 0},
        "sha256": DIGEST,
        "protected": {"type": "boolean", "default": False},
    },
}
RESOURCE = {"type": "object", "maxProperties": 128, "additionalProperties": True}

FAMILIES: dict[str, str] = {
    "product.help": "help", "fleet.nodes": "resource_list",
    "docker.info": "resource_detail", "docker.df": "metrics",
    "docker.images": "resource_list", "docker.networks": "resource_list", "docker.volumes": "resource_list",
    "docker.pull": "mutation", "docker.build": "mutation", "docker.rmi": "mutation", "docker.prune": "mutation",
    "container.list": "resource_list", "container.inspect": "resource_detail", "container.logs": "text",
    "container.stats": "metrics", "container.top": "resource_list", "container.search": "resource_list",
    "container.start": "mutation", "container.stop": "mutation", "container.restart": "mutation",
    "container.pause": "mutation", "container.resume": "mutation", "container.pull": "mutation",
    "container.recreate": "mutation", "container.exec": "command",
    "host.status": "status", "host.info": "resource_detail", "host.uptime": "metrics",
    "host.resources": "metrics", "host.services": "resource_list", "host.network": "resource_detail",
    "host.mounts": "resource_list", "host.ports": "resource_list", "host.doctor": "diagnostic_report",
    "compose.list": "resource_list", "compose.status": "status", "compose.up": "mutation",
    "compose.down": "mutation", "compose.restart": "mutation", "compose.recreate": "mutation",
    "compose.logs": "text", "compose.build": "mutation", "compose.pull": "mutation", "compose.refresh": "status",
    "files.read": "file_content", "files.find": "resource_list", "processes.list": "resource_list",
    "filesystem.usage": "metrics", "files.compare": "diff", "host.exec": "command",
    "host.exec_many": "fanout_command", "files.transfer": "transfer",
    "zfs.pools": "resource_list", "zfs.datasets": "resource_list", "zfs.snapshots": "resource_list",
    "logs.syslog": "text", "logs.journal": "text", "logs.kernel": "text", "logs.auth": "text",
}


def closed(required: list[str], properties: dict[str, Any], **extra: Any) -> dict[str, Any]:
    schema = {"type": "object", "additionalProperties": False, "required": required, "properties": properties}
    schema.update(extra)
    return schema


def output_channel(field: str) -> dict[str, Any]:
    return {
        "oneOf": [
            {"required": [field], "not": {"required": [f"{field}_artifact"]}},
            {"required": [f"{field}_artifact"], "not": {"required": [field]}},
            {"not": {"anyOf": [{"required": [field]}, {"required": [f"{field}_artifact"]}]}},
        ]
    }


def family_schema(family: str) -> dict[str, Any]:
    if family == "help":
        item = closed(["name", "summary"], {"name": SHORT, "summary": SHORT})
        return closed(["topics", "operations"], {"topics": {"type": "array", "maxItems": 256, "items": item}, "operations": {"type": "array", "maxItems": 1024, "items": item}})
    if family == "resource_list":
        return closed(["items", "count", "truncated"], {
            "items": {"type": "array", "maxItems": 10000, "items": RESOURCE},
            "count": {"type": "integer", "minimum": 0}, "truncated": {"type": "boolean"},
            "next_offset": {"type": ["integer", "null"], "minimum": 0},
        })
    if family == "resource_detail":
        return closed(["resource"], {"resource": RESOURCE})
    if family == "metrics":
        return closed(["metrics"], {"metrics": RESOURCE, "sampled_at": {"type": "string", "format": "date-time"}})
    if family == "status":
        return closed(["status"], {"status": SHORT, "details": RESOURCE, "refreshed": {"type": "boolean"}})
    if family == "mutation":
        return closed(["changed", "action", "summary"], {
            "changed": {"type": "boolean"}, "action": SHORT, "summary": SHORT,
            "backend_ref": SHORT, "target_revision": SHORT, "details": RESOURCE,
        })
    if family == "text":
        schema = closed(["bytes", "truncated", "encoding"], {
            "content": TEXT, "content_artifact": ARTIFACT,
            "bytes": {"type": "integer", "minimum": 0}, "truncated": {"type": "boolean"},
            "encoding": {"enum": ["utf-8", "binary", "unknown"]}, "line_count": {"type": "integer", "minimum": 0},
        })
        schema.update(output_channel("content")); return schema
    if family == "file_content":
        schema = copy.deepcopy(family_schema("text")); schema["properties"].update({"kind": {"enum": ["file", "directory", "tree"]}, "entries": {"type": "array", "maxItems": 10000, "items": RESOURCE}}); return schema
    if family == "command":
        schema = closed(["exit_code", "timed_out", "truncated"], {
            "exit_code": {"type": "integer", "minimum": -1, "maximum": 255},
            "stdout": TEXT, "stdout_artifact": ARTIFACT, "stderr": TEXT, "stderr_artifact": ARTIFACT,
            "timed_out": {"type": "boolean"}, "truncated": {"type": "boolean"},
        }, allOf=[output_channel("stdout"), output_channel("stderr")]); return schema
    if family == "fanout_command":
        per_target = closed(["target", "ok"], {"target": SHORT, "ok": {"type": "boolean"}, "output": family_schema("command"), "diagnostic_codes": {"type": "array", "uniqueItems": True, "items": {"type": "string"}}})
        return closed(["results", "success_count", "failure_count", "cancelled_count"], {"results": {"type": "array", "maxItems": 256, "items": per_target}, "success_count": {"type": "integer", "minimum": 0}, "failure_count": {"type": "integer", "minimum": 0}, "cancelled_count": {"type": "integer", "minimum": 0}})
    if family == "transfer":
        return closed(["source", "destination", "bytes", "verified"], {"source": SHORT, "destination": SHORT, "bytes": {"type": "integer", "minimum": 0}, "source_digest": DIGEST, "destination_digest": DIGEST, "verified": {"type": "boolean"}, "artifact": ARTIFACT})
    if family == "diff":
        schema = closed(["equal", "summary"], {"equal": {"type": "boolean"}, "summary": SHORT, "patch": TEXT, "patch_artifact": ARTIFACT, "source_digest": DIGEST, "target_digest": DIGEST}); schema.update(output_channel("patch")); return schema
    if family == "diagnostic_report":
        check = closed(["code", "status", "summary"], {"code": {"type": "string", "pattern": "^[a-z][a-z0-9_.-]+$"}, "status": {"enum": ["ok", "warning", "failed", "skipped"]}, "summary": SHORT, "evidence": RESOURCE})
        return closed(["overall", "checks"], {"overall": {"enum": ["ok", "warning", "failed"]}, "checks": {"type": "array", "maxItems": 256, "items": check}})
    raise ValueError(f"unknown result family {family}")


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def digest(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def build(classifications: dict[str, Any]) -> dict[str, Any]:
    operations = {item["name"]: item for item in classifications["operations"]}
    if set(FAMILIES) != set(operations):
        raise ValueError(f"result-family coverage mismatch: missing={sorted(set(operations)-set(FAMILIES))}, extra={sorted(set(FAMILIES)-set(operations))}")
    schemas = []
    for name in sorted(operations):
        operation = operations[name]; family = FAMILIES[name]; schema = family_schema(family)
        schema.update({"$schema": "https://json-schema.org/draft/2020-12/schema", "$id": operation["result_schema"], "title": f"{name} canonical result payload"})
        schemas.append({"operation_name": name, "schema_id": operation["result_schema"], "family": family, "schema": schema})
    bundle = {"format_version": 1, "classification_sha256": classifications["classification_sha256"], "schema_count": len(schemas), "schemas": schemas}
    bundle["result_schema_sha256"] = digest(schemas)
    validate(bundle, classifications)
    return bundle


def validate(bundle: dict[str, Any], classifications: dict[str, Any]) -> None:
    schemas = bundle.get("schemas")
    if not isinstance(schemas, list) or len(schemas) != EXPECTED_COUNT or bundle.get("schema_count") != EXPECTED_COUNT: raise ValueError("expected 59 result schemas")
    if bundle.get("classification_sha256") != classifications.get("classification_sha256"): raise ValueError("classification digest mismatch")
    if bundle.get("result_schema_sha256") != digest(schemas): raise ValueError("result schema digest is stale")
    operations = {item["name"]: item for item in classifications["operations"]}; names=set(); ids=set()
    for record in schemas:
        name=record["operation_name"]; schema=record["schema"]; schema_id=record["schema_id"]
        if name in names or schema_id in ids: raise ValueError(f"duplicate result contract {name}")
        names.add(name); ids.add(schema_id)
        if name not in operations or schema_id != operations[name]["result_schema"] or schema.get("$id") != schema_id: raise ValueError(f"result schema identity drift for {name}")
        if schema.get("additionalProperties") is not False: raise ValueError(f"result schema not closed for {name}")
        if not record.get("family"): raise ValueError(f"missing result family for {name}")
    if names != set(operations): raise ValueError("result schema coverage mismatch")


def load(path: Path) -> dict[str, Any]: return json.loads(path.read_text(encoding="utf-8"))
def write(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=True) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    parser=argparse.ArgumentParser(); parser.add_argument("action", choices=("generate","check")); parser.add_argument("--classifications",type=Path,default=CLASSIFICATIONS); parser.add_argument("--output",type=Path,default=OUTPUT); args=parser.parse_args()
    try:
        classifications=load(args.classifications); generated=build(classifications)
        if args.action=="generate": write(args.output,generated); print(f"wrote {EXPECTED_COUNT} result schemas ({generated['result_schema_sha256'][:12]})"); return 0
        committed=load(args.output); validate(committed,classifications)
        if committed != generated: raise ValueError("committed result schemas are stale")
        print(f"operation result contracts are valid ({EXPECTED_COUNT} schemas, {committed['result_schema_sha256'][:12]})"); return 0
    except (OSError,ValueError,json.JSONDecodeError,KeyError) as exc: print(f"error: {exc}",file=sys.stderr); return 1

if __name__=="__main__": raise SystemExit(main())
