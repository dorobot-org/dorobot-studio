"""Check the Task page comparisons against the exact programs and measured Nexus trials."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
from zipfile import ZipFile


def main(rerun_worker):
    here = Path(__file__).resolve().parent
    summary = json.loads((here / "results.json").read_bytes())
    with ZipFile(here / "trials.zip") as z:

        def read(name):
            return json.loads(z.read(name))

        for case, expected_path in (
            ("g1-handoff-changes", "delta_rad"),
            ("G1-earlier-changes", "delta_rad"),
            ("Nano-earlier-changes", "delta_rad"),
            ("nano-pickup-changes", "grasp_association"),
        ):
            report = read(f"{case}/comparison.json")
            assert report["display"] == summary[case]["display"]
            plans = {}
            for side in ("before", "after"):
                prefix = f"{case}/{side}/"
                raw = z.read(prefix + "controller.json")
                sha = hashlib.sha256(raw).hexdigest()
                plan = plans[side] = json.loads(raw)
                source, result = read(prefix + "source.json"), read(
                    prefix + "result.json"
                )
                assert sha == report[side]["sha256"] == result["controller_hash"]
                assert source["backend"] == "metal" and source["nexus"] == "0.2.1"
                assert (
                    sha[:12] in report["display"]
                    and plan["provider"] == report[side]["provider"]
                )
                if expected_path == "delta_rad":
                    assert (
                        result["task_success"]
                        and result["requested_delta"] == plan["delta_rad"]
                    )
                    trajectory = read(prefix + "trajectory.json")
                    measured = (
                        trajectory[-1]["measured_rad"] - trajectory[0]["measured_rad"]
                    )
                    assert abs(measured - result["measured_delta"]) < 1e-6
                else:
                    assert (
                        hashlib.sha256(z.read(prefix + "motor-targets.f64")).hexdigest()
                        == result["actuator_audit"]["sha256"]
                    )
                    assert result["task_success"] == (side == "after")
            delta = next(
                row for row in report["changes"] if row["path"] == [expected_path]
            )
            for side in ("before", "after"):
                assert delta[side] == plans[side][expected_path]
            if report["relation"] == "parent":
                assert (
                    plans["after"]["previous_trial"]["controller_hash"]
                    == report["before"]["sha256"]
                )
            else:
                assert (
                    report["relation"] == "earlier"
                    and "previous_trial" not in plans["after"]
                )
            if rerun_worker:
                worker = here.parents[4] / "tools/aisle_workbench/worker.py"
                with TemporaryDirectory() as tmp:
                    root = Path(tmp)
                    for side in ("before", "after"):
                        folder = root / report[side]["revision"]
                        folder.mkdir()
                        (folder / "controller.json").write_bytes(
                            z.read(f"{case}/{side}/controller.json")
                        )
                    cmd = [
                        sys.executable,
                        str(worker),
                        "compare-controller",
                        "--robot",
                        report["robot"],
                        "--controller",
                        str(root / report["after"]["revision"] / "controller.json"),
                        "--controller-sha256",
                        report["after"]["sha256"],
                        "--out",
                        str(root / "fresh"),
                    ]
                    if report["relation"] == "earlier":
                        cmd.extend(
                            [
                                "--baseline",
                                str(
                                    root
                                    / report["before"]["revision"]
                                    / "controller.json"
                                ),
                            ]
                        )
                    subprocess.run(cmd, check=True)
                    assert (
                        json.loads((root / "fresh/comparison.json").read_bytes())
                        == report
                    )
        restored = read("restored-g1.json")
        assert restored == read("g1-handoff-changes/comparison.json")
        unavailable = read("unavailable.json")
        assert unavailable["restored_parent"] == restored["before"]["sha256"]
        assert unavailable["selected"] == restored["after"]["sha256"]
        assert (
            "unavailable" in unavailable["missing"]
            and "contents changed" in unavailable["changed"]
        )
    print(
        "Verified four native comparisons, eight Nexus outcomes, restart and missing-parent recovery."
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--rerun-worker",
        action="store_true",
        help="Rebuild every comparison from the archived controller bytes",
    )
    main(parser.parse_args().rerun_worker)
