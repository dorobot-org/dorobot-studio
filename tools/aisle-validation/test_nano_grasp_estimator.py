import hashlib
import unittest

import numpy as np

from nano_grasp_estimator import FEATURES, METHOD, checkpoint_bytes, parameters, predict


class NanoGraspTests(unittest.TestCase):
    def test_checkpoint_is_bound_to_its_coefficients_and_training_dataset(self):
        count = len(FEATURES)
        checkpoint = dict(
            method=METHOD,
            features=list(FEATURES),
            model_hash="1" * 64,
            teacher_controller_sha256="2" * 64,
            dataset_sha256="3" * 64,
            training_sha256="4" * 64,
            mean=[0] * count,
            scale=[1] * count,
            weights=[0, 1] + [0] * (count - 2),
            bias=-0.5,
        )
        plan = dict(
            grasp_estimator=checkpoint,
            model_hash=checkpoint["model_hash"],
            parent_controller_sha256=checkpoint["teacher_controller_sha256"],
            training_dataset=checkpoint["dataset_sha256"],
            grasp_checkpoint=hashlib.sha256(checkpoint_bytes(checkpoint)).hexdigest(),
        )
        model = parameters(plan)
        red = np.zeros((360, 480, 3), dtype=np.uint8)
        red[220:320, 82:230, 0] = 255
        self.assertTrue(predict(model, red, "red", 0, 0))
        self.assertFalse(predict(model, red, "green", 0, 0))
        self.assertIsNone(parameters({}))
        for changes in (
            {"training_dataset": "5" * 64},
            {"grasp_checkpoint": "6" * 64},
            {"grasp_estimator": checkpoint | {"bias": 0.0}},
        ):
            with self.assertRaises(ValueError):
                parameters(plan | changes)
        with self.assertRaisesRegex(ValueError, "Incomplete"):
            parameters({"grasp_checkpoint": plan["grasp_checkpoint"]})
