"""Check constraints at the boundary between agent output and the simulator."""

import json
from pathlib import Path
import tempfile
import unittest

import worker


class InspectionTests(unittest.TestCase):
    def setUp(self):
        self.profile = {
            "robot": "Nano",
            "model_hash": "model",
            "allowed_joints": ["shoulder"],
            "limits": {"shoulder": [-0.08, 0.14]},
        }

    def test_joint_and_displacement_guard(self):
        for joint, delta in [
            ("other", 0.05),
            ("shoulder", 0.0),
            ("shoulder", True),
            ("shoulder", float("nan")),
            ("shoulder", -0.09),
            ("shoulder", 0.16),
        ]:
            with self.subTest(joint=joint, delta=delta), self.assertRaises(ValueError):
                worker.validate(dict(joint=joint, delta_rad=delta), self.profile)
        self.assertEqual(
            worker.validate(dict(joint="shoulder", delta_rad=-0.06), self.profile)[
                "delta_rad"
            ],
            -0.06,
        )

    def test_feedback_is_bound_to_retained_controller_and_robot(self):
        with tempfile.TemporaryDirectory() as root:
            run = Path(root)
            controller = run / "controller.json"
            worker.save(controller, {"joint": "shoulder", "delta_rad": 0.05})
            result = dict(
                robot="Nano",
                model_hash="model",
                controller_hash=worker.digest(controller),
                joint="shoulder",
                requested_delta=0.05,
                measured_delta=0.049,
                within_limits=True,
                task_success=True,
                fixture="standard",
                extra_field="private",
            )
            worker.save(run / "result.json", result)
            public = worker.measured_feedback(run, self.profile)
            self.assertNotIn("extra_field", public)
            self.assertEqual(public["measured_delta"], 0.049)
            with self.assertRaises(ValueError):
                worker.measured_feedback(run, self.profile | {"robot": "G1"})
            controller.write_text(json.dumps({"joint": "other"}))
            with self.assertRaises(ValueError):
                worker.measured_feedback(run, self.profile)
