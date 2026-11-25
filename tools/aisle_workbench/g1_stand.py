"""Floating-base G1 development fixture for tracking a shoulder target while standing."""

import json
from pathlib import Path
import xml.etree.ElementTree as ET

import numpy as np
from PIL import Image

from worker import BackgroundCadence, digest, publish_background_rgb, save


def impact_parameters(values):
    bounds = {
        "mass_kg": (0.20, 0.30),
        "speed_m_s": (1.10, 1.30),
        "lateral_m": (-0.015, 0.015),
    }
    if not isinstance(values, dict) or set(values) != set(bounds):
        raise ValueError("Expected block mass, launch speed and lateral offset")
    for name, (low, high) in bounds.items():
        if type(values[name]) not in (float, int) or not low <= values[name] <= high:
            raise ValueError(f"Impact {name} must be between {low} and {high}")
    return values


DEFAULT_IMPACT_AIM = "frontal-torso-v3"


def launch_position(recipe, aim):
    if aim == "pelvis-v1":
        return [0.29, recipe["lateral_m"], 0.16]
    if aim == DEFAULT_IMPACT_AIM:
        # With the standing controller engaged, the frontal torso hull reaches
        # x=0.109 m at z=0.195 m. The 65 mm block touches around x=0.14 m.
        # A 55 mm approach limits gravity-induced drop before frontal contact.
        flight_s = (0.195 - 0.14) / recipe["speed_m_s"]
        return [0.195, recipe["lateral_m"], 0.195 + 0.5 * 9.81 * flight_s**2]
    if aim != "torso-v2":
        raise ValueError("Unknown G1 impact calibration")
    # The neutral G1 torso collision mesh spans z=0.035..0.356 m in the pelvis
    # frame. Aim at its mid-height at x=0.05 m, compensating for ballistic drop.
    flight_s = (0.29 - 0.05) / recipe["speed_m_s"]
    return [0.29, recipe["lateral_m"], 0.195 + 0.5 * 9.81 * flight_s**2]


class StandingController:
    """Generate bounded targets from encoder and IMU samples, before each physics step."""

    def __init__(self, plan, names, lo, hi, speed, dt, warmup, reach_weights=None):
        self.plan = plan
        self.bounds = (lo, hi)
        self.max_step = speed * dt
        self.dt = dt
        self.warmup = warmup
        self.weights = reach_weights
        self.ankles = [
            names.index(f"{side}_ankle_pitch_joint") for side in ("left", "right")
        ]
        self.waist_pitch = names.index("waist_pitch_joint")
        self.waist_yaw = names.index("waist_yaw_joint")
        self.shoulder = names.index(plan["joint"])
        self.targets = np.zeros(len(names))
        self.initial_angle = None
        self.previous_pitch = 0.0
        self.correction = 0.0
        self.step = 0

    def advance(self, q, quaternion):
        w, x, y, z = quaternion
        pitch = np.arcsin(np.clip(2 * (w * y - z * x), -1, 1))
        desired = self.targets.copy()
        desired[self.waist_pitch] = self.plan.get("trunk_pitch_rad", 0.0)
        desired[self.waist_yaw] = self.plan.get("trunk_yaw_rad", 0.0)
        desired[self.ankles] = np.clip(
            2.5 * pitch + 0.23 * (pitch - self.previous_pitch) / self.dt, -0.4, 0.4
        )
        self.previous_pitch = pitch
        if self.step >= self.warmup:
            angle = float(q[self.shoulder])
            if self.initial_angle is None:
                self.initial_angle = angle
            phase = min(1.0, (self.step - self.warmup + 1) * self.dt)
            reference = self.initial_angle + self.plan["delta_rad"] * phase
            error = reference - angle
            if self.weights is None:
                self.correction += 1.5 * self.dt * error
            else:
                from g1_reach_checkpoint import features

                self.correction = self.weights @ features(phase, error)
            self.correction = float(np.clip(self.correction, -0.04, 0.04))
            desired[self.shoulder] = reference + self.correction
        self.targets += np.clip(
            np.clip(desired, *self.bounds) - self.targets, -self.max_step, self.max_step
        )
        self.step += 1
        return self.targets


