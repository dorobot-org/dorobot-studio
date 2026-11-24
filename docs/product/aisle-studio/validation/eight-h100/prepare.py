"""Prepare a separate real provider program for each worker before the fleet run."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[5]
TASK = ("Revise the red floor cube to coffee table transfer after the recorded wheel-support "
        "loss. Limit forward speed to 0.080 m/s and preserve the original physical acceptance criteria.")


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--provider", choices=("codex", "octos-deepseek"), required=True)
    cli.add_argument("--feedback", type=Path, required=True)
    cli.add_argument("--out", type=Path, required=True)
    args = cli.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    start = time.monotonic_ns()

    def author(index):
        folder = args.out / f"{index + 1:04d}-Nano-prepare"
        command = [sys.executable, str(ROOT / "tools/aisle_workbench/worker.py"), "prepare",
                   "--robot", "Nano", "--provider", args.provider, "--task", TASK, "--feedback",
                   str(args.feedback), "--out", str(folder)]
        with (args.out / f"author-{index}.log").open("w") as log:
            process = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=300)
        row = dict(worker=index, exit=process.returncode)
        if process.returncode == 0:
            program = (folder / "controller.json").read_bytes()
            row.update(provider=json.loads((folder / "provider-metrics.json").read_bytes()),
                       sha256=hashlib.sha256(program).hexdigest(), program=json.loads(program))
        return row

    with ThreadPoolExecutor(max_workers=3) as pool:
        rows = list(pool.map(author, range(8)))
    (args.out / "results.json").write_text(json.dumps(dict(start_ns=start, end_ns=time.monotonic_ns(),
                                                           authors=rows), indent=2) + "\n")
    if not all(r["exit"] == 0 and r["program"].get("max_forward_speed_m_s") == 0.08
               and r["program"]["color_order"] == "red" for r in rows):
        sys.exit("One or more preparations failed the declared controller contract")


if __name__ == "__main__":
    main()
