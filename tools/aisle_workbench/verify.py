"""Launch a fresh native Studio, exercise the pages and verify evidence after restart."""

import argparse
import os
from pathlib import Path
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--studio", type=Path, required=True)
    parser.add_argument("--remote", default="makepad-remote")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--backend", choices=["headless", "offscreen"], default="headless"
    )
    parser.add_argument("--size", choices=["1024x768", "1440x900"], default="1440x900")
    parser.add_argument("--dark", action="store_true")
    parser.add_argument("--codex", action="store_true")
    args = parser.parse_args()
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = output / "ui.json"
    env = os.environ | {
        "MAKEPAD_HAL_BACKEND": args.backend,
        "DOROBOT_UX_REPORT": str(report),
        "DOROBOT_STUDIO_STATE_DIR": str(output / "state"),
    }
    for stage in ["journey", "recovery"]:
        report.unlink(missing_ok=True)
        command = [str(args.studio.resolve()), "--remote", "--screen", "aisle-task"]
        if args.dark:
            command.append("--dark")
        with (output / f"{stage}.log").open("w") as log:
            process = subprocess.Popen(
                command, env=env, stdout=log, stderr=subprocess.STDOUT
            )
            try:
                deadline = time.monotonic() + 30
                while not report.exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError(f"Studio failed to start. See {log.name}")
                    time.sleep(0.1)
                check = [
                    sys.executable,
                    str(Path(__file__).with_name("journey.py")),
                    "--remote",
                    args.remote,
                    "--pid",
                    str(process.pid),
                    "--report",
                    str(report),
                    "--out",
                    str(output / "verified"),
                    "--size",
                    args.size,
                ]
                if stage == "recovery":
                    check.append("--recover")
                else:
                    if args.codex:
                        check.append("--codex")
                    if args.backend == "offscreen":
                        check.append("--screenshots")
                subprocess.run(check, check=True, timeout=330)
            finally:
                if process.poll() is None:
                    process.terminate()
                process.wait(timeout=15)


if __name__ == "__main__":
    main()
