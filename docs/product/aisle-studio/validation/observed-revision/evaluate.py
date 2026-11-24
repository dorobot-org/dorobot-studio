"""Run the three frozen programs on their shared held-out Nano layouts."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess
import sys

PROOF = Path(__file__).resolve().parent
ROOT = PROOF.parents[4]


def read(path):
    return json.loads(path.read_bytes())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    parser.add_argument("--jobs", type=int, default=3, choices=range(1, 10))
    args = parser.parse_args()
    protocol = read(PROOF / "protocol.json")
    assert sha(PROOF / "disclosure.json") == protocol["disclosure_sha256"]
    for name, digest in read(PROOF / "frozen.json").items():
        assert sha(PROOF / name) == digest
    seeds = read(PROOF / "disclosure.json")["seeds"]
    assert len(seeds) == len(set(seeds)) == protocol["held_out_layouts"]
    assert protocol["development_seed"] not in seeds
    args.out.mkdir(parents=True, exist_ok=False)

    def run(pair):
        name, seed = pair
        folder = args.out / f"{name}-{seed}"
        with folder.with_suffix(".log").open("w") as log:
            status = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "tools/aisle_workbench/worker.py"),
                    "run",
                    "--robot",
                    "Nano",
                    "--controller",
                    str(PROOF / name / "controller.json"),
                    "--seed",
                    str(seed),
                    "--target-motion",
                    protocol["fixture"],
                    "--observations-only",
                    "--out",
                    str(folder),
                ],
                stdout=log,
                stderr=subprocess.STDOUT,
            )
        if status.returncode or not (folder / "result.json").is_file():
            return dict(
                program=name,
                seed=seed,
                error=f"Worker exit {status.returncode}. See {folder.name}.log",
            )
        r = read(folder / "result.json")
        assert r["requested"] == protocol["order"] and r["seed"] == seed
        assert r["controller_hash"] == sha(PROOF / name / "controller.json")
        return dict(
            program=name,
            seed=seed,
            controller_sha256=r["controller_hash"],
            task_success=r["task_success"],
            cubes={c: r["cubes"][c]["passed"] for c in r["requested"]},
            moving_grasp={
                c: r["moving_grasp"]["cubes"].get(c, {}).get("passed", False)
                for c in r["requested"]
            },
            motor_sha256=r["actuator_audit"]["sha256"],
            ticks=r["physics_ticks"],
            supported_ticks=r["wheel_supported_ticks"],
            actuator_audit=r["actuator_audit"]["passed"],
            max_joint_excursion_rad=r["max_joint_excursion_rad"],
            stop_reason=r["error"],
        )

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        rows = list(
            pool.map(
                run,
                [
                    (name, seed)
                    for seed in seeds
                    for name in ("baseline", "codex", "deepseek")
                ],
            )
        )
    (args.out / "scores.json").write_text(json.dumps(rows, indent=2) + "\n")
    if any("error" in row for row in rows):
        raise SystemExit("See scores.json for unfinished workers")


if __name__ == "__main__":
    main()
