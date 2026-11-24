from pathlib import Path
import json
import unittest
import numpy as np
from PIL import Image

from photo_room import camera, proposals


class PhotoRoomTests(unittest.TestCase):
    def test_real_photograph_recovers_metric_footprint_and_height(self):
        path = (
            Path(__file__).resolve().parents[2]
            / "docs/product/aisle-studio/validation/photo-room/photo-input.json"
        )
        sheet = json.loads(path.read_bytes())
        self.assertNotIn("obstacles", sheet)
        with Image.open(path.parent / sheet["image"]) as image:
            rgb = np.asarray(image.convert("RGB"))
        result = proposals(rgb, sheet)
        self.assertEqual(len(result["objects"]), 1)
        obj = result["objects"][0]
        np.testing.assert_allclose(
            obj["center_xy_m"], [1.28, -0.45], rtol=0, atol=0.025
        )
        np.testing.assert_allclose(obj["size_m"], [0.4, 0.31, 0.33], rtol=0, atol=0.025)
        self.assertLess(obj["max_vertex_error_px"], 1)
        self.assertEqual(proposals(np.zeros_like(rgb), sheet)["objects"], [])

    def test_height_requires_a_resolved_camera(self):
        marks = [
            dict(metres=[x, y], pixel=[200 * x + 10, 200 * y + 15])
            for x, y in [(0, 0), (1, 0), (0, 1), (1, 1), (0.5, 0.5)]
        ]
        with self.assertRaisesRegex(ValueError, "focal"):
            camera(marks, 800, 520)
