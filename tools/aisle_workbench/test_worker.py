"""Check constraints at the boundary between agent output and the simulator."""

import os
import subprocess
import sys
import time
import json
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

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

    def test_approach_envelope_and_robot_binding(self):
        plan = dict(kind="visual-approach", color="red", standoff_m=0.45)
        self.assertEqual(worker.validate(plan, self.profile), plan)
        for edit in [
            dict(color="yellow"),
            dict(standoff_m=True),
            dict(standoff_m=0.39),
            dict(standoff_m=float("nan")),
            dict(kind="pick"),
        ]:
            with self.subTest(edit=edit), self.assertRaises(ValueError):
                worker.validate(plan | edit, self.profile)
        with self.assertRaises(ValueError):
            worker.validate(plan, self.profile | {"robot": "G1"})

    def test_standing_shoulder_requires_g1_and_bounded_displacement(self):
        plan = dict(kind="standing-inspection", joint="shoulder", delta_rad=-0.06)
        g1 = self.profile | {"robot": "G1"}
        self.assertEqual(worker.validate(plan, g1), plan)
        with self.assertRaises(ValueError):
            worker.validate(plan, self.profile)
        with self.assertRaises(ValueError):
            worker.validate(plan | {"delta_rad": -0.5}, g1)

    def test_standing_turn_requires_a_supported_angle_and_neutral_pitch(self):
        meta = dict(
            robot="G1", allowed_joints=["shoulder"], limits={"shoulder": [-1, 1]}
        )
        task = dict(kind="standing-inspection", joint="shoulder", delta_rad=-0.08)
        for angle in (0.34906585, 0.52359878):
            self.assertEqual(
                worker.validate(task | {"trunk_yaw_rad": angle}, meta)["trunk_yaw_rad"],
                angle,
            )
        for fields in (
            {"trunk_yaw_rad": 0.6},
            {"trunk_yaw_rad": 0.52359878, "trunk_pitch_rad": -0.10},
            {"trunk_yaw_rad": 0.52359878, "kind": "joint-inspection"},
        ):
            with self.assertRaises(ValueError):
                worker.validate(task | fields, meta)

    def test_upright_trunk_requires_standing_and_the_bounded_target(self):
        meta = self.profile | {"robot": "G1"}
        task = dict(kind="standing-inspection", joint="shoulder", delta_rad=-0.08)
        self.assertNotIn("trunk_pitch_rad", worker.validate(task, meta))
        self.assertEqual(
            worker.validate(task | {"trunk_pitch_rad": -0.1}, meta)["trunk_pitch_rad"],
            -0.1,
        )
        for override in (
            {"trunk_pitch_rad": 0.2},
            {"trunk_pitch_rad": False},
            {"kind": "joint-inspection", "trunk_pitch_rad": -0.1},
        ):
            with self.assertRaises(ValueError):
                worker.validate(task | override, meta)

    def test_tracking_is_opt_in_and_bound_to_a_transfer(self):
        plan = dict(kind="table-transfer", color_order="red", track_grasp=True)
        self.assertEqual(worker.validate(plan, self.profile), plan)
        with self.assertRaisesRegex(ValueError, "tracking"):
            worker.validate(plan | {"kind": "visual-approach"}, self.profile)
        with self.assertRaisesRegex(ValueError, "tracking"):
            worker.validate(plan | {"track_grasp": "true"}, self.profile)

    def test_transfer_forward_speed_bound_includes_existing_controllers(self):
        original = dict(kind="table-transfer", color_order="red")
        self.assertEqual(worker.validate(original, self.profile), original)
        for cap in (0.025, 0.04, 0.13):
            plan = original | dict(max_forward_speed_m_s=cap)
            self.assertEqual(worker.validate(plan, self.profile), plan)
        for cap in (True, None, 0, 0.14, float("nan"), float("inf")):
            with self.assertRaisesRegex(ValueError, "forward speed"):
                worker.validate(
                    original | dict(max_forward_speed_m_s=cap), self.profile
                )
        with self.assertRaisesRegex(ValueError, "forward speed"):
            worker.validate(
                dict(kind="visual-approach", color="red", max_forward_speed_m_s=0.06),
                self.profile,
            )

    def test_association_requires_tracking_and_a_known_rule(self):
        plan = dict(kind="table-transfer", color_order="red", track_grasp=True)
        for rule in worker.GRASP_ASSOCIATIONS:
            selected = plan | dict(grasp_association=rule)
            self.assertEqual(worker.validate(selected, self.profile), selected)
            with self.assertRaises(ValueError):
                worker.validate(selected | {"track_grasp": False}, self.profile)
        with self.assertRaises(ValueError):
            worker.validate(plan | {"grasp_association": "unchecked"}, self.profile)

    def test_single_named_transfer_binds_saved_and_new_controllers(self):
        for color in ("red", "blue", "green"):
            plan = dict(
                kind="table-transfer",
                task=f"Move the {color} floor cube onto the coffee table.",
            )
            self.assertEqual(
                worker.validate(plan | {"color_order": color}, self.profile)[
                    "color_order"
                ],
                color,
            )
            for wrong in [
                dict(color_order="red,green,blue"),
                dict(kind="visual-approach", color=color),
            ]:
                with self.assertRaisesRegex(ValueError, "operator requested only"):
                    worker.validate(plan | wrong, self.profile)
        for instruction in (
            "Move all three floor cubes onto the coffee table in red, green, blue order.",
            "Move the cube left of the red cube onto the coffee table.",
            "Move the red cube onto the coffee table. Then move the nearest cube.",
        ):
            self.assertIsNone(worker.named_cube_transfer(instruction))

    def test_author_receives_the_named_target_and_an_overbroad_reply_is_rejected(self):
        task = "Please put the green cube on the coffee table."
        args = SimpleNamespace(
            robot="Nano",
            task=task,
            feedback=None,
            provider="octos-deepseek",
            out=Path("."),
        )
        reply = dict(
            supported=True, kind="table-transfer", color_order="red,green,blue"
        )
        with patch.object(
            worker, "request_controller", return_value=(reply, "test-model")
        ) as author:
            with self.assertRaisesRegex(ValueError, "only the green cube"):
                worker.prepare(args, self.profile)
        schema = author.call_args.args[2]
        self.assertEqual(schema["properties"]["color_order"]["enum"], ["green"])
        self.assertEqual(schema["properties"]["kind"]["enum"], ["table-transfer"])
        # An older plan with no instruction still uses the existing action-envelope check.
        self.assertEqual(
            worker.validate(reply, self.profile),
            dict(kind="table-transfer", color_order="red,green,blue"),
        )

    def test_transfer_requires_nano_and_a_distinct_color_order(self):
        for order in ["blue", "green,blue,red"]:
            p = dict(kind="table-transfer", color_order=order)
            self.assertEqual(worker.validate(p, self.profile), p)
            with self.assertRaises(ValueError):
                worker.validate(p, self.profile | {"robot": "G1"})
        for invalid in ("red,red", "any", None):
            with self.assertRaises(ValueError):
                worker.validate(
                    dict(kind="table-transfer", color_order=invalid), self.profile
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

    def test_transfer_revision_gets_per_cube_verdict_without_simulator_pose(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            worker.save(run / "controller.json", {"kind": "table-transfer"})
            worker.save(
                run / "result.json",
                dict(
                    robot="Nano",
                    model_hash="model",
                    controller_hash=worker.digest(run / "controller.json"),
                    requested=["red", "blue"],
                    phase="carry",
                    task_success=False,
                    kind="table-transfer",
                    cubes={"red": {"passed": True, "final_position_m": [0, 1, 0.56]}},
                ),
            )
            feedback = worker.measured_feedback(run, self.profile)
            self.assertEqual(feedback["settled"], {"red": True, "blue": False})
            self.assertNotIn("final_position_m", json.dumps(feedback))
            (run / "actions.jsonl").write_text(
                "\n".join(
                    json.dumps(row)
                    for row in [
                        dict(op="phase", tick=10, color="blue", name="carry"),
                        dict(
                            op="jaw observation",
                            tick=20,
                            color="blue",
                            visible=False,
                            private_pose=[1, 2, 3],
                        ),
                        dict(
                            op="table observation",
                            tick=30,
                            distance_m=0.34,
                            height_m=0.54,
                            angle_rad=0.02,
                        ),
                    ]
                )
            )
            feedback = worker.measured_feedback(run, self.profile)
            self.assertEqual(
                feedback["onboard_observations"]["blue"]["last_wrist_jaw"],
                {"tick": 20, "cube_visible": False},
            )
            self.assertNotIn("private_pose", json.dumps(feedback))

    def test_octos_accepts_single_fenced_object_and_retains_malformed_reply(self):
        template = {"supported": True, "delta_rad": 0.1, "joint": "shoulder"}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            for valid, reply in [
                (True, "```json\n" + json.dumps(template) + "\n```"),
                (False, "No plan issued."),
            ]:
                response = json.dumps({"model": "deepseek-v4-flash", "text": reply})
                with patch(
                    "worker.subprocess.run",
                    return_value=subprocess.CompletedProcess([], 0, stdout=response),
                ):
                    if valid:
                        worker.octos_controller(
                            "task", {"required": list(template)}, path, path
                        )
                        self.assertEqual(
                            json.loads((path / "reply.json").read_text()), template
                        )
                    else:
                        with self.assertRaisesRegex(
                            ValueError, "unreadable controller"
                        ):
                            worker.octos_controller(
                                "task", {"required": list(template)}, path, path
                            )
                    self.assertEqual(
                        json.loads((path / "provider-response.json").read_text())[
                            "text"
                        ],
                        reply,
                    )

    def test_octos_rejects_provider_fallback_or_invalid_schema(self):
        template = {"joint": "shoulder", "delta_rad": -0.06, "supported": True}
        schema = {"required": list(template)}
        with tempfile.TemporaryDirectory() as directory:
            for model, plan in [
                ("different-model", template),
                ("deepseek-v4-flash", template | {"extra": 1}),
                ("deepseek-v4-flash", template | {"supported": "yes"}),
            ]:
                result = subprocess.CompletedProcess(
                    [], 0, stdout=json.dumps({"model": model, "text": json.dumps(plan)})
                )
                with (
                    patch("worker.subprocess.run", return_value=result),
                    self.assertRaises(ValueError),
                ):
                    worker.octos_controller(
                        "inspect", schema, directory, Path(directory)
                    )
            self.assertFalse((Path(directory) / "reply.json").exists())

    @unittest.skipUnless(os.name == "posix", "Studio process groups require POSIX")
    def test_lost_studio_pipe_stops_the_worker_and_its_codex_child(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            out = root / "job"
            out.mkdir()
            (out / "worker.log").touch()
            child = root / "codex"
            child.write_text('#!/bin/sh\necho $$ > "$AISLE_TEST_PID"\nexec sleep 30\n')
            child.chmod(0o700)
            marker = root / "child.pid"
            process = subprocess.Popen(
                [
                    sys.executable,
                    worker.__file__,
                    "prepare",
                    "--robot",
                    "Nano",
                    "--out",
                    str(out),
                    "--provider",
                    "codex",
                    "--reserved",
                ],
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
                env=os.environ
                | {
                    "PATH": str(root) + os.pathsep + os.environ["PATH"],
                    "AISLE_TEST_PID": str(marker),
                },
            )
            try:
                deadline = time.monotonic() + 8
                while not marker.is_file() and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue(marker.exists())
                process.stdin.close()
                self.assertLess(process.wait(timeout=5), 0)
                pid = marker.read_text().strip()
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    state = subprocess.run(
                        ["ps", "-o", "stat=", "-p", pid], capture_output=True, text=True
                    ).stdout.strip()
                    if not state or state.startswith("Z"):
                        break
                    time.sleep(0.05)
                self.assertTrue(not state or state.startswith("Z"))
                self.assertFalse((out / "controller.json").exists())
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, 9)
                    process.wait()
