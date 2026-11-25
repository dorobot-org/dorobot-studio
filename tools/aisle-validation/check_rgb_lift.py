"""Single-cube physical feasibility probe. The Studio capability remains visual approach."""

import argparse
import math
from pathlib import Path
import sys
from PIL import Image

import numpy as np

from nano_living import LivingSimulation, NAVIGATION_POSTURE, WheelGuard, wheel_rates
from nano_rgb_policy import CAMERAS, CUBE_CENTRE_Z, VisualServo
from run_rgb_approach import COLORS, scene_recipe, worker

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "nano-three-cubes"))
from model import Kinematics, MODEL  # noqa: E402


def encoders(sim):
    return np.asarray(sim.robot.get_qpos())[sim.dofs]


def localize(sim, servo):
    packet = {
        name: sim.rig[name].render(rgb=True, depth=False, segmentation=False)[0]
        for name in CAMERAS
    }
    command = servo.command(
        dict(**packet, encoders=encoders(sim), stamp_ns=np.int64(sim.tick * 10_000_000))
    )
    if command["xy"] is None:
        raise ValueError(f"Lost the floor cube while aligning: {command['state']}")
    return np.array(command["xy"])


def align_for_left_arm(sim, servo):
    guard = WheelGuard()
    for turn_in_place in [False, True]:
        for _ in range(100):
            xy = localize(sim, servo)
            bearing = math.atan2(xy[1], xy[0])
            distance = float(np.linalg.norm(xy))
            # Present the cube below the left shoulder using the arm's reachable workspace.
            error = bearing - (0.71 if turn_in_place else 0)
            speed = (
                float(np.clip((distance - 0.345) * 0.85, 0, 0.12))
                if not turn_in_place and abs(error) < 0.075
                else 0.0
            )
            rates = wheel_rates(speed, 0, float(np.clip(error * 1.8, -0.45, 0.45)))
            sim.advance(guard.accept(rates, guard.next_seq))
            if abs(error) < 0.020 and (turn_in_place or abs(distance - 0.345) < 0.009):
                break
        else:
            raise ValueError("RGB alignment timed out")
    for _ in range(2):
        sim.advance(guard.accept(np.zeros(3), guard.next_seq))
    return localize(sim, servo)


