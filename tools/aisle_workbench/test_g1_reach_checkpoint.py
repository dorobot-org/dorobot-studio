import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from g1_reach_checkpoint import METHOD, load
from worker import digest, save


class CheckpointTests(unittest.TestCase):
    def setUp(self):
        scratch = TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.dir = Path(scratch.name)
        self.plan = dict(
            robot="G1",
            joint="right_shoulder_pitch_joint",
            delta_rad=-0.08,
            model_hash="model",
            parent_controller_sha256="teacher",
            training_dataset="dataset",
        )
        save(self.dir / "training.json", {"frames": 100})
        self.weights = dict(
            **self.plan,
            method=METHOD,
            dataset_sha256="dataset",
            weights=[0.01, 0.01, 0.02],
            training_sha256=digest(self.dir / "training.json")
        )
        self.store()

    def store(self):
        save(self.dir / "reach-checkpoint.json", self.weights)
        self.plan["reach_checkpoint"] = digest(self.dir / "reach-checkpoint.json")

    def test_same_revision_and_model(self):
        self.assertEqual(load(self.dir, self.plan).tolist(), self.weights["weights"])
        for change in (
            dict(delta_rad=0.08),
            dict(training_dataset="another"),
            dict(joint="left_shoulder_pitch_joint"),
        ):
            with self.subTest(change=change), self.assertRaisesRegex(
                ValueError, "does not match"
            ):
                load(self.dir, self.plan | change)

    def test_modified_training_and_weights(self):
        save(self.dir / "training.json", {"frames": 99})
        with self.assertRaisesRegex(ValueError, "does not match"):
            load(self.dir, self.plan)
        (self.dir / "reach-checkpoint.json").write_text(
            json.dumps(self.weights | {"weights": [1, 2, 3]})
        )
        with self.assertRaisesRegex(ValueError, "digest"):
            load(self.dir, self.plan)
