import importlib.util
import tempfile
import unittest
import zipfile
from pathlib import Path
from uuid import UUID


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts/generate-release-sbom.py"
SPEC = importlib.util.spec_from_file_location("generate_release_sbom", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
SBOM = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SBOM)


class ReleaseSbomTests(unittest.TestCase):
    def test_render_is_deterministic_and_uses_valid_purls_and_uuid(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            wheel = root / "nested" / "Example_Package-1.2.3-py3-none-any.whl"
            wheel.parent.mkdir()
            with zipfile.ZipFile(wheel, "w") as archive:
                archive.writestr(
                    "Example_Package-1.2.3.dist-info/METADATA",
                    "Metadata-Version: 2.3\nName: Example_Package\nVersion: 1.2.3\n",
                )
            binary = root / "soma"
            binary.write_bytes(b"binary")

            first = SBOM.render([wheel, binary], root)
            second = SBOM.render([binary, wheel], root)

        self.assertEqual(first, second)
        serial = UUID(first["serialNumber"].removeprefix("urn:uuid:"))
        self.assertEqual(serial.version, 5)
        components = {component["name"]: component for component in first["components"]}
        wheel_component = components["Example_Package"]
        self.assertEqual(wheel_component["purl"], "pkg:pypi/example-package@1.2.3")
        properties = {
            item["name"]: item["value"] for item in wheel_component["properties"]
        }
        self.assertEqual(
            properties["soma:artifact:path"],
            "nested/Example_Package-1.2.3-py3-none-any.whl",
        )

    def test_wheel_metadata_requires_name_and_version(self):
        with tempfile.TemporaryDirectory() as directory:
            wheel = Path(directory) / "broken-1.0-py3-none-any.whl"
            with zipfile.ZipFile(wheel, "w") as archive:
                archive.writestr(
                    "broken-1.0.dist-info/METADATA",
                    "Metadata-Version: 2.3\nName: broken\n",
                )
            with self.assertRaisesRegex(ValueError, "Name and Version"):
                SBOM.wheel_metadata(wheel)


if __name__ == "__main__":
    unittest.main()
