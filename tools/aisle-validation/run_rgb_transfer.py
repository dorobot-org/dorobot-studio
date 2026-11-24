"""Run a Nano floor-to-table episode in Nexus and score contact, release and settling."""

import argparse
from collections import Counter, deque
from importlib.metadata import version
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

import numpy as np
from PIL import Image
from aisle import kinematics
from aisle.sim.nexus_backend import quat_wxyz_to_matrix
from verify_arm_reach import ArmModel

from nano_actuator_audit import MotorEnvelope, MotorWriter, replay
from nano_living import (
    CAMERAS,
    NAVIGATION_POSTURE,
    LivingSimulation,
    WHEEL_RADIUS,
    WheelGuard,
    wheel_rates,
)
from run_rgb_approach import COLORS, HERE, RECIPE, scene_recipe, worker

MODEL = HERE.parents[1] / "data/adora_nano/adora_nano.urdf"
POSTURE = NAVIGATION_POSTURE | dict(
    head_pan_joint=0.25, head_tilt_joint=0.65, left_j6_gripper=-0.25
)


def finger_gap(point, position, rotation, bounds, cube_radius):
    """Lower bound on surface clearance from an oriented finger box to a cube."""
    local = rotation.T @ (point - position)
    center, half = bounds
    return float(
        np.linalg.norm(np.maximum(abs(local - center) - half, 0)) - cube_radius
    )


