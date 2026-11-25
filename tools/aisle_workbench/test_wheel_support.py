import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from wheel_support import feedback


class SupportTests(unittest.TestCase):
    def test_command_that_produced_the_scored_tick(self):
        with TemporaryDirectory() as tmp:
            folder = Path(tmp)
            actions = [
                dict(op="wheel", tick=0, speed=0.08, turn=-0.1),
                dict(op="sense", tick=20),
                dict(op="joint", tick=20, steps=10, values={"private": 123}),
            ]
            (folder / "actions.jsonl").write_text("\n".join(map(json.dumps, actions)))
            r = dict(
                physics_ticks=30,
                wheel_supported_ticks=28,
                unsupported_wheel_ranges=[
                    dict(first_step=20, last_step=20, phase="locate"),
                    dict(first_step=21, last_step=21, phase="pick"),
                ],
            )
            v = feedback(folder, r, {})["wheel_support"]
            self.assertEqual(v["unsupported_ticks"], 2)
            a, b = v["first_intervals"]
            self.assertEqual(
                a["active_command"],
                dict(issued_tick=0, kind="wheel", forward_m_s=0.08, yaw_rad_s=-0.1),
            )
            self.assertEqual(b["active_command"], dict(issued_tick=20, kind="joint"))
            with self.assertRaisesRegex(ValueError, "reported count"):
                feedback(folder, r | dict(wheel_supported_ticks=29), {})

    def test_old_and_capped_trials(self):
        with TemporaryDirectory() as tmp:
            p = Path(tmp)
            self.assertEqual(feedback(p, {}, {}), {})
            r = dict(physics_ticks=30, wheel_supported_ticks=30)
            self.assertEqual(feedback(p, r, {}), {})
            self.assertEqual(
                feedback(p, r, dict(max_forward_speed_m_s=0.06))["wheel_support"][
                    "max_forward_speed_m_s"
                ],
                0.06,
            )
