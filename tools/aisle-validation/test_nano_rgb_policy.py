import unittest
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np

from nano_rgb_policy import CAMERAS, VisualServo


class VisualServoTests(unittest.TestCase):
    def setUp(self):
        camera = dict(
            parent_link="mount",
            resolution=[80, 60],
            link_from_camera_gl=np.eye(4),
            intrinsics=[[100, 0, 40], [0, 100, 30], [0, 0, 1]],
        )
        config = dict(
            model="nano.urdf",
            joint_names=["pan"],
            color="red",
            standoff_m=0.45,
            cameras={name: camera for name in CAMERAS},
        )
        chain = SimpleNamespace(
            joint_names=["pan"], forward=lambda q: (np.array([0.5, 0, 0.4]), np.eye(3))
        )
        with patch("nano_rgb_policy.load_urdf_chain", return_value=chain):
            self.servo = VisualServo(config)
        self.packet = {name: np.zeros((60, 80, 3), np.uint8) for name in CAMERAS}
        self.packet.update(stamp_ns=np.int64(1), encoders=np.zeros(1))

    def test_privileged_input_and_stale_frames_rejected(self):
        for key in (
            "depth",
            "segmentation",
            "world_from_camera",
            "base_position",
            "cube_pose",
        ):
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.servo.command(self.packet | {key: np.zeros(3)})
        self.servo.command(self.packet)
        with self.assertRaises(ValueError):
            self.servo.command(self.packet)

    def test_commands_stop_on_target_loss_and_ambiguity(self):
        cmd = self.servo.command(self.packet)
        self.assertEqual((cmd["state"], cmd["speed"], cmd["yaw_rate"]), ("lost", 0, 0))
        head = self.packet["head"]
        head[25:35, 35:45, 0] = 255
        self.packet["stamp_ns"] = np.int64(2)
        cmd = self.servo.command(self.packet)
        self.assertEqual(cmd["cameras"], ["head"])
        self.assertGreater(cmd["speed"], 0)
        head[15:25, 15:25, 0] = 255
        self.packet["stamp_ns"] = np.int64(3)
        cmd = self.servo.command(self.packet)
        self.assertEqual(
            (cmd["state"], cmd["speed"], cmd["yaw_rate"]), ("ambiguous", 0, 0)
        )


if __name__ == "__main__":
    unittest.main()
