import json
from pathlib import Path
import tempfile
import unittest

from device_preflight import replay_samples
from g1_dds_state import JOINTS
from worker import ROOT


class DevicePreflightTests(unittest.TestCase):
    def replay(self, change=None):
        plan = {"joint": "right_shoulder_pitch_joint", "delta_rad": -0.08}
        q = [0.0] * 29
        row = dict(
            publisher=11, mode_pr=0, mode_machine=5, q=q, dq=q, quat=[1, 0, 0, 0]
        )
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "telemetry.jsonl"
            with p.open("w") as f:
                for i in range(2200):
                    sample = dict(
                        row,
                        source_ns=10**10 + i * 5_000_000,
                        received_ns=2 * 10**10 + i * 5_000_000,
                        tick=(2**32 - 1000 + i) % 2**32,
                    )
                    if i == 600 and change:
                        sample.update(change)
                    f.write(json.dumps(sample) + "\n")
            return replay_samples(p, plan, ROOT / "data/g1/g1.urdf")

    def test_pitch_roll_mapping_and_tick_wrap(self):
        targets, indices, metrics = self.replay()
        self.assertEqual(
            (JOINTS[15], JOINTS[22], len(JOINTS)),
            ("left_shoulder_pitch_joint", "right_shoulder_pitch_joint", 29),
        )
        self.assertEqual(len(indices), 2200)
        self.assertEqual(metrics["max_sample_age_ms"], 0)
        self.assertLess(targets[-1, 22], -0.08)
        self.assertEqual(float(targets[-1, 15]), 0)

    def test_invalid_device_windows_stop(self):
        for data, reason in [
            ({"mode_pr": 1}, "pitch/roll"),
            ({"mode_machine": 9}, "machine mode"),
            ({"publisher": 9}, "publisher"),
            ({"tick": 2**32 - 401}, "tick repeated"),
            ({"source_ns": 14_000_000_000}, "gap"),
            ({"quat": [0, 0, 0, 0]}, "normalized"),
            ({"q": [9.0] * 29}, "position exceeds"),
            ({"dq": [99.0] * 29}, "speed exceeds"),
        ]:
            with self.subTest(data=data), self.assertRaisesRegex(ValueError, reason):
                self.replay(data)
