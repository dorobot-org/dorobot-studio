import json
from zipfile import ZipFile

import numpy as np
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

    def test_measured_camera_mounts_survive_moving_the_survey(self):
        with ZipFile(EXAMPLE.with_name("nano-camera-survey.zip")) as archive:
            archive.extractall(self.root / "example")
        example = self.root / "example"
        expected = json.loads((example / "fixture-truth.json").read_bytes())["cameras"]
        survey = example / "survey.json"
        fit = import_room(survey, self.saved, SCENE)
        for name, report in fit["settings"]["camera_mount_checks"].items():
            self.assertEqual((report["fit_points"], report["check_points"]), (9, 5))
            self.assertLess(report["max_check_px"], 1)
            actual = np.array(fit["settings"]["cameras"][name]["link_from_camera_gl"])
            truth = np.array(expected[name]["link_from_camera_gl"])
            self.assertLess(np.linalg.norm(actual[:3, 3] - truth[:3, 3]), 0.001)
            cosine = (np.trace(actual[:3, :3].T @ truth[:3, :3]) - 1) / 2
            self.assertLess(np.degrees(np.arccos(np.clip(cosine, -1, 1))), 0.3)
        example.rename(self.root / "moved-away")
        again = self.root / "edit"
        again.mkdir()
        updated = import_room(
            self.saved / "survey.json",
            again,
            SCENE,
            "ottoman 1.38 -0.45 0.35",
            digest(self.saved / "room.json"),
        )
        self.assertEqual(updated["settings"], fit["settings"])
        self.assertNotIn("images", fit["settings"])
        stored = json.loads((again / "survey.json").read_bytes())
        self.assertEqual(len(stored["camera_calibration"]["images"]), 42)
        rig = stored["camera_calibration"]
        ref = rig["cameras"]["head"]["landmarks"][0]["image"]
        rig["images"][ref] = rig["images"][
            rig["cameras"]["left_wrist"]["landmarks"][0]["image"]
        ]
        with self.assertRaisesRegex(ValueError, "digest differs"):
            sensor_settings(stored, again / "survey.json", MODEL_SHA)

    def test_held_out_landmarks_are_distinct_and_checked(self):
        from camera_survey import fit_mount

        with ZipFile(EXAMPLE.with_name("nano-camera-survey.zip")) as archive:
            row = json.loads(archive.read("captures/landmarks.json"))["cameras"][
                "left_wrist"
            ]
        check = next(p for p in row["landmarks"] if p["use"] == "check")
        check["pixel"][0] += 12
        with self.assertRaisesRegex(
            ValueError, "left_wrist check landmark reprojection"
        ):
            fit_mount("left_wrist", row)
        check["link_m"] = next(
            p["link_m"] for p in row["landmarks"] if p["use"] == "fit"
        )
        with self.assertRaisesRegex(ValueError, "distinct"):
            fit_mount("left_wrist", row)

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
