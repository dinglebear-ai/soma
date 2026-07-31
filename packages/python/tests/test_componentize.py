import textwrap
import unittest

from soma_provider import scan_componentize_compatibility


class ComponentizeCompatibilityTests(unittest.TestCase):
    def test_dependency_free_provider_is_eligible_for_build_validation(self):
        report = scan_componentize_compatibility(
            textwrap.dedent(
                """
                import json
                import math

                def run(value: int):
                    return json.dumps(math.sqrt(value))
                """
            ),
            filename="provider.py",
        )

        self.assertTrue(report["experimental"])
        self.assertTrue(report["compatible"])
        self.assertTrue(report["requires_build_validation"])
        self.assertEqual(report["imports"], ["json", "math"])
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

    def test_external_imports_require_wheel_evidence(self):
        missing = scan_componentize_compatibility("import requests")
        self.assertFalse(missing["compatible"])
        self.assertIn(
            "dependency_wheels_unscanned",
            {finding["code"] for finding in missing["findings"]},
        )

        pure = scan_componentize_compatibility(
            "import requests",
            wheel_files=["requests-2.32.0-py3-none-any.whl"],
        )
        self.assertTrue(pure["compatible"])
        self.assertIn(
            "dependency_mapping_unverified",
            {finding["code"] for finding in pure["findings"]},
        )

    def test_native_wheels_and_extensions_are_rejected(self):
        for artifact in [
            "orjson-3.10.0-cp312-abi3-manylinux_2_17_x86_64.whl",
            "extension.so",
        ]:
            with self.subTest(artifact=artifact):
                report = scan_componentize_compatibility(
                    "import orjson", wheel_files=[artifact]
                )
                self.assertFalse(report["compatible"])

    def test_syntax_errors_are_actionable(self):
        report = scan_componentize_compatibility("def broken(:", filename="broken.py")
        self.assertFalse(report["compatible"])
        self.assertEqual(report["findings"][0]["code"], "python_syntax_error")
        self.assertEqual(report["findings"][0]["subject"], "broken.py")


if __name__ == "__main__":
    unittest.main()
