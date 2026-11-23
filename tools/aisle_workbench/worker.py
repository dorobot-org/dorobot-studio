"""Reference inspection backed by the AISLE Nexus physics adapter."""

import argparse
import hashlib
import json
import math
import subprocess
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODELS = {
    "Nano": ("adora_nano/adora_nano.urdf", "left_j2_pitch", "right_j2_pitch"),
    "G1": ("g1/g1.urdf", "left_shoulder_pitch_joint", "right_shoulder_pitch_joint"),
}
REFERENCE = "Inspect the left shoulder with a small positive joint movement."


def save(path, value):
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(value, indent=2, allow_nan=False))
    tmp.replace(path)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def describe(robot):
    relative, left, right = MODELS[robot]
    model = ROOT / "data" / relative
    tree = ET.parse(model)
    meshes = sorted({m.attrib["filename"] for m in tree.iter("mesh")})
    missing = [m for m in meshes if not (model.parent / m).is_file()]
    if missing:
        raise ValueError(
            f"Missing {len(missing)} meshes for {robot}, including {missing[0]}. Run tools/fetch_robot_meshes.py g1 for G1."
        )
    limits = {}
    for joint in tree.findall("joint"):
        if joint.get("type") == "revolute":
            lim = joint.find("limit").attrib
            limits[joint.get("name")] = [float(lim["lower"]), float(lim["upper"])]
    return dict(
        robot=robot,
        model=str(model),
        model_hash=digest(model),
        mesh_count=len(meshes),
        joint_count=sum(j.get("type") != "fixed" for j in tree.findall("joint")),
        limits=limits,
        allowed_joints=[left, right],
    )


def validate(plan, meta):
    joint = plan["joint"]
    if joint not in meta["allowed_joints"]:
        raise ValueError("Select a shoulder belonging to this robot")
    delta = plan["delta_rad"]
    if (
        type(delta) not in (float, int)
        or not math.isfinite(delta)
        or not 0.03 <= abs(delta) <= 0.15
    ):
        raise ValueError(
            "Inspection displacement must be between 0.03 and 0.15 rad in magnitude"
        )
    if not meta["limits"][joint][0] <= delta <= meta["limits"][joint][1]:
        raise ValueError("Target outside the robot joint limits")
    return dict(joint=joint, delta_rad=delta)


def measured_feedback(path, meta):
    previous = json.loads((path / "result.json").read_text())
    parent = path / "controller.json"
    if (
        previous.get("robot") != meta["robot"]
        or previous.get("model_hash") != meta["model_hash"]
        or previous.get("controller_hash") != digest(parent)
    ):
        raise ValueError(
            "The selected run and controller must match the current robot model"
        )
    return {
        k: previous[k]
        for k in (
            "robot",
            "joint",
            "requested_delta",
            "measured_delta",
            "within_limits",
            "task_success",
            "fixture",
            "controller_hash",
        )
    }


def prepare(args, meta):
    feedback = measured_feedback(args.feedback, meta) if args.feedback else None
    if args.provider == "reference":
        if args.task.strip() != REFERENCE:
            raise ValueError(
                "Use Codex for custom instructions or restore the reference task"
            )
        plan = dict(joint=meta["allowed_joints"][0], delta_rad=0.10)
    else:
        schema = dict(
            type="object",
            additionalProperties=False,
            required=["joint", "delta_rad", "supported"],
            properties=dict(
                joint=dict(type="string", enum=meta["allowed_joints"]),
                delta_rad=dict(type="number"),
                supported=dict(type="boolean"),
            ),
        )
        save(args.out / "schema.json", schema)
        prompt = (
            "Translate the instruction into one supported fixed-base simulated shoulder inspection. "
            "Use radians, displacement magnitude 0.03–0.15. Set supported=false for unrelated or ambiguous "
            "tasks. No other motion or tools. Robot: "
            + args.robot
            + ". Joint limits: "
            + json.dumps({j: meta["limits"][j] for j in meta["allowed_joints"]})
            + ". Instruction: "
            + args.task
        )
        if feedback:
            prompt += "\nPrevious measured trial to revise: " + json.dumps(feedback)
            save(args.out / "feedback.json", feedback)
        with tempfile.TemporaryDirectory() as empty:
            subprocess.run(
                [
                    "codex",
                    "exec",
                    "--ephemeral",
                    "--skip-git-repo-check",
                    "--sandbox",
                    "read-only",
                    "--output-schema",
                    str(args.out / "schema.json"),
                    "-o",
                    str(args.out / "reply.json"),
                    "-",
                ],
                cwd=empty,
                input=prompt,
                text=True,
                check=True,
                timeout=150,
            )
        plan = json.loads((args.out / "reply.json").read_text())
        if plan.get("supported") is not True:
            raise ValueError("Requested task exceeds this controller's capabilities")
    return dict(
        **validate(plan, meta),
        **({"previous_trial": feedback} if feedback else {}),
        robot=args.robot,
        task=args.task,
        provider=args.provider,
        model_hash=meta["model_hash"],
    )


