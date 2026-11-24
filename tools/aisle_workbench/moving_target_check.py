"""Replay Nano camera decisions and independently verify the moving exposures in Studio."""

import json
from pathlib import Path
import sys

import numpy as np

from sensor_timing_check import prepare_visual_approach

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
from verify_rgb_replay import replay


def audit(run, model, mode, delayed=False):
    result = json.loads((run / "result.json").read_bytes())
    rows = [json.loads(line) for line in (run / "trace.jsonl").read_text().splitlines()]
    decisions = replay(run, model)
    assert decisions == len(rows) == result["decisions"]
    assert result["target_motion"]["fixture"] == mode and result["task_success"] == (
        not delayed
    )
    assert result["wheel_supported_ticks"] == result["physics_ticks"]
    assert (
        result["obstacle_contact_ticks"] == 0
        and not result["sampled_joint_limit_violations"]
    )
    assert abs(result["standoff_m"] - result["final_distance_m"]) <= 0.035
    for row in rows:
        packet = run / "observations" / f"{row['receive_ns'] // 10_000_000:07d}.npz"
        with np.load(packet, allow_pickle=False) as exposure:
            assert set(exposure.files) == {
                "stamp_ns",
                "encoders",
                "head",
                "left_wrist",
                "right_wrist",
            }
            assert int(exposure["stamp_ns"]) == row["capture_ns"]
    if mode == "stationary":
        assert result["cube_motion_m"] < 0.005
    else:
        errors = []
        moving = 0
        for row in rows:
            sample = row["target_motion"]
            xyz = np.array(sample["world_m"])
            before = sample["world_one_tick_before_m"]
            if (
                before is not None
                and np.linalg.norm(xyz[:2] - before[:2]) / 0.01 > 0.02
            ):
                moving += 1
                if row["sensor_fault"] or row["capture_ns"] != row["receive_ns"]:
                    continue
                xy = np.array(row["command"]["xy"], dtype=float)
                assert xy.shape == (2,) and np.isfinite(xy).all()
                errors.append(float(np.linalg.norm(xy - row["evaluator_xy"])))
        m = result["target_motion"]
        dy = (
            rows[-1]["target_motion"]["world_m"][1]
            - rows[0]["target_motion"]["world_m"][1]
        )
        assert m["passed"] == (not delayed) and m["pulse_ticks"] == [
            76,
            96,
            116,
            136,
            156,
        ]
        assert moving == m["moving_exposures"] >= 3
        assert len(errors) == m["localized_moving_exposures"]
        if delayed:
            assert not errors and m["max_moving_error_m"] is None
            assert result["sensor_timing"]["recoveries"] == 1
        else:
            assert moving == len(errors) and np.isclose(
                max(errors), m["max_moving_error_m"]
            )
            assert max(errors) <= 0.035
        assert abs(dy - m["displacement_y_m"]) < 0.001
        assert (dy if mode == "slide-positive" else -dy) >= 0.10
    return {
        "motion": result["target_motion"],
        "camera_timing": result["sensor_timing"],
        "passed": result["task_success"],
        "controller_sha256": result["controller_hash"],
        "seed": result["seed"],
        "decisions_replayed": decisions,
        "final_distance_m": result["final_distance_m"],
        "grounded_steps": result["wheel_supported_ticks"],
        "robot_motion_m": result["motion_m"],
    }


def native_journey(args, checks, page, click, remote, done, grab):
    model = ROOT / "data/adora_nano/adora_nano.urdf"
    if args.recover:
        page("evidence")
        row = done(
            lambda w: (w["result"] or {}).get("target_motion", {}).get("fixture")
            == "slide-negative",
            "Last moving-target trial restored",
        )
        audit(Path(row["selected"]), model, "slide-negative")
        (args.out / "recovery.json").write_text(json.dumps(checks, indent=2))
        return
    prepare_visual_approach(args, page, click, remote, done)
    summaries = []
    for mode, button, delayed in [
        ("stationary", None, False),
        ("slide-positive", "slide_pos", False),
        ("slide-positive", None, True),
        ("slide-negative", "slide_neg", False),
    ]:
        page("scene")
        if button:
            click("aisle_scene." + button)
        if delayed or mode == "slide-negative":
            click("aisle_scene.late_rgb" if delayed else "aisle_scene.on_time")
        timing = "intermittent" if delayed else "normal"
        row = done(
            lambda w: w["target_motion"] == mode and w["rgb_timing"] == timing,
            mode + " fixture with " + timing + " camera timing",
        )

        if mode == "slide-positive" and not delayed:
            grab("moving-cube-scene")
        previous = row["selected"]
        page("run")
        click("aisle_run.start_run")
        if button:
            done(
                lambda w: w["busy"]
                and (w["progress"] or {}).get("target_motion") == mode
                and w["progress"].get("moving_exposures", 0) > 0,
                "Live onboard observation while target moves",
                120,
            )
            grab("moving-cube-run-" + mode)
        row = done(
            lambda w: not w["busy"]
            and w["selected"] != previous
            and (w["result"] or {}).get("target_motion", {}).get("fixture") == mode
            and w["result"]["sensor_timing"]["fixture"] == timing,
            mode + " trial finished",
            140,
        )
        summaries.append(audit(Path(row["selected"]), model, mode, delayed))
        page("evidence")
        grab("moving-cube-evidence-" + mode + "-" + timing)
    assert len({s["controller_sha256"] for s in summaries}) == 1
    assert len({s["seed"] for s in summaries}) == 1
    checks.append(
        dict(check="Matched stationary and sliding targets", trials=summaries)
    )
    (args.out / "journey.json").write_text(json.dumps(checks, indent=2))
