import unittest
from collections import Counter, deque
from types import SimpleNamespace

import numpy as np

from nano_rgb_transfer_policy import Controller, visible_in_jaws
from run_rgb_transfer import Judge, finger_gap


class TransferTests(unittest.TestCase):
    def test_blue_background_does_not_confirm_grasp(self):
        rgb = np.zeros((360, 480, 3), np.uint8)
        rgb[:, :80] = [25, 70, 200]
        self.assertFalse(visible_in_jaws(rgb, "blue"))
        rgb[219:321, 81:231] = [25, 70, 200]
        self.assertTrue(visible_in_jaws(rgb, "blue"))
        self.assertFalse(visible_in_jaws(rgb, "green"))

    def test_unreachable_waypoint_stops_before_moving(self):
        controller = Controller.__new__(Controller)
        controller.names = ["arm"]
        controller.targets = np.zeros(1)
        sent = []

        def solve(target, q):
            if target[0] > 0.5:
                raise ValueError("unreachable")
            return q

        controller.model = SimpleNamespace(
            arm=controller.names,
            grip=lambda q: (np.zeros(3), None),
            solve_supported_grasp=solve,
        )
        controller.joints = lambda cmd, steps: sent.append(cmd)
        with self.assertRaisesRegex(ValueError, "unreachable"):
            controller.reach([1.0, 0.0, 0.0], 60, 3, supported=True)
        self.assertEqual(sent, [])

    def test_rotated_finger_box_gives_conservative_gap(self):
        basis = np.array([[0, -1, 0], [1, 0, 0], [0, 0, 1]])
        pos = np.array([0.4, -0.1, 1])
        box = (np.zeros(3), np.array([0.05, 0.01, 0.01]))
        self.assertAlmostEqual(
            finger_gap(pos + [0.08, 0, 0], pos, basis, box, 0.035), 0.035
        )
        self.assertLess(finger_gap(pos + [0, 0.07, 0], pos, basis, box, 0.035), 0)

    def test_cached_manifold_cannot_supply_grasp_or_table_load(self):
        def body(z):
            return SimpleNamespace(
                get_pos=lambda: np.array([0.0, 0.0, z]),
                get_quat=lambda: np.array([1.0, 0.0, 0.0, 0.0]),
            )

        normal = [0.0]
        rows = [dict(kind="normal", link=i, free_body=2, impulse=0) for i in (23, 24)]
        state = SimpleNamespace(
            debug_rigid_contact_impulses=lambda _: [
                dict(body_a=2, body_b=1, normal_impulse=normal)
            ],
            debug_multibody_contact_impulses=lambda _: rows,
            debug_contacts=lambda _: [dict(body_a=i, body_b=2) for i in (1, 23, 24)],
        )
        sim = SimpleNamespace(
            scene=SimpleNamespace(state=state),
            engine=SimpleNamespace(viewer=None),
            robot=SimpleNamespace(get_qpos=lambda: []),
            names=[],
            dofs=[],
        )
        judge = Judge.__new__(Judge)
        judge.colors = ["red"]
        judge.object_ids = {1: "coffee_table", 2: "red"}
        judge.links = judge.slots = {23: "left_finger_a", 24: "left_finger_b"}
        judge.wheels = set()
        judge.grasped, judge.collisions, judge.excursions = (
            Counter(),
            Counter(),
            Counter(),
        )
        judge.supported = 0
        judge.initial = {"red": np.zeros(3)}
        judge.cubes, judge.radii = {"red": body(0.56)}, {"red": 0.035}
        judge.jaws = [(body(1), (np.zeros(3), np.full(3, 0.01)))]
        judge.tail = {"red": deque(maxlen=100)}
        judge.tick(sim)
        _, supported, held, gap = judge.tail["red"][-1]
        self.assertFalse(supported or held)
        self.assertGreater(gap, 0.001)
        self.assertEqual(judge.grasped["red"], 0)
        normal[0] = 0.0004
        for row in rows:
            row["impulse"] = 0.001
        judge.tick(sim)
        _, supported, held, _ = judge.tail["red"][-1]
        self.assertTrue(supported and held)
        self.assertEqual(judge.grasped["red"], 0)
        normal[0] = 0
        judge.tick(sim)
        self.assertEqual(judge.grasped["red"], 1)
