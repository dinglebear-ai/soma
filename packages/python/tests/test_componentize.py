import base64
import csv
import hashlib
import io
import tempfile
import textwrap
import unittest
import warnings
import zipfile
from pathlib import Path
from unittest.mock import patch

import soma_provider._componentize as componentize
from soma_provider import scan_componentize_compatibility


def _wheel(
    root: Path,
    *,
    distribution: str,
    version: str,
    modules: tuple[str, ...],
    native_entry: bool = False,
    corrupt_record: bool = False,
) -> Path:
    normalized = distribution.replace("-", "_")
    filename = root / f"{normalized}-{version}-py3-none-any.whl"
    dist_info = f"{normalized}-{version}.dist-info"
    newline = chr(10)
    files: dict[str, bytes] = {
        f"{dist_info}/METADATA": (
            f"Metadata-Version: 2.1{newline}Name: {distribution}{newline}"
            f"Version: {version}{newline}"
        ).encode(),
        f"{dist_info}/WHEEL": (
            f"Wheel-Version: 1.0{newline}Root-Is-Purelib: true{newline}"
            f"Tag: py3-none-any{newline}"
        ).encode(),
        f"{dist_info}/top_level.txt": (newline.join(modules) + newline).encode(),
    }
    for module in modules:
        files[f"{module}/__init__.py"] = f"NAME = {module!r}{newline}".encode()
    if native_entry:
        files[f"{modules[0]}/extension.so"] = b"native"

    record = io.StringIO()
    writer = csv.writer(record, lineterminator=newline)
    for name, body in sorted(files.items()):
        digest = base64.urlsafe_b64encode(hashlib.sha256(body).digest()).decode().rstrip("=")
        writer.writerow([name, f"sha256={digest}", str(len(body))])
    record_name = f"{dist_info}/RECORD"
    writer.writerow([record_name, "", ""])
    files[record_name] = record.getvalue().encode()
    if corrupt_record:
        files[f"{modules[0]}/__init__.py"] += b"changed"

    with zipfile.ZipFile(filename, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, body in sorted(files.items()):
            archive.writestr(name, body)
    return filename


class ComponentizeCompatibilityTests(unittest.TestCase):
    def test_dependency_free_provider_is_eligible_for_build_validation(self):
        source = textwrap.dedent(
            """
            import json
            import math

            def run(value: int):
                return json.dumps(math.sqrt(value))
            """
        )
        report = scan_componentize_compatibility(source, filename="provider.py")

        self.assertEqual(report["schema_version"], 2)
        self.assertEqual(report["policy_version"], "soma-componentize-v1")
        self.assertTrue(report["experimental"])
        self.assertTrue(report["compatible"])
        self.assertTrue(report["requires_build_validation"])
        self.assertEqual(report["imports"], ["json", "math"])
        self.assertEqual(report["source_sha256"], hashlib.sha256(source.encode()).hexdigest())
        self.assertEqual(report["findings"], [])

    def test_ambient_authority_assumptions_fail_closed(self):
        report = scan_componentize_compatibility(
            textwrap.dedent(
                """
                import socket
                import threading

                def run():
                    open("/tmp/value")
                    return socket.socket()
                """
            )
        )

        self.assertFalse(report["compatible"])
        codes = {finding["code"] for finding in report["findings"]}
        self.assertIn("socket_assumption", codes)
        self.assertIn("threading_assumption", codes)
        self.assertIn("filesystem_assumption", codes)

    def test_async_functions_fail_closed(self):
        report = scan_componentize_compatibility(
            "async def run(value):\n    return value\n",
            filename="async_provider.py",
        )
        self.assertFalse(report["compatible"])
        self.assertIn(
            "async_runtime_assumption",
            {finding["code"] for finding in report["findings"]},
        )

    def test_external_imports_require_authenticated_wheel_evidence(self):
        missing = scan_componentize_compatibility("import requests")
        self.assertFalse(missing["compatible"])
        self.assertIn(
            "dependency_wheels_unscanned",
            {finding["code"] for finding in missing["findings"]},
        )

        with tempfile.TemporaryDirectory() as directory:
            wheel = _wheel(
                Path(directory),
                distribution="requests",
                version="2.32.0",
                modules=("requests",),
            )
            report = scan_componentize_compatibility(
                "import requests", wheel_files=[str(wheel)]
            )

        self.assertTrue(report["compatible"])
        self.assertEqual(report["import_distributions"], {"requests": "requests"})
        self.assertEqual(report["wheel_evidence"][0]["modules"], ["requests"])
        self.assertTrue(report["wheel_evidence"][0]["record_verified"])
        self.assertEqual(report["findings"], [])

    def test_missing_and_ambiguous_distribution_mapping_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = _wheel(
                root,
                distribution="first-provider",
                version="1.0.0",
                modules=("shared_module",),
            )
            second = _wheel(
                root,
                distribution="second-provider",
                version="1.0.0",
                modules=("shared_module",),
            )
            ambiguous = scan_componentize_compatibility(
                "import shared_module", wheel_files=[str(first), str(second)]
            )
            absent = scan_componentize_compatibility(
                "import missing_module", wheel_files=[str(first)]
            )

        self.assertIn(
            "dependency_distribution_ambiguous",
            {finding["code"] for finding in ambiguous["findings"]},
        )
        self.assertIn(
            "dependency_distribution_missing",
            {finding["code"] for finding in absent["findings"]},
        )

    def test_native_wheels_extensions_and_corrupt_records_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            native = _wheel(
                root,
                distribution="native-package",
                version="1.0.0",
                modules=("native_package",),
                native_entry=True,
            )
            corrupt = _wheel(
                root,
                distribution="corrupt-package",
                version="1.0.0",
                modules=("corrupt_package",),
                corrupt_record=True,
            )
            native_report = scan_componentize_compatibility(
                "import native_package", wheel_files=[str(native)]
            )
            corrupt_report = scan_componentize_compatibility(
                "import corrupt_package", wheel_files=[str(corrupt)]
            )

        self.assertIn(
            "native_extension_entry",
            {finding["code"] for finding in native_report["findings"]},
        )
        self.assertIn(
            "dependency_wheel_record_mismatch",
            {finding["code"] for finding in corrupt_report["findings"]},
        )
        self.assertFalse(native_report["compatible"])
        self.assertFalse(corrupt_report["compatible"])

    def test_duplicate_and_overexpanded_wheels_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            duplicate = root / "duplicate-1.0.0-py3-none-any.whl"
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                with zipfile.ZipFile(duplicate, "w") as archive:
                    archive.writestr("duplicate/__init__.py", b"first")
                    archive.writestr("duplicate/__init__.py", b"second")
            duplicate_report = scan_componentize_compatibility(
                "import duplicate", wheel_files=[str(duplicate)]
            )

            expanded = _wheel(
                root,
                distribution="expanded-package",
                version="1.0.0",
                modules=("expanded_package",),
            )
            with (
                patch.object(componentize, "MAX_WHEEL_EXPANDED_BYTES", 32),
                patch.object(
                    zipfile.ZipFile,
                    "read",
                    side_effect=AssertionError("unsafe wheel content was decompressed"),
                ),
            ):
                expanded_report = componentize.scan_componentize_compatibility(
                    "import expanded_package", wheel_files=[str(expanded)]
                )

        self.assertIn(
            "dependency_wheel_duplicate_path",
            {finding["code"] for finding in duplicate_report["findings"]},
        )
        self.assertIn(
            "dependency_wheel_expanded_limit",
            {finding["code"] for finding in expanded_report["findings"]},
        )
        self.assertFalse(duplicate_report["compatible"])
        self.assertFalse(expanded_report["compatible"])
        self.assertEqual(duplicate_report["wheel_evidence"][0]["modules"], [])
        self.assertFalse(duplicate_report["wheel_evidence"][0]["record_verified"])
        self.assertEqual(expanded_report["wheel_evidence"][0]["modules"], [])
        self.assertFalse(expanded_report["wheel_evidence"][0]["record_verified"])

    def test_invalid_wheel_preserves_actionable_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            wheel = Path(directory) / "broken-1.0.0-py3-none-any.whl"
            wheel.write_bytes(b"not a zip archive")
            report = scan_componentize_compatibility(
                "import broken", wheel_files=[str(wheel)]
            )

        self.assertFalse(report["compatible"])
        self.assertIn(
            "dependency_wheel_invalid",
            {finding["code"] for finding in report["findings"]},
        )
        self.assertEqual(report["wheel_evidence"][0]["path"], str(wheel.resolve()))
        self.assertFalse(report["wheel_evidence"][0]["pure_python"])
        self.assertFalse(report["wheel_evidence"][0]["record_verified"])

    def test_unreadable_fake_wheel_names_are_not_evidence(self):
        report = scan_componentize_compatibility(
            "import requests",
            wheel_files=["requests-2.32.0-py3-none-any.whl"],
        )
        self.assertFalse(report["compatible"])
        self.assertIn(
            "dependency_wheel_unreadable",
            {finding["code"] for finding in report["findings"]},
        )

    def test_syntax_errors_are_actionable(self):
        report = scan_componentize_compatibility("def broken(:", filename="broken.py")
        self.assertFalse(report["compatible"])
        self.assertEqual(report["findings"][0]["code"], "python_syntax_error")
        self.assertEqual(report["findings"][0]["subject"], "broken.py")


if __name__ == "__main__":
    unittest.main()
