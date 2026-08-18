#!/usr/bin/env python3
"""Enforce cold, warm, reload, mixed-provider, and soak budgets."""

from __future__ import annotations

import argparse
import asyncio
import gc
import json
import subprocess
import sys
import tempfile
import time
import tomllib
import tracemalloc
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "packages/python/python"
POLICY = tomllib.loads((ROOT / "release/python-platform-policy.toml").read_text())
BUDGET = POLICY["performance"]

def milliseconds(seconds: float) -> float:
    return seconds * 1000.0

def require(label: str, actual: float, limit: float) -> None:
    if actual > limit:
        raise SystemExit(f"{label} budget exceeded: {actual:.3f}ms > {limit}ms")

def provider_source(name: str) -> str:
    return f'''from soma_provider import tool

@tool
def {name}(value: int) -> dict[str, int]:
    return {{"value": value + 1}}
'''

async def invoke_many(runtime, modules, iterations: int) -> float:
    started = time.perf_counter()
    for index in range(iterations):
        module = modules[index % len(modules)]
        kind, tool = runtime.resolve_tool(module, tool_name(module))
        result = await runtime.call_python(tool, {"value": index}, {"request_id": str(index)})
        if result != {"value": index + 1}:
            raise SystemExit("provider soak returned an incorrect result")
    return milliseconds(time.perf_counter() - started) / iterations

def tool_name(module) -> str:
    return next(name for name, value in vars(module).items() if callable(value) and getattr(value, "__soma_tool__", None) is not None)

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--full", action="store_true")
    args = parser.parse_args()
    iterations = int(BUDGET["soak_iterations"] if args.full else min(BUDGET["soak_iterations"], 250))

    probe = "import sys,time; sys.path.insert(0, sys.argv[1]); started=time.perf_counter(); import soma_provider; print((time.perf_counter()-started)*1000)"
    import_samples_ms = []
    for _ in range(int(BUDGET["sdk_import_trials"])):
        imported = subprocess.run(
            [sys.executable, "-I", "-c", probe, str(PACKAGE)],
            check=True,
            text=True,
            capture_output=True,
        )
        import_samples_ms.append(float(imported.stdout.strip()))
    # Each subprocess is a cold Python interpreter. Use the best isolated sample
    # as the intrinsic import-cost estimate so scheduler contention on shared
    # development/CI hosts does not turn this gate into a load detector.
    import_ms = min(import_samples_ms)
    require("sdk import", import_ms, float(BUDGET["sdk_import_ms"]))

    sys.path.insert(0, str(PACKAGE))
    import soma_provider._runtime as runtime

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        paths = []
        for name in ("alpha", "beta", "gamma"):
            path = root / f"{name}.py"
            path.write_text(provider_source(name))
            paths.append(path)

        cold_started = time.perf_counter()
        modules = [runtime.load_module(path) for path in paths]
        for path, module in zip(paths, modules, strict=True):
            runtime.catalog(path, module)
        cold_ms = milliseconds(time.perf_counter() - cold_started)
        require("catalog cold", cold_ms, float(BUDGET["catalog_cold_ms"]))

        warm_iterations = 100 if args.full else 25
        warm_started = time.perf_counter()
        for _ in range(warm_iterations):
            for path, module in zip(paths, modules, strict=True):
                runtime.catalog(path, module)
        warm_ms = milliseconds(time.perf_counter() - warm_started) / (warm_iterations * len(paths))
        require("catalog warm average", warm_ms, float(BUDGET["catalog_warm_average_ms"]))

        invoke_ms = asyncio.run(invoke_many(runtime, modules, iterations))
        require("invocation warm average", invoke_ms, float(BUDGET["invocation_warm_average_ms"]))

        reload_iterations = 50 if args.full else 10
        reload_started = time.perf_counter()
        for index in range(reload_iterations):
            path = paths[index % len(paths)]
            runtime.load_module(path)
        reload_ms = milliseconds(time.perf_counter() - reload_started) / reload_iterations
        require("reload average", reload_ms, float(BUDGET["reload_average_ms"]))

        gc.collect()
        tracemalloc.start()
        before = tracemalloc.take_snapshot()
        asyncio.run(invoke_many(runtime, modules, iterations))
        gc.collect()
        after = tracemalloc.take_snapshot()
        growth = sum(stat.size_diff for stat in after.compare_to(before, "filename") if stat.size_diff > 0)
        tracemalloc.stop()
        if growth > int(BUDGET["soak_memory_bytes"]):
            raise SystemExit(
                f"soak memory budget exceeded: {growth} > {BUDGET['soak_memory_bytes']}"
            )

    print(json.dumps({"sdk_import_ms": import_ms, "sdk_import_samples_ms": import_samples_ms, "catalog_cold_ms": cold_ms, "catalog_warm_average_ms": warm_ms, "invocation_warm_average_ms": invoke_ms, "reload_average_ms": reload_ms, "soak_iterations": iterations, "soak_growth_bytes": growth}, sort_keys=True))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
