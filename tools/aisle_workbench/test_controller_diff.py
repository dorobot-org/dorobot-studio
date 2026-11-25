import hashlib
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from controller_diff import changes, compare, display


class ControllerDiffTest(unittest.TestCase):
    def setUp(self):
        tmp = TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)

    def save(self, name, **fields):
        path = self.root / name / "controller.json"
        path.parent.mkdir()
        path.write_text(json.dumps({"robot": "G1", **fields}))
        return path, hashlib.sha256(path.read_bytes()).hexdigest()

    def test_measured_parent_precedes_an_unrelated_earlier_revision(self):
        parent, digest = self.save("1-G1-run", delta_rad=-0.08)
        earlier, _ = self.save("2-G1-prepare", delta_rad=0.5)
        selected, expected = self.save(
            "3-G1-prepare", delta_rad=-0.06, previous_trial={"controller_hash": digest}
        )
        review = compare(self.root, "G1", selected, expected, earlier)
        self.assertEqual(review["before"]["sha256"], digest)
        self.assertEqual(review["relation"], "parent")
        self.assertIn("Joint displacement · -0.08 rad → -0.06 rad", display(review))
        parent.write_text("{}")
        with self.assertRaisesRegex(ValueError, "unavailable"):
            compare(self.root, "G1", selected, expected, earlier)
        selected.write_text("{}")
        with self.assertRaisesRegex(ValueError, "contents changed"):
            compare(self.root, "G1", selected, expected, earlier)

    def test_trained_program_compares_with_its_teacher(self):
        _, grandparent = self.save("1-G1-prepare", delta_rad=0.08)
        previous = {"controller_hash": grandparent}
        _, teacher = self.save("2-G1-prepare", previous_trial=previous)
        selected, expected = self.save(
            "3-G1-train-reach",
            previous_trial=previous,
            parent_controller_sha256=teacher,
            reach_checkpoint="receipt",
        )
        review = compare(self.root, "G1", selected, expected)
        self.assertEqual(review["before"]["sha256"], teacher)
        self.assertEqual(
            review["changes"], [{"path": ("reach_checkpoint",), "after": "receipt"}]
        )

    def test_independent_revision_and_missing_first_baseline(self):
        old, _ = self.save("1-G1-prepare", task="first")
        selected, expected = self.save("2-G1-prepare", task="second")
        review = compare(self.root, "G1", selected, expected, old)
        self.assertEqual(review["relation"], "earlier")
        self.assertIn("Controller settings are identical", display(review))
        with self.assertRaisesRegex(ValueError, "first saved revision"):
            compare(self.root, "G1", selected, expected)

    def test_distinguishes_absent_null_boolean_and_numeric_values(self):
        delta = list(
            changes(
                {"n": 1, "settings": {"one": True, "gone": None}},
                {"n": 1.0, "settings": {"one": 1, "new": None}},
            )
        )
        self.assertEqual(
            delta,
            [
                {"path": ("settings", "gone"), "before": None},
                {"path": ("settings", "new"), "after": None},
                {"path": ("settings", "one"), "before": True, "after": 1},
            ],
        )

    def test_array_elements_keep_order_and_types(self):
        self.assertEqual(
            list(changes([True, [2]], [1, [3]])),
            [
                {"path": ("0",), "before": True, "after": 1},
                {"path": ("1", "0"), "before": 2, "after": 3},
            ],
        )
