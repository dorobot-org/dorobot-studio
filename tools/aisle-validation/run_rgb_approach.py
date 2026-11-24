"""Development trial for the grounded Nano in Nexus. See nano_rgb_policy for the sensor boundary."""

import argparse
from collections import Counter, deque
import io
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys

import numpy as np
from PIL import Image

from nano_living import (
    CAMERAS,
    LivingSimulation,
    NAVIGATION_POSTURE,
    WheelGuard,
    room_specs,
    wheel_rates,
)

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "aisle_workbench"))
import worker  # noqa: E402

COLORS = {
    "red": [0.94, 0.10, 0.08],
    "green": [0.10, 0.86, 0.12],
    "blue": [0.10, 0.26, 0.94],
}
RECIPE = "nano-living-room-floor-cubes-v1"


def scene_recipe(seed):
    specs = [s for s in room_specs(seed) if s["fixed"]]
    rng = np.random.default_rng(seed)
    for (name, color), y in zip(COLORS.items(), rng.permutation([-0.32, -0.04, 0.22])):
        specs.append(
            dict(
                name=name,
                size=[0.04] * 3,
                color=[*color, 1.0],
                fixed=False,
                position=[rng.uniform(0.67, 0.89), y + rng.uniform(-0.04, 0.04), 0.015],
            )
        )
    return specs


def controller(args):
    if args.color:
        return dict(provider="reference", color=args.color, standoff_m=args.distance)
    from argparse import Namespace

    meta = worker.describe("Nano")
    plan = worker.prepare(
        Namespace(
            robot="Nano",
            provider=args.provider,
            task=args.task,
            out=args.out,
            feedback=None,
        ),
        meta,
    )
    if plan.get("kind") != "visual-approach":
        raise ValueError("The requested task does not select a visual approach")
    return plan


def sensor_packet(sim, out, save_frame=True):
    encoders = np.asarray(sim.robot.get_qpos())[sim.dofs]
    data = dict(encoders=encoders, stamp_ns=np.int64(sim.tick * 10_000_000))
    for name in CAMERAS:
        data[name] = sim.rig[name].render(rgb=True, depth=False, segmentation=False)[0]
        if save_frame:
            Image.fromarray(data[name]).save(out / f"{name}-{sim.tick}.png")
    payload = io.BytesIO()
    np.savez_compressed(payload, **data)
    return payload.getvalue(), data["head"]


def evaluate_pose(sim, cube):
    from aisle.sim.nexus_backend import quat_wxyz_to_matrix

    base = np.asarray(sim.robot.get_pos())
    rotation = quat_wxyz_to_matrix(sim.robot.get_quat())
    xy = (rotation.T @ (cube.get_pos() - base))[:2]
    return base, xy


def approach_scene(model, seed, room_objects, room_settings):
    return LivingSimulation(
        model,
        specs=[*scene_recipe(seed), *room_objects],
        room_settings=room_settings,
        resolution=(480, 360),
        posture=NAVIGATION_POSTURE | dict(head_pan_joint=0, head_tilt_joint=0.35),
    )


def camera_config(sim, model):
    cameras = {
        name: {
            k: sim.camera_specs[name][k]
            for k in ("resolution", "intrinsics", "parent_link", "link_from_camera_gl")
        }
        for name in CAMERAS
    }
    return dict(
        cameras=cameras,
        model=str(model),
        joint_names=sim.names,
    )


MAX_OBSERVATION_AGE_NS = 250_000_000
MAX_MISSING_OBSERVATIONS = 10


def timing_fault(captured_ns, received_ns, last_accepted_ns):
    if captured_ns < 0 or captured_ns > received_ns:
        return "invalid timestamp"
    if captured_ns <= last_accepted_ns:
        return "repeated or out-of-order packet"
    if received_ns - captured_ns > MAX_OBSERVATION_AGE_NS:
        return "late packet"
    return None


