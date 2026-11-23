"""Drive a running Studio via Makepad pointer events and verify retained AISLE results."""

import argparse
import json
from pathlib import Path
import subprocess
import time


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--remote", required=True)
    p.add_argument("--pid", required=True)
    p.add_argument("--report", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--screenshots", action="store_true")
    p.add_argument("--codex", action="store_true")
    p.add_argument("--size", choices=["1024x768", "1440x900"])
    p.add_argument(
        "--recover",
        action="store_true",
        help="Verify a previously stopped run after restarting Studio",
    )
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    checks = []

    def report():
        for _ in range(40):
            try:
                return json.loads(a.report.read_text())
            except (FileNotFoundError, json.JSONDecodeError):
                time.sleep(0.1)
        raise RuntimeError("Studio did not write an instrumentation report")

    def wait(test, label, seconds=30):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            j = report()
            if test(j):
                return j
            time.sleep(0.12)
        raise RuntimeError(f"{label}: {report()['state'].get('aisle')}")

    def remote(*args):
        result = subprocess.run(
            [a.remote, "app", a.pid, *args],
            capture_output=True,
            text=True,
            check=True,
            timeout=40,
        )
        for line in result.stdout.splitlines():
            response = json.loads(line)
            if "err" in response or response.get("verified") is False:
                raise RuntimeError(response)

    def click(key):
        j = wait(
            lambda j: j["controls"][key]["visible"]
            and j["controls"][key]["rect"][2] > 0
            and j["controls"][key]["enabled"] is not False,
            key,
        )
        # The probe writes during sync. Wait for Makepad to draw the new layout
        # before choosing screen coordinates for a newly enabled widget.
        for _ in range(8):
            prior = j["controls"][key]["rect"]
            time.sleep(0.12)
            j = report()
            if j["controls"][key]["rect"] == prior and j["controls"][key]["visible"]:
                break
        x, y, w, h = j["controls"][key]["rect"]
        remote("click", "w=0", f"x={x + w / 2}", f"y={y + h / 2}", "wait=1")

    def page(name):
        click("workspace_tabs.plugin_tabs.plugin_tab_aisle_" + name)
        wait(lambda j: j["state"]["aisle_panel"] == "aisle_" + name, name)

    def done(test, label, seconds=30):
        row = wait(lambda j: test(j["state"]["aisle"]), label, seconds)["state"][
            "aisle"
        ]
        checks.append(dict(check=label, robot=row["robot"], status=row["status"]))
        return row

    def grab(name):
        if a.screenshots:
            time.sleep(0.4)
            remote("grab", "w=0", "out=" + str((a.out / (name + ".png")).resolve()))

    if a.size:
        width, height = a.size.split("x")
        remote(
            "window", "k=resize", "width=" + width, "height=" + height, "w=0", "wait=1"
        )
    if a.recover:
        page("evidence")
        row = done(
            lambda w: w["result"] and w["result"].get("outcome") == "Stopped",
            "stopped run recovered on restart",
        )
        assert (Path(row["selected"]) / "stopped.json").exists()
        page("task")
        click("aisle_task.g1")
        done(
            lambda w: w["controller"] and w["controller"]["robot"] == "G1",
            "G1 controller restored",
        )
        (a.out / "recovery.json").write_text(json.dumps(checks, indent=2))
        return
    for robot, joints in [("Nano", 17), ("G1", 29)]:
        page("task")
        click("aisle_task." + robot.lower())
        done(lambda w: w["robot"] == robot, robot + " profile")
        if a.codex and robot == "G1":
            click("aisle_task.instruction")
            remote("key", "w=0", "c=KeyA", "logo=1", "wait=1")
            remote(
                "text",
                "w=0",
                "t=Inspect the right shoulder with a displacement of minus 0.08 rad",
                "wait=1",
            )
            click("aisle_task.codex")
        else:
            click("aisle_task.reference")
        row = done(
            lambda w: w["controller"]
            and w["controller"]["robot"] == robot
            and not w["busy"],
            robot + " prepare",
            180,
        )
        if a.codex and robot == "G1":
            assert (
                row["controller"]["provider"] == "codex"
                and row["controller"]["delta_rad"] < 0
            )
        grab(robot + "-task")
        page("scene")
        click("aisle_scene.inspect")
        done(
            lambda w: w["scene_joints"] == joints
            and not w["busy"]
            and w["status"].endswith("scene"),
            robot + " model",
        )
        if robot == "G1":
            click("aisle_scene.compliant")
            done(lambda w: w["compliant"], "G1 compliant fixture selected")
        grab(robot + "-scene")
        page("run")
        before = report()["state"]["aisle"]["selected"]
        click("aisle_run.start_run")
        row = done(
            lambda w: w["selected"] != before
            and w["result"]
            and w["result"]["robot"] == robot
            and not w["busy"],
            robot + " run",
            100,
        )
        path = Path(row["selected"])
        assert row["live"] == row["selected"]
        result = json.loads((path / "result.json").read_text())
        assert (
            result == row["result"]
            and result["task_success"]
            and result["within_limits"]
        )
        assert result["fixture"] == ("standard" if robot == "Nano" else "compliant")
        assert (
            len(json.loads((path / "trajectory.json").read_text())) == result["frames"]
            and (path / "frame.png").stat().st_size > 300
        )
        grab(robot + "-run")
        page("evidence")
        grab(robot + "-evidence")
        checks.append(dict(check=robot + " scored and retained", **result))
    click("aisle_evidence.previous")
    done(lambda w: w["result"] and w["result"]["robot"] == "Nano", "previous evidence")
    click("aisle_evidence.next")
    done(lambda w: w["result"] and w["result"]["robot"] == "G1", "next evidence")
    page("task")
    if a.codex:
        click("aisle_task.instruction")
        remote("key", "w=0", "c=KeyA", "logo=1", "wait=1")
        remote(
            "text",
            "w=0",
            "t=Repeat the previous trial on the same shoulder with three quarters of the previously requested signed displacement",
            "wait=1",
        )
        click("aisle_task.revise")
    else:
        click("aisle_task.reference")
    revision = done(
        lambda w: not w["busy"] and w["revision"] == 1, "G1 second revision", 180
    )
    if a.codex:
        plan = revision["controller"]
        assert plan["previous_trial"]["controller_hash"] == result["controller_hash"]
        assert plan["joint"] == result["joint"] and abs(plan["delta_rad"] + 0.06) < 1e-6
        page("run")
        before = revision["selected"]
        click("aisle_run.start_run")
        updated = done(
            lambda w: w["selected"] != before
            and not w["busy"]
            and w["result"].get("task_success"),
            "revised controller measured",
            100,
        )
        checks.append(dict(check="G1 evidence-informed revision", **updated["result"]))
        page("task")
    click("aisle_task.older")
    done(lambda w: w["revision"] == 0, "G1 earlier revision")
    click("aisle_task.newer")
    done(lambda w: w["revision"] == 1, "G1 later revision")
    page("run")
    click("aisle_run.start_run")
    click("aisle_run.stop_run")
    row = done(
        lambda w: not w["busy"] and w["result"].get("outcome") == "Stopped",
        "operator stop retained",
    )
    assert (Path(row["selected"]) / "stopped.json").is_file()
    page("evidence")
    (a.out / "journey.json").write_text(json.dumps(checks, indent=2))


if __name__ == "__main__":
    main()
