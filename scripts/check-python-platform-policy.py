#!/usr/bin/env python3
"""Validate the Python platform release, dependency, and runtime policy."""

from __future__ import annotations

import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "release/python-platform-policy.toml"

def fail(message: str) -> None:
    raise SystemExit(f"python platform policy: {message}")

def require_text(path: Path, fragments: list[str]) -> None:
    text = path.read_text(encoding="utf-8")
    for fragment in fragments:
        if fragment not in text:
            fail(f"{path.relative_to(ROOT)} is missing {fragment!r}")

def main() -> int:
    policy = tomllib.loads(POLICY_PATH.read_text(encoding="utf-8"))
    if policy.get("schema_version") != 1:
        fail("schema_version must be 1")
    for key in ("trusted_publishing", "require_sha256", "require_provenance", "require_sbom"):
        if policy.get(key) is not True:
            fail(f"{key} must remain enabled")
    for key in ("allow_source_distributions", "allow_git_dependencies", "allow_url_dependencies", "allow_local_path_dependencies"):
        if policy.get(key) is not False:
            fail(f"{key} must remain disabled")
    componentize_version = str(policy["componentize_py_version"])
    wasmtime_version = str(policy["wasmtime_version"])
    python_requires = str(policy["python_requires"])
    require_text(ROOT / "packages/python/python/soma_provider/_componentize.py", [f'COMPONENTIZE_PY_VERSION = "{componentize_version}"'])
    require_text(ROOT / "crates/soma/application/src/componentize.rs", [f'const COMPONENTIZE_PY_VERSION: &str = "{componentize_version}";'])
    require_text(
        ROOT / "crates/shared/provider-adapters/Cargo.toml",
        [
            f'wasmtime = {{ version = "{wasmtime_version}"',
            f'wasmtime-wasi = {{ version = "{wasmtime_version}"',
            'features = ["anyhow", "cache", "component-model"',
        ],
    )
    componentize = policy.get("componentize", {})
    build_timeout = componentize.get("build_timeout_seconds")
    verify_timeout = componentize.get("verify_timeout_seconds")
    min_invocation_timeout = componentize.get("min_invocation_timeout_ms")
    min_fuel = componentize.get("min_fuel")
    min_memory = componentize.get("min_memory_bytes")
    min_tables = componentize.get("min_table_elements")
    min_instances = componentize.get("min_instances")
    if build_timeout != 120 or verify_timeout != 600:
        fail("componentize build and verification limits must remain 120s and 600s")
    if (min_invocation_timeout, min_fuel) != (30_000, 10_000_000):
        fail("componentize invocation minimums must remain 30 seconds and 10000000 fuel")
    if (min_memory, min_tables, min_instances) != (67_108_864, 10_000, 64):
        fail("componentize runtime minimums must remain 64 MiB, 10000 tables, and 64 instances")
    componentize_build = ROOT / "crates/soma/application/src/componentize/build.rs"
    require_text(
        componentize_build,
        [
            f"const BUILD_TIMEOUT_SECS: u64 = {build_timeout};",
            f"const VERIFY_TIMEOUT_SECS: u64 = {verify_timeout};",
        ],
    )
    if "--stub-wasi" in componentize_build.read_text(encoding="utf-8"):
        fail("componentize builds must import host-provided WASI instead of trapping stubs")
    wasmtime_cache = policy.get("wasmtime_cache", {})
    cache_file_limit = wasmtime_cache.get("file_count_soft_limit")
    cache_size_limit = wasmtime_cache.get("files_total_size_soft_limit")
    if cache_file_limit != 256 or cache_size_limit != 2_147_483_648:
        fail("Wasmtime cache limits must remain 256 files and 2 GiB")
    require_text(
        ROOT / "crates/shared/provider-adapters/src/wasm_limits.rs",
        [
            f"const COMPONENTIZE_MIN_TIMEOUT_MS: u64 = {min_invocation_timeout:_};",
            f"const COMPONENTIZE_MIN_FUEL: u64 = {min_fuel:_};",
            f"const COMPONENTIZE_MIN_MEMORY_BYTES: usize = {min_memory // (1024 * 1024)} * 1024 * 1024;",
            f"const COMPONENTIZE_MIN_TABLE_ELEMENTS: usize = {min_tables:_};",
            f"const COMPONENTIZE_MIN_INSTANCES: usize = {min_instances};",
            "with_componentize_minimums",
        ],
    )
    require_text(
        ROOT / "crates/shared/provider-adapters/src/wasm.rs",
        [
            f"const WASMTIME_CACHE_FILE_COUNT_SOFT_LIMIT: u64 = {cache_file_limit};",
            f"const WASMTIME_CACHE_BYTES_SOFT_LIMIT: u64 = {cache_size_limit:_};",
            f"const COMPONENTIZE_ARTIFACT_COMPILE_TIMEOUT_SECS: u64 = {verify_timeout};",
            "const DEFAULT_ARTIFACT_COMPILE_TIMEOUT_SECS: u64 = 30;",
            'const COMPONENTIZE_MARKER_NAME: &[u8] = b"soma.componentize-py.v1";',
            "const VERIFY_MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;",
            "const VERIFY_MAX_TABLE_ELEMENTS: usize = 10_000;",
            "config.cache(Some(wasmtime_cache()?));",
            "artifact_compile_timeout(&bytes)",
        ],
    )
    project = tomllib.loads((ROOT / "packages/python/pyproject.toml").read_text(encoding="utf-8"))
    if project["project"].get("requires-python") != python_requires:
        fail("pyproject Python requirement differs from policy")
    if project["project"].get("dependencies") != []:
        fail("runtime Python dependencies must remain empty")
    if project.get("dependency-groups", {}).get("dev") != ["maturin==1.14.1"]:
        fail("development dependencies must remain exactly pinned")
    lock = (ROOT / "packages/python/uv.lock").read_text(encoding="utf-8")
    if 'source = { registry = "https://pypi.org/simple" }' not in lock:
        fail("uv lockfile must use the permitted PyPI registry")
    for blocked in ("git+", "source = { git", "source = { url", "source = { path"):
        if blocked in lock:
            fail(f"uv lockfile contains disallowed source {blocked!r}")
    editable = re.findall(r'source = \{ editable = "([^"]+)" \}', lock)
    if editable not in ([], ["."]):
        fail("only the package itself may be editable")
    provenance = "actions/attest-build-provenance@e3fe62ef559997059fe8380e7d2b4c909e2d65f4"
    sbom = "actions/attest-sbom@5729fe4dc697fb7538e4e94fd44d040aac1367b2"
    require_text(ROOT / ".github/workflows/python-wheels.yml", ["pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33", provenance, sbom, "environment: pypi", "attestations: true", "soma-provider.cdx.json", "SHA256SUMS"])
    require_text(ROOT / ".github/workflows/release.yml", [provenance, sbom, "soma-provider-v${python_version}", "soma-release.cdx.json", "SHA256SUMS"])
    require_text(
        ROOT / ".github/workflows/python-platform-soak.yml",
        [
            "cold and restarted-warm",
            "XDG_CACHE_HOME",
            "exact_componentize_build_validates_under_soma_wasmtime",
        ],
    )
    performance = policy.get("performance", {})
    for key in ("sdk_import_trials", "sdk_import_ms", "catalog_cold_ms", "catalog_warm_average_ms", "invocation_warm_average_ms", "reload_average_ms", "soak_iterations", "soak_memory_bytes"):
        value = performance.get(key)
        if not isinstance(value, int) or value <= 0:
            fail(f"performance.{key} must be positive")
    print("python platform policy passed")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
