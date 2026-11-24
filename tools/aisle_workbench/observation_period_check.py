"""Compare G1 motor traces while sampling operator RGB at two simulation periods."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

import numpy as np

from worker import ROOT, digest, save


def compare(controller, output):
    output.mkdir(parents=True, exist_ok=False)
    summary = dict(
        controller_sha256=digest(controller),
        source_commit=subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
        ).strip(),
        periods={},
    )
    worker = Path(__file__).with_name("worker.py")
    for seconds in (1, 5):
        trial = output / f"period-{seconds}"
        subprocess.run(
            [
                sys.executable,
                str(worker),
                "run",
                "--robot",
                "G1",
                "--fixture",
                "impact",
                "--controller",
                str(controller),
                "--observations-only",
                "--observation-period-s",
                str(seconds),
                "--out",
                str(trial),
            ],
            check=True,
        )
        result = json.loads((trial / "result.json").read_bytes())
        frames = [
            json.loads(line)
            for line in (trial / "background-observations.jsonl")
            .read_text()
            .splitlines()
        ]
        expected = [0, *range(200 * seconds - 1, 2000, 200 * seconds)]
        preview = (trial / "preview.rgb").read_bytes()
        if (
            [row["tick"] for row in frames] != expected
            or frames[-1]["sha256"] != hashlib.sha256(preview[4:]).hexdigest()
            or int.from_bytes(preview[:4], "big") != expected[-1]
        ):
            raise ValueError(
                "G1 sampled frames differ from the requested simulation clock"
            )
        if (
            not result["task_success"]
            or result["both_feet_loaded_steps"] != 2000
            or result["recording"]
            or (trial / "recording.json").exists()
        ):
            raise ValueError(
                "G1 background run lost its stance or produced a full recording"
            )
        summary["periods"][seconds] = dict(observations=frames, verdict=result)
    with np.load(output / "period-1/motor-work.npz") as one, np.load(
        output / "period-5/motor-work.npz"
    ) as five:
        for key in ("target", "q_after"):
            summary[f"max_{key}_difference_rad"] = float(
                np.max(np.abs(one[key] - five[key]))
            )
    save(output / "comparison.json", summary)
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("controller", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    compare(args.controller, args.output)
