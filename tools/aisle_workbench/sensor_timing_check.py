"""Check delivered camera stamps, wheel braking and recovery in native Studio."""

from collections import Counter
import json
from pathlib import Path
import sys

import numpy as np


def audit(run, result):
    rows = [json.loads(line) for line in (run / "trace.jsonl").read_text().splitlines()]
    captures = sorted((run / "observations").glob("*.npz"))
    assert len(captures) == len(rows) == result["decisions"]
    faults = Counter()
    last_accepted = -1
    last_rates = np.zeros(3)
    halted = None
    recovered = None
    for packet, row in zip(captures, rows, strict=True):
        with np.load(packet, allow_pickle=False) as actual:
            captured = int(actual["stamp_ns"])
        received = int(packet.stem) * 10_000_000
        assert (captured, received) == (row["capture_ns"], row["receive_ns"])
        assert 0 <= captured <= received
        cause = None
        if captured <= last_accepted:
            cause = "repeated or out-of-order packet"
        elif received - captured > 250_000_000:
            cause = "late packet"
        assert row["sensor_fault"] == cause
        rates = np.asarray(row["wheels"])
        if cause:
            faults[cause] += 1
            assert row["command"]["speed"] == row["command"]["yaw_rate"] == 0
            np.testing.assert_allclose(
                rates, np.sign(last_rates) * np.maximum(np.abs(last_rates) - 1, 0)
            )
            if np.all(rates == 0) and halted is None:
                halted = received
        else:
            last_accepted = captured
            if halted is not None and recovered is None and np.any(rates != 0):
                recovered = received
        last_rates = rates
    timing = result["sensor_timing"]
    assert faults == timing["rejected"]
    assert result["wheel_supported_ticks"] == result["physics_ticks"]
    assert (
        not result["obstacle_contact_ticks"]
        and not result["sampled_joint_limit_violations"]
    )
    expected = {
        "normal": ({}, True, 0),
        "intermittent": (
            {"repeated or out-of-order packet": 3, "late packet": 5},
            True,
            1,
        ),
        "disconnected": ({"repeated or out-of-order packet": 10}, False, 0),
    }
    errors, passed, recoveries = expected[timing["fixture"]]
    assert errors == faults and result["task_success"] == passed
    assert timing["recoveries"] == recoveries and timing["timed_out"] == (not passed)
    if faults:
        assert halted is not None and (recovered is not None) == passed
    if passed:
        assert abs(result["final_distance_m"] - result["standoff_m"]) <= 0.035
    return dict(
        fixture=timing["fixture"],
        rejected=errors,
        stopped_at_ns=halted,
        resumed_at_ns=recovered,
        controller_hash=result["controller_hash"],
        final_distance_m=result["final_distance_m"],
        physics_ticks=result["physics_ticks"],
        success=passed,
        recoveries=recoveries,
    )


def native_journey(args, checks, page, click, remote, done, grab):
    if args.recover:
        page("evidence")
        row = done(
            lambda w: w["result"]
            and w["result"].get("sensor_timing", {}).get("timed_out"),
            "Camera timeout retained after restart",
        )
        assert not row["result"]["task_success"]
        (args.out / "recovery.json").write_text(json.dumps(checks, indent=2))
        return
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
    remote(
        "text",
        "w=0",
        "t=Approach the red floor cube and stop at 0.45 metres using the onboard cameras.",
        "wait=1",
    )
    click(
        "aisle_task.octos" if args.provider == "octos-deepseek" else "aisle_task.codex"
    )
    done(
        lambda w: not w["busy"]
        and w["controller"]
        and w["controller"].get("kind") == "visual-approach",
        "Visual approach prepared",
        180,
    )
    summaries = []
    for mode, button in [
        ("normal", None),
        ("intermittent", "late_rgb"),
        ("disconnected", "frozen_rgb"),
    ]:
        page("scene")
        if button:
            click("aisle_scene." + button)
        row = done(lambda w: w["rgb_timing"] == mode, mode + " camera timing selected")
        if mode == "intermittent":
            grab("camera-timing-scene")
        previous = row["selected"]
        page("run")
        click("aisle_run.start_run")
        row = done(
            lambda w: not w["busy"]
            and w["selected"] != previous
            and w["result"]
            and w["result"].get("sensor_timing", {}).get("fixture") == mode,
            mode + " camera timing measured",
            150,
        )
        summaries.append(audit(Path(row["selected"]), row["result"]))
        page("evidence")
        grab("camera-timing-" + mode)
    assert len({s["controller_hash"] for s in summaries}) == 1
    checks.append(
        dict(check="Matched Nano camera delivery conditions", trials=summaries)
    )
    (args.out / "journey.json").write_text(json.dumps(checks, indent=2))
