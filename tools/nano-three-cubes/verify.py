"""Verify all three physical outcomes from measured simulation trajectories."""

import argparse
import hashlib
import json
import xml.etree.ElementTree as ET
from pathlib import Path

import numpy as np


def verify(root):
    rows = json.loads((root / "trajectory.json").read_text())
    manifest = json.loads((root / "manifest.json").read_text())
    names = manifest["joint_names"]
    time = np.array([r["time_s"] for r in rows])
    base = np.array([r["base"] for r in rows])
    grip = np.array([r["grip"] for r in rows])
    joints = np.array([r["q"] for r in rows])
    tail = time >= time[-1] - 2
    gripper = names.index("left_j6_gripper")
    excess = 0.0
    for joint in ET.parse(root / "nano-planar.urdf").getroot().findall("joint"):
        if joint.get("name") not in names or joint.get("type") == "continuous":
            continue
        limit = joint.find("limit")
        values = joints[:, names.index(joint.get("name"))]
        excess = max(
            excess,
            float(limit.get("lower")) - values.min(),
            values.max() - float(limit.get("upper")),
        )
    checks = {
        "exactly_three_cubes": set(manifest["cube_names"]) == {"red", "green", "blue"},
        "monotonic_sample_clock": bool(np.all(np.diff(time) > 0)),
        "finite_robot_state": bool(
            all(np.isfinite(a).all() for a in (base, grip, joints))
        ),
        "joint_limits_respected": bool(excess < 0.03),
        "robot_navigated": bool(np.linalg.norm(base[-1, :2] - base[0, :2]) > 1),
        "gripper_open": bool(np.all(joints[tail, gripper] < -0.10)),
    }
    outcomes = {}
    for name in ("red", "green", "blue"):
        cube = np.array([r["cubes"][name] for r in rows])
        carried = np.array([r["phase"] == f"carry {name} to table" for r in rows])
        speed = np.linalg.norm(np.diff(cube[tail], axis=0), axis=1) / np.diff(
            time[tail]
        )
        tests = {
            "finite": bool(np.isfinite(cube).all()),
            "started_on_floor": bool(abs(cube[0, 2] - 0.02) < 0.005),
            "lifted_above_table": bool(cube[:, 2].max() > 0.44),
            "held_during_transport": bool(
                carried.any()
                and np.all(cube[carried, 2] > 0.42)
                and np.all(np.linalg.norm(cube[carried] - grip[carried], axis=1) < 0.06)
            ),
            "inside_table_edges": bool(
                np.all(abs(cube[tail, 0] - 0.55) < 0.415)
                and np.all(abs(cube[tail, 1] - 0.30) < 0.29)
            ),
            "resting_on_table": bool(np.all(abs(cube[tail, 2] - 0.40) < 0.007)),
            "stable_two_seconds": bool(len(speed) >= 35 and speed.max() < 0.015),
            "hand_withdrawn": bool(
                np.min(np.linalg.norm(grip[tail] - cube[tail], axis=1)) > 0.08
            ),
        }
        outcomes[name] = dict(
            ok=all(tests.values()),
            checks=tests,
            final_xyz_m=cube[-1].tolist(),
            peak_z_m=float(cube[:, 2].max()),
        )
    report = dict(
        ok=all(checks.values()) and all(v["ok"] for v in outcomes.values()),
        checks=checks,
        cubes=outcomes,
        max_joint_limit_excess_rad=float(excess),
        simulation_seconds=float(time[-1]),
        frames=len(rows),
        hardware_tested=False,
        source_model_sha256=manifest["model_sha256"],
        trajectory_sha256=hashlib.sha256(
            (root / "trajectory.json").read_bytes()
        ).hexdigest(),
    )
    (root / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    result = verify(parser.parse_args().run)
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["ok"] else 1)
