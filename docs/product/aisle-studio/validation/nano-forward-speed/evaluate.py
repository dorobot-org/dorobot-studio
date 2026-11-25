"""Run the frozen baseline and selected Nano controller on each disclosed layout."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

PROOF = Path(__file__).resolve().parent
WORKER = PROOF.parents[4] / "tools/aisle_workbench/worker.py"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    parser.add_argument(
        "--seed", type=int, help="Run just one of the disclosed layouts"
    )
    args = parser.parse_args()
    protocol = json.loads((PROOF / "protocol.json").read_bytes())
    disclosure = (PROOF / "disclosure.json").read_bytes()
    assert hashlib.sha256(disclosure).hexdigest() == protocol["disclosure_sha256"]
    seeds = json.loads(disclosure)["seeds"]
    assert len(seeds) == len(set(seeds)) == 3 and 0 not in seeds
    if args.seed is not None:
        if args.seed not in seeds:
            parser.error("Choose a disclosed layout")
        seeds = [args.seed]
    selected = json.loads((PROOF / "development.json").read_bytes())
    hashes = {
        "baseline": protocol["reference_controller_sha256"],
        "selected": selected["controller_sha256"],
    }
    args.out.mkdir(parents=True, exist_ok=False)
    for seed in seeds:
        for tag, digest in hashes.items():
            controller = PROOF / (tag + "-controller.json")
            assert hashlib.sha256(controller.read_bytes()).hexdigest() == digest
            trial = args.out / f"{tag}-{seed}"
            with trial.with_suffix(".log").open("w") as log:
                subprocess.run(
                    [
                        sys.executable,
                        str(WORKER),
                        "run",
                        "--robot",
                        "Nano",
                        "--seed",
                        str(seed),
                        "--controller",
                        str(controller),
                        "--observations-only",
                        "--out",
                        str(trial),
                    ],
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    check=True,
                )
            result = json.loads((trial / "result.json").read_bytes())
            assert result["controller_hash"] == digest and result["seed"] == seed
            print(f"{tag} {seed} {result['task_success']}", flush=True)


if __name__ == "__main__":
    main()
