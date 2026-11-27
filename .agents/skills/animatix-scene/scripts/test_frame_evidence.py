import argparse
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zlib

from PIL import Image

import frame_evidence as evidence


class FrameEvidenceTest(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        self.args = argparse.Namespace(
            expectations=root / "plan.json", source=root / "scene.amx",
            output=root / "proof", runner=root / "runner", input=[],
        )
        self.args.source.write_text("#0s\n")
        self.args.runner.touch()
        evidence.write(self.args.expectations, {
            "fps": "24", "checks": [{"frames": "0", "expect": "visible dot"}],
        })
        buffer = io.BytesIO()
        Image.new("RGBA", (2, 2), "green").save(buffer, format="PNG")
        self.png = buffer.getvalue()

    def capture_bytes(self, data):
        def run_renderer(*args, **kwargs):
            (self.args.output / "f000.png").write_bytes(data)
            evidence.write(self.args.output / "samples.json", {"times": [0.0], "result": {"ok": True}})

        with patch.object(evidence.subprocess, "run", side_effect=run_renderer):
            evidence.capture(self.args)

    def test_valid_png_round_trip(self):
        self.capture_bytes(self.png)
        evidence.verify(self.args)

    def test_capture_rejects_truncated_png_before_issuing_receipt(self):
        self.capture_bytes_expect_failure(self.png[:8])

    def test_capture_rejects_missing_end_crc(self):
        self.capture_bytes_expect_failure(self.png[:-4])

    def test_capture_rejects_undecodable_pixels_with_valid_chunk_crc(self):
        # Preserve IHDR and IEND. The IDAT chunk has a valid checksum but
        # contains an unrecognized PNG scanline filter.
        payload = b"IDAT" + zlib.compress(b"\xff" + bytes(8) + b"\x00" + bytes(8))
        chunk = (len(payload) - 4).to_bytes(4, "big") + payload + zlib.crc32(payload).to_bytes(4, "big")
        self.capture_bytes_expect_failure(self.png[:33] + chunk + self.png[-12:])

    def capture_bytes_expect_failure(self, data):
        with self.assertRaisesRegex(ValueError, "Invalid PNG f000.png"):
            self.capture_bytes(data)
        self.assertFalse((self.args.output / "receipt.json").exists())

    def test_verify_rejects_old_receipt_for_matching_broken_png(self):
        self.capture_bytes(self.png)
        path = self.args.output / "frame-000000.png"
        path.write_bytes(self.png[:8])
        receipt_path = self.args.output / "receipt.json"
        receipt = evidence.read(receipt_path)
        receipt["samples"][0]["sha256"] = evidence.sha(path)
        evidence.write(receipt_path, receipt)
        with self.assertRaisesRegex(ValueError, "Invalid PNG frame-000000.png"):
            evidence.verify(self.args)


if __name__ == "__main__":
    unittest.main()
