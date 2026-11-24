"""Replay Nano pickup observations, contact impulses and bounded motor setpoints."""

import argparse
import hashlib
import json
from pathlib import Path
import sys

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/aisle-validation"))

from aisle.sim.nexus_backend import quat_wxyz_to_matrix
from nano_actuator_audit import MotorEnvelope, replay
from nano_rgb_policy import CUBE_CENTRE_Z, VisualServo


def require(condition, message):
    if not condition:
        raise ValueError(message)


def jsonl(path):
    return [json.loads(line) for line in path.read_text().splitlines()]


def same(actual, expected, field):
    if actual is None or expected is None:
        require(actual is expected, field)
    else:
        np.testing.assert_allclose(actual, expected, atol=1e-7, rtol=0, err_msg=field)


def verify_optics(folder, color, samples, actions, trace, calibration):
    servo = VisualServo(calibration | {"color": color, "standoff_m": 0.45})
    require(len(actions) == len(samples) <= 8, "First descent observation count")
    last_point = None
    last_stamp = 0
    velocity = np.zeros(2)
    errors = []
    for index, (sample, action) in enumerate(zip(samples, actions)):
        tick = action["tick"]
        require(tick == sample["tick"], "Evaluator and policy observation ticks")
        stamp = action["stamp_ns"]
        with np.load(
            folder / "grasp-observations" / f"{tick:06}.npz", allow_pickle=False
        ) as packet:
            require(
                int(packet["stamp_ns"]) == stamp == tick * 10_000_000, "Packet time"
            )
            observed = servo.command(packet, preferred_camera="left_wrist")
        require(
            observed["cameras"] == action["cameras"] == sample["cameras"],
            "Camera identity",
        )
        point = observed["xy"]
        same(point, action["xy"], "Optical estimate")
        same(point, sample["xy"], "Evaluator copy of optical estimate")
        cube = np.asarray(trace[tick]["world_m"])
        rotation = quat_wxyz_to_matrix(sample["base_quat_wxyz"])
        truth = (rotation.T @ (cube - sample["base_m"]))[:2]
        same(truth, sample["truth_xy_m"], "Private ground truth")
        if point is not None:
            point = np.asarray(point)
            error = float(np.linalg.norm(point - truth))
            same(error, sample["error_m"], "Measured optical error")
            errors.append(error)
            if last_point is not None:
                dt = (stamp - last_stamp) / 1e9
                velocity = np.clip(
                    0.35 * velocity + 0.65 * (point - last_point) / dt, -0.06, 0.06
                )
            last_point, last_stamp = point, stamp
        else:
            require(sample["error_m"] is None, "Missing target error")
        age = (stamp - last_stamp) / 1e9
        require(last_point is not None and age <= 1.6, "Stale pickup estimate")
        same(
            velocity,
            action["velocity_xy_m_s"],
            "Velocity from consecutive RGB estimates",
        )
        prediction = last_point + velocity * (age + (0.8 if index == 7 else 0.5))
        same(prediction, action["intercept_xy_m"], "Policy intercept")
        same(prediction, sample["intercept_xy_m"], "Evaluator copy of intercept")
        height = 0.115 + (index + 1) * (CUBE_CENTRE_Z + 0.004 - 0.115) / 8
        same(height, action["height_m"], "Commanded descent height")
    return {
        "replayed": len(samples),
        "localized": len(errors),
        "max_error_m": max(errors, default=None),
    }


def verify_cube(
    folder, color, motion, phases, actions, calibration, direction, end_tick
):
    start = motion["start_tick"]
    first = next(
        i for i, p in enumerate(phases) if p["name"] == "pick" and p["color"] == color
    )
    require(start == phases[first]["tick"], "First pickup start")
    closed = first + 1 < len(phases)
    end = phases[first + 1]["tick"] if closed else end_tick
    rows = jsonl(folder / f"motion-{color}.jsonl")
    require(
        [r["tick"] for r in rows] == list(range(start + 1, end + 1)),
        "Incomplete first-attempt trace",
    )
    trace = {r["tick"]: r for r in rows}
    observations = [
        a
        for a in actions
        if a["op"] == "grasp observation"
        and a["color"] == color
        and start <= a["tick"] <= end
    ]
    optics = verify_optics(
        folder, color, motion["samples"], observations, trace, calibration
    )
    initial = np.asarray(motion["initial_world_m"])
    last = initial
    speed = 0.0
    contact = None
    loaded = commands = 0
    for row in rows:
        tick = row["tick"]
        current = np.asarray(row["world_m"])
        require(np.isfinite(current).all(), "Nonfinite position")
        expected = (
            contact is None
            and 160 <= tick - start - 1 <= 1000
            and direction * (last[1] - initial[1]) < 0.095
        )
        require(row["velocity_command"] is bool(expected), "Unexpected fixture command")
        commands += int(expected)
        impulses = row["finger_impulses_ns"]
        require(
            set(impulses) <= {"left_finger_a", "left_finger_b"}
            and all(np.isfinite(v) and v > 1e-6 for v in impulses.values()),
            "Finger impulse",
        )
        if contact is None and impulses:
            contact = {
                "tick": tick,
                "prior_velocity_y_m_s": speed,
                "displacement_y_m": last[1] - initial[1],
            }
        if (
            len(impulses) == 2
            and not row["other_support"]
            and current[2] - motion["floor_cube_center_z"] > 0.05
        ):
            loaded += 1
        speed = float((current[1] - last[1]) / 0.01)
        last = current
    require(commands == motion["commands"], "Fixture command count")
    recorded = motion["first_contact"]
    require((contact is None) == (recorded is None), "First contact missing")
    if contact is not None:
        for key, value in contact.items():
            same(value, recorded[key], key)
    require(
        motion["first_attempt_lift_ticks"] == (loaded if closed else None),
        "Opposing-finger elevated samples",
    )
    passed = bool(
        contact
        and closed
        and loaded >= 20
        and direction * contact["prior_velocity_y_m_s"] > 0.003
        and direction * contact["displacement_y_m"] > 0.012
    )
    require(motion["passed"] is passed, "Moving first-attempt result")
    return dict(
        passed=passed, first_contact=contact, elevated_opposed_ticks=loaded, **optics
    )


