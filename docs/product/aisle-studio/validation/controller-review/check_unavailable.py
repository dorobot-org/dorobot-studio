"""Exercise missing-parent recovery and post-selection tampering in a fresh native workspace."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time


def main(args):
    destination = args.out.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    root = destination / "state/aisle"
    root.mkdir(parents=True)

    # Inputs are actual recorded programs. All modifications are confined to these copies.
    def install(name, source):
        path = root / name / "controller.json"
        path.parent.mkdir()
        shutil.copyfile(source, path)
        return path

    install("01-G1-prepare", args.unrelated)
    selected = install("02-G1-prepare", args.revision)
    plan = json.loads(selected.read_bytes())
    assert plan["robot"] == "G1"
    parent = hashlib.sha256(args.parent.read_bytes()).hexdigest()
    assert parent == plan["previous_trial"]["controller_hash"]
    assert hashlib.sha256(args.unrelated.read_bytes()).hexdigest() != parent
    (root / "task-robot.json").write_text(json.dumps("G1"))
    report = destination / "ui.json"
    repo = Path(__file__).resolve().parents[5]
    env = os.environ | dict(
        DOROBOT_STUDIO_SOURCE_ROOT=str(repo),
        MAKEPAD_HAL_BACKEND="headless",
        DOROBOT_UX_REPORT=str(report),
        DOROBOT_STUDIO_STATE_DIR=str(root.parent),
    )
    with (destination / "native.log").open("w") as log:
        app = subprocess.Popen(
            [args.studio, "--remote", "--screen", "aisle-task"],
            env=env,
            stdout=log,
            stderr=log,
        )

        def wait(test):
            until = time.monotonic() + 35
            ui = {}
            while time.monotonic() < until and app.poll() is None:
                if report.exists():
                    ui = json.loads(report.read_bytes())
                    if test(ui):
                        return ui
                time.sleep(0.1)
            raise RuntimeError(str(ui.get("state", {}).get("aisle", {}).get("status")))

        def button():
            control = "aisle_task.review_controller"
            row = wait(
                lambda u: u["controls"][control]["enabled"]
                and u["controls"][control]["rect"][2] > 0
            )
            x, y, width, height = row["controls"][control]["rect"]
            result = subprocess.check_output(
                [
                    args.remote,
                    "app",
                    str(app.pid),
                    "click",
                    "w=0",
                    f"x={x+width/2}",
                    f"y={y+height/2}",
                    "wait=1",
                ],
                text=True,
            )
            assert all("err" not in json.loads(line) for line in result.splitlines())

        def state(ui):
            return ui["state"]["aisle"]

        try:
            wait(lambda ui: state(ui)["controller"] == plan)
            button()
            first = state(
                wait(
                    lambda ui: f"Parent {parent[:12]} is unavailable"
                    in state(ui)["status"]
                )
            )
            assert first["controller_diff"] is None and first["controller"] == plan
            install("03-G1-run", args.parent)
            button()
            second = state(
                wait(
                    lambda ui: (state(ui)["controller_diff"] or {})
                    .get("before", {})
                    .get("sha256")
                    == parent
                )
            )
            diff = second["controller_diff"]
            button()
            wait(lambda ui: state(ui)["controller_diff"] is None)
            with selected.open("ab") as output:
                output.write(b"\n")
            button()
            last = state(
                wait(
                    lambda ui: "Selected controller contents changed"
                    in state(ui)["status"]
                )
            )
            assert last["controller_diff"] is None and last["controller"] == plan
            proof = dict(
                missing=first["status"],
                restored_parent=diff["before"]["sha256"],
                selected=diff["after"]["sha256"],
                changed=last["status"],
            )
            (destination / "checks.json").write_text(json.dumps(proof, indent=2) + "\n")
        finally:
            app.terminate()
            try:
                app.wait(timeout=15)
            except subprocess.TimeoutExpired:
                app.kill()
                app.wait(timeout=10)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for arg in ("studio", "remote", "parent", "revision", "unrelated", "out"):
        p.add_argument("--" + arg, type=Path, required=True)
    main(p.parse_args())
