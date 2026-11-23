import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
import sys
from functools import partial

from room_revision import (
    digest,
    import_room as import_survey,
    load_revision,
    sensor_settings,
)

EXAMPLE = (
    Path(__file__).resolve().parents[2]
    / "docs/product/aisle-studio/validation/room-survey/survey.json"
)
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "aisle-validation"))
MODEL_SHA = digest(
    Path(__file__).resolve().parents[2] / "data/adora_nano/adora_nano.urdf"
)
import_room = partial(import_survey, model_sha=MODEL_SHA)
SCENE = [dict(name="floor", size=[12, 10, 0.1], position=[0, 0, -0.0843])]


class RoomTests(unittest.TestCase):
    def setUp(self):
        temp = TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.saved = self.root / "first"
        self.saved.mkdir()
        self.revision = import_room(EXAMPLE, self.saved, SCENE)

    def test_survey_preserves_scale_and_edit_lineage(self):
        box = self.revision["obstacles"][0]
        self.assertAlmostEqual(box["size"][0], 0.40, places=3)
        self.assertAlmostEqual(box["position"][1], -0.45, places=3)
        parent = digest(self.saved / "room.json")
        edit = self.root / "second"
        edit.mkdir()
        result = import_room(
            self.saved / "survey.json", edit, SCENE, "ottoman 1.38 -0.45 0.35", parent
        )
        self.assertEqual(result["parent_sha256"], parent)
        self.assertEqual(result["obstacles"][0]["position"][:2], [1.38, -0.45])
        self.assertEqual(digest(self.saved / "room.json"), parent)
        self.assertEqual(
            load_revision(
                edit / "room.json", digest(edit / "room.json"), SCENE, MODEL_SHA
            )["obstacles"],
            result["obstacles"],
        )

    def test_modified_survey_cannot_change_a_saved_revision(self):
        parent = digest(self.saved / "room.json")
        (self.saved / "survey.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "changed"):
            load_revision(self.saved / "room.json", parent, SCENE)
        with self.assertRaisesRegex(ValueError, "selected hash"):
            load_revision(self.saved / "room.json", "a different hash", SCENE)

    def test_overlapping_and_non_metric_edits_are_rejected(self):
        for edit in ("ottoman 0 0 .3", "ottoman 8 0 .3", "ottoman 1 0 nan"):
            with self.subTest(edit), self.assertRaises(ValueError):
                import_room(EXAMPLE, self.root, SCENE, edit)
        sheet = json.loads(EXAMPLE.read_text())
        sheet.pop("camera_calibration")
        sheet["image"] = str(EXAMPLE.with_name("reference.png"))
        sheet["floor_marks"][2] = sheet["floor_marks"][1]
        survey = self.root / "bad.json"
        survey.write_text(json.dumps(sheet))
        with self.assertRaisesRegex(ValueError, "coordinates"):
            import_room(survey, self.root, SCENE)

    def test_calibration_must_match_model_and_realizable_optics(self):
        sheet = json.loads(EXAMPLE.read_text())
        with self.assertRaisesRegex(ValueError, "model hash"):
            sensor_settings(sheet, EXAMPLE, "wrong hash")
        calibration = json.loads(EXAMPLE.with_name("camera-fixture.json").read_text())
        sheet["camera_calibration"] = calibration
        calibration["cameras"]["head"]["intrinsics"][0][2] += 10
        with self.assertRaisesRegex(ValueError, "centered"):
            sensor_settings(sheet, EXAMPLE, MODEL_SHA)
