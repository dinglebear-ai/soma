from collections.abc import Iterable
from typing import Literal, TypedDict

class ComponentizeFinding(TypedDict):
    code: str
    severity: Literal["error", "warning"]
    message: str
    line: int | None
    subject: str | None
class ComponentizeWheelEvidence(TypedDict):
    path: str
    filename: str
    sha256: str
    distribution: str | None
    version: str | None
    modules: list[str]
    pure_python: bool
    record_verified: bool
    entries: int
class ComponentizeReport(TypedDict):
    schema_version: int
    policy_version: str
    componentize_py_version: str
    experimental: bool
    compatible: bool
    requires_build_validation: bool
    filename: str
    source_sha256: str
    imports: list[str]
    external_imports: list[str]
    import_distributions: dict[str, str]
    wheel_files: list[str]
    wheel_evidence: list[ComponentizeWheelEvidence]
    findings: list[ComponentizeFinding]
def scan_componentize_compatibility(source: str, *, filename: str = ..., wheel_files: Iterable[str] = ...) -> ComponentizeReport: ...
