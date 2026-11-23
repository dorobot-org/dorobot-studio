"""Freeze and evaluate a Nano controller on predeclared Nexus floor layouts."""

import argparse
import fcntl
import hashlib
from importlib.metadata import version
import json
import os
from pathlib import Path
import secrets
import signal
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

from aisle import kinematics
from aisle.sim import nexus_backend
from nano_actuator_audit import MotorEnvelope, replay
from run_rgb_transfer import COLORS, HERE, MODEL, RECIPE, worker


def read(path):
    return json.loads(path.read_text())


def signature(value):
    raw = json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)
    return hashlib.sha256(raw.encode()).hexdigest()


def inputs():
    paths = [
        HERE / name
        for name in (
            "nano_transfer_suite.py",
            "run_rgb_transfer.py",
            "nano_rgb_transfer_policy.py",
            "nano_rgb_policy.py",
            "nano_rgb_table.py",
            "nano_actuator_audit.py",
            "nano_living.py",
            "nano_visuals.py",
            "nano_cameras.py",
            "run_rgb_approach.py",
            "verify_arm_reach.py",
            "nano_reach_physics.py",
        )
    ]
    paths += [HERE.parent / "nano-three-cubes/model.py", Path(worker.__file__)]
    meshes = sorted({m.attrib["filename"] for m in ET.parse(MODEL).iter("mesh")})
    adapter = Path(nexus_backend.__file__).resolve()
    return dict(
        recipe=RECIPE,
        urdf=worker.digest(MODEL),
        mesh_inventory_sha256=signature(
            {m: worker.digest(MODEL.parent / m) for m in meshes}
        ),
        studio_files={str(p.relative_to(worker.ROOT)): worker.digest(p) for p in paths},
        aisle_files={
            p.name: worker.digest(p)
            for p in (
                Path(kinematics.__file__),
                adapter,
                adapter.with_name("nexus_physics.toml"),
            )
        },
        packages={
            n: version(n)
            for n in (
                "dimforge-nexus3d",
                "numpy",
                "scipy",
                "opencv-python-headless",
                "Pillow",
            )
        },
        python=sys.version.split()[0],
    )


def prepare(directory, controller, seeds=None):
    model = worker.describe("Nano")
    plan = read(controller)
    if (
        plan.get("model_hash") != model["model_hash"]
        or plan.get("robot") != "Nano"
        or worker.validate(plan, model).get("kind") != "table-transfer"
        or set(plan["color_order"].split(",")) != set(COLORS)
    ):
        raise ValueError("Select a prepared three-cube Nano table-transfer controller")
    hidden = seeds is None
    if hidden:
        seeds = secrets.SystemRandom().sample(range(1 << 16, 1 << 32), 50)
    if not seeds or len(set(seeds)) != len(seeds) or min(seeds) < 0:
        raise ValueError("Layouts must have distinct nonnegative seeds")
    members = [dict(seed=seed, salt=secrets.token_hex(24)) for seed in seeds]
    public = dict(
        version=1,
        split="hidden" if hidden else "development",
        cases=len(seeds),
        success_requirement=45 if hidden else None,
        required_guard_bypasses=0,
        controller_sha256=worker.digest(controller),
        color_order=plan["color_order"].split(","),
        inputs=inputs(),
        commitments=[signature(member) for member in members],
    )
    directory.mkdir(parents=True, exist_ok=False)
    with open(
        directory / "private-layouts.json",
        "x",
        opener=lambda p, flags: os.open(p, flags, 0o600),
    ) as file:
        json.dump(members, file)
    (directory / "controller.json").write_bytes(controller.read_bytes())
    worker.save(directory / "suite.json", public)
    return dict(
        manifest_sha256=worker.digest(directory / "suite.json"),
        split=public["split"],
        cases=len(seeds),
    )


def cube_passed(cube, table_height):
    height = cube["final_position_m"][2]
    return (
        cube["opposing_finger_airborne_ticks"] >= 20
        and cube["table_contact_tail_ticks"] == 100
        and cube["detached"]
        and cube["min_finger_clearance_tail_m"] > 0.001
        and cube["nearest_table_edge_m"] > 0.035
        and table_height + 0.015 <= height <= table_height + 0.037
        and cube["settle_motion_m"] < 0.003
    )


