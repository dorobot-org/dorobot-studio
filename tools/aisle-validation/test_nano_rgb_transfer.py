import json
from pathlib import Path
import tempfile
import unittest
from collections import Counter, deque
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np

from nano_rgb_transfer_policy import Controller, release_candidates, visible_in_jaws
from nano_wrist_timing import WristTiming
from run_rgb_transfer import Judge, finger_gap


class TransferTests(unittest.TestCase):
    def test_synchronized_policy_read_rejects_recent_but_misaligned_wrist_image(self):
        with tempfile.TemporaryDirectory() as tmp:
            guard = WristTiming("intermittent", Path(tmp), False)
            for tick in range(0, 40, 5):
                image = np.full((4, 4, 3), tick, dtype=np.uint8)
                accepted = guard.accept(tick, lambda: image, synchronized=True)
                self.assertEqual(accepted, tick < 20)
            np.testing.assert_array_equal(guard.accepted_rgb, np.full((4, 4, 3), 15))
            entry = json.loads(
                (Path(tmp) / "wrist-delivery.jsonl").read_text().splitlines()[-1]
            )
            self.assertEqual((entry["tick"], entry["capture_tick"]), (35, 25))
            self.assertEqual(entry["fault"], "unsynchronized wrist packet")
            self.assertEqual(guard.last_accepted_ns, 150_000_000)

    def test_forward_limit_preserves_braking_reverse_and_steering(self):
        controller = Controller.__new__(Controller)
        controller.max_forward_speed = 0.08
        commands = []
        controller.act = lambda kind, **args: commands.append(
            (kind, args["speed"], args["turn"])
        )
        for speed, turn in [(0.12, 0.2), (0.055, -0.1), (-0.065, 0), (0, 0.35), (0, 0)]:
            controller.drive(speed, turn)
        self.assertEqual(
            commands,
            [
                ("wheel", 0.08, 0.2),
                ("wheel", 0.055, -0.1),
                ("wheel", -0.065, 0),
                ("wheel", 0, 0.35),
                ("wheel", 0, 0),
            ],
        )

    def test_clear_reachable_region_between_preferred_release_slots(self):
        side, span = 0.161, (-0.1867, 0.2318)
        occupied = np.array([0.3375, 0.1217])
        choices = list(release_candidates(side, span))
        clear = [
            kind
            for along, inset, kind in choices
            if 0.181 <= along <= 0.212
            and np.linalg.norm(np.array([0.3075 + inset, along]) - occupied) >= 0.078
        ]
        self.assertTrue(clear)
        self.assertEqual(set(clear), {"grid"})
        self.assertTrue(
            all(span[0] <= y <= span[1] and depth >= 0.085 for y, depth, _ in choices)
        )

    def test_placement_turn_tracks_cube_across_wrist_views(self):
        controller = Controller.__new__(Controller)
        controller.release_slot = Mock(return_value=True)
        controller.drive = Mock()
        controller.act = Mock()
        controller.table_views = Mock(return_value=[])
        controller.release_observation = Mock()
        controller.held_in_wrist_view = Mock(return_value=True)
        controller.table = lambda sample: sample["rim"]
        controller.release_landmarks = lambda sample, *_: sample["cubes"]

        def packet(angle, keys, along_shift):
            normal = np.array([np.cos(angle), np.sin(angle)])
            tangent = np.array([-normal[1], normal[0]])
            rim = dict(
                normal=normal,
                along=tangent,
                distance=0.3,
                height=0.54,
                span=[-0.2, 0.25],
            )
            return {
                "rim": rim,
                "cubes": {
                    key: 0.43 * normal + (0.15 + along_shift) * tangent for key in keys
                },
            }

        # The right wrist sees the cube initially. A shared exposure lets the
        # controller keep tracking when only the left wrist sees it after the turn.
        start = packet(0.0, ["right"], 0)
        mid = packet(0.1, ["left", "right"], 0.002)
        end = packet(0.20, ["left"], 0.004)
        controller.sense = Mock(side_effect=[mid, end, end])
        with patch("nano_rgb_transfer_policy.occupied_table_points", return_value=[]):
            rim, obstacles = controller.reposition_for_release(
                start["rim"], [np.array([0.43, 0.15])], start, "green", ["red"]
            )
        expected = 0.43 * rim["normal"] + 0.154 * rim["along"]
        np.testing.assert_allclose(obstacles, [expected])
        self.assertTrue(
            any(
                call.kwargs.get("initial_view_match") is False
                for call in controller.act.call_args_list
            )
        )
        controller.release_observation.assert_called_once()
        controller.held_in_wrist_view.assert_called_once_with("green", end)

        # With no common landmark between consecutive views the base must brake.
        controller.sense = Mock(return_value=packet(0.1, ["left"], 0))
        controller.release_observation.reset_mock()
        controller.drive.reset_mock()
        with self.assertRaisesRegex(ValueError, "consecutive"):
            controller.reposition_for_release(start["rim"], [], start, "green", ["red"])
        self.assertEqual(controller.drive.call_args.kwargs, {})
        controller.release_observation.assert_not_called()

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

    def test_unreachable_open_jaw_withdrawal_sends_no_motor_targets(self):
        controller = Controller.__new__(Controller)
        controller.names = ["arm", "gripper"]
        controller.grip = "gripper"
        controller.targets = np.array([0.0, 0.84])
        commands = []

        def solve(point, state):
            if state[1] < 0 and point[0] < 0.39:
                raise ValueError("open jaw path is unreachable")
            return np.array([point[0], state[1]])

        controller.model = SimpleNamespace(
            arm=["arm"],
            grip=lambda q: (np.array([q[0], 0, 0]), None),
            solve_supported_grasp=solve,
        )
        controller.joints = lambda cmd, steps: commands.append(cmd)
        with self.assertRaisesRegex(ValueError, "open jaw"):
            controller.release_paths(np.array([0.4, 0.0]), 0.55)
        self.assertEqual(commands, [])
        np.testing.assert_array_equal(controller.targets, [0.0, 0.84])
        incoming, outgoing = controller.release_paths(np.array([0.5, 0.0]), 0.55)
        self.assertEqual((len(incoming), len(outgoing)), (12, 9))
        self.assertEqual(commands, [])

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
            tick=1,
            scene=SimpleNamespace(state=state),
            engine=SimpleNamespace(viewer=None),
            robot=SimpleNamespace(get_qpos=lambda: []),
            names=[],
            dofs=[],
        )
        judge = Judge.__new__(Judge)
        judge.colors = ["red"]
        judge.airborne_in_jaw = {"red": False}
        judge.pick_contacts = {}
        judge.object_ids = {1: "coffee_table", 2: "red"}
        judge.links = judge.slots = {23: "left_finger_a", 24: "left_finger_b"}
        judge.wheels = set()
        judge.grasped, judge.collisions, judge.excursions = (
            Counter(),
            Counter(),
            Counter(),
        )
        judge.supported = 0
        judge.unsupported_ranges = []
        judge.phase = "place"
        judge.initial = {"red": np.zeros(3)}
        judge.cubes, judge.radii = {"red": body(0.56)}, {"red": 0.035}
        judge.jaws = [(body(1), (np.zeros(3), np.full(3, 0.01)))]
        judge.tail = {"red": deque(maxlen=100)}
        judge.tick(sim)
        _, supported, held, gap = judge.tail["red"][-1]
        self.assertFalse(supported or held)
        self.assertEqual(judge.pick_contacts["red"], ({}, False))
        self.assertFalse(judge.airborne_in_jaw["red"])
        self.assertGreater(gap, 0.001)
        self.assertEqual(judge.grasped["red"], 0)
        sim.tick += 1
        normal[0] = 0.0004
        for row in rows:
            row["impulse"] = 0.001
        judge.tick(sim)
        _, supported, held, _ = judge.tail["red"][-1]
        self.assertTrue(supported and held)
        forces = {"left_finger_a": 0.001, "left_finger_b": 0.001}
        self.assertEqual(judge.pick_contacts["red"], (forces, True))
        self.assertFalse(judge.airborne_in_jaw["red"])
        self.assertEqual(judge.grasped["red"], 0)
        sim.tick += 1
        normal[0] = 0
        judge.tick(sim)
        self.assertEqual(
            judge.unsupported_ranges,
            [dict(phase=judge.phase, first_step=1, last_step=3)],
        )
        self.assertEqual(judge.grasped["red"], 1)
        self.assertEqual(judge.pick_contacts["red"], (forces, False))