class Judge:
    def __init__(self, sim, colors, record_labels=False, trace_fingers=False):
        self.colors = colors
        self.airborne_in_jaw = {n: False for n in colors} if record_labels else None
        objects = {s["name"]: b for s, b in zip(sim.specs, sim.bodies)}
        self.cubes = {name: objects[name] for name in colors}
        self.table = objects["coffee_table"]
        self.size = next(
            np.array(s["size"]) for s in sim.specs if s["name"] == "coffee_table"
        )
        self.initial = {n: b.get_pos().copy() for n, b in self.cubes.items()}
        self.object_ids = {
            sim.scene.gpu_index(0, b.handles[0]): k for k, b in objects.items()
        }
        self.links = {
            sim.scene.gpu_index(0, sim.robot.link_bodies[0][link.index]): link.name
            for link in sim.robot.links
        }
        self.slots = {link.index: link.name for link in sim.robot.links}
        bounds = ArmModel(MODEL).bounds
        self.jaws = [
            (sim.robot.get_link(name), bounds[name])
            for name in ("left_finger_a", "left_finger_b")
        ]
        self.radii = {
            s["name"]: np.linalg.norm(s["size"]) / 2
            for s in sim.specs
            if s["name"] in colors
        }
        self.wheels = {f"wheel_{i}" for i in (1, 2, 3)}
        self.grasped = Counter()
        self.collisions = Counter()
        self.supported = 0
        self.unsupported_ranges = []
        self.excursions = Counter()
        self.tail = {n: deque(maxlen=100) for n in colors}
        self.phase = "start"
        self.pick_contacts = {} if trace_fingers else None
        xml = ET.parse(MODEL)
        self.bounds = {
            j: (float(x.get("lower")), float(x.get("upper")))
            for j in sim.names
            if (x := xml.find(f"joint[@name='{j}']/limit")) is not None
            and x.get("lower")
        }

    def tick(self, sim):
        ground = False
        fingers = {c: set() for c in self.colors}
        touching_gripper = set()
        finger_impulses = (
            {c: {} for c in self.colors} if self.pick_contacts is not None else None
        )
        table_support = set()
        other_support = set()
        impact = set()
        state, viewer = sim.scene.state, sim.engine.viewer
        # Contact manifolds in Nexus 0.2.1 may persist after separation.
        # Current normal impulses establish load at this physics step.
        for row in state.debug_rigid_contact_impulses(viewer):
            if max(row["normal_impulse"], default=0) <= 1e-6:
                continue
            pair = {
                self.object_ids.get(row["body_a"]),
                self.object_ids.get(row["body_b"]),
            }
            other_support.update(pair.intersection(self.colors))
            if "coffee_table" in pair:
                table_support.update(pair.intersection(self.colors))
        for row in state.debug_multibody_contact_impulses(viewer):
            if row["kind"] != "normal" or row["impulse"] <= 1e-6:
                continue
            link, obj = (
                self.slots.get(row["link"]),
                self.object_ids.get(row["free_body"]),
            )
            if link in self.wheels and obj == "floor":
                ground = True
            elif obj in fingers and link in ("left_finger_a", "left_finger_b"):
                fingers[obj].add(link)
                if finger_impulses is not None:
                    finger_impulses[obj][link] = finger_impulses[obj].get(
                        link, 0.0
                    ) + float(row["impulse"])
                touching_gripper.add(obj)
            elif obj in fingers:
                other_support.add(obj)
            elif obj and obj not in COLORS and obj != "floor":
                impact.add((link, obj))
        # Keep even unloaded furniture manifolds in the conservative collision audit.
        for contact in state.debug_contacts(viewer):
            a, b = contact["body_a"], contact["body_b"]
            if a not in self.links:
                a, b = b, a
            link, obj = self.links.get(a), self.object_ids.get(b)
            if link and obj and obj not in COLORS and obj != "floor":
                impact.add((link, obj))
        self.supported += int(ground)
        if not ground:
            last = self.unsupported_ranges[-1] if self.unsupported_ranges else None
            if (
                last
                and last["phase"] == self.phase
                and last["last_step"] + 1 == sim.tick
            ):
                last["last_step"] = sim.tick
            else:
                self.unsupported_ranges.append(
                    dict(phase=self.phase, first_step=sim.tick, last_step=sim.tick)
                )
        self.collisions.update(f"{self.phase}/{link}/{obj}" for link, obj in impact)
        q = np.asarray(sim.robot.get_qpos())[sim.dofs]
        if not np.isfinite(q).all():
            raise ValueError("Nonfinite measured joint state")
        for i, j in enumerate(sim.names):
            if j in self.bounds:
                lo, hi = self.bounds[j]
                self.excursions[j] = max(
                    self.excursions[j], float(max(0, lo - q[i], q[i] - hi))
                )
        jaws = [
            (link.get_pos(), quat_wxyz_to_matrix(link.get_quat()), bounds)
            for link, bounds in self.jaws
        ]
        for n, cube in self.cubes.items():
            pos = np.asarray(cube.get_pos()).copy()
            if not np.isfinite(pos).all():
                raise ValueError("Nonfinite cube state")
            elevated = pos[2] - self.initial[n][2] > 0.05
            if self.pick_contacts is not None:
                self.pick_contacts[n] = (finger_impulses[n], n in other_support)
            if self.airborne_in_jaw is not None:
                self.airborne_in_jaw[n] = bool(
                    elevated and fingers[n] and n not in other_support
                )
            if n not in other_support and len(fingers[n]) == 2 and elevated:
                self.grasped[n] += 1
            gap = min(
                finger_gap(pos, p, r, bounds, self.radii[n]) for p, r, bounds in jaws
            )
            self.tail[n].append((pos, n in table_support, n in touching_gripper, gap))

    def result(self, sim):
        table = self.table.get_pos()
        top = float(table[2] + self.size[2] / 2)
        cubes = {}
        for n in self.colors:
            tail = list(self.tail[n])
            if not tail:
                continue
            positions = np.array([p for p, _, _, _ in tail])
            contacts = sum(int(supported) for _, supported, _, _ in tail)
            min_gap = min(gap for _, _, _, gap in tail)
            detached = min_gap > 0.001 and all(not held for _, _, held, _ in tail)
            pos = positions[-1]
            # A 40 mm cube can project half its space diagonal onto either horizontal axis.
            edge_clearance = float(
                np.min(self.size[:2] / 2 - np.abs(pos[:2] - table[:2]))
            )
            inside = edge_clearance > 0.035
            travel = float(np.linalg.norm(np.ptp(positions, axis=0)))
            passed = (
                self.grasped[n] >= 20
                and contacts == 100
                and detached
                and inside
                and top + 0.015 <= pos[2] <= top + 0.037
                and travel < 0.003
            )
            cubes[n] = dict(
                passed=bool(passed),
                opposing_finger_airborne_ticks=self.grasped[n],
                final_position_m=pos.tolist(),
                table_contact_tail_ticks=contacts,
                detached=detached,
                min_finger_clearance_tail_m=min_gap,
                cube_center_over_table=inside,
                nearest_table_edge_m=edge_clearance,
                required_table_inset_m=0.035,
                settle_motion_m=travel,
            )
        return dict(
            cubes=cubes,
            table_top_m=top,
            wheel_supported_ticks=self.supported,
            unsupported_wheel_ranges=self.unsupported_ranges,
            physics_ticks=sim.tick,
            robot_furniture_contacts=dict(self.collisions),
            max_joint_excursion_rad=max(self.excursions.values(), default=0),
            root_pose_writes=sim.root_pose_writes,
        )


