"""Reference inspection backed by the AISLE Nexus physics adapter."""

import argparse
import hashlib
import itertools
import json
import math
import os
import signal
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODELS = {
    "Nano": ("adora_nano/adora_nano.urdf", "left_j2_pitch", "right_j2_pitch"),
    "G1": ("g1/g1.urdf", "left_shoulder_pitch_joint", "right_shoulder_pitch_joint"),
}
CUBE_ORDERS = [
    ",".join(order)
    for count in (1, 2, 3)
    for order in itertools.permutations(("red", "green", "blue"), count)
]
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
    if plan.get("kind") == "table-transfer":
        if meta["robot"] != "Nano" or plan.get("color_order") not in CUBE_ORDERS:
            raise ValueError(
                "Table transfer requires an ordered selection of distinct Nano floor cubes"
            )
        return dict(kind="table-transfer", color_order=plan["color_order"])
    if plan.get("kind") == "visual-approach":
        if (
            meta["robot"] != "Nano"
            or plan.get("color") not in ("red", "green", "blue")
            or type(plan.get("standoff_m")) not in (int, float)
            or not 0.4 <= plan["standoff_m"] <= 0.55
        ):
            raise ValueError(
                "Visual approach requires Nano, one cube color and 0.40–0.55 m standoff"
            )
        return {k: plan[k] for k in ("kind", "color", "standoff_m")}
    standing = plan.get("kind") == "standing-inspection" and meta["robot"] == "G1"
    if not standing and plan.get("kind", "joint-inspection") != "joint-inspection":
        raise ValueError("Unknown controller capability")
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
    return dict(joint=joint, delta_rad=delta) | (
        {"kind": "standing-inspection"} if standing else {}
    )


def onboard_transfer_feedback(log, requested):
    """Summarize the controller's own sensor-derived readings for its outer author."""
    observations = {}
    color = None
    if not log.is_file():
        return observations
    with log.open() as stream:
        for line in stream:
            row = json.loads(line)
            if row.get("op") == "phase" and row.get("color") in requested:
                color = row["color"]
            if color is None or type(row.get("tick")) is not int:
                continue
            if (
                row["op"] == "jaw observation"
                and row.get("color") == color
                and type(row.get("visible")) is bool
            ):
                observations.setdefault(color, {})["last_wrist_jaw"] = {
                    "tick": row["tick"],
                    "cube_visible": row["visible"],
                }
            elif row["op"] == "table observation" and all(
                type(row.get(key)) in (int, float) and math.isfinite(row[key])
                for key in ("distance_m", "height_m", "angle_rad")
            ):
                observations.setdefault(color, {})["last_rgb_table_rim"] = {
                    key: row[key]
                    for key in ("tick", "distance_m", "height_m", "angle_rad")
                }
    return observations


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
    if previous.get("kind") == "table-transfer":
        # A revision may use the public verdict. World poses and contact geometry stay private.
        verdicts = {
            color: previous["cubes"].get(color, {}).get("passed", False)
            for color in previous["requested"]
        }
        return {
            k: previous[k]
            for k in (
                "robot",
                "kind",
                "requested",
                "phase",
                "task_success",
                "controller_hash",
            )
        } | {
            "settled": verdicts,
            "onboard_observations": onboard_transfer_feedback(
                path / "actions.jsonl", previous["requested"]
            ),
        }
    if previous.get("kind") == "visual-approach":
        return {
            k: previous[k]
            for k in (
                "robot",
                "kind",
                "color",
                "standoff_m",
                "rgb_distance_m",
                "last_state",
                "controller_hash",
            )
        }
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


