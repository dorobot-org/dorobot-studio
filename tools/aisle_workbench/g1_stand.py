"""Floating-base G1 development fixture for tracking a shoulder target while standing."""

import json
from pathlib import Path
import xml.etree.ElementTree as ET

import numpy as np
from PIL import Image

from worker import digest, save


def simulate(out, model, plan):
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
    robot.set_dofs_kp(
        [
            180 if "knee" in n or "hip" in n else 65 if "ankle" in n else 70
            for n in names
        ],
        dofs,
    )
    robot.set_dofs_kv(
        [5 if any(j in n for j in ("knee", "hip", "ankle")) else 3 for n in names], dofs
    )
    scene.build()
    floor_id = scene.gpu_index(0, floor.handles[0])
    ankles = [names.index(f"{s}_ankle_pitch_joint") for s in ("left", "right")]
    shoulder = names.index(plan["joint"])
    targets = np.zeros(len(names))
    initial_xy = robot.get_pos()[:2].copy()
    previous_pitch = 0.0
    warmup, scored = 200, 2000
    both = 0
    peak_tilt = drift = excursion = 0.0
    other_links = set()
    rows = []
    source = json.loads((out / "source.json").read_text())
    save(
        out / "source.json",
        source
        | dict(
            control_sha256=digest(Path(__file__)),
            physics_dt_s=dt,
            gravity_m_s2=-9.81,
            warmup_steps=warmup,
            floor_z_m=float(floor_z),
        ),
    )
    correction = 0.0
    initial_angle = None
    for i in range(warmup + scored):
        # The control law consumes only body attitude (simulated IMU) and joint state.
        w, x, y, z = robot.get_quat()
        pitch = np.arcsin(np.clip(2 * (w * y - z * x), -1, 1))
        desired = targets.copy()
        desired[ankles] = np.clip(
            2.5 * pitch + 0.23 * (pitch - previous_pitch) / dt, -0.4, 0.4
        )
        previous_pitch = pitch
        if i >= warmup:
            angle = float(robot.get_qpos()[dofs[shoulder]])
            if initial_angle is None:
                initial_angle = angle
            reference = initial_angle + plan["delta_rad"] * min(
                1, (i - warmup + 1) * dt
            )
            correction = float(
                np.clip(correction + 1.5 * dt * (reference - angle), -0.04, 0.04)
            )
            desired[shoulder] = reference + correction
        targets += np.clip(np.clip(desired, lo, hi) - targets, -speed * dt, speed * dt)
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
        if step % 20 == 0 or step == 1:
            row = dict(
                kind="standing-inspection",
                step=step,
                measured_rad=float(q[shoulder]),
                tilt_deg=float(np.degrees(tilt)),
                both_feet=feet.issubset(loaded),
            )
            rows.append(row)
            rgb = np.asarray(camera.render()[0], dtype=np.uint8)
            tmp = out / "preview.tmp"
            tmp.write_bytes((step - 1).to_bytes(4, "big") + rgb.tobytes())
            tmp.replace(out / "preview.rgb")
            save(out / "progress.json", row)
    Image.fromarray(rgb).save(out / "frame.png")
    save(out / "trajectory.json", rows)
    measured = float(q[shoulder] - initial_angle)
    passed = (
        both == scored
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
        fixture="standing-development",
        joint=plan["joint"],
        requested_delta=plan["delta_rad"],
        measured_delta=measured,
        within_limits=excursion <= 0.005,
        max_joint_excursion_rad=excursion,
        steps=scored,
        frames=len(rows),
        both_feet_loaded_steps=both,
        floor_contact_links_other_than_feet=sorted(other_links),
        peak_tilt_deg=float(np.degrees(peak_tilt)),
        max_pelvis_drift_m=drift,
        root_pose_writes=0,
    )