def execute(
    out,
    seed,
    colors,
    record=False,
    room_objects=(),
    room_settings=None,
    observation_progress=False,
    live_preview=False,
    observation_period_s=1,
    target_motion="stationary",
):
    from nano_grasp_estimator import parameters

    controller = out / "controller.json"
    plan = json.loads(controller.read_bytes()) if controller.is_file() else {}
    detector = parameters(plan)
    if detector is not None and plan["model_hash"] != worker.digest(MODEL):
        raise ValueError("Nano grasp checkpoint requires its training URDF")
    sim = LivingSimulation(
        MODEL,
        specs=[*scene_recipe(seed), *room_objects],
        resolution=(480, 360),
        posture=POSTURE,
        room_settings=room_settings,
    )
    dof = sim.robot.get_joint("left_j6_gripper").dofs_idx_local
    sim.robot.set_dofs_kp([5.0], dof)
    sim.robot.set_dofs_kv([0.25], dof)
    camera_fields = ("parent_link", "link_from_camera_gl", "intrinsics", "resolution")
    calibration = {
        n: {k: sim.camera_specs[n][k] for k in camera_fields} for n in CAMERAS
    }
    config = dict(
        model=str(MODEL),
        joint_names=sim.names,
        cameras=calibration,
        colors=colors,
        rolling_radius_m=WHEEL_RADIUS,
        physics_dt_s=sim.scene.dt,
        optical_calibration_source=(room_settings or {}).get(
            "calibration_source", "Nominal URDF-mounted simulation optics"
        ),
    )
    if plan.get("track_grasp"):
        config["grasp_rgb_association"] = "wrist-agreement-v1"
    worker.save(out / "calibration.json", config)
    if detector is not None:
        config["grasp_plan"] = {
            k: plan[k]
            for k in (
                "grasp_estimator",
                "grasp_checkpoint",
                "training_dataset",
                "parent_controller_sha256",
                "model_hash",
            )
        }
    if plan.get("track_grasp"):
        config["track_grasp"] = True
    slides = {}
    metadata = worker.describe("Nano")
    xml = ET.parse(MODEL)
    speeds = {
        j: float(xml.find(f"joint[@name='{j}']/limit").get("velocity"))
        for j in metadata["limits"]
    }
    guard = WheelGuard()
    judge = Judge(
        sim,
        colors,
        record_labels=record or detector is not None,
        trace_fingers=target_motion != "stationary",
    )
    envelope = MotorEnvelope(MODEL, sim.names, sim.scene.dt)
    journal = out / "motor-targets.f64"
    policy = HERE / "nano_rgb_transfer_policy.py"
    source_path = out / "source.json"
    source = json.loads(source_path.read_text()) if source_path.exists() else {}
    worker.save(
        source_path,
        source
        | dict(
            recipe=RECIPE,
            **(
                {"grasp_motion": target_motion} if target_motion != "stationary" else {}
            ),
            seed=seed,
            model_sha256=worker.digest(MODEL),
            policy_sha256=worker.digest(policy),
            support_code_hashes={
                f.name: worker.digest(f)
                for f in (
                    HERE / "nano_grasp_estimator.py",
                    *(
                        (HERE / "nano_moving_grasp.py",)
                        if target_motion != "stationary"
                        else ()
                    ),
                    HERE / "nano_rgb_table.py",
                    HERE / "nano_actuator_audit.py",
                    HERE / "nano_visuals.py",
                    HERE / "verify_arm_reach.py",
                    HERE / "nano_rgb_policy.py",
                    HERE / "nano_living.py",
                    HERE / "nano_cameras.py",
                    HERE / "run_rgb_approach.py",
                    HERE.parent / "nano-three-cubes" / "model.py",
                    Path(__file__),
                )
            },
            nexus=version("dimforge-nexus3d"),
        ),
    )

    rows = []
    samples = []
    if record:
        for name in CAMERAS:
            (out / "camera" / name).mkdir(parents=True)
    active_color = colors[0]

    preview_times = []
    preview_period_ns = 1_000_000_000 // 30
    next_preview_ns = time.monotonic_ns()

    def head_rgb():
        return sim.rig["head"].render(rgb=True, depth=False, segmentation=False)[0]

    def show_preview(rgb):
        tmp = out / "preview.tmp"
        tmp.write_bytes(struct.pack(">I", sim.tick) + rgb.tobytes())
        tmp.replace(out / "preview.rgb")
        if live_preview:
            preview_times.append((sim.tick, time.monotonic_ns()))

    def publish(sample=False, rgb=None):
        if rgb is None:
            rgb = head_rgb()
        if sample:
            for name in CAMERAS:
                frame = (
                    rgb
                    if name == "head"
                    else sim.rig[name].render(
                        rgb=True, depth=False, segmentation=False
                    )[0]
                )
                Image.fromarray(frame).save(
                    out / "camera" / name / f"{len(samples):04d}.png", compress_level=1
                )
            samples.append(
                dict(
                    step=sim.tick,
                    state=np.asarray(sim.robot.get_qpos())[sim.dofs].tolist(),
                    action=sim.targets.tolist(),
                    airborne_in_jaw=[judge.airborne_in_jaw[n] for n in colors],
                )
            )
        if not live_preview or not sample:
            show_preview(rgb)
        row = dict(
            kind="table-transfer",
            step=sim.tick + 1,
            state=judge.phase,
            color=active_color,
            requested=colors,
            table_contacts=sum(bool(v) and v[-1][1] for v in judge.tail.values()),
        )
        if not rows or rows[-1]["step"] != row["step"]:
            rows.append(row)
        worker.save(out / "progress.json", row)
        return rgb

    def step():
        nonlocal next_preview_ns
        if sim.tick >= 60000:
            raise ValueError("episode step budget exhausted")
        slide = slides.get(active_color)
        applied = slide.before_step() if slide else False
        motors.apply(sim.robot, sim.dofs, sim.targets)
        sim.scene.step()
        sim.tick += 1
        judge.tick(sim)
        if slide:
            slide.after_step(*judge.pick_contacts[active_color], applied)
        rgb = None
        if live_preview:
            now = time.monotonic_ns()
            if now >= next_preview_ns:
                next_preview_ns += (
                    1 + (now - next_preview_ns) // preview_period_ns
                ) * preview_period_ns
                rgb = head_rgb()
                show_preview(rgb)
        if record and sim.tick % 20 == 0:
            publish(sample=True, rgb=rgb)
        elif observation_progress and sim.tick % 100 == 0:
            if sim.tick % (100 * observation_period_s) == 0:
                rgb = sim.rig["head"].render(rgb=True, depth=False, segmentation=False)[
                    0
                ]
                worker.publish_background_rgb(out, sim.tick, rgb)
            worker.save(
                out / "progress.json",
                dict(
                    kind="table-transfer",
                    step=sim.tick,
                    state=judge.phase,
                    color=active_color,
                    requested=colors,
                ),
            )

    def move(command):
        if command["op"] == "wheel":
            speed, turn = command["speed"], command["turn"]
            if not -0.08 <= speed <= 0.13 or not -0.5 <= turn <= 0.5:
                raise ValueError("wheel command outside the Nano envelope")
            rates = guard.accept(wheel_rates(speed, 0, turn), guard.next_seq)
            for _ in range(20):
                sim.targets[sim.wheel_indices] += rates * 0.01
                step()
        else:
            values = command["values"]
            count = command["steps"]
            if (
                type(count) is not int
                or not 1 <= count <= 1000
                or not values
                or any(
                    j not in metadata["limits"]
                    or type(v) not in (int, float)
                    or not metadata["limits"][j][0] <= v <= metadata["limits"][j][1]
                    for j, v in values.items()
                )
            ):
                raise ValueError("joint command outside the Nano envelope")
            indices = [sim.names.index(j) for j in values]
            start = sim.targets[indices].copy()
            delta = np.array(list(values.values())) - start
            count = max(
                count,
                int(
                    np.ceil(
                        np.max(
                            1.5 * np.abs(delta) / np.array([speeds[j] for j in values])
                        )
                        / 0.01
                    )
                ),
            )
            for i in range(count):
                f = (i + 1) / count
                sim.targets[indices] = start + f * f * (3 - 2 * f) * delta
                step()
            for _ in range(20):
                step()

    phases = []
    error = None
    finished = False
    frame_count = 0
    packet = None
    with (
        journal.open("wb") as motor_log,
        (out / "actions.jsonl").open("w") as log,
        (out / "policy.log").open("w") as err,
        tempfile.TemporaryDirectory() as empty,
    ):
        motors = MotorWriter(motor_log, envelope, sim.targets)
        with subprocess.Popen(
            [sys.executable, str(policy)],
            cwd=empty,
            env=os.environ
            | {"PYTHONPATH": str(Path(kinematics.__file__).resolve().parents[1])},
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=err,
        ) as child:
            try:
                child.stdin.write(json.dumps(config).encode() + b"\n")
                child.stdin.flush()
                while line := child.stdout.readline():
                    command = json.loads(line)
                    op = command["op"]
                    print(
                        json.dumps(dict(tick=sim.tick, **command)), file=log, flush=True
                    )
                    if op == "sense":
                        frame_count += 1
                        packet = dict(
                            encoders=np.asarray(sim.robot.get_qpos())[sim.dofs],
                            stamp_ns=np.int64(sim.tick * 10_000_000),
                        )
                        for n in CAMERAS:
                            packet[n] = sim.rig[n].render(
                                rgb=True, depth=False, segmentation=False
                            )[0]
                        data = io.BytesIO()
                        np.savez(data, **packet)
                        payload = data.getvalue()
                        child.stdin.write(struct.pack(">I", len(payload)) + payload)
                    elif op == "jaw observation" and command["color"] == active_color:
                        if judge.airborne_in_jaw is not None:
                            with (out / "jaw-diagnostics.jsonl").open(
                                "a"
                            ) as diagnostics:
                                print(
                                    json.dumps(
                                        dict(
                                            tick=sim.tick,
                                            color=active_color,
                                            visible=command["visible"],
                                            airborne_in_jaw=judge.airborne_in_jaw[
                                                active_color
                                            ],
                                            gripper_rad=float(
                                                packet["encoders"][
                                                    sim.names.index("left_j6_gripper")
                                                ]
                                            ),
                                            roll_rad=float(
                                                packet["encoders"][
                                                    sim.names.index("left_j5_roll")
                                                ]
                                            ),
                                        )
                                    ),
                                    file=diagnostics,
                                )
                        # Retain the exact policy exposure used to confirm a grasp.
                        Image.fromarray(packet["left_wrist"]).save(
                            out / f"jaw-{sim.tick:05}-{active_color}.png"
                        )
                        child.stdin.write(b'{"ok":true}\n')
                    elif op == "grasp observation" and command["color"] == active_color:
                        if (
                            command["stamp_ns"] != int(packet["stamp_ns"])
                            or command["stamp_ns"] != sim.tick * 10_000_000
                        ):
                            raise ValueError(
                                "Grasp observation refers to a different exposure"
                            )
                        folder = out / "grasp-observations"
                        folder.mkdir(exist_ok=True)
                        np.savez_compressed(folder / f"{sim.tick:06}.npz", **packet)
                        if active_color in slides:
                            slides[active_color].observe(command)
                        child.stdin.write(b'{"ok":true}\n')
                    elif op == "table observation":
                        child.stdin.write(b'{"ok":true}\n')
                    elif op in ("wheel", "joint"):
                        move(command)
                        child.stdin.write(b'{"ok":true}\n')
                    elif (
                        op == "phase"
                        and command["color"] in colors
                        and command["name"]
                        in ("locate", "pick", "carry", "place", "release", "back away")
                    ):
                        with (out / "phase-measurements.jsonl").open("a") as private:
                            print(
                                json.dumps(dict(next=command, **judge.result(sim))),
                                file=private,
                            )
                        if judge.phase == "pick" and active_color in slides:
                            slides[active_color].finish_pick(
                                judge.grasped[active_color]
                            )
                        active_color = command["color"]
                        judge.phase = command["name"]
                        if (
                            target_motion != "stationary"
                            and judge.phase == "pick"
                            and active_color not in slides
                        ):
                            from nano_moving_grasp import FloorSlide

                            slides[active_color] = FloorSlide(
                                sim,
                                judge.cubes[active_color],
                                target_motion,
                                out / f"motion-{active_color}.jsonl",
                                float(judge.initial[active_color][2]),
                                judge.grasped[active_color],
                            )
                        phases.append(dict(tick=sim.tick, **command))
                        for name in CAMERAS:
                            frame = sim.rig[name].render(
                                rgb=True, depth=False, segmentation=False
                            )[0]
                            Image.fromarray(frame).save(
                                out
                                / f"{len(phases):02}-{command['color']}-{judge.phase.replace(' ', '-')}-{name}.png"
                            )
                        worker.save(out / "phases.json", phases)
                        child.stdin.write(b'{"ok":true}\n')
                    elif op == "finished":
                        finished = True
                        child.stdin.write(b'{"ok":true}\n')
                        child.stdin.flush()
                        break
                    else:
                        raise ValueError("Unknown transfer protocol command")
                    child.stdin.flush()
                if not finished:
                    raise ValueError(
                        "Controller stopped before completing the requested sequence"
                    )
            except Exception as exc:
                error = str(exc)
            finally:
                child.stdin.close()
                if not finished and child.poll() is None:
                    child.terminate()
                child.wait(timeout=10)
    if record:
        frame = publish()
        Image.fromarray(frame).save(out / "frame.png")
        if live_preview:
            worker.save(
                out / "preview-timing.json",
                dict(target_fps=30, clock="monotonic_ns", tick_time=preview_times),
            )
        worker.save(out / "trajectory.json", rows)
        worker.save(
            out / "recording.json",
            dict(
                joints=sim.names,
                fps=5,
                cameras=list(CAMERAS),
                label_colors=colors,
                samples=samples,
            ),
        )
    if error and (out / "policy.log").stat().st_size:
        error = (out / "policy.log").read_text().strip().splitlines()[-1][-350:]
    failed_observation = None
    if (
        error
        and judge.phase == "pick"
        and plan.get("track_grasp")
        and packet is not None
    ):
        snapshot = out / "last-pickup-observation.npz"
        np.savez_compressed(snapshot, **packet)
        failed_observation = dict(
            file=snapshot.name,
            sha256=worker.digest(snapshot),
            stamp_ns=int(packet["stamp_ns"]),
            stopped_tick=sim.tick,
            color=active_color,
        )
    audit = replay(journal, envelope, sim.tick)
    result = judge.result(sim)
    passed = (
        finished
        and child.returncode == 0
        and all(result["cubes"].get(n, {}).get("passed") for n in colors)
        and not judge.collisions
        and judge.supported == sim.tick
        and result["max_joint_excursion_rad"] <= 0.005
        and not sim.root_pose_writes
        and audit["passed"]
        and not motors.rejected
    )
    motion = {color: slide.result() for color, slide in slides.items()}
    if target_motion != "stationary":
        passed = (
            passed
            and set(motion) == set(colors)
            and all(v["passed"] for v in motion.values())
        )
    return result | dict(
        **(
            {"moving_grasp": dict(fixture=target_motion, cubes=motion)}
            if target_motion != "stationary"
            else {}
        ),
        **(
            {"last_pickup_observation": failed_observation}
            if failed_observation
            else {}
        ),
        passed=bool(passed),
        error=error,
        requested=colors,
        phase=judge.phase,
        onboard_observations=frame_count,
        measured_room_bodies=sim.room_readback(),
        actuator_audit=audit,
        rejected_motor_writes=motors.rejected,
        frames=len(samples),
        recording=bool(samples),
        carry_annotations=bool(samples),
        grasp_checkpoint=plan.get("grasp_checkpoint"),
        training_dataset=plan.get("training_dataset") if detector is not None else None,
        seed=seed,
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=0)
    delivery = parser.add_mutually_exclusive_group()
    delivery.add_argument("--colors", choices=COLORS, nargs="+")
    delivery.add_argument("--controller", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    colors = args.colors or list(COLORS)
    frozen = None
    if args.controller is not None:
        frozen = args.controller.read_bytes()
        plan = json.loads(frozen)
        profile = worker.describe("Nano")
        if (
            plan.get("model_hash") != profile["model_hash"]
            or plan.get("robot") != "Nano"
            or worker.validate(plan, profile).get("kind") != "table-transfer"
        ):
            parser.error("Choose a prepared Nano transfer artifact for this URDF")
        colors = plan["color_order"].split(",")
    if len(colors) != len(set(colors)):
        parser.error("select distinct cubes")
    args.out.mkdir(parents=True, exist_ok=False)
    if frozen is not None:
        (args.out / "controller.json").write_bytes(frozen)
        worker.save(
            args.out / "source.json",
            {"controller_sha256": worker.digest(args.out / "controller.json")},
        )
    result = execute(args.out, args.seed, colors)
    worker.save(args.out / "result.json", result)
    print(json.dumps(result, indent=2))
    sys.exit(0 if result["passed"] else 1)
