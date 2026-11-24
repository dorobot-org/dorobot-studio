"""Verify actual left-wrist pixels, stale delivery and motor targets on a Nano pick."""

from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

import numpy as np
from PIL import Image


def audit(folder):
    def load(name):
        return json.loads((folder / name).read_bytes())

    result = load("result.json")
    timing = result["wrist_timing"]
    mode = timing["fixture"]
    rows = [
        json.loads(line)
        for line in (folder / "wrist-delivery.jsonl").read_bytes().splitlines()
    ]
    names = load("calibration.json")["joint_names"]
    assert len(names) == 17
    left_arm = [i for i, n in enumerate(names) if n.startswith("left_j")]
    assert len(left_arm) == 6
    raw = (folder / "motor-targets.f64").read_bytes()
    motors = np.frombuffer(raw, dtype="<f8").reshape(-1, len(names))
    assert hashlib.sha256(raw).hexdigest() == result["actuator_audit"]["sha256"]
    assert (
        motors.shape[0] == result["physics_ticks"] + 1
        and result["actuator_audit"]["passed"]
    )
    assert result["wheel_supported_ticks"] == result["physics_ticks"]
    assert result["root_pose_writes"] == result["rejected_motor_writes"] == 0
    sources = {r["tick"]: r["source_sha256"] for r in rows}
    assert len(sources) == len(rows) == timing["received"]
    last = -1
    faults = Counter()
    hold = resumed = None
    for r in rows:
        t, c = r["tick"], r["capture_tick"]
        assert c <= t and sources[c] == r["delivered_sha256"]
        assert (r["capture_ns"], r["receive_ns"]) == (c * 10_000_000, t * 10_000_000)
        cause = None
        if c <= last:
            cause = "repeated or out-of-order packet"
        elif t - c > 25:
            cause = "late packet"
        assert cause == r["fault"]
        if cause:
            faults[cause] += 1
            if hold is None:
                hold = t
                assert np.max(abs(motors[t, left_arm] - motors[t - 1, left_arm])) > 1e-6
        else:
            last = c
            if hold is not None and resumed is None:
                resumed = t
    for r in rows[:15]:
        path = folder / "wrist-exposures" / f"{r['tick']:07d}.png"
        with Image.open(path) as image:
            assert image.size == (480, 360) and image.mode == "RGB"
            assert hashlib.sha256(image.tobytes()).hexdigest() == r["source_sha256"]
    expected = {
        "normal": {},
        "intermittent": {"repeated or out-of-order packet": 3, "late packet": 5},
        "disconnected": {"repeated or out-of-order packet": 10},
    }
    assert faults == expected[mode] == timing["rejected"]
    assert timing["timed_out"] == (mode == "disconnected")
    assert timing["recoveries"] == int(mode == "intermittent")
    end = resumed if resumed is not None else len(motors) - 1
    if hold is not None:
        assert np.all(motors[hold : end + 1] == motors[hold])
    if resumed is not None:
        assert np.any(motors[resumed + 1, left_arm] != motors[resumed, left_arm])
    if mode == "disconnected":
        assert result["error"] == "Left wrist RGB timed out during pick"
        assert result["phase"] == "pick" and not result["cubes"]["red"]["passed"]
        assert not result["task_success"]
    elif result["requested"] == ["red"]:
        assert result["cubes"]["red"]["passed"] and result["task_success"]
    controller = hashlib.sha256((folder / "controller.json").read_bytes()).hexdigest()
    assert controller == result["controller_hash"]
    return dict(
        fixture=mode,
        controller=controller,
        task_success=result["task_success"],
        first_hold_tick=hold,
        resume_tick=resumed,
        held_ticks=end - hold if hold is not None else 0,
        motor_sha256=result["actuator_audit"]["sha256"],
        physics_ticks=result["physics_ticks"],
        rejected=dict(faults),
    )


def native_journey(args, checks, page, click, remote, done, grab):
    if args.recover:
        page("evidence")
        row = done(
            lambda w: w["result"]
            and w["result"].get("wrist_timing", {}).get("timed_out"),
            "Wrist timeout recovered after restart",
        )
        audit(Path(row["selected"]))
        (args.out / "recovery.json").write_text(json.dumps(checks, indent=2))
        return
    three = args.wrist_faults == "three"
    order = "red,green,blue" if three else "red"
    instruction = (
        "Move all three floor cubes onto the coffee table in red, green, blue order."
        if three
        else "Move the red floor cube onto the coffee table."
    )
    page("task")
    click("aisle_task.nano")
    click("aisle_task.instruction")
    remote(
        "key",
        "w=0",
        "c=KeyA",
        "logo=1" if sys.platform == "darwin" else "ctrl=1",
        "wait=1",
    )
    remote("text", "w=0", "t=" + instruction, "wait=1")
    click("aisle_task.codex" if args.provider == "codex" else "aisle_task.octos")
    done(
        lambda w: not w["busy"]
        and (w.get("controller") or {}).get("color_order") == order,
        f"Prepared transfer for {order}",
        180,
    )
    outcomes = []
    for mode, button in [
        ("normal", None),
        ("intermittent", "late_rgb"),
        ("disconnected", "frozen_rgb"),
    ]:
        page("scene")
        if button:
            click("aisle_scene." + button)
        state = done(
            lambda w: w["rgb_timing"] == mode, mode + " wrist delivery selected"
        )
        previous = state["selected"]
        page("run")
        click("aisle_run.start_run")
        state = done(
            lambda w: not w["busy"]
            and w["selected"] != previous
            and (w.get("result") or {}).get("wrist_timing", {}).get("fixture") == mode,
            mode + " wrist transfer measured",
            360,
        )
        outcome = audit(Path(state["selected"]))
        assert state["result"]["requested"] == order.split(",")
        assert outcome["task_success"] == (mode != "disconnected")
        outcomes.append(outcome)
        page("evidence")
        grab("wrist-" + mode)
    assert len({r["controller"] for r in outcomes}) == 1
    checks.append(
        dict(check="Wrist RGB freshness and recorded actuator holds", trials=outcomes)
    )
    (args.out / "journey.json").write_text(json.dumps(checks, indent=2))
