import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import numpy as np

from decision_observations import CAMERAS, observation


class DecisionObservationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.tick = 40
        self.stamp = self.tick * 10_000_000
        self.data = {
            c: np.full((360, 480, 3), i, dtype=np.uint8) for i, c in enumerate(CAMERAS)
        }
        self.data.update(
            encoders=np.zeros(2, dtype=np.float32), stamp_ns=np.int64(self.stamp)
        )
        self.save("calibration.json", dict(physics_dt_s=0.01, joint_names=["a", "b"]))
        self.result = dict(kind="table-transfer", physics_ticks=40)
        self.save("result.json", self.result)
        (self.root / "actions.jsonl").write_text(
            json.dumps(
                dict(op="grasp observation", tick=self.tick, stamp_ns=self.stamp)
            )
            + "\n"
        )
        self.frame = self.root / "grasp-observations/000040.npz"
        self.frame.parent.mkdir()
        self.output = self.root / "view.rgb"

    def save(self, name, value):
        (self.root / name).write_text(json.dumps(value))

    def test_view_uses_packet_exposure_and_camera_order(self):
        np.savez_compressed(self.frame, **self.data)
        row = observation(self.root, self.tick, self.output)
        self.assertEqual(row["stamp_ns"], self.stamp)
        self.assertEqual(
            self.output.read_bytes(),
            np.concatenate([self.data[c] for c in CAMERAS], axis=1).tobytes(),
        )

    def test_wrong_time_and_damaged_failure_packet_do_not_publish(self):
        self.data["stamp_ns"] += 1
        np.savez_compressed(self.frame, **self.data)
        with self.assertRaisesRegex(ValueError, "timestamp"):
            observation(self.root, self.tick, self.output)
        bad = self.root / "last-pickup-observation.npz"
        bad.write_bytes(b"truncated")
        self.result["last_pickup_observation"] = dict(
            file=bad.name,
            stopped_tick=self.tick,
            stamp_ns=self.stamp,
            sha256=hashlib.sha256(b"original").hexdigest(),
        )
        self.save("result.json", self.result)
        with self.assertRaisesRegex(ValueError, "receipt"):
            observation(self.root, self.tick, self.output)
        self.assertFalse(self.output.exists())