def verify_trial(folder):
    def read(name):
        return json.loads((folder / name).read_text())

    result, source, plan, calibration = map(
        read, ("result.json", "source.json", "controller.json", "calibration.json")
    )
    model = ROOT / "data/adora_nano/adora_nano.urdf"
    calibration["model"] = str(model)
    require(
        hashlib.sha256(model.read_bytes()).hexdigest()
        == plan["model_hash"]
        == result["model_hash"]
        == source["model_sha256"],
        "Nano URDF hash",
    )
    require(
        hashlib.sha256((folder / "controller.json").read_bytes()).hexdigest()
        == source["controller_sha256"]
        == result["controller_hash"],
        "Controller hash",
    )
    require(calibration["physics_dt_s"] == 0.01, "Nexus timestep")
    mode = result["moving_grasp"]["fixture"]
    require(source["grasp_motion"] == mode, "Fixture identity")
    direction = {"slide-positive": 1, "slide-negative": -1}[mode]
    ticks = result["physics_ticks"]
    motor = replay(
        folder / "motor-targets.f64",
        MotorEnvelope(model, calibration["joint_names"]),
        ticks,
    )
    require(motor == result["actuator_audit"], "Motor audit")
    actions = jsonl(folder / "actions.jsonl")
    phases = [a for a in actions if a["op"] == "phase"]
    require(phases == read("phases.json"), "Phase chronology")
    observed_colors = {p["color"] for p in phases if p["name"] == "pick"}
    motion = result["moving_grasp"]["cubes"]
    require(set(motion) == observed_colors, "Measured pickup attempts")
    cubes = {
        color: verify_cube(
            folder, color, value, phases, actions, calibration, direction, ticks
        )
        for color, value in motion.items()
    }
    requested = plan["color_order"].split(",")
    require(result["requested"] == requested, "Requested cubes")
    if plan.get("track_grasp"):
        for color, value in cubes.items():
            if value["passed"]:
                require(
                    value["replayed"] == 8 and value["localized"] > 0,
                    "Successful grasp requires replayable onboard exposures",
                )
    placements = {}
    for color, cube in result["cubes"].items():
        height = cube["final_position_m"][2] - result["table_top_m"]
        placements[color] = bool(
            cube["opposing_finger_airborne_ticks"] >= 20
            and cube["table_contact_tail_ticks"] == 100
            and cube["detached"]
            and cube["min_finger_clearance_tail_m"] > 0.001
            and cube["nearest_table_edge_m"] > 0.035
            and 0.015 <= height <= 0.037
            and cube["settle_motion_m"] < 0.003
        )
        require(placements[color] is cube["passed"], "Settled placement result")
    passed = bool(
        actions
        and actions[-1]["op"] == "finished"
        and not result["error"]
        and motor["passed"]
        and not result["rejected_motor_writes"]
        and not result["root_pose_writes"]
        and result["wheel_supported_ticks"] == ticks
        and result["max_joint_excursion_rad"] <= 0.005
        and not result["robot_furniture_contacts"]
        and all(placements.get(c) and cubes.get(c, {}).get("passed") for c in requested)
    )
    require(
        result["task_success"] is passed and result["passed"] is passed, "Task verdict"
    )
    failure = result.get("last_pickup_observation")
    diagnostic = {}
    if failure is not None:
        require(
            result["error"] and failure["stopped_tick"] == ticks, "Failed pickup stop"
        )
        require(
            failure["color"] in requested
            and failure["file"] == "last-pickup-observation.npz",
            "Failed pickup snapshot identity",
        )
        snapshot = folder / "last-pickup-observation.npz"
        require(
            hashlib.sha256(snapshot.read_bytes()).hexdigest() == failure["sha256"],
            "Failed pickup observation hash",
        )
        captures = [a["tick"] for a in actions if a["op"] == "sense"]
        require(
            captures and captures[-1] * 10_000_000 == failure["stamp_ns"],
            "Last observed pickup tick",
        )
        servo = VisualServo(
            calibration | {"color": failure["color"], "standoff_m": 0.45}
        )
        with np.load(snapshot, allow_pickle=False) as packet:
            require(
                int(packet["stamp_ns"]) == failure["stamp_ns"],
                "Failed pickup capture time",
            )
            reading = servo.command(packet, preferred_camera="left_wrist")
        diagnostic["last_pickup"] = {
            "color": failure["color"],
            "tick": captures[-1],
            "optical_state": reading["state"],
            "cameras": reading["cameras"],
            "xy": reading["xy"],
        }
    return dict(
        **diagnostic,
        passed=passed,
        seed=result["seed"],
        provider=plan["provider"],
        tracked=plan.get("track_grasp", False),
        fixture=mode,
        cubes=cubes,
        placements=placements,
        audited_ticks=ticks,
        motor_sha256=motor["sha256"],
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trial", type=Path)
    print(json.dumps(verify_trial(parser.parse_args().trial), indent=2))