def simulate(
    out,
    model,
    plan,
    impact=False,
    parameters=None,
    reach_weights=None,
    impact_aim=DEFAULT_IMPACT_AIM,
    observations_only=False,
    observation_period_s=1,
    observation_clock="simulation",
):
    from aisle.kinematics import load_urdf_chain
    from aisle.sim import nexus_backend as nx

    robot_xml = ET.parse(model)
    feet = {f"{side}_ankle_roll_link" for side in ("left", "right")}
    # Place the floor under the collision spheres at neutral posture. The pelvis is free.
    sole_heights = []
    for foot in feet:
        chain = load_urdf_chain(model, "pelvis", foot)
        p, rot = chain.forward(np.zeros(len(chain.joint_names)))
        for collision in robot_xml.findall(f"link[@name='{foot}']/collision"):
            center = np.fromstring(collision.find("origin").get("xyz"), sep=" ")
            radius = float(collision.find("geometry/sphere").get("radius"))
            sole_heights.append((p + rot @ center)[2] - radius)
    floor_z = min(sole_heights) - 0.002
    dt = 0.005
    engine = nx._ensure_nexus()
    scene = nx._new_scene(
        engine, {"sim": {"dt": dt, "gravity": [0, 0, -9.81]}}, 1, [0.65] * 3
    )
    floor = scene.add_box(
        "floor", [5, 5, 0.10], [0, 0, floor_z - 0.05], fixed=True, friction=0.8
    )
    # Batch archives retain the full-rate numerical traces, while operator trials
    # additionally save the camera and joint/action samples for native Replay.
    record_episode = parameters is None and not observations_only
    cadence = BackgroundCadence(observation_clock, observation_period_s, 200)
    samples = []
    if record_episode:
        (out / "camera").mkdir()
    parameters = impact_parameters(
        parameters
        if parameters is not None
        else {"mass_kg": 0.25, "speed_m_s": 1.20, "lateral_m": 0.0}
    )
    block_side, block_mass = 0.065, parameters["mass_kg"]
    block = None
    if impact:
        block = scene.add_box(
            "disturbance_block",
            [block_side] * 3,
            [1, 0, floor_z + block_side],
            density=block_mass / block_side**3,
            color=(0.94, 0.56, 0.12),
        )
    native = scene.state.load_urdf_robot(
        0,
        str(model),
        scene.nexus3d.UrdfLoaderOptions(make_roots_fixed=False, convex_hull=True),
    )
    scene.state.set_robot_joint_dynamics(
        native, armature=[0.03] * native.n_dofs, damping=[0.1] * native.n_dofs
    )
    robot = nx.NexusRobot(scene, [native], "g1")
    scene.robots.append(robot)
    camera = scene.add_camera(
        (480, 320), 45, pos=(1.8, -1.3, 0.38), lookat=(0, 0, -0.13)
    )
    joints = [j for j in robot.joints if len(j.dofs_idx_local) == 1]
    names = [j.name for j in joints]
    dofs = [j.dofs_idx_local[0] for j in joints]
    limits = [robot_xml.find(f"joint[@name='{n}']/limit") for n in names]
    lo, hi, speed, effort = np.array(
        [
            [float(v.get(k)) for k in ("lower", "upper", "velocity", "effort")]
            for v in limits
        ]
    ).T
    max_force = np.array(native.max_force)
    max_force[dofs] = effort
    native.max_force = max_force.tolist()
    kp = np.array(
        [
            180 if "knee" in n or "hip" in n else 65 if "ankle" in n else 70
            for n in names
        ]
    )
    kd = np.array(
        [5 if any(j in n for j in ("knee", "hip", "ankle")) else 3 for n in names]
    )
    robot.set_dofs_kp(kp.tolist(), dofs)
    robot.set_dofs_kv(kd.tolist(), dofs)
    scene.build()
    floor_id = scene.gpu_index(0, floor.handles[0])
    waist_yaw = names.index("waist_yaw_joint")
    heading = plan.get("trunk_yaw_rad", 0.0)
    waist_pitch = names.index("waist_pitch_joint")
    trunk_target = plan.get("trunk_pitch_rad", 0.0)
    shoulder = names.index(plan["joint"])
    initial_xy = robot.get_pos()[:2].copy()
    block_id = scene.gpu_index(0, block.handles[0]) if block is not None else None
    launch_at = 600  # Three seconds into the scored trial.
    launch_pos = launch_position(parameters, impact_aim)
    launch_velocity = [-parameters["speed_m_s"], 0, 0]
    wrist_name = plan["joint"].split("_", 1)[0] + "_wrist_yaw_link"
    wrist = robot.get_link(wrist_name)
    chain = load_urdf_chain(model, "pelvis", wrist_name)
    chain_dofs = [dofs[names.index(j)] for j in chain.joint_names]
    wrist_rows, balance_rows = [], []
    wrist_start = wrist_goal = None
    warmup, scored = 200, 2000
    both = 0
    peak_tilt = drift = excursion = 0.0
    other_links = set()
    rows = []
    impact_rows, energy_rows = [], []
    last_contact = recovery_start = recovery_step = None
    positive_work = negative_work = 0.0
    source = json.loads((out / "source.json").read_text())
    save(
        out / "source.json",
        source
        | dict(
            control_sha256=digest(Path(__file__)),
            physics_dt_s=dt,
            gravity_m_s2=-9.81,
            warmup_steps=warmup,
            trunk_yaw_target_rad=heading,
            trunk_pitch_target_rad=trunk_target,
            floor_z_m=float(floor_z),
            disturbance=(
                dict(
                    calibration=impact_aim,
                    mass_kg=block_mass,
                    side_m=block_side,
                    initial_position_m=launch_pos,
                    initial_velocity_m_s=launch_velocity,
                    launch_step=launch_at + 1,
                )
                if impact
                else None
            ),
        ),
    )
    controller = StandingController(
        plan, names, lo, hi, speed, dt, warmup, reach_weights
    )
    controller_inputs = []
    for i in range(warmup + scored):
        before = robot.get_qpos()[dofs].astype(float)
        orientation = robot.get_quat().copy()
        targets = controller.advance(before, orientation)
        if i == warmup:
            initial_angle = controller.initial_angle
            # The evaluator freezes a world-frame goal against the warmed-up pose.
            chain_q = robot.get_qpos()[chain_dofs].copy()
            relative_start = chain.forward(chain_q)[0]
            chain_q[chain.joint_names.index(plan["joint"])] += plan["delta_rad"]
            relative_delta = chain.forward(chain_q)[0] - relative_start
            wrist_start = wrist.get_pos().astype(float).copy()
            initial_root_quat = robot.get_quat().copy()
            wrist_goal = (
                wrist_start + nx.quat_wxyz_to_matrix(initial_root_quat) @ relative_delta
            )
        if block is not None and i == warmup + launch_at:
            # Only the independent fixture body receives an initial pose and velocity.
            scene.set_body_pose(0, block.handles[0], launch_pos, [1, 0, 0, 0])
            scene.set_body_velocity(0, block.handles[0], launch_velocity)
        velocity = robot.get_dofs_velocity()[dofs].astype(float)
        if record_episode:
            controller_inputs.append((before, orientation, velocity, targets.copy()))
        robot.control_dofs_position(targets.tolist(), dofs)
        scene.step()
        q = robot.get_qpos()[dofs]
        pos = robot.get_pos()
        rotation = nx.quat_wxyz_to_matrix(robot.get_quat())
        if not all(np.isfinite(v).all() for v in (q, pos, rotation)):
            raise ValueError("Nonfinite G1 sensor measurement")
        excursion = max(excursion, float(np.maximum(lo - q, q - hi).max()))
        if i < warmup:
            continue
        # Nexus exposes joint states and configured PD gains through this API.
        # Estimate joint work from capped PD demand and measured travel per step.
        torque = np.clip(kp * (targets - before) - kd * velocity, -effort, effort)
        work = torque * (q - before)
        if not np.isfinite(work).all():
            raise ValueError("Nonfinite estimated G1 motor work")
        positive_work += float(np.maximum(work, 0).sum())
        negative_work += float(np.minimum(work, 0).sum())
        energy_rows.append((before, velocity, targets.copy(), q.copy()))
        tilt = float(np.arccos(np.clip(rotation[2, 2], -1, 1)))
        peak_tilt = max(peak_tilt, tilt)
        drift = max(drift, float(np.linalg.norm(pos[:2] - initial_xy)))
        impulses = scene.state.debug_multibody_contact_impulses(engine.viewer)
        loaded = {
            robot.links[c["link"]].name
            for c in impulses
            if c["kind"] == "normal"
            and c["impulse"] > 1e-6
            and c["free_body"] == floor_id
        }
        both += int(feet.issubset(loaded))
        other_links.update(loaded - feet)
        step = i - warmup + 1
        wrist_xyz = wrist.get_pos().astype(float)
        wrist_error = float(np.linalg.norm(wrist_xyz - wrist_goal))
        if not np.isfinite(wrist_error):
            raise ValueError("Invalid G1 wrist position")
        wrist_rows.append(wrist_xyz.copy())
        balance_rows.append(
            (
                feet.issubset(loaded),
                bool(loaded - feet),
                tilt,
                np.linalg.norm(pos[:2] - initial_xy),
                q[shoulder] - initial_angle,
            )
        )
        if block is not None and step > launch_at:
            hits = [
                c
                for c in impulses
                if c["kind"] == "normal"
                and c["impulse"] > 1e-6
                and c["free_body"] == block_id
            ]
            if hits:
                last_contact = step
                recovery_start = recovery_step = None
                impact_rows.extend(
                    dict(
                        step=step,
                        link=robot.links[c["link"]].name,
                        normal_impulse_ns=c["impulse"],
                    )
                    for c in hits
                )
            elif last_contact is not None and recovery_step is None:
                stable = (
                    feet.issubset(loaded)
                    and not (loaded - feet)
                    and wrist_error <= 0.010
                    and tilt < np.deg2rad(4)
                    and np.linalg.norm(pos[:2] - initial_xy) < 0.03
                    and abs(q[shoulder] - initial_angle - plan["delta_rad"]) < 0.015
                )
                if stable:
                    if recovery_start is None:
                        recovery_start = step
                    if step - recovery_start + 1 >= 100:
                        recovery_step = step
                else:
                    recovery_start = None
        exposure_due = observations_only and cadence.due(step, first=step == 1)
        if exposure_due:
            rgb = np.asarray(camera.render()[0], dtype=np.uint8)
            publish_background_rgb(out, step - 1, rgb)
        if step % 20 == 0 or step == 1:
            row = dict(
                kind="standing-inspection",
                step=step,
                measured_rad=float(q[shoulder]),
                tilt_deg=float(np.degrees(tilt)),
                both_feet=feet.issubset(loaded),
            )
            rows.append(row)
            if observations_only:
                save(out / "progress.json", row)
                continue
            rgb = np.asarray(camera.render()[0], dtype=np.uint8)
            if record_episode and step % 20 == 0:
                Image.fromarray(rgb).save(out / "camera" / f"{len(samples):04d}.png")
                samples.append(
                    dict(step=step, state=q.tolist(), action=targets.tolist())
                )
            tmp = out / "preview.tmp"
            tmp.write_bytes((step - 1).to_bytes(4, "big") + rgb.tobytes())
            tmp.replace(out / "preview.rgb")
            save(out / "progress.json", row)
    Image.fromarray(rgb).save(out / "frame.png")
    save(out / "trajectory.json", rows)
    if record_episode:
        save(
            out / "recording.json",
            dict(joints=names, samples=samples, fps=10, cameras=["operator"]),
        )
    wrist_rows = np.asarray(wrist_rows)
    reach = dict(
        link=wrist_name,
        frame="world",
        start_m=wrist_start.tolist(),
        target_m=wrist_goal.tolist(),
        final_m=wrist_rows[-1].tolist(),
        requested_travel_m=float(np.linalg.norm(wrist_goal - wrist_start)),
        final_error_m=wrist_error,
        max_final_100_error_m=float(
            np.linalg.norm(wrist_rows[-100:] - wrist_goal, axis=1).max()
        ),
        tolerance_m=0.010,
    )
    reach["passed"] = reach["max_final_100_error_m"] <= reach["tolerance_m"]
    np.savez_compressed(
        out / "wrist-trace.npz",
        measured_m=wrist_rows,
        target_m=wrist_goal,
        start_m=wrist_start,
        initial_root_quat=initial_root_quat,
        balance=np.asarray(balance_rows),
        dt=dt,
    )
    reach["trace_sha256"] = digest(out / "wrist-trace.npz")
    energy = np.asarray(energy_rows)
    np.savez_compressed(
        out / "motor-work.npz",
        joints=names,
        q_before=energy[:, 0],
        velocity_before=energy[:, 1],
        target=energy[:, 2],
        q_after=energy[:, 3],
        kp=kp,
        kd=kd,
        effort_limit=effort,
        dt=dt,
    )
    if record_episode:
        q_input, quat_input, dq_input, target_input = map(
            np.asarray, zip(*controller_inputs)
        )
        np.savez_compressed(
            out / "standing-inputs.npz",
            joints=names,
            q=q_input,
            quat=quat_input,
            dq=dq_input,
            target=target_input,
            dt=dt,
        )
    if impact:
        save(out / "impact-contacts.json", impact_rows)
    recovery_s = (
        (recovery_step - last_contact) * dt if recovery_step is not None else None
    )
    disturbance = (
        dict(
            contacts=len(impact_rows),
            normal_impulse_sum_ns=sum(r["normal_impulse_ns"] for r in impact_rows),
            links=sorted({r["link"] for r in impact_rows}),
            recovery_s=recovery_s,
            last_contact_step=last_contact,
            recovered_step=recovery_step,
            passed=bool(
                impact_rows
                and recovery_s is not None
                and recovery_s <= 2
                and (
                    impact_aim == "pelvis-v1"
                    or any(row["link"] == "torso_link" for row in impact_rows)
                )
            ),
        )
        if impact
        else None
    )
    measured = float(q[shoulder] - initial_angle)
    passed = (
        (not impact or (disturbance["passed"] and reach["passed"]))
        and both == scored
        and not other_links
        and drift < 0.10
        and peak_tilt < np.deg2rad(12)
        and excursion <= 0.005
        and abs(measured - plan["delta_rad"])
        < min(0.025, 0.15 * abs(plan["delta_rad"]))
    )
    return dict(
        kind="standing-inspection",
        task_success=bool(passed),
        fixture="standing-impact-development" if impact else "standing-development",
        disturbance=disturbance,
        wrist_reach=reach,
        trunk_pitch_target_rad=trunk_target,
        trunk_yaw_target_rad=heading,
        settled_trunk_yaw_rad=float(np.mean(energy[-100:, 3, waist_yaw])),
        settled_trunk_pitch_rad=float(np.mean(energy[-100:, 3, waist_pitch])),
        estimated_positive_joint_work_j=positive_work,
        estimated_negative_joint_work_j=negative_work,
        motor_work_sha256=digest(out / "motor-work.npz"),
        **(
            {"standing_inputs_sha256": digest(out / "standing-inputs.npz")}
            if record_episode
            else {}
        ),
        joint=plan["joint"],
        requested_delta=plan["delta_rad"],
        measured_delta=measured,
        within_limits=excursion <= 0.005,
        max_joint_excursion_rad=excursion,
        steps=scored,
        frames=len(rows),
        recording=record_episode,
        recorded_frames=len(samples),
        both_feet_loaded_steps=both,
        floor_contact_links_other_than_feet=sorted(other_links),
        peak_tilt_deg=float(np.degrees(peak_tilt)),
        max_pelvis_drift_m=drift,
        root_pose_writes=0,
    )