def octos_controller(prompt, schema, directory, output):
    model = os.environ.get("DOROBOT_OCTOS_MODEL", "deepseek-v4-flash")
    if not model.startswith("deepseek-"):
        raise ValueError("Choose a DeepSeek model for the Octos provider")
    profile = os.environ.get("DOROBOT_OCTOS_PROFILE")
    command = ["octos", "chat"]
    if profile:
        command.extend(["--profile", profile])
    command.extend(
        [
            "--provider",
            "deepseek",
            "--model",
            model,
            "--base-url",
            "https://api.deepseek.com/v1",
            "--api-type",
            "openai",
            "--sandbox",
            "read-only",
            "--ask-for-approval",
            "never",
            "--no-session-persistence",
            "--max-iterations",
            "1",
            "--no-retry",
            "--json",
            "-m",
            prompt
            + "\nReturn exactly one JSON object matching this schema. No markdown or tool calls.\n"
            + json.dumps(schema),
        ]
    )
    with (output / "provider.log").open("w") as log:
        reply = subprocess.run(
            command,
            cwd=directory,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=log,
            text=True,
            timeout=150,
        )
    if reply.returncode:
        raise RuntimeError(
            "Octos could not complete DeepSeek inference. Check the selected Octos profile and provider.log"
        )
    envelope = json.loads(reply.stdout)
    actual = envelope.get("model")
    if actual != model:
        raise ValueError("Octos did not report the requested DeepSeek model")
    raw = envelope.get("text")
    if not isinstance(raw, str):
        raise ValueError("Octos returned no text for the requested DeepSeek controller")
    save(output / "provider-response.json", {"model": actual, "text": raw})
    lines = raw.strip().splitlines()
    if (
        len(lines) >= 3
        and lines[0].strip().lower() in ("```", "```json")
        and lines[-1].strip() == "```"
    ):
        raw = "\n".join(lines[1:-1])
    try:
        plan = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ValueError(
            "DeepSeek returned an unreadable controller. The response is saved in provider-response.json."
        ) from exc
    if (
        not isinstance(plan, dict)
        or set(plan) != set(schema["required"])
        or type(plan["supported"]) is not bool
    ):
        raise ValueError("DeepSeek returned invalid controller fields")
    save(output / "reply.json", plan)
    usage = {
        key: envelope[key]
        for key in ("input_tokens", "output_tokens")
        if type(envelope.get(key)) is int
    }
    return actual, usage


def request_controller(provider, prompt, schema, output):
    output = output.resolve()
    save(output / "schema.json", schema)
    save(
        output / "controller-request.json",
        {"provider": provider, "instruction": prompt, "output_schema": schema},
    )
    model_name = None
    started = time.monotonic()
    usage = {}
    if provider == "octos-deepseek":
        with tempfile.TemporaryDirectory() as empty:
            model_name, usage = octos_controller(prompt, schema, empty, output)
    elif provider == "codex":
        with (
            tempfile.TemporaryDirectory() as empty,
            (output / "provider.log").open("w+") as log,
        ):
            subprocess.run(
                [
                    "codex",
                    "exec",
                    "--ephemeral",
                    "--skip-git-repo-check",
                    "--sandbox",
                    "read-only",
                    "--output-schema",
                    str(output / "schema.json"),
                    "-o",
                    str(output / "reply.json"),
                    "-",
                ],
                cwd=empty,
                stdout=log,
                stderr=subprocess.STDOUT,
                input=prompt,
                text=True,
                check=True,
                timeout=150,
            )
            log.seek(0)
            for line in log:
                if line.startswith("model: "):
                    model_name = line.removeprefix("model: ").strip()
                    break
    else:
        raise ValueError(f"Unknown controller provider: {provider}")
    save(
        output / "provider-metrics.json",
        dict(
            provider=provider,
            model=model_name,
            wall_seconds=time.monotonic() - started,
            **usage,
        ),
    )
    plan = json.loads((output / "reply.json").read_text())
    return plan, model_name


def room_for_mobile(args, meta):
    room_path = getattr(args, "room_revision", None)
    if room_path is None:
        return [], None
    from room_revision import load_revision
    from run_rgb_approach import scene_recipe

    revision = load_revision(
        room_path, args.room_hash, scene_recipe(args.seed), meta["model_hash"]
    )
    return revision["obstacles"], revision.get("settings")