def receipt(case, member, plan):
    if not (case / "result.json").is_file():
        return dict(
            passed=False, verified=False, issue="Missing complete physics result"
        )
    r, src, calib = (
        read(case / name) for name in ("result.json", "source.json", "calibration.json")
    )
    files = plan["inputs"]["studio_files"]
    expected_policy = files["tools/aisle-validation/nano_rgb_transfer_policy.py"]
    if (
        r["requested"] != plan["color_order"]
        or r["seed"] != member["seed"]
        or src["seed"] != member["seed"]
        or src["recipe"] != plan["inputs"]["recipe"]
        or src["model_sha256"] != plan["inputs"]["urdf"]
        or src["policy_sha256"] != expected_policy
        or calib["colors"] != r["requested"]
    ):
        raise ValueError("Controller, scene or seed differs from preregistration")
    retained = src["support_code_hashes"]
    expected_files = {Path(path).name: value for path, value in files.items()}
    for name in (
        "nano_rgb_table.py",
        "nano_actuator_audit.py",
        "nano_visuals.py",
        "verify_arm_reach.py",
        "nano_rgb_policy.py",
        "nano_living.py",
        "nano_cameras.py",
        "run_rgb_approach.py",
        "model.py",
        "run_rgb_transfer.py",
    ):
        if retained.get(name) != expected_files[name]:
            raise ValueError(f"Episode used a different {name}")
    audit = replay(
        case / "motor-targets.f64",
        MotorEnvelope(MODEL, calib["joint_names"]),
        r["physics_ticks"],
    )
    if audit != r["actuator_audit"]:
        raise ValueError("Retained motor data differs from the trial receipt")
    bypasses = (
        audit["invalid_rows"]
        + audit["unmatched_ticks"]
        + int(not audit["initial_targets_valid"])
        + r["root_pose_writes"]
    )
    if any(
        cube["passed"] != cube_passed(cube, r["table_top_m"])
        for cube in r["cubes"].values()
    ):
        raise ValueError("Cube measurements differ from recorded verdict")
    scored = (
        set(r["cubes"]) == set(COLORS)
        and all(c["passed"] for c in r["cubes"].values())
        and r["wheel_supported_ticks"] == r["physics_ticks"]
        and not r["robot_furniture_contacts"]
        and r["max_joint_excursion_rad"] <= 0.005
        and not r["error"]
        and not r["rejected_motor_writes"]
        and bypasses == 0
    )
    if r["passed"] and not scored:
        raise ValueError("Inconsistent success receipt")
    return dict(
        passed=bool(r["passed"] and scored),
        verified=True,
        guard_bypasses=bypasses,
        rejected_motor_writes=r["rejected_motor_writes"],
        physics_ticks=r["physics_ticks"],
        motor_sha256=audit["sha256"],
        actions_sha256=worker.digest(case / "actions.jsonl"),
        result_sha256=worker.digest(case / "result.json"),
        calibration_sha256=worker.digest(case / "calibration.json"),
        source_sha256=worker.digest(case / "source.json"),
        error=r["error"],
        cubes=r["cubes"],
        robot_furniture_contacts=r["robot_furniture_contacts"],
    )


def summarize(directory, manifest_sha, plan, members):
    rows = []
    for index, member in enumerate(members):
        case = directory / f"case-{index:03}"
        try:
            row = receipt(case, member, plan)
        except (KeyError, TypeError, OSError, ValueError) as exc:
            row = dict(passed=False, verified=False, issue=str(exc))
        rows.append(dict(index=index, **member, **row))
    successes = sum(r["passed"] for r in rows)
    verified = sum(r["verified"] for r in rows)
    bypasses = sum(r.get("guard_bypasses", 0) for r in rows)
    qualified = (
        plan["split"] == "hidden"
        and len(rows) == 50
        and verified == 50
        and successes >= 45
        and bypasses == 0
    )
    report = dict(
        manifest_sha256=manifest_sha,
        split=plan["split"],
        cases=len(rows),
        successes=successes,
        verified=verified,
        verified_guard_bypasses=bypasses,
        qualified=qualified,
        rows=rows,
    )
    worker.save(directory / "report.json", report)
    return {k: v for k, v in report.items() if k != "rows"}


def run(directory, expected, jobs):
    manifest_sha = worker.digest(directory / "suite.json")
    plan = read(directory / "suite.json")
    members = read(directory / "private-layouts.json")
    if (
        expected != manifest_sha
        or worker.digest(directory / "controller.json") != plan["controller_sha256"]
        or [signature(m) for m in members] != plan["commitments"]
        or len(members) != plan["cases"]
        or inputs() != plan["inputs"]
    ):
        raise ValueError("Suite commitment or frozen inputs changed")
    with (directory / "run.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        # A started case consumes its slot even when interrupted. Never replace a failed attempt.
        todo = [
            i
            for i in range(len(members))
            if not (directory / f"case-{i:03}.log").exists()
        ]
        active = {}
        try:
            while todo or active:
                while todo and len(active) < jobs:
                    i = todo.pop(0)
                    case = directory / f"case-{i:03}"
                    cmd = [
                        sys.executable,
                        str(HERE / "run_rgb_transfer.py"),
                        "--seed",
                        str(members[i]["seed"]),
                        "--colors",
                        *plan["color_order"],
                        "--out",
                        str(case),
                    ]
                    with case.with_suffix(".log").open("x") as log:
                        active[i] = subprocess.Popen(
                            cmd,
                            stdout=log,
                            stderr=subprocess.STDOUT,
                            start_new_session=True,
                            pass_fds=(lock.fileno(),),
                        )
                for i, process in list(active.items()):
                    if process.poll() is not None:
                        del active[i]
                if active:
                    time.sleep(1)
        finally:
            for process in active.values():
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
            for process in active.values():
                process.wait()
        if inputs() != plan["inputs"]:
            raise ValueError("Inputs changed during the run")
        return summarize(directory, manifest_sha, plan, members)


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    commands = cli.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare")
    prep.add_argument("--controller", type=Path, required=True)
    prep.add_argument("--development-seeds", type=int, nargs="+")
    execute = commands.add_parser("run")
    execute.add_argument("--manifest-sha256", required=True)
    execute.add_argument("--jobs", type=int, default=1)
    for cmd in (prep, execute):
        cmd.add_argument("--suite", type=Path, required=True)
    args = cli.parse_args()
    if args.command == "prepare":
        result = prepare(args.suite, args.controller, args.development_seeds)
    else:
        if args.jobs < 1:
            cli.error("--jobs must be positive")
        result = run(args.suite.resolve(), args.manifest_sha256, args.jobs)
    print(json.dumps(result, indent=2))
    if args.command == "run":
        return int(
            result["verified"] != result["cases"]
            or (result["split"] == "hidden" and not result["qualified"])
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
