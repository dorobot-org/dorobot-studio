"""Batched independent worlds for the offline Nano reach audit."""

import math
import time
import xml.etree.ElementTree as ET
import numpy as np

from nano_living import NAVIGATION_POSTURE
from nano_teleop import model_joints

SUFFIXES = ("j1_yaw", "j2_pitch", "j3_pitch", "j4_pitch", "j5_roll", "j6_gripper")
MIRROR = np.array([1.0, -1.0, 1.0])


class PhysicsProbe:
    def __init__(self, model, plans):
        from aisle.sim.nexus_backend import _ensure_nexus, _new_scene, NexusRobot

        self.plans = plans
        self.cases = [
            (name, side, target, direction)
            for name, target, direction in plans
            for side in ("right", "left")
        ]
        self.count = len(self.cases)
        self.names, self.limits = model_joints(model)
        self.initial = np.array([NAVIGATION_POSTURE.get(n, 0.0) for n in self.names])
        self.engine = _ensure_nexus()
        self.scene = _new_scene(
            self.engine,
            {"sim": {"dt": 0.01, "gravity": [0, 0, -9.81]}},
            self.count,
            [0.6] * 3,
        )
        floor = self.scene.add_box(
            "floor",
            [12, 10, 0.1],
            [0, 0, -0.0843],
            fixed=True,
            friction=0.7,
            density=500,
            color=[0.65, 0.62, 0.57, 1.0],
        )
        natives = []
        for env in range(self.count):
            native = self.scene.state.load_urdf_robot(
                env,
                str(model),
                self.scene.nexus3d.UrdfLoaderOptions(
                    make_roots_fixed=True, convex_hull=True
                ),
            )
            self.scene.state.set_robot_joint_dynamics(
                native, armature=[0.01] * native.n_dofs, damping=[0.1] * native.n_dofs
            )
            natives.append(native)
        self.robot = NexusRobot(self.scene, natives, "adora_nano")
        self.scene.robots.append(self.robot)
        self.dofs = [self.robot.get_joint(n).dofs_idx_local[0] for n in self.names]
        self.scene.build()
        self.natives = natives
        stride = self.scene.state.multibody_links_per_env()
        root = ET.parse(model).getroot()
        children = [
            root.find(f"joint[@name='{n}']/child").get("link") for n in self.names
        ]
        joint_slots = []
        tcp_slots = []
        self.body_names = {}
        for env, native in enumerate(natives):
            slots = dict(
                zip(native.link_names, self.scene.state.robot_link_slots(native))
            )
            joint_slots.append([env * stride + slots[n] for n in children])
            tcp_slots.append(env * stride + slots[self.cases[env][1] + "_tool_mount"])
            for handle, name in zip(native.link_body_handles, native.link_names):
                self.body_names[self.scene.gpu_index(env, handle)] = (env, name)
            self.body_names[self.scene.gpu_index(env, floor.handles[env])] = (
                env,
                "floor",
            )
        self.joint_slots = np.array(joint_slots)
        self.tcp_slots = np.array(tcp_slots)
        self.traces = []
        self.results = {}

    def read(self):
        coords, positions, *_ = self.scene.state.read_multibody_links(
            self.engine.viewer
        )
        # Nexus 0.2.1's URDF revolute joints store their scalar in slot 3.
        # Its readback docstring says slot 5; check against robot_state below
        # so a bridge-layout change cannot silently validate zero angles.
        return np.asarray(coords)[self.joint_slots, 3], np.asarray(positions)[
            self.tcp_slots
        ]

    def run(self):
        # Reset only before the test. Each world has a different active arm and
        # target; all other coordinates and geometry are identical.
        q = np.zeros((self.count, self.robot.n_dofs))
        q[:, self.dofs] = self.initial
        self.robot.set_qpos(q)
        for _ in range(100):
            self.robot.control_dofs_position(self.initial, self.dofs)
            self.scene.step()
        start, positions = self.read()
        # Validate the optimized one-transfer read against the public robot API.
        for env in (0, self.count // 2, self.count - 1):
            reference = self.scene.state.robot_state(
                self.engine.viewer, self.natives[env]
            )
            np.testing.assert_allclose(
                start[env], np.asarray(reference[0])[self.dofs], atol=1e-7
            )
            link_index = list(self.natives[env].link_names).index(
                self.cases[env][1] + "_tool_mount"
            )
            np.testing.assert_allclose(
                positions[env], np.asarray(reference[1])[link_index], atol=1e-7
            )
        targets = np.broadcast_to(self.initial, (self.count, len(self.names))).copy()
        indices = []
        for env, (_, side, target, _) in enumerate(self.cases):
            ids = [self.names.index(side + "_" + s) for s in SUFFIXES]
            indices.append(ids)
            targets[env, ids] = target
        travel = max(1, math.ceil(float(np.abs(targets - start).max()) / 0.3 / 0.01))
        rows = []
        tcp = []
        commands = []
        times = []
        contact_frames = []
        contacts = [set() for _ in self.cases]
        began = time.monotonic()
        for tick in range(travel + 300):
            command = start + (targets - start) * min(1.0, (tick + 1) / travel)
            self.robot.control_dofs_position(command, self.dofs)
            self.scene.step()
            if tick % 10 == 0 or tick == travel + 299:
                actual, pos = self.read()
                rows.append(actual.copy())
                tcp.append(pos.copy())
                commands.append(command.copy())
                times.append((tick + 1) * 0.01)
                touching = np.zeros(self.count, dtype=bool)
                for contact in self.scene.state.debug_contacts(self.engine.viewer):
                    a = self.body_names[int(contact["body_a"])]
                    b = self.body_names[int(contact["body_b"])]
                    if a[0] != b[0]:
                        raise RuntimeError("Contact crossed isolated environments")
                    env = a[0]
                    side = self.cases[env][1]
                    if a[1].startswith(side + "_") or b[1].startswith(side + "_"):
                        contacts[env].add(tuple(sorted((a[1], b[1]))))
                        touching[env] = True
                contact_frames.append(touching)
            if tick % 100 == 0:
                print(
                    f"physics worlds={self.count} tick={tick}/{travel + 300} wall_s={time.monotonic() - began:.1f}",
                    flush=True,
                )
        rows = np.asarray(rows)
        tcp = np.asarray(tcp)
        commands = np.asarray(commands)
        contact_frames = np.asarray(contact_frames)
        actual, pos = self.read()
        # Verify every final world against the regular readback as well.
        for env, native in enumerate(self.natives):
            reference = self.scene.state.robot_state(self.engine.viewer, native)
            np.testing.assert_allclose(
                actual[env], np.asarray(reference[0])[self.dofs], atol=1e-7
            )
        for env, (name, side, target, direction) in enumerate(self.cases):
            ids = indices[env]
            best_projection = best_tcp = None
            if direction is not None:
                direction = np.asarray(direction) * (MIRROR if side == "left" else 1)
                projections = tcp[:, env] @ direction
                best = int(np.argmax(projections))
                best_projection = float(projections[best])
                best_tcp = tcp[best, env].tolist()
            self.traces.append(
                dict(
                    name=name,
                    side=side,
                    q=rows[:, env],
                    tcp=tcp[:, env],
                    command=commands[:, env],
                    time_s=np.asarray(times),
                    contact=contact_frames[:, env],
                )
            )
            result = dict(
                side=side,
                target_rad=list(target),
                actual_rad=actual[env, ids].tolist(),
                tcp_m=pos[env].tolist(),
                target_error_deg=float(
                    np.degrees(np.abs(actual[env, ids] - target)).max()
                ),
                best_projection_m=best_projection,
                best_tcp_m=best_tcp,
                contacts=sorted(contacts[env]),
                trace_index=env,
                duration_s=(travel + 300) * 0.01,
            )
            self.results.setdefault(name, {})[side] = result

    def pair(self, name):
        pair = self.results[name]
        left, right = pair["left"], pair["right"]
        endpoint = float(
            np.linalg.norm(np.array(left["tcp_m"]) - np.array(right["tcp_m"]) * MIRROR)
        )
        angles = float(
            np.degrees(np.abs(np.array(left["actual_rad"]) - right["actual_rad"])).max()
        )
        lt, rt = self.traces[left["trace_index"]], self.traces[right["trace_index"]]
        path_error = np.linalg.norm(lt["tcp"] - rt["tcp"] * MIRROR, axis=1) * 1000
        peak = int(np.argmax(path_error))
        return dict(
            name=name,
            left=left,
            right=right,
            mirror_endpoint_error_mm=endpoint * 1000,
            max_joint_difference_deg=angles,
            max_path_mirror_error_mm=float(path_error[peak]),
            path_error_peak_time_s=float(lt["time_s"][peak]),
            contact_at_path_error_peak=bool(lt["contact"][peak] or rt["contact"][peak]),
            path_within_5mm=bool(path_error.max() <= 5),
            symmetry_pass=endpoint <= 0.005 and angles <= 1.0,
        )
