"""Exercise controller selection across robot changes, a Studio restart and real Nexus runs."""

import hashlib
import json
import sys
from pathlib import Path


def native_journey(args, checks, page, click, remote, done, grab):
    state = args.report.parent / "state/aisle"

    def simulate(robot, expected, label):
        page("run")
        before = done(lambda w: not w["busy"], label + " ready")["selected"]
        click("aisle_run.start_run")
        row = done(
            lambda w: not w["busy"]
            and w["selected"] != before
            and w["result"]
            and w["result"].get("robot") == robot,
            label,
            120,
        )
        trial = Path(row["selected"])
        result = json.loads((trial / "result.json").read_text())
        actual = hashlib.sha256((trial / "controller.json").read_bytes()).hexdigest()
        assert actual == result["controller_hash"] == expected["sha256"]
        assert (
            result["task_success"]
            and abs(result["requested_delta"] - expected["delta"]) < 1e-6
        )
        assert abs(result["measured_delta"] - expected["delta"]) < 0.01
        return dict(
            controller_sha256=actual,
            requested_rad=result["requested_delta"],
            measured_rad=result["measured_delta"],
            trial=trial.name,
        )

    def selected(robot, index, provider):
        row = done(
            lambda w: not w["busy"]
            and w["robot"] == robot
            and w["revision"] == index
            and (w["controller"] or {}).get("provider") == provider,
            f"{robot} revision {index + 1} selected",
            180,
        )
        pointer = json.loads((state / f"selected-{robot}-controller.json").read_text())
        raw = (state / pointer["revision"] / "controller.json").read_bytes()
        assert json.loads(raw) == row["controller"]
        assert hashlib.sha256(raw).hexdigest() == pointer["sha256"]
        return dict(**pointer, provider=provider, delta=json.loads(raw)["delta_rad"])

    page("task")
    if args.recover:
        record = json.loads((args.out / "journey.json").read_text())[-1]["profiles"]
        done(
            lambda w: w["robot"] == "Nano" and (w["result"] or {}).get("robot") == "G1",
            "Restored Nano Task alongside the last G1 trial",
        )
        for robot, data in record.items():
            page("task")
            if robot != "Nano":
                click("aisle_task.g1")
            restored = selected(robot, 0, data["earlier"]["provider"])
            assert restored == data["earlier"]
            grab(robot + "-restored-controller")
            run = simulate(robot, restored, robot + " restored controller measured")
            checks.append(
                dict(check="Measured restored controller", robot=robot, **run)
            )
        # Change only this disposable test workspace after the controller was loaded.
        # The local worker must check the bytes it actually reads before simulation.
        chosen = state / record["G1"]["earlier"]["revision"] / "controller.json"
        original = chosen.read_bytes()
        chosen.write_bytes(original + b" ")
        try:
            click("aisle_run.start_run")
            stopped = done(
                lambda w: not w["busy"]
                and "controller contents changed" in w["status"].lower(),
                "Changed G1 program rejected before simulation",
                60,
            )
            directory = Path(stopped["selected"])
            assert (directory / "error.json").is_file()
            artifacts = {
                name: (directory / name).exists()
                for name in ("trajectory.json", "result.json")
            }
            assert not any(artifacts.values())
            checks.append(
                dict(
                    check="Rejected changed controller",
                    trial=directory.name,
                    artifacts=artifacts,
                )
            )
        finally:
            chosen.write_bytes(original)
        (args.out / "recovery.json").write_text(json.dumps(checks, indent=2))
        return

    profiles = {}
    for robot in ("Nano", "G1"):
        page("task")
        click("aisle_task." + robot.lower())
        click("aisle_task.instruction")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        remote(
            "text",
            "w=0",
            "t=Inspect the left shoulder with a negative 0.06 radian movement.",
            "wait=1",
        )
        click("aisle_task." + ("codex" if args.provider == "codex" else "octos"))
        earlier = selected(robot, 0, args.provider)
        assert abs(earlier["delta"] + 0.06) < 1e-6
        click("aisle_task.reference")
        later = selected(robot, 1, "reference")
        assert later["delta"] > 0 and later["sha256"] != earlier["sha256"]
        measured = simulate(robot, later, robot + " reference controller measured")
        page("task")
        click("aisle_task.older")
        assert selected(robot, 0, args.provider) == earlier
        profiles[robot] = dict(earlier=earlier, later=later, measured=measured)
    click("aisle_task.nano")
    assert selected("Nano", 0, args.provider) == profiles["Nano"]["earlier"]
    checks.append(
        dict(check="Per-robot selected controllers before restart", profiles=profiles)
    )
    (args.out / "journey.json").write_text(json.dumps(checks, indent=2))