class SlidingCube:
    """Independent Nexus fixture and evaluator. Its measurements never enter RGB packets."""

    def __init__(self, cube, mode):
        self.cube = cube
        self.direction = {"slide-positive": 1, "slide-negative": -1}[mode]
        self.mode = mode
        self.initial = np.asarray(cube.get_pos()).copy()
        self.before_last_step = None
        self.pulse_ticks = []
        self.moving_exposures = 0
        self.errors = []

    PULSE_TICKS = (76, 96, 116, 136, 156)

    def advance(self, sim, rates):
        # Keep the normal 20 physics steps and a single wheel command per camera
        # period. Four steps after a pulse the next exposure sees a moving cube.
        contacts = [sim.advance(rates, capture_contacts=True, steps=16)]
        if sim.tick in self.PULSE_TICKS:
            sim.scene.set_body_velocity(
                0, self.cube.handles[0], [0, self.direction * 0.60, 0]
            )
            self.pulse_ticks.append(sim.tick)
        contacts.append(sim.advance(rates, capture_contacts=True, steps=3))
        self.before_last_step = np.asarray(self.cube.get_pos()).copy()
        contacts.append(sim.advance(rates, capture_contacts=True, steps=1))
        return np.concatenate(contacts)

    def observe(self, truth, estimate, fresh):
        world = np.asarray(self.cube.get_pos())
        before = self.before_last_step
        if before is not None and np.linalg.norm(world[:2] - before[:2]) / 0.01 > 0.02:
            self.moving_exposures += 1
            if fresh and estimate is not None:
                self.errors.append(float(np.linalg.norm(truth - estimate)))
        return {
            "world_m": world.tolist(),
            "world_one_tick_before_m": before.tolist() if before is not None else None,
        }

    def result(self):
        displacement = float(self.cube.get_pos()[1] - self.initial[1])
        maximum = max(self.errors, default=None)
        return dict(
            fixture=self.mode,
            commanded_world_y_m_s=self.direction * 0.60,
            pulse_ticks=self.pulse_ticks,
            displacement_y_m=displacement,
            moving_exposures=self.moving_exposures,
            localized_moving_exposures=len(self.errors),
            max_moving_error_m=maximum,
            passed=tuple(self.pulse_ticks) == self.PULSE_TICKS
            and displacement * self.direction >= 0.10
            and self.moving_exposures >= 3
            and len(self.errors) == self.moving_exposures
            and maximum is not None
            and maximum <= 0.035,
        )


