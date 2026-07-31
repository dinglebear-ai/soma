import importlib.util
import json
from pathlib import Path
import unittest


class ComponentConformanceTests(unittest.TestCase):
    def test_reference_python_matches_shared_component_fixtures(self) -> None:
        root = Path(__file__).resolve().parents[3]
        examples = root / "examples" / "providers" / "components"
        spec = importlib.util.spec_from_file_location(
            "reference_conformance_python",
            examples / "reference-python.py",
        )
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        fixtures = json.loads(
            (examples / "conformance-v1.json").read_text(encoding="utf-8")
        )
        self.assertGreater(len(fixtures), 0)
        for fixture in fixtures:
            with self.subTest(fixture=fixture["name"]):
                arguments = fixture["input"]["arguments"]
                self.assertEqual(
                    module.conformance_echo(**arguments),
                    fixture["expected"],
                )


if __name__ == "__main__":
    unittest.main()
