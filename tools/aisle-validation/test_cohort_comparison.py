"""Exercise paired verdicts using the published, measured Nano motor records."""

import json
import sys
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from zipfile import ZipFile
from shutil import copyfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "aisle_workbench"))

from nano_suite_evidence import EVIDENCE, pack
from nano_transfer_suite import summarize, worker
from cohort_comparison import compare

GUIDE = worker.ROOT / "docs/product/aisle-studio/validation/transfer-wrist-timing"


def two_cases(directory, order, missing=None):
    directory.mkdir()
    with ZipFile(GUIDE / "qualification.zip") as original:
        manifest = json.loads(original.read("suite.json"))
        rows = json.loads(original.read("report.json"))["rows"]
        manifest.update(
            cases=len(order),
            split="development",
            success_requirement=None,
            commitments=[manifest["commitments"][i] for i in order],
        )
        worker.save(directory / "suite.json", manifest)
        (directory / "controller.json").write_bytes(original.read("controller.json"))
        members = []
        for dest, source in enumerate(order):
            members.append({k: rows[source][k] for k in ("salt", "seed")})
            if source == missing:
                continue
            case = directory / f"case-{dest:03d}"
            case.mkdir()
            for evidence in EVIDENCE[:6]:
                (case / evidence).write_bytes(
                    original.read(f"case-{source:03d}/{evidence}")
                )
    summarize(directory, worker.digest(directory / "suite.json"), manifest, members)
    archive = directory / "cohort.zip"
    pack(directory, archive)
    return archive


class CohortComparison(unittest.TestCase):
    def test_reordered_layout_with_missing_result(self):
        with TemporaryDirectory() as root:
            base = two_cases(Path(root) / "base", [0, 1])
            candidate = two_cases(Path(root) / "candidate", [1, 0], missing=0)
            result = compare(base, candidate)
            self.assertEqual(result["cases"], 2)
            self.assertEqual(
                result["counts"],
                dict(
                    both_passed=1, both_failed=0, improved=0, regressed=0, unverified=1
                ),
            )
            (changed,) = result["changes"]
            self.assertEqual(
                (changed["baseline_index"], changed["candidate_index"]), (0, 1)
            )
            self.assertEqual(
                changed["candidate_reasons"], ["Missing complete physics result"]
            )
            # The complete report cannot be verified against the other archive's public commitment.
            candidate.with_name("suite.json").write_bytes(
                base.with_name("suite.json").read_bytes()
            )
            with self.assertRaisesRegex(ValueError, "Manifest"):
                compare(base, candidate)

    def test_real_g1_matched_impacts(self):
        with TemporaryDirectory() as root:
            inputs = []
            for name in ("g1-upright-trunk", "g1-standing-heading"):
                path = Path(root) / name
                path.mkdir()
                copyfile(
                    GUIDE.parent / name / "matched-suite.json", path / "suite.json"
                )
                copyfile(GUIDE.parent / name / "matched.zip", path / "cohort.zip")
                inputs.append(path / "cohort.zip")
            result = compare(*inputs)
            self.assertEqual(
                (result["baseline"]["successes"], result["candidate"]["successes"]),
                (17, 20),
            )
            self.assertEqual(
                [r["baseline_index"] for r in result["changes"]], [9, 12, 13]
            )
            self.assertTrue(
                all(
                    r["outcome"] == "improved" and r["baseline_reasons"]
                    for r in result["changes"]
                )
            )
