"""Record the previous and corrected negative-slide Nano trials on the same evaluator."""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[5]
PREVIOUS = "b2de5f2c7a5ef8c3bf03f5a6976ba9db038329ae"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New output directory")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    controller = Path(__file__).with_name("controller.json").resolve()
    with tempfile.TemporaryDirectory() as work:
        previous = Path(work)
        shutil.copytree(
            ROOT / "tools",
            previous / "tools",
            ignore=shutil.ignore_patterns("__pycache__"),
        )
        (previous / "data").symlink_to(ROOT / "data", target_is_directory=True)
        for name in ("nano_rgb_policy.py", "nano_rgb_transfer_policy.py"):
            path = f"tools/aisle-validation/{name}"
            frozen = subprocess.check_output(
                ["git", "-C", ROOT, "show", f"{PREVIOUS}:{path}"]
            )
            (previous / path).write_bytes(frozen)
        for source, trial, expected in (
            (previous, "0001-Nano-run", False),
            (ROOT, "0002-Nano-run", True),
        ):
            folder = output / trial
            subprocess.run(
                [
                    sys.executable,
                    source / "tools/aisle_workbench/worker.py",
                    "run",
                    "--robot",
                    "Nano",
                    "--controller",
                    controller,
                    "--seed",
                    "0",
                    "--target-motion",
                    "slide-negative",
                    "--out",
                    folder,
                ],
                check=True,
            )
            verdict = json.loads((folder / "result.json").read_bytes())
            if verdict["task_success"] != expected:
                raise RuntimeError(
                    f"Unexpected measured outcome for {trial}. See {folder}/result.json"
                )


if __name__ == "__main__":
    main()