def simulate(args, meta):
    import numpy as np
    from PIL import Image
    from aisle.sim import nexus_backend as nx
    from importlib.metadata import version

    plan = json.loads(args.controller.read_text())
    if plan["model_hash"] != meta["model_hash"] or plan["robot"] != args.robot:
        raise ValueError("Controller and robot model differ")
    control = validate(plan, meta)
    save(args.out / "controller.json", plan)
    fixture = "fixed-base, gravity-compensated inspection"
    engine = nx._ensure_nexus()
    save(
        args.out / "source.json",
        dict(
            adapter=nx.__name__,
            backend=engine.backend_name,
            fixture_profile=args.fixture,
            adapter_hash=digest(Path(nx.__file__)),
            nexus=version("dimforge-nexus3d"),
            fixture=fixture,
        ),
    )
    scene = nx._new_scene(
        engine, {"sim": {"dt": 0.01, "gravity": [0, 0, 0]}}, 1, [0.82] * 3
    )
    robot = scene.add_urdf_robot(Path(meta["model"]), args.robot, True)
    if args.robot == "Nano":
        pos, target_z = (0.89, -0.74, 0.69), 0.25
    else:
        pos, target_z = (1.8, -1.4, 1.05), 0.03
    cam = scene.add_camera((480, 320), 44, pos=pos, lookat=(0, 0, target_z))
    scene.build()
    (dof,) = robot.get_joint(control["joint"]).dofs_idx_local
    initial = robot.get_qpos().copy()
    target = initial.copy()
    lo, hi = meta["limits"][control["joint"]]
    if not lo <= initial[dof] + control["delta_rad"] <= hi:
        raise ValueError(
            "Command from measured starting position exceeds the joint limits"
        )
    robot.set_dofs_kp([18 if args.fixture == "compliant" else 28] * robot.n_dofs)
    robot.set_dofs_kv([3] * robot.n_dofs)
    rows = []
    names = [
        name
        for _, name in sorted(
            (i, j.name) for j in robot.joints for i in j.dofs_idx_local
        )
    ]
    if len(names) != len(initial) or len(set(names)) != len(names):
        raise ValueError("Inspection requires one named joint per measured coordinate")
    recording = []
    camera = args.out / "camera"
    camera.mkdir()
    in_bounds = True
    for index in range(121):
        target[dof] = initial[dof] + control["delta_rad"] * min(index / 60, 1)
        robot.control_dofs_position(target)
        scene.step()
        measured = robot.get_qpos()
        if not np.isfinite(measured).all():
            raise ValueError("Nonfinite measured robot state")
        in_bounds = in_bounds and bool(lo <= measured[dof] <= hi)
        if index % 5:
            continue
        row = dict(
            step=index + 1,
            target_rad=float(target[dof]),
            measured_rad=float(measured[dof]),
        )
        rows.append(row)
        recording.append(
            dict(step=index + 1, state=measured.tolist(), action=target.tolist())
        )
        rgb = np.asarray(cam.render()[0], dtype=np.uint8)
        image = Image.fromarray(rgb).convert("RGB")
        path = args.out / "frame.tmp"
        image.save(path, "PNG")
        (camera / f"{len(rows) - 1:04d}.png").write_bytes(path.read_bytes())
        path.replace(args.out / "frame.png")
        tmp = args.out / "preview.tmp"
        tmp.write_bytes(index.to_bytes(4, "big") + image.tobytes())
        tmp.replace(args.out / "preview.rgb")
        save(args.out / "progress.json", row)
    save(args.out / "trajectory.json", rows)
    save(args.out / "recording.json", dict(joints=names, samples=recording))
    actual = rows[-1]["measured_rad"] - float(initial[dof])
    passed = abs(actual - control["delta_rad"]) < 0.025 and in_bounds
    return dict(
        robot=args.robot,
        recording=True,
        task_success=passed,
        frames=len(rows),
        steps=121,
        joint=control["joint"],
        measured_delta=actual,
        requested_delta=control["delta_rad"],
        within_limits=in_bounds,
        fixture=args.fixture,
        model_hash=meta["model_hash"],
        controller_hash=digest(args.controller),
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("operation", choices=["scene", "prepare", "run", "export"])
    p.add_argument("--robot", choices=MODELS, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--provider", choices=["reference", "codex"], default="reference")
    p.add_argument("--task", default=REFERENCE)
    p.add_argument("--feedback", type=Path)
    p.add_argument("--controller", type=Path)
    p.add_argument("--trial", type=Path)
    p.add_argument("--fixture", choices=["standard", "compliant"], default="standard")
    p.add_argument("--reserved", action="store_true", help=argparse.SUPPRESS)
    args = p.parse_args()
    args.out = args.out.resolve()
    if args.reserved:
        if {p.name for p in args.out.iterdir()} != {"worker.log"}:
            raise ValueError("Studio job directory already in use")
    else:
        args.out.mkdir(parents=True, exist_ok=False)
    try:
        if args.operation == "export":
            from replay import export

            save(args.out / "export.json", export(args.trial.resolve(), args.out))
            return
        meta = describe(args.robot)
        save(args.out / "scene.json", meta)
        if args.operation == "prepare":
            save(args.out / "controller.json", prepare(args, meta))
        elif args.operation == "run":
            save(args.out / "result.json", simulate(args, meta))
    except Exception as error:
        save(args.out / "error.json", {"message": str(error)})
        raise


if __name__ == "__main__":
    main()
