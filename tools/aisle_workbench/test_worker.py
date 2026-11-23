"""Check constraints at the boundary between agent output and the simulator."""

import os
import subprocess
import sys
import time
import json
from pathlib import Path
import tempfile
import unittest
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
