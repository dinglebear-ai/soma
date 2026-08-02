#!/usr/bin/env python3
"""Generate and validate canonical operation classifications for Synapse."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
LEGACY_FIXTURE = ROOT / "docs/unify/03-contracts/examples/synapse-operations.json"
DEFAULT_OUTPUT = ROOT / "docs/unify/03-contracts/examples/synapse-canonical-operations.json"
EXPECTED_COUNT = 59


@dataclass(frozen=True)
class MutationDecision:
    risk: str
    reversibility: str
    planning: str
    progress: str
    cancellation: str
    verification: str
    fanout: str
    retry: str
    idempotent: bool


MUTATIONS: dict[str, MutationDecision] = {
    "docker.pull": MutationDecision("safe", "conditional", "optional", "required", "optional", "required", "unsupported", "safe", True),
    "docker.build": MutationDecision("privileged", "conditional", "required", "required", "optional", "required", "unsupported", "never", False),
    "docker.rmi": MutationDecision("destructive", "conditional", "required", "optional", "optional", "required", "unsupported", "conditional", True),
    "docker.prune": MutationDecision("destructive", "irreversible", "required", "optional", "optional", "required", "unsupported", "never", False),
    "container.start": MutationDecision("disruptive", "reversible", "required", "unsupported", "unsupported", "required", "unsupported", "safe", True),
    "container.stop": MutationDecision("disruptive", "reversible", "required", "optional", "optional", "required", "unsupported", "safe", True),
    "container.restart": MutationDecision("disruptive", "reversible", "required", "optional", "optional", "required", "unsupported", "never", False),
    "container.pause": MutationDecision("disruptive", "reversible", "required", "unsupported", "unsupported", "required", "unsupported", "safe", True),
    "container.resume": MutationDecision("disruptive", "reversible", "required", "unsupported", "unsupported", "required", "unsupported", "safe", True),
    "container.pull": MutationDecision("safe", "conditional", "optional", "required", "optional", "required", "unsupported", "safe", True),
    "container.recreate": MutationDecision("destructive", "conditional", "required", "optional", "optional", "required", "unsupported", "never", False),
    "container.exec": MutationDecision("privileged", "irreversible", "required", "optional", "optional", "unsupported", "unsupported", "never", False),
    "compose.up": MutationDecision("disruptive", "conditional", "required", "optional", "optional", "required", "unsupported", "conditional", True),
    "compose.down": MutationDecision("destructive", "conditional", "required", "optional", "optional", "required", "unsupported", "conditional", True),
    "compose.restart": MutationDecision("disruptive", "reversible", "required", "optional", "optional", "required", "unsupported", "never", False),
    "compose.recreate": MutationDecision("destructive", "conditional", "required", "optional", "optional", "required", "unsupported", "never", False),
    "compose.build": MutationDecision("privileged", "conditional", "required", "required", "optional", "required", "unsupported", "never", False),
    "compose.pull": MutationDecision("safe", "conditional", "optional", "required", "optional", "required", "unsupported", "safe", True),
    "host.exec": MutationDecision("privileged", "irreversible", "required", "optional", "optional", "unsupported", "unsupported", "never", False),
    "host.exec_many": MutationDecision("privileged", "irreversible", "required", "optional", "optional", "unsupported", "required", "never", False),
    "files.transfer": MutationDecision("destructive", "conditional", "required", "required", "optional", "required", "unsupported", "conditional", False),
}

LONG_READS = {
    "container.logs",
    "container.stats",
    "files.find",
    "files.compare",
    "host.doctor",
    "logs.syslog",
    "logs.journal",
    "logs.kernel",
    "logs.auth",
}


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def digest(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def target_kind(name: str) -> str | dict[str, str]:
    if name == "product.help":
        return {"custom": "product.surface"}
    if name == "fleet.nodes":
        return {"custom": "fleet.inventory"}
    if name.startswith("docker."):
        return "image" if name in {"docker.pull", "docker.build", "docker.rmi"} else "docker_daemon"
    if name.startswith("container."):
        return "docker_daemon" if name in {"container.list", "container.stats", "container.search"} else "container"
    if name.startswith("compose."):
        return "host" if name in {"compose.list", "compose.refresh"} else "compose_project"
    if name.startswith("host."):
        return "host"
    if name.startswith("files."):
        return "file"
    if name == "processes.list" or name == "filesystem.usage":
        return "host"
    if name.startswith("zfs."):
        return "host"
    if name.startswith("logs."):
        return "log_source"
    raise ValueError(f"no target-kind decision for {name}")


def evidence(name: str, access: str) -> list[str]:
    if name == "product.help":
        return ["configuration"]
    if name in {"docker.df", "container.stats", "host.resources", "filesystem.usage"}:
        return ["metrics", "runtime_state"]
    if name in {"container.logs", "logs.syslog", "logs.journal", "logs.kernel", "logs.auth"}:
        return ["logs"]
    if name in {"files.read", "files.find"}:
        return ["artifact"]
    if name == "files.compare":
        return ["diff"]
    if name in {"host.exec", "host.exec_many", "container.exec"}:
        return ["artifact", "logs"]
    if name in {"docker.build", "docker.pull", "container.pull", "compose.build", "compose.pull"}:
        return ["artifact", "logs", "runtime_state"]
    if name == "files.transfer":
        return ["artifact", "diff"]
    if access == "mutation":
        return ["diff", "runtime_state"]
    if name in {"docker.images", "docker.networks", "docker.volumes", "container.inspect", "compose.status", "compose.list", "compose.refresh", "host.mounts", "host.network", "host.ports", "zfs.pools", "zfs.datasets", "zfs.snapshots"}:
        return ["configuration", "runtime_state"]
    return ["runtime_state"]


def requirements(name: str, access: str) -> list[str]:
    if name == "product.help":
        return ["product.help"]
    if name.startswith(("docker.", "container.", "compose.")):
        requirements = ["runtime.docker"]
        if name in {"docker.build", "compose.build"}:
            requirements.append("filesystem.read")
        return requirements
    if name == "fleet.nodes":
        return ["fleet.repository"]
    if name.startswith("files."):
        if name == "files.transfer":
            return ["filesystem.read", "filesystem.write", "fleet.transfer"]
        return ["filesystem.read"]
    if name == "processes.list":
        return ["process.read"]
    if name == "filesystem.usage":
        return ["filesystem.read"]
    if name in {"host.exec", "host.exec_many"}:
        values = ["process.exec"]
        if name == "host.exec_many":
            values.append("fleet.fanout")
        return values
    if name.startswith("host."):
        return ["runtime.host"]
    if name.startswith("zfs."):
        return ["runtime.zfs"]
    if name.startswith("logs."):
        return ["logs.read"]
    raise ValueError(f"no capability requirements for {name}")


def schema_id(name: str, kind: str, version: int = 1) -> str:
    return f"schema.operations.{name}.{kind}.v{version}"


def diagnostic_codes(name: str, access: str, verification: str) -> list[str]:
    codes = {
        "backend.unavailable",
        "capability.unsupported",
        "internal.failure",
        "operation.cancelled",
        "operation.timeout",
        "request.invalid",
        "target.not_found",
    }
    if access == "mutation":
        codes.update(
            {
                "authorization.denied",
                "authorization.expired",
                "authorization.required",
                "mutation.uncertain",
                "plan.required",
                "plan.stale",
            }
        )
        if verification != "unsupported":
            codes.update({"verification.failed", "verification.inconclusive"})

    if name == "product.help":
        codes.add("product.unavailable")
    elif name.startswith(("docker.", "container.", "compose.")):
        codes.update({"docker.conflict", "docker.not_found", "docker.unavailable"})
    elif name == "fleet.nodes":
        codes.update({"fleet.empty", "fleet.unavailable"})
    elif name.startswith("files."):
        codes.update(
            {
                "filesystem.not_found",
                "filesystem.path_denied",
                "filesystem.too_large",
            }
        )
    elif name in {"host.exec", "host.exec_many"}:
        codes.update(
            {
                "command.failed",
                "command.rejected",
                "host.not_found",
                "host.unreachable",
                "output.truncated",
            }
        )
    elif name.startswith("host.") or name in {"processes.list", "filesystem.usage"}:
        codes.update({"host.not_found", "host.unreachable"})
    elif name.startswith("logs."):
        codes.update({"logs.truncated", "logs.unavailable"})
    elif name.startswith("zfs."):
        codes.update({"zfs.not_found", "zfs.unavailable"})
    return sorted(codes)


def valid_diagnostic_code(value: str) -> bool:
    segments = value.split(".")
    return len(segments) >= 2 and all(
        segment
        and segment[0].islower()
        and segment[0].isalpha()
        and all(character.islower() or character.isdigit() or character in "-_" for character in segment)
        and not segment.endswith(("-", "_"))
        for segment in segments
    )


def parameter_group(fields: list[str]) -> dict[str, list[str]]:
    return {"fields": sorted(fields)}


def read_lifecycle(name: str) -> tuple[str, str, str, str, str]:
    if name in LONG_READS:
        return "unsupported", "optional", "optional", "unsupported", "unsupported"
    return "unsupported", "unsupported", "unsupported", "unsupported", "unsupported"


def build_operation(legacy: dict[str, Any]) -> dict[str, Any]:
    name = str(legacy["canonical_name"])
    access = "mutation" if legacy["legacy_access"] == "write" else "read"
    required = list(legacy["required_params"])
    required_any = [list(group) for group in legacy["required_any"]]

    if access == "mutation":
        try:
            decision = MUTATIONS[name]
        except KeyError as exc:
            raise ValueError(f"missing explicit mutation decision for {name}") from exc
        planning = decision.planning
        progress = decision.progress
        cancellation = decision.cancellation
        verification = decision.verification
        fanout = decision.fanout
        risk = decision.risk
        reversibility = decision.reversibility
        retry = decision.retry
        idempotent = decision.idempotent
    else:
        planning, progress, cancellation, verification, fanout = read_lifecycle(name)
        risk = "safe"
        reversibility = "reversible"
        retry = "safe"
        idempotent = False

    return {
        "name": name,
        "schema_version": 1,
        "parameter_schema": schema_id(name, "parameters"),
        "result_schema": schema_id(name, "result"),
        "diagnostic_codes": diagnostic_codes(name, access, verification),
        "target_kind": target_kind(name),
        "access": access,
        "risk": risk,
        "reversibility": reversibility,
        "required": parameter_group(required),
        "required_any": [parameter_group(group) for group in required_any],
        "planning": planning,
        "progress": progress,
        "cancellation": cancellation,
        "verification": verification,
        "fanout": fanout,
        "retry": retry,
        "idempotent": idempotent,
        "evidence": sorted(evidence(name, access)),
        "requirements": sorted(requirements(name, access)),
    }


def validate(bundle: dict[str, Any], legacy: dict[str, Any]) -> None:
    operations = bundle.get("operations")
    if not isinstance(operations, list) or len(operations) != EXPECTED_COUNT:
        raise ValueError(f"expected {EXPECTED_COUNT} canonical operations")
    if bundle.get("operation_count") != len(operations):
        raise ValueError("operation_count does not match operations length")
    if bundle.get("legacy_semantic_sha256") != legacy.get("semantic_sha256"):
        raise ValueError("legacy semantic digest does not match pinned fixture")
    if bundle.get("classification_sha256") != digest(operations):
        raise ValueError("classification digest is stale")

    legacy_by_name = {item["canonical_name"]: item for item in legacy["operations"]}
    canonical_names = [item.get("name") for item in operations]
    if len(set(canonical_names)) != len(canonical_names):
        raise ValueError("canonical operation names are not unique")
    if set(canonical_names) != set(legacy_by_name):
        missing = sorted(set(legacy_by_name) - set(canonical_names))
        extra = sorted(set(canonical_names) - set(legacy_by_name))
        raise ValueError(f"canonical coverage mismatch: missing={missing}, extra={extra}")

    mutation_names = {name for name, item in legacy_by_name.items() if item["legacy_access"] == "write"}
    if mutation_names != set(MUTATIONS):
        missing = sorted(mutation_names - set(MUTATIONS))
        extra = sorted(set(MUTATIONS) - mutation_names)
        raise ValueError(f"mutation decision mismatch: missing={missing}, extra={extra}")

    for item in operations:
        name = item["name"]
        donor = legacy_by_name[name]
        expected_access = "mutation" if donor["legacy_access"] == "write" else "read"
        if item["access"] != expected_access:
            raise ValueError(f"access mismatch for {name}")
        if item["required"]["fields"] != sorted(donor["required_params"]):
            raise ValueError(f"required parameter drift for {name}")
        expected_any = sorted(sorted(group) for group in donor["required_any"])
        actual_any = sorted(group["fields"] for group in item["required_any"])
        if actual_any != expected_any:
            raise ValueError(f"alternative parameter drift for {name}")
        if item["access"] == "read":
            if item["risk"] != "safe" or item["idempotent"]:
                raise ValueError(f"invalid read classification for {name}")
        else:
            if item["risk"] in {"destructive", "privileged"} and item["planning"] == "unsupported":
                raise ValueError(f"risky mutation lacks planning for {name}")
            if item["retry"] == "safe" and not item["idempotent"]:
                raise ValueError(f"safe retry lacks idempotency for {name}")
        expected_parameters = schema_id(name, "parameters", item["schema_version"])
        expected_result = schema_id(name, "result", item["schema_version"])
        if item.get("parameter_schema") != expected_parameters:
            raise ValueError(f"parameter schema identity drift for {name}")
        if item.get("result_schema") != expected_result:
            raise ValueError(f"result schema identity drift for {name}")
        codes = item.get("diagnostic_codes")
        if not isinstance(codes, list) or not codes or codes != sorted(set(codes)):
            raise ValueError(f"invalid diagnostic code set for {name}")
        if not all(valid_diagnostic_code(code) for code in codes):
            raise ValueError(f"invalid diagnostic code for {name}")
        if not item["evidence"] or not item["requirements"]:
            raise ValueError(f"missing evidence or requirements for {name}")


def build_bundle(legacy: dict[str, Any]) -> dict[str, Any]:
    operations = [build_operation(item) for item in legacy["operations"]]
    bundle = {
        "format_version": 1,
        "legacy_semantic_sha256": legacy["semantic_sha256"],
        "operation_count": len(operations),
        "operations": operations,
    }
    bundle["classification_sha256"] = digest(operations)
    validate(bundle, legacy)
    return bundle


def load(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("generate", "check"))
    parser.add_argument("--legacy-fixture", type=Path, default=LEGACY_FIXTURE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args()

    try:
        legacy = load(args.legacy_fixture)
        generated = build_bundle(legacy)
        if args.action == "generate":
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(
                json.dumps(generated, indent=2, ensure_ascii=True) + "\n",
                encoding="utf-8",
            )
            print(f"wrote {args.output} with {EXPECTED_COUNT} classifications ({generated['classification_sha256'][:12]})")
            return 0
        committed = load(args.output)
        validate(committed, legacy)
        if committed != generated:
            raise ValueError("committed canonical classification fixture is stale")
        print(f"canonical classifications are valid ({EXPECTED_COUNT} operations, {committed['classification_sha256'][:12]})")
        return 0
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