def prepare(args, meta):
    observation = None
    feedback = measured_feedback(args.feedback, meta) if args.feedback else None
    model_name = None
    if args.provider == "reference":
        if args.task.strip() != REFERENCE:
            raise ValueError(
                "Choose an agent for custom instructions or restore the reference task"
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
        if args.robot == "Nano":
            schema["properties"].update(
                kind=dict(
                    type="string",
                    enum=["joint-inspection", "visual-approach", "table-transfer"],
                ),
                color=dict(type="string", enum=["red", "green", "blue"]),
                color_order=dict(type="string", enum=CUBE_ORDERS),
                standoff_m=dict(type="number"),
            )
            schema["required"] += ["kind", "color", "standoff_m", "color_order"]
            prompt = (
                "Select one Nano capability. For joint-inspection use a fixed-base shoulder "
                "displacement of 0.03–0.15 rad in magnitude, within limits "
                + json.dumps({j: meta["limits"][j] for j in meta["allowed_joints"]})
                + ". For visual-approach use the floating base to approach one named red, green or blue "
                "floor cube and stop 0.40–0.55 metres from it (default 0.45 m). Use only onboard "
                "RGB and encoders. For table-transfer use the left arm to pick floor cubes and place "
                "them on the coffee table. Select named colors in their requested order, or red,green,blue "
                "for all three. This is a development simulation. Other destinations, unspecified "
                "objects, excessive displacements "
                "and other motions are unsupported. Set supported=false for these. Fill unused "
                "fields with legal defaults. Operator instruction: " + args.task
            )
        else:
            schema["properties"]["kind"] = dict(
                type="string", enum=["joint-inspection", "standing-inspection"]
            )
            schema["required"].append("kind")
            prompt = (
                "Select a G1 simulation capability. Use joint-inspection for shoulder inspections by default. "
                "If the operator explicitly asks to stand while holding or tracking a signed shoulder "
                "displacement, select standing-inspection. It uses a free pelvis and a 10 second foot-supported "
                "development trial. For either use one of "
                + json.dumps(meta["allowed_joints"])
                + " with signed displacement 0.03–0.15 rad. Reject walking, external pushes, hardware and "
                "unsupported tasks by setting supported=false. Instruction: "
                + args.task
            )
        if getattr(args, "onboard_context", False):
            if args.robot != "Nano":
                raise ValueError("Onboard scene observation requires the Nano profile")
            sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
            from onboard_scene import capture

            obstacles, settings = room_for_mobile(args, meta)
            public, observation = capture(
                args.out,
                meta,
                args.seed,
                getattr(args, "room_hash", None),
                obstacles,
                settings,
            )
            prompt += (
                "\nUse only the following onboard measurements for spatial references. y_left_m increases to the robot's left. distance_m is horizontal range. Select only observed targets and reject unresolvable requests. Sensor observation: "
                + json.dumps(public)
            )
        elif args.robot == "Nano":
            prompt += "\nIf selecting the target requires comparing its current position with others and no onboard measurements are provided, reject the request."
        if feedback:
            prompt += "\nPrevious measured trial to revise: " + json.dumps(feedback)
            save(args.out / "feedback.json", feedback)
        plan, model_name = request_controller(args.provider, prompt, schema, args.out)
        if plan.get("supported") is not True:
            raise ValueError("Requested task exceeds this controller's capabilities")
    validated = validate(plan, meta)
    if observation and validated.get("kind") in ("visual-approach", "table-transfer"):
        selected = (
            [validated["color"]]
            if validated["kind"] == "visual-approach"
            else validated["color_order"].split(",")
        )
        if any(public["objects"][color]["status"] != "localized" for color in selected):
            raise ValueError("Onboard cameras could not localize each selected target")
    return dict(
        **validated,
        **({"scene_observation": observation} if observation else {}),
        **({"previous_trial": feedback} if feedback else {}),
        robot=args.robot,
        task=args.task,
        provider=args.provider,
        **(
            {"policy_model": model_name or "Codex (model unreported)"}
            if args.provider != "reference"
            else {}
        ),
        model_hash=meta["model_hash"],
    )


def simulate(args, meta):
    import numpy as np
    from PIL import Image
    from aisle.sim import nexus_backend as nx
    from importlib.metadata import version

    original = args.controller.read_bytes()
    plan = json.loads(original)
    if plan["model_hash"] != meta["model_hash"] or plan["robot"] != args.robot:
        raise ValueError("Controller and robot model differ")
    control = validate(plan, meta)
    if "scene_observation" in plan:
        from onboard_scene import bind

        bind(
            args.controller.parent,
            args.out,
            plan["scene_observation"],
            getattr(args, "seed", 0),
            getattr(args, "room_hash", None),
        )
    (args.out / "controller.json").write_bytes(original)
    mobile = control.get("kind") in ("visual-approach", "table-transfer")
    fixture = (
        "floating base, wheel contact under gravity"
        if mobile
        else "fixed-base, zero-gravity inspection"
    )
    standing = control.get("kind") == "standing-inspection"
    impact = args.fixture == "impact"
    impact_recipe = getattr(args, "impact_recipe", None)
    if impact_recipe is not None and not (standing and impact):
        raise ValueError("A bounded impact recipe requires a standing G1 impact trial")
    if impact and not standing:
        raise ValueError("Impact fixture requires a G1 standing controller")
    if standing:
        fixture = "floating G1, gravity and loaded feet"
    repo = Path(nx.__file__).resolve().parents[3]
    revision = (
        subprocess.check_output(
            ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True
        ).strip()
        if (repo / ".git").exists()
        else None
    )
    engine = nx._ensure_nexus()
    save(
        args.out / "source.json",
        dict(
            adapter=nx.__name__,
            controller_sha256=hashlib.sha256(original).hexdigest(),
            aisle_revision=revision,
            backend=engine.backend_name,
            fixture_profile=(
                "standing-impact-development"
                if impact
                else (
                    "standing-development"
                    if standing
                    else "living-room"
                    if mobile
                    else args.fixture
                )
            ),
            adapter_hash=digest(Path(nx.__file__)),
            nexus=version("dimforge-nexus3d"),
            fixture=fixture,
        ),
    )
    if standing:
        from g1_stand import DEFAULT_IMPACT_AIM, simulate as standing_trial

        weights = None
        if "reach_checkpoint" in plan:
            from g1_reach_checkpoint import load

            weights = load(args.controller.parent, plan)
            for name in ("reach-checkpoint.json", "training.json"):
                (args.out / name).write_bytes(
                    (args.controller.parent / name).read_bytes()
                )
        return standing_trial(
            args.out,
            Path(meta["model"]),
            control,
            impact=impact,
            reach_weights=weights,
            impact_aim=getattr(args, "impact_aim", DEFAULT_IMPACT_AIM),
            parameters=json.loads(impact_recipe.read_text()) if impact_recipe else None,
        ) | dict(
            **(
                {
                    "reach_checkpoint": plan["reach_checkpoint"],
                    "training_dataset": plan["training_dataset"],
                }
                if weights is not None
                else {}
            ),
            robot=args.robot,
            controller_hash=hashlib.sha256(original).hexdigest(),
            model_hash=meta["model_hash"],
        )
    if mobile:
        sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
        room_objects, room_settings = room_for_mobile(args, meta)
        room_path = getattr(args, "room_revision", None)
        if room_path is not None:
            import shutil

            for artifact in (
                "room.json",
                "reference-image",
                "survey.json",
                "room-preview.rgb",
            ):
                shutil.copyfile(room_path.with_name(artifact), args.out / artifact)
            source = json.loads((args.out / "source.json").read_text())
            save(
                args.out / "source.json",
                source | {"room_revision_sha256": args.room_hash},
            )
        if control["kind"] == "table-transfer":
            from run_rgb_transfer import execute

            result = execute(
                args.out,
                getattr(args, "seed", 0),
                control["color_order"].split(","),
                record=True,
                room_objects=room_objects,
                room_settings=room_settings,
            )
            return result | dict(
                robot=args.robot,
                fixture="living-room",
                controller_hash=hashlib.sha256(original).hexdigest(),
                model_hash=meta["model_hash"],
                kind=control["kind"],
                room_revision_sha256=getattr(args, "room_hash", None),
                room_obstacles=[b["name"] for b in room_objects],
                task_success=result["passed"],
            )
        from run_rgb_approach import trial

        result = trial(
            args.out,
            getattr(args, "seed", 0),
            plan,
            rgb_timing=getattr(args, "rgb_timing", "normal"),
            target_motion=getattr(args, "target_motion", "stationary"),
            room_objects=room_objects,
            room_settings=room_settings,
        )
        return dict(
            **result,
            robot=args.robot,
            fixture="living-room",
            controller_hash=hashlib.sha256(original).hexdigest(),
            model_hash=meta["model_hash"],
            kind=control["kind"],
            task_success=result["passed"],
            room_revision_sha256=getattr(args, "room_hash", None),
            room_obstacles=[b["name"] for b in room_objects],
            color=plan["color"],
            standoff_m=plan["standoff_m"],
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
    tolerance = min(0.025, 0.15 * abs(control["delta_rad"]))
    passed = abs(actual - control["delta_rad"]) < tolerance and in_bounds
    return dict(
        robot=args.robot,
        recording=True,
        task_success=passed,
        tolerance_rad=tolerance,
        frames=len(rows),
        steps=121,
        joint=control["joint"],
        measured_delta=actual,
        requested_delta=control["delta_rad"],
        within_limits=in_bounds,
        fixture=args.fixture,
        model_hash=meta["model_hash"],
        controller_hash=hashlib.sha256(original).hexdigest(),
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "operation",
        choices=[
            "scene",
            "prepare",
            "run",
            "export",
            "room",
            "room-draft",
            "room-apply",
            "evaluate",
            "train-reach",
        ],
    )
    p.add_argument("--robot", choices=MODELS, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument(
        "--provider",
        choices=["reference", "codex", "octos-deepseek"],
        default="reference",
    )
    p.add_argument("--task", default=REFERENCE)
    p.add_argument("--feedback", type=Path)
    p.add_argument("--controller", type=Path)
    p.add_argument("--trial", type=Path)
    p.add_argument("--archive", type=Path)
    p.add_argument("--dataset", type=Path)
    p.add_argument(
        "--impact-aim",
        choices=["pelvis-v1", "torso-v2", "frontal-torso-v3"],
        default="frontal-torso-v3",
        help="Calibration of the simulated G1 impact",
    )
    p.add_argument("--onboard-context", action="store_true")
    p.add_argument(
        "--rgb-timing",
        choices=["normal", "intermittent", "disconnected"],
        default="normal",
    )
    p.add_argument(
        "--target-motion",
        choices=["stationary", "slide-positive", "slide-negative"],
        default="stationary",
    )
    p.add_argument("--room-source", type=Path)
    p.add_argument("--room-revision", type=Path)
    p.add_argument("--room-hash")
    p.add_argument("--room-edit")
    p.add_argument(
        "--impact-recipe", type=Path, help="Bounded G1 simulation impact JSON"
    )
    p.add_argument("--seed", type=int, default=0, help="Living-room layout seed")
    p.add_argument(
        "--fixture", choices=["standard", "compliant", "impact"], default="standard"
    )
    p.add_argument("--reserved", action="store_true", help=argparse.SUPPRESS)
    args = p.parse_args()
    if args.seed < 0:
        p.error("Scene seed must be nonnegative")
    args.out = args.out.resolve()
    if args.reserved:
        # Studio alone holds the write end. A separate observer can terminate the
        # owned group even while the physics engine blocks inside native code.
        if os.name == "posix":
            if os.getpgrp() != os.getpid():
                raise RuntimeError("Studio workers require their own process group")
            if os.fork() == 0:
                try:
                    os.read(0, 1)
                    os.killpg(os.getpgrp(), signal.SIGKILL)
                finally:
                    os._exit(0)
        if {p.name for p in args.out.iterdir()} != {"worker.log"}:
            raise ValueError("Studio job directory already in use")
    else:
        args.out.mkdir(parents=True, exist_ok=False)
    try:
        if args.operation == "evaluate":
            if args.archive is None:
                raise ValueError(
                    "Choose an evaluation archive beside its preregistered suite.json"
                )
            sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
            public = args.archive.with_name("suite.json")
            if json.loads(public.read_text()).get("profile") == "g1-standing-reach-v1":
                from g1_reach_suite import verify as verify_g1

                save(
                    args.out / "evaluation.json",
                    verify_g1(args.archive, digest(public)),
                )
                return
            from nano_suite_evidence import verify

            expected = digest(public)
            save(
                args.out / "evaluation.json",
                verify(args.archive, expected, include_trials=True),
            )
            return
        if args.operation == "room-apply":
            if args.robot != "Nano" or args.room_revision is None:
                raise ValueError("Choose a Nano room draft")
            sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
            room_for_mobile(args, describe("Nano"))
            ref = json.loads(args.room_revision.with_name("room-ref.json").read_bytes())
            if ref != {
                "sha256": args.room_hash,
                "revision": args.room_revision.parent.name,
            }:
                raise ValueError("The reviewed draft identity has changed")
            save(args.out / "room-ref.json", ref)
            return
        if args.operation == "room-draft":
            if args.robot != "Nano" or args.room_source is None:
                raise ValueError("Select a Nano reference photo input")
            sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
            from photo_room import draft_room
            from run_rgb_approach import scene_recipe

            draft_room(
                args.room_source.resolve(),
                args.out,
                scene_recipe(args.seed),
                describe("Nano")["model_hash"],
                args.provider,
            )
            return
        if args.operation == "room":
            if args.robot != "Nano":
                raise ValueError("A floor survey requires the Nano living-room profile")
            sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
            from run_rgb_approach import scene_recipe
            from room_revision import import_room

            source = args.room_source
            if source is None and args.room_revision is not None:
                source = args.room_revision.with_name("survey.json")
            if source is None:
                raise ValueError("Choose a room survey JSON file")
            import_room(
                source.resolve(),
                args.out,
                scene_recipe(args.seed),
                args.room_edit,
                parent=args.room_hash if args.room_source is None else None,
                model_sha=describe("Nano")["model_hash"],
            )
            return
        if args.operation == "train-reach":
            if args.robot != "G1" or args.dataset is None or args.trial is None:
                raise ValueError("Choose a G1 standing reach episode to train")
            from g1_reach_checkpoint import train

            train(args.dataset, args.trial, args.out)
            return
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