def trial(
    out,
    seed,
    plan,
    room_objects=(),
    room_settings=None,
    rgb_timing="normal",
    target_motion="stationary",
):
    if rgb_timing not in ("normal", "intermittent", "disconnected"):
        raise ValueError("Unknown RGB timing fixture")
    if target_motion not in ("stationary", "slide-positive", "slide-negative"):
        raise ValueError("Unknown target motion fixture")
    meta = worker.describe("Nano")
    model = Path(meta["model"])
    sim = approach_scene(model, seed, room_objects, room_settings)
    by_name = {spec["name"]: body for spec, body in zip(sim.specs, sim.bodies)}
    cube = by_name[plan["color"]]
    sliding = (
        SlidingCube(cube, target_motion) if target_motion != "stationary" else None
    )
    initial_cube = np.array(cube.get_pos())
    initial, initial_xy = evaluate_pose(sim, cube)

    def body_id(body):
        return sim.scene.gpu_index(0, body.handles[0])

    floor = body_id(by_name["floor"])
    links = {
        link.name: sim.scene.gpu_index(0, sim.robot.link_bodies[0][link.index])
        for link in sim.robot.links
    }
    wheels = {links[f"wheel_{i}"] for i in (1, 2, 3)}
    robot_ids = set(links.values())
    obstacles = {body_id(body) for name, body in by_name.items() if name != "floor"}
    config = camera_config(sim, model) | dict(
        color=plan["color"], standoff_m=plan["standoff_m"]
    )
    worker.save(out / "calibration.json", config)
    (out / "observations").mkdir()
    guard = WheelGuard()
    buffer = deque(maxlen=3)
    frozen = None
    last_accepted_ns = -1
    missed = 0
    rejected = Counter()
    recoveries = 0
    trajectory = []
    grounded = set()
    hits = set()
    settled = 0
    errors = []
    joint_violations = set()
    limits = [(j, sim.names.index(j), *bounds) for j, bounds in meta["limits"].items()]
    with (
        (out / "trace.jsonl").open("w") as log,
        subprocess.Popen(
            [sys.executable, str(HERE / "nano_rgb_policy.py")],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
        ) as policy,
    ):
        try:
            policy.stdin.write(json.dumps(config).encode() + b"\n")
            for sequence in range(100):
                received_ns = sim.tick * 10_000_000
                current = sensor_packet(sim, out, sequence == 0)
                if rgb_timing != "normal":
                    buffer.append(current)
                    if sequence == 3:
                        frozen = current
                    if sequence >= 4 and (rgb_timing == "disconnected" or sequence < 7):
                        current = frozen
                    elif 7 <= sequence < 12:
                        current = buffer[0]
                payload, head = current
                (out / "observations" / f"{sim.tick:07d}.npz").write_bytes(payload)
                with np.load(io.BytesIO(payload), allow_pickle=False) as delivered:
                    captured_ns = int(delivered["stamp_ns"])
                fault = timing_fault(captured_ns, received_ns, last_accepted_ns)
                if fault:
                    missed += 1
                    rejected[fault] += 1
                    command = dict(
                        state="waiting for current RGB",
                        xy=None,
                        cameras=[],
                        speed=0.0,
                        yaw_rate=0.0,
                    )
                else:
                    recoveries += int(missed > 0)
                    missed = 0
                    last_accepted_ns = captured_ns
                    policy.stdin.write(struct.pack(">I", len(payload)) + payload)
                    policy.stdin.flush()
                    command = json.loads(policy.stdout.readline())
                speed, yaw = command["speed"], command["yaw_rate"]
                if not -0.045 <= speed <= 0.13 or not -0.5 <= yaw <= 0.5:
                    raise ValueError("visual servo exceeded wheel command envelope")
                seen = command["xy"]
                progress = dict(
                    kind="visual-approach",
                    capture_ns=captured_ns,
                    receive_ns=received_ns,
                    sensor_fault=fault,
                    step=sim.tick + 1,
                    state=command["state"],
                    color=plan["color"],
                    standoff_m=plan["standoff_m"],
                    distance_m=(
                        float(np.linalg.norm(seen)) if seen is not None else None
                    ),
                )
                _, truth = evaluate_pose(sim, cube)
                motion_sample = {}
                if sliding is not None:
                    motion_sample = {
                        "target_motion": sliding.observe(
                            truth, seen, fault is None and captured_ns == received_ns
                        )
                    }
                    progress.update(
                        target_motion=target_motion,
                        moving_exposures=sliding.moving_exposures,
                    )
                trajectory.append(progress)
                preview = out / "preview.tmp"
                preview.write_bytes(struct.pack(">I", sim.tick) + head.tobytes())
                preview.replace(out / "preview.rgb")
                worker.save(out / "progress.json", progress)
                if command["xy"] is not None:
                    errors.append(float(np.linalg.norm(truth - command["xy"])))
                rates = guard.accept(wheel_rates(speed, 0, yaw), sequence)
                contacts = (
                    sliding.advance(sim, rates)
                    if sliding is not None
                    else sim.advance(rates, capture_contacts=True)
                )
                for tick, a, b in contacts:
                    if (a == floor and b in wheels) or (b == floor and a in wheels):
                        grounded.add(int(tick))
                    if (a in robot_ids and b in obstacles) or (
                        b in robot_ids and a in obstacles
                    ):
                        hits.add(int(tick))
                q = np.asarray(sim.robot.get_qpos())[sim.dofs]
                joint_violations.update(
                    j for j, i, lo, hi in limits if not lo - 0.005 <= q[i] <= hi + 0.005
                )
                print(
                    json.dumps(
                        dict(
                            sequence=sequence,
                            tick=sim.tick,
                            capture_ns=captured_ns,
                            receive_ns=received_ns,
                            sensor_fault=fault,
                            command=command,
                            wheels=rates.tolist(),
                            evaluator_xy=truth.tolist(),
                            **motion_sample,
                        ),
                        allow_nan=False,
                    ),
                    file=log,
                    flush=True,
                )
                settled = (
                    settled + 1
                    if command["state"] == "reached"
                    and np.max(np.abs(rates)) < 1e-5
                    and (
                        sliding is None
                        or received_ns > sliding.PULSE_TICKS[-1] * 10_000_000
                    )
                    else 0
                )
                if settled >= 3 or (
                    command["xy"] is None
                    and (fault is None or missed >= MAX_MISSING_OBSERVATIONS)
                    and np.all(np.abs(rates) < 1e-5)
                ):
                    break
        finally:
            policy.stdin.close()
            if policy.wait(timeout=10) != 0:
                raise RuntimeError("RGB policy process failed")
    _, final_frame = sensor_packet(sim, out)
    Image.fromarray(final_frame).save(out / "frame.png")
    worker.save(out / "trajectory.json", trajectory)
    base, final_xy = evaluate_pose(sim, cube)
    distance = float(np.linalg.norm(final_xy))
    motion = float(np.linalg.norm(base[:2] - initial[:2]))
    motion_result = (
        sliding.result() if sliding is not None else {"fixture": "stationary"}
    )
    passed = (
        motion_result.get("passed", True)
        and settled >= 3
        and abs(distance - plan["standoff_m"]) <= 0.035
        and abs(math.atan2(final_xy[1], final_xy[0])) < 0.12
        and motion >= 0.07
        and len(grounded) == sim.tick
        and not hits
        and not joint_violations
    )
    return dict(
        recipe=RECIPE,
        seed=seed,
        split="development",
        target_motion=motion_result,
        tracking_sources={
            name: worker.digest(HERE / name)
            for name in ("nano_rgb_policy.py", "run_rgb_approach.py", "nano_living.py")
        },
        sensor_timing=dict(
            fixture=rgb_timing,
            maximum_age_ns=MAX_OBSERVATION_AGE_NS,
            rejected=dict(rejected),
            recoveries=recoveries,
            timed_out=missed >= MAX_MISSING_OBSERVATIONS,
        ),
        measured_room_bodies=sim.room_readback(),
        task="visual approach",
        passed=passed,
        controller=plan,
        model_sha256=meta["model_hash"],
        seconds_simulated=sim.tick * 0.01,
        initial_distance_m=float(np.linalg.norm(initial_xy)),
        final_distance_m=distance,
        motion_m=motion,
        cube_motion_m=float(np.linalg.norm(cube.get_pos() - initial_cube)),
        rgb_distance_m=trajectory[-1]["distance_m"],
        decisions=sequence + 1,
        settled_observations=settled,
        max_localization_error_m=max(errors, default=None),
        wheel_supported_ticks=len(grounded),
        physics_ticks=sim.tick,
        obstacle_contact_ticks=len(hits),
        sampled_joint_limit_violations=sorted(joint_violations),
        last_state=command["state"],
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--provider", choices=["octos-deepseek", "codex"], default="octos-deepseek"
    )
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument(
        "--task", help="author the approach controller using the selected provider"
    )
    source.add_argument("--color", choices=COLORS, help="reference controller target")
    parser.add_argument(
        "--distance", type=float, default=0.45, help="reference standoff in metres"
    )
    parser.add_argument("--seed", type=int, default=0, help="development layout seed")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--rgb-timing",
        choices=["normal", "intermittent", "disconnected"],
        default="normal",
    )
    parser.add_argument(
        "--target-motion",
        choices=["stationary", "slide-positive", "slide-negative"],
        default="stationary",
    )
    args = parser.parse_args()
    if not 0.4 <= args.distance <= 0.55:
        parser.error("distance must be within [0.40, 0.55] m")
    if os.environ.get("DOROBOT_AISLE_ROOT"):
        # The policy subprocess inherits the same AISLE FK implementation.
        src = str(Path(os.environ["DOROBOT_AISLE_ROOT"]).resolve() / "src")
        sys.path.insert(0, src)
        os.environ["PYTHONPATH"] = os.pathsep.join(
            filter(None, [src, os.environ.get("PYTHONPATH")])
        )
    args.out.mkdir(parents=True, exist_ok=False)
    plan = controller(args)
    worker.save(args.out / "controller.json", plan)
    result = trial(
        args.out,
        args.seed,
        plan,
        rgb_timing=args.rgb_timing,
        target_motion=args.target_motion,
    )
    worker.save(args.out / "result.json", result)
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
