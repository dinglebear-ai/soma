import json
import math
import struct
import unittest
from pathlib import Path

from soma_runner_protocol import (
    MAX_FRAME_BYTES,
    ProtocolError,
    decode_frame,
    encode_frame,
    negotiate_features,
    negotiate_version,
)


FIXTURES = json.loads(
    Path(__file__).with_name("runner_protocol_v1.json").read_text(encoding="utf-8")
)


class RunnerProtocolTests(unittest.TestCase):
    def test_shared_golden_messages_round_trip(self):
        for name, message in FIXTURES.items():
            with self.subTest(name=name):
                frame = encode_frame(message)
                self.assertEqual(struct.unpack(">I", frame[:4])[0], len(frame) - 4)
                self.assertEqual(decode_frame(frame), message)

    def test_unicode_is_utf8_and_non_finite_values_are_rejected(self):
        # Escaped rather than literal so this file stays ASCII (the repo's
        # asciicheck gate covers *.py); the runtime strings are identical.
        greeting = "\u3053\u3093\u306b\u3061\u306f"  # "konnichiwa" in hiragana
        message = {"status": "ok", "result": {"message": f"{greeting} \U0001f44b"}}
        frame = encode_frame(message)
        self.assertIn(greeting.encode(), frame)
        self.assertEqual(decode_frame(frame), message)

        for value in (math.nan, math.inf, -math.inf):
            with self.subTest(value=value), self.assertRaisesRegex(
                ProtocolError, "invalid Python runner JSON"
            ):
                encode_frame({"value": value})

    def test_version_and_feature_negotiation(self):
        self.assertEqual(
            negotiate_version({"major": 1, "minor": 2}, {"major": 1, "minor": 4}),
            {"major": 1, "minor": 2},
        )
        with self.assertRaisesRegex(ProtocolError, "major mismatch"):
            negotiate_version({"major": 1, "minor": 0}, {"major": 2, "minor": 0})
        self.assertEqual(
            negotiate_features(
                ["health", "cancel", "invoke"], ["describe", "invoke", "health"]
            ),
            ["health", "invoke"],
        )

    def test_rejects_incomplete_oversized_and_malformed_frames(self):
        for length in range(4):
            with self.subTest(length=length), self.assertRaisesRegex(
                ProtocolError, "header is incomplete"
            ):
                decode_frame(b"\0" * length)

        with self.assertRaisesRegex(ProtocolError, "limit"):
            decode_frame(struct.pack(">I", MAX_FRAME_BYTES + 1))
        with self.assertRaisesRegex(ProtocolError, "length mismatch"):
            decode_frame(struct.pack(">I", 5) + b"{}")
        with self.assertRaisesRegex(ProtocolError, "length mismatch"):
            decode_frame(struct.pack(">I", 2) + b"{}x")
        with self.assertRaisesRegex(ProtocolError, "invalid Python runner JSON"):
            decode_frame(struct.pack(">I", 1) + b"{")


if __name__ == "__main__":
    unittest.main()
