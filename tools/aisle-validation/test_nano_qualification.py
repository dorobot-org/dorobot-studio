import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch
from zipfile import ZipFile

import numpy as np

import nano_transfer_suite as suite
from nano_suite_evidence import pack
from nano_actuator_audit import MotorEnvelope, MotorWriter, replay


class MotorTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.directory = Path(tmp.name)
        self.model = self.directory / "model.urdf"
        self.model.write_text(
            '<robot><joint type="revolute" name="arm"><limit lower="-1" upper="1" velocity="2"/>'
            '</joint><joint type="continuous" name="wheel"><limit velocity="12"/></joint></robot>'
        )
        self.envelope = MotorEnvelope(self.model, ["arm", "wheel"])
        self.robot = Mock()
        self.stream = io.BytesIO()
        self.writer = MotorWriter(self.stream, self.envelope, [0.0, 0.0])

    def audit(self, ticks, extra=b""):
        path = self.directory / "journal.f64"
        path.write_bytes(self.stream.getvalue() + extra)
        return replay(path, self.envelope, ticks)

    def test_coverage_and_wheel_rate(self):
        self.writer.apply(self.robot, [2, 1], [0.02, -0.05])
        self.robot.control_dofs_position.assert_called_once_with([0.02, -0.05], [2, 1])
        self.assertTrue(self.audit(1)["passed"])
        self.assertEqual(self.audit(2)["unmatched_ticks"], 1)
        extra = np.asarray([0.02, 0.2], dtype="<f8").tobytes()
        self.assertEqual(self.audit(2, extra)["invalid_rows"], 1)
        with self.assertRaisesRegex(ValueError, "Truncated"):
            self.audit(1, b"x")

    def test_rejection_precedes_nexus_write(self):
        for row in ([float("nan"), 0], [2, 0], [-0.021, 0], [0, 0.051], [0]):
            with self.assertRaises(ValueError):
                self.writer.apply(self.robot, [2, 1], row)
        self.robot.control_dofs_position.assert_not_called()
        self.assertEqual(self.writer.rejected, 5)
        self.assertTrue(self.audit(0)["passed"])

    def test_joint_order_must_cover_all_motors(self):
        for names in (["wheel"], ["arm", "arm"], ["wheel", "other"]):
            with self.assertRaises(ValueError):
                MotorEnvelope(self.model, names)


class SuiteTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.folder = Path(tmp.name)
        self.directory = self.folder / "suite"
        self.controller = self.folder / "controller.json"
        self.plan = dict(
            robot="Nano",
            model_hash="sample",
            kind="table-transfer",
            color_order="red,green,blue",
        )
        suite.worker.save(self.controller, self.plan)
        for target, value in (
            ("worker.describe", {"model_hash": "sample", "robot": "Nano"}),
            ("inputs", {"test": True}),
        ):
            mock = patch("nano_transfer_suite." + target, return_value=value)
            mock.start()
            self.addCleanup(mock.stop)

    def test_commitment_is_checked_before_starting_any_case(self):
        manifest = suite.prepare(self.directory, self.controller, [7, 4])
        members = suite.read(self.directory / "private-layouts.json")
        members.reverse()
        suite.worker.save(self.directory / "private-layouts.json", members)
        with self.assertRaisesRegex(ValueError, "commitment"):
            suite.run(self.directory, manifest["manifest_sha256"], 1)
        self.assertEqual(list(self.directory.glob("case-*")), [])

    def test_interrupted_layout_is_counted_and_never_retried(self):
        manifest = suite.prepare(self.directory, self.controller, [3])
        (self.directory / "case-000.log").write_text("interrupted")
        with patch("nano_transfer_suite.subprocess.Popen") as launch:
            report = suite.run(self.directory, manifest["manifest_sha256"], 1)
        launch.assert_not_called()
        self.assertEqual(
            (report["cases"], report["verified"], report["successes"]), (1, 0, 0)
        )
        self.assertFalse(report["qualified"])
        archive = self.folder / "interrupted.zip"
        pack(self.directory, archive)
        with ZipFile(archive) as z:
            self.assertEqual(
                z.read("report.json"), (self.directory / "report.json").read_bytes()
            )
            self.assertEqual(z.read("case-000.log"), b"interrupted")
            self.assertNotIn("case-000/wrist-delivery.jsonl", z.namelist())

    def test_hidden_requires_45_successes_and_all_50_audits(self):
        manifest = suite.prepare(self.directory, self.controller)
        plan = suite.read(self.directory / "suite.json")
        members = suite.read(self.directory / "private-layouts.json")
        self.assertEqual(len({m["seed"] for m in members}), 50)
        self.assertEqual(plan["commitments"], [suite.signature(m) for m in members])
        self.assertNotIn("seed", str(plan))
        self.assertEqual(
            (self.directory / "private-layouts.json").stat().st_mode & 0o777, 0o600
        )
        for successes, verified, bypasses, expected in [
            (45, 50, 0, True),
            (44, 50, 0, False),
            (49, 49, 0, False),
            (50, 50, 1, False),
        ]:
            receipts = [
                dict(
                    passed=i < successes,
                    verified=i < verified,
                    guard_bypasses=int(i < bypasses),
                )
                for i in range(50)
            ]
            with patch.object(suite, "receipt", side_effect=receipts):
                result = suite.summarize(
                    self.directory, manifest["manifest_sha256"], plan, members
                )
            self.assertEqual(result["qualified"], expected)