def lift(sim, model, xy):
    grip = "left_j6_gripper"
    joints = [*model.arm, grip]
    indices = [sim.names.index(j) for j in joints]
    velocity_limits = np.array(
        [
            float(model.root.find(f"joint[@name='{j}']/limit").get("velocity"))
            for j in joints
        ]
    )
    bounds = np.array([model.limits[j] for j in joints])
    max_excursion = 0.0

    def move(q, steps):
        nonlocal max_excursion
        for name, value in zip(sim.names, q):
            lo, hi = model.limits[name]
            if not np.isfinite(value) or not lo <= value <= hi:
                raise ValueError(f"Target outside {name} limits")
        start = sim.targets.copy()
        # A cubic smoothstep peaks at 1.5 times the average joint speed.
        min_duration = np.max(
            1.5 * np.abs(q[indices] - start[indices]) / velocity_limits
        )
        steps = max(steps, math.ceil(min_duration / 0.01))
        for i in range(steps):
            f = (i + 1) / steps
            sim.targets[indices] = start[indices] + f * f * (3 - 2 * f) * (
                q[indices] - start[indices]
            )
            sim.robot.control_dofs_position(sim.targets.tolist(), sim.dofs)
            sim.scene.step()
            sim.tick += 1
            measured = encoders(sim)[indices]
            if not np.isfinite(measured).all():
                raise ValueError("Nonfinite measured left arm joint")
            max_excursion = max(
                max_excursion,
                float(np.max(bounds[:, 0] - measured)),
                float(np.max(measured - bounds[:, 1])),
            )
            if max_excursion > 0.005:
                raise ValueError(
                    "Measured arm outside joint limits by more than 0.005 rad"
                )
        sim.advance([0, 0, 0])

    def reach(z, steps, subdivisions):
        q = sim.targets.copy()
        start, _ = model.grip(q)
        for waypoint in np.linspace(start, np.r_[xy, z], subdivisions + 1)[1:]:
            q = model.solve(waypoint, q, attempts=2)
            move(q, steps // subdivisions)

    dof = sim.robot.get_joint(grip).dofs_idx_local
    sim.robot.set_dofs_kp([5.0], dof)
    sim.robot.set_dofs_kv([0.25], dof)
    reach(0.115, 250, 1)
    reach(CUBE_CENTRE_Z + 0.004, 250, 8)
    q = sim.targets.copy()
    q[sim.names.index(grip)] = 0.55
    move(q, 180)
    reach(0.085, 320, 8)
    return max_excursion


def select_target(args):
    if not args.task:
        return dict(provider="reference", color=args.color or "red")
    prompt = (
        "Author one simulated Nano left-arm test lift. The robot visually approaches one "
        "named red, green or blue floor cube, closes the fingers, lifts and holds it for "
        "one second. Choose the color in the task. Mark unsupported for ambiguous targets, "
        "different arms, multiple cubes, carrying, placing or any other task. "
        "Operator request: " + args.task
    )
    schema = dict(
        type="object",
        additionalProperties=False,
        required=["supported", "color"],
        properties=dict(
            supported=dict(type="boolean"), color=dict(type="string", enum=list(COLORS))
        ),
    )
    reply, model = worker.request_controller(args.provider, prompt, schema, args.out)
    if reply.get("supported") is not True or reply.get("color") not in COLORS:
        raise ValueError("Unsupported single-cube lift request")
    return dict(
        provider=args.provider, policy_model=model, color=reply["color"], task=args.task
    )


def check(args):
    model = Kinematics(MODEL)
    sim = LivingSimulation(
        MODEL,
        specs=scene_recipe(args.seed),
        resolution=(480, 360),
        posture=NAVIGATION_POSTURE
        | dict(head_pan_joint=0.25, head_tilt_joint=0.65, left_j6_gripper=-0.25),
    )
    if model.names != sim.names:
        raise ValueError("Arm solver and robot disagree on joint order")
    cameras = {
        k: {
            field: sim.camera_specs[k][field]
            for field in (
                "parent_link",
                "link_from_camera_gl",
                "intrinsics",
                "resolution",
            )
        }
        for k in CAMERAS
    }
    servo = VisualServo(
        dict(
            color=args.color,
            standoff_m=0.45,
            model=str(MODEL),
            cameras=cameras,
            joint_names=sim.names,
        )
    )

    objects = {s["name"]: b for s, b in zip(sim.specs, sim.bodies)}
    cube = objects[args.color]
    initial = float(cube.get_pos()[2])
    xy = align_for_left_arm(sim, servo)
    max_excursion = lift(sim, model, xy)

    # The judge observes end-state contacts and height after the policy's last arm command.
    body = sim.scene.gpu_index(0, cube.handles[0])
    floor = sim.scene.gpu_index(0, objects["floor"].handles[0])
    links = {
        link.name: sim.scene.gpu_index(0, sim.robot.link_bodies[0][link.index])
        for link in sim.robot.links
    }
    fingers = {links["left_finger_a"], links["left_finger_b"]}
    wheels = {links[f"wheel_{i}"] for i in (1, 2, 3)}
    held = wheel_ticks = 0
    heights = []
    for _ in range(5):
        contacts = sim.advance(np.zeros(3), capture_contacts=True)
        for tick in range(sim.tick - 19, sim.tick + 1):
            touched = {}
            for _, a, b in contacts[contacts[:, 0] == tick]:
                touched.setdefault(a, set()).add(b)
                touched.setdefault(b, set()).add(a)
            held += int(fingers.issubset(touched.get(body, set())))
            wheel_ticks += int(bool(wheels & touched.get(floor, set())))
        heights.append(float(cube.get_pos()[2]))
    for name in CAMERAS:
        frame = sim.rig[name].render(rgb=True, depth=False, segmentation=False)[0]
        Image.fromarray(frame).save(args.out / f"hold-{name}.png")
    result = dict(
        task="single-cube test lift",
        controller="local RGB servo and URDF IK",
        seed=args.seed,
        color=args.color,
        model_sha256=worker.digest(MODEL),
        estimated_cube_base_xy_m=xy.tolist(),
        max_arm_excursion_during_interpolation_rad=max_excursion,
        initial_cube_z_m=initial,
        hold_cube_z_m=heights,
        min_lift_m=min(heights) - initial,
        opposing_finger_contact_ticks=held,
        wheel_supported_hold_ticks=wheel_ticks,
        hold_ticks=100,
        root_pose_writes=sim.root_pose_writes,
    )
    result["passed"] = (
        result["min_lift_m"] > 0.06
        and held >= 90
        and wheel_ticks == 100
        and not sim.root_pose_writes
    )
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=0)
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--color", choices=COLORS)
    source.add_argument("--task", help="Author the test lift with a model")
    parser.add_argument(
        "--provider", choices=["octos-deepseek", "codex"], default="octos-deepseek"
    )
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    try:
        plan = select_target(args)
        args.color = plan["color"]
        worker.save(args.out / "controller.json", plan)
        result = check(args) | {"author": plan}
    except Exception as error:
        worker.save(
            args.out / "result.json",
            dict(
                passed=False,
                task="single-cube test lift",
                seed=args.seed,
                color=args.color,
                reason=str(error),
            ),
        )
        raise
    worker.save(args.out / "result.json", result)
    print(result)
    sys.exit(0 if result["passed"] else 1)
