"""Codex-operated Nano living-room simulation, using AISLE's Nexus wrapper.

The base uses two bounded planar translation drives. Arm and cube use Nexus
contact physics. No attachment, grasp weld, or cube pose writes are used.
Commands arrive as JSON lines on stdin; each produces observed state.
"""

import argparse
import hashlib
import importlib.metadata
import json
import math
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

import cv2
import numpy as np
from scipy.spatial.transform import Rotation

from model import GRIP_POINT, MODEL, Kinematics

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "aisle-validation"))
from nano_cameras import create_rig
from nano_living_stream import PreviewHub
from nano_visuals import register_visuals


class Demo:
    def __init__(self, output, aisle, socket_path=None):
        sys.path.insert(0, str(aisle / "src"))
        from aisle.sim.nexus_backend import NexusRobot, _ensure_nexus, _new_scene

        self.output = output
        output.mkdir(parents=True, exist_ok=False)
        self.model = Kinematics()
        self.engine = _ensure_nexus("metal")
        self.scene = _new_scene(
            self.engine, {"sim": {"dt": 0.01, "gravity": [0, 0, -9.81]}}, 1, [0.65] * 3
        )
        self.bodies, self.specs = [], []

        def add(name, size, position, **kwargs):
            name = f"{name}_{len(self.bodies)}"
            body = self.scene.add_box(name, size, position, **kwargs)
            self.bodies.append(body)
            self.specs.append(dict(name=name, size=size, color=kwargs["color"]))
            return body

        add(
            "floor",
            [5, 4, 0.10],
            [0, 0, -0.05],
            fixed=True,
            friction=1,
            color=[0.63, 0.52, 0.40, 1],
        )
        add(
            "back_wall",
            [5, 0.10, 1.7],
            [0, 2, 0.85],
            fixed=True,
            color=[0.79, 0.81, 0.77, 1],
        )
        add(
            "side_wall",
            [0.10, 4, 1.7],
            [-2.5, 0, 0.85],
            fixed=True,
            color=[0.83, 0.79, 0.72, 1],
        )
        add(
            "sofa_seat",
            [1.7, 0.65, 0.30],
            [0.25, 1.3, 0.24],
            fixed=True,
            color=[0.18, 0.33, 0.36, 1],
        )
        add(
            "sofa_back",
            [1.7, 0.15, 0.65],
            [0.25, 1.62, 0.45],
            fixed=True,
            color=[0.15, 0.29, 0.32, 1],
        )
        for x in [-0.65, 1.15]:
            add(
                "sofa_arm",
                [0.15, 0.8, 0.43],
                [x, 1.3, 0.32],
                fixed=True,
                color=[0.15, 0.29, 0.32, 1],
            )
        self.table_center = np.array([0.55, 0.30])
        self.table_top = 0.38
        add(
            "coffee_table",
            [0.90, 0.65, 0.045],
            [0.55, 0.30, self.table_top - 0.0225],
            fixed=True,
            friction=0.8,
            color=[0.31, 0.17, 0.09, 1],
        )
        for x in [0.18, 0.92]:
            for y in [0.06, 0.54]:
                add(
                    "table_leg",
                    [0.055, 0.055, 0.335],
                    [x, y, 0.1675],
                    fixed=True,
                    color=[0.17, 0.12, 0.09, 1],
                )
        add(
            "side_cabinet",
            [0.45, 0.5, 0.55],
            [-1.9, 1.1, 0.275],
            fixed=True,
            color=[0.55, 0.37, 0.21, 1],
        )
        self.cubes = {
            name: add(
                name,
                [0.04] * 3,
                [-0.65, y, 0.025],
                friction=1.0,
                density=300,
                color=color,
            )
            for name, y, color in (
                ("red", -0.5, [0.9, 0.12, 0.07, 1]),
                ("green", -0.95, [0.15, 0.8, 0.22, 1]),
                ("blue", -1.35, [0.12, 0.35, 0.95, 1]),
            )
        }
        self.active = "red"
        self.cube = self.cubes[self.active]
        # A reduced planar base with two actuated translation joints. Moving
        # a fixed root directly loses its transport velocity in contact
        # dynamics; these joints carry velocity through the multibody solver.
        root = ET.parse(MODEL).getroot()
        for mesh in root.findall(".//mesh"):
            mesh.set("filename", str((MODEL.parent / mesh.get("filename")).resolve()))
        for name in ("demo_world", "demo_x_carriage"):
            link = ET.SubElement(root, "link", name=name)
            inertial = ET.SubElement(link, "inertial")
            ET.SubElement(inertial, "mass", value="0.01")
            ET.SubElement(
                inertial,
                "inertia",
                ixx="0.00001",
                iyy="0.00001",
                izz="0.00001",
                ixy="0",
                ixz="0",
                iyz="0",
            )
        for name, parent, child, axis, origin in (
            ("demo_base_x", "demo_world", "demo_x_carriage", "1 0 0", "0 0 0"),
            ("demo_base_y", "demo_x_carriage", "base_link", "0 1 0", "0 0 0.0843"),
        ):
            joint = ET.SubElement(root, "joint", name=name, type="prismatic")
            ET.SubElement(joint, "parent", link=parent)
            ET.SubElement(joint, "child", link=child)
            ET.SubElement(joint, "origin", xyz=origin, rpy="0 0 0")
            ET.SubElement(joint, "axis", xyz=axis)
            ET.SubElement(
                joint, "limit", lower="-2.5", upper="2.5", effort="300", velocity="0.15"
            )
        planar = output / "nano-planar.urdf"
        ET.ElementTree(root).write(planar, encoding="unicode")
        nx = self.scene.nexus3d
        native = self.scene.state.load_urdf_robot(
            0,
            str(planar),
            nx.UrdfLoaderOptions(make_roots_fixed=True, convex_hull=True),
        )
        self.scene.state.set_robot_joint_dynamics(
            native, armature=[0.01] * native.n_dofs, damping=[0.1] * native.n_dofs
        )
        self.visuals = register_visuals(self.scene, native, MODEL)
        self.robot = NexusRobot(self.scene, [native], "adora_nano")
        self.scene.robots.append(self.robot)
        self.rig, self.camera_specs = create_rig(self.scene, self.robot, (320, 240))
        self.camera = self.scene.add_camera(
            (960, 720), 52, pos=(3.0, -4.1, 3.0), lookat=(-0.1, 0.25, 0.32)
        )
        self.close_camera = self.scene.add_camera(
            (640, 480), 44, pos=(0.5, -1.8, 0.95), lookat=(-0.5, -0.4, 0.2)
        )
        self.scene.build()
        for side in ("left", "right"):
            dof = self.robot.get_joint(f"{side}_j6_gripper").dofs_idx_local
            self.robot.set_dofs_kp([5.0], dof)
            self.robot.set_dofs_kv([0.25], dof)
        self.base_indices = [
            self.robot.get_joint(f"demo_base_{axis}").dofs_idx_local[0]
            for axis in ("x", "y")
        ]
        self.robot.set_dofs_kp([2000.0, 2000.0], self.base_indices)
        self.robot.set_dofs_kv([150.0, 150.0], self.base_indices)
        native = [
            n
            for _, n in sorted(
                (j.dofs_idx_local[0], j.name) for j in self.robot.joints if j.n_dofs
            )
        ]
        self.indices = [native.index(n) for n in self.model.names]
        self.q = np.zeros(17)
        self.q[self.model.names.index("left_j6_gripper")] = -0.25
        self.base = np.array([-1.35, -0.9, 0.0])
        self.base_z = 0.0843
        self.robot.set_qpos(self.to_native(self.q))
        self.tick = 0
        self.phase = "inspect"
        self.rows = []
        self.hub = PreviewHub(socket_path) if socket_path else None
        self.sequence = 0
        self.stream_frames = 0
        self.stream_clients_max = 0
        self.video = cv2.VideoWriter(
            str(output / "demo.mp4"), cv2.VideoWriter_fourcc(*"mp4v"), 20, (960, 720)
        )
        if not self.video.isOpened():
            raise RuntimeError("Video writer did not open")
        self.commands = (output / "commands.jsonl").open("w")
        self.step(50)
        manifest = {
            "engine": "Nexus",
            "backend": "Metal",
            "operator": "Codex via JSON commands",
            "base_model": "two actuated planar translation joints; wheel traction is not the drive model",
            "arm_cube": "Nexus contact physics; no grasp attachment or cube pose writes",
            "scene": "created living-room fixture; known geometry and simulator observations",
            "model": str(MODEL),
            "model_sha256": hashlib.sha256(MODEL.read_bytes()).hexdigest(),
            "model_provenance": json.loads(
                (MODEL.parent / "provenance.json").read_text()
            ),
            "visual_meshes": self.visuals,
            "cameras": self.camera_specs,
            "socket": str(socket_path) if socket_path else None,
            "cube_names": list(self.cubes),
            "joint_names": self.model.names,
            "table_top_m": self.table_top,
            "source_hashes": {
                name: hashlib.sha256(
                    Path(__file__).with_name(name).read_bytes()
                ).hexdigest()
                for name in ("demo.py", "model.py")
            },
            "versions": {
                p: importlib.metadata.version(p)
                for p in ("dimforge-nexus3d", "scipy", "opencv-python-headless")
            },
        }
        (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

    def to_native(self, wire):
        native = np.zeros(self.robot.n_dofs)
        native[self.indices] = wire
        native[self.base_indices] = self.base[:2]
        return native

    def observed_q(self):
        return np.asarray(self.robot.get_qpos())[self.indices]

    def grip_world(self):
        link = self.robot.get_link("left_wrist_housing")
        wxyz = link.get_quat()
        if np.linalg.norm(wxyz) < 0.5:
            raise ValueError(
                f"Invalid link pose: {link.name}, {link.get_pos()}, {wxyz}; q={self.observed_q()}"
            )
        r = Rotation.from_quat([*wxyz[1:], wxyz[0]]).as_matrix()
        return np.asarray(link.get_pos()) + r @ GRIP_POINT

    def status(self):
        return {
            "time_s": round(self.tick * 0.01, 2),
            "phase": self.phase,
            "base": [*self.robot.get_link("base_link").get_pos()[:2].tolist(), 0.0],
            "cube": self.cube.get_pos().tolist(),
            "active_cube": self.active,
            "cubes": {
                name: body.get_pos().tolist() for name, body in self.cubes.items()
            },
            "grip": self.grip_world().tolist(),
            "q": self.observed_q().tolist(),
        }

    def step(self, count):
        for _ in range(count):
            self.robot.control_dofs_position(self.to_native(self.q))
            self.scene.step()
            self.tick += 1
            if self.tick % 5 == 0:
                self.capture()

    def capture(self):
        rgb = np.asarray(self.camera.render()[0], dtype=np.uint8)
        frame = cv2.cvtColor(rgb, cv2.COLOR_RGB2BGR)
        cv2.rectangle(frame, (0, 0), (960, 75), (23, 25, 30), -1)
        cv2.putText(
            frame,
            "CODEX + AISLE / NEXUS | ADORA NANO",
            (22, 28),
            cv2.FONT_HERSHEY_SIMPLEX,
            0.66,
            (245, 245, 245),
            1,
            cv2.LINE_AA,
        )
        cv2.putText(
            frame,
            f"{self.active.upper()} | {self.phase.upper()}    simulation {self.tick * 0.01:05.1f}s",
            (22, 57),
            cv2.FONT_HERSHEY_SIMPLEX,
            0.63,
            (120, 220, 245),
            1,
            cv2.LINE_AA,
        )
        p = self.grip_world()
        self.close_camera.set_pose(
            pos=(p + [0.45, -0.60, 0.35]).tolist(), lookat=p.tolist()
        )
        close = cv2.cvtColor(
            np.asarray(self.close_camera.render()[0], dtype=np.uint8), cv2.COLOR_RGB2BGR
        )
        frame[458:698, 620:940] = cv2.resize(close, (320, 240))
        cv2.rectangle(frame, (618, 436), (942, 700), (35, 39, 45), 2)
        cv2.rectangle(frame, (620, 436), (940, 458), (35, 39, 45), -1)
        cv2.putText(
            frame,
            "EXTERNAL GRASP VIEW",
            (631, 452),
            cv2.FONT_HERSHEY_SIMPLEX,
            0.43,
            (245, 245, 245),
            1,
            cv2.LINE_AA,
        )
        self.video.write(frame)
        self.last_frame = frame
        state = self.status()
        self.rows.append(state)
        # Atomic preview files for a read-only live viewer/Studio adapter.
        cv2.imwrite(str(self.output / "live.tmp.png"), frame)
        (self.output / "live.tmp.png").replace(self.output / "live.png")
        (self.output / "live.tmp.json").write_text(json.dumps(state) + "\n")
        (self.output / "live.tmp.json").replace(self.output / "live.json")
        if self.hub and self.tick % 20 == 0:
            self.publish()

    def publish(self, finished=False):
        images, cameras = [], []
        for name, camera in self.rig.items():
            rgb = np.asarray(camera.render()[0], dtype=np.uint8)
            pixels = cv2.cvtColor(rgb, cv2.COLOR_RGB2BGRA).tobytes()
            h, w = rgb.shape[:2]
            images.append(pixels)
            cameras.append(dict(name=name, width=w, height=h, bytes=len(pixels)))
        transforms = []
        for body in self.bodies:
            wxyz = body.get_quat()
            t = np.eye(4)
            t[:3, :3] = Rotation.from_quat([*wxyz[1:], wxyz[0]]).as_matrix()
            t[:3, 3] = body.get_pos()
            transforms.append(t.T.ravel().tolist())
        base = self.robot.get_link("base_link")
        header = dict(
            version=1,
            session=f"Codex three cubes — {self.output.name}",
            sequence=self.sequence,
            sim_time_ns=self.tick * 10_000_000,
            fps=5.0,
            joint_names=self.model.names,
            joints=self.observed_q().tolist(),
            targets=self.q.tolist(),
            base_position=base.get_pos().tolist(),
            base_quat_wxyz=base.get_quat().tolist(),
            objects=self.specs,
            transforms=transforms,
            cameras=cameras,
            phase=f"Preview only · Codex · {self.active}: {self.phase}",
            finished=finished,
        )
        self.hub.publish(header, images)
        self.sequence += 1
        self.stream_frames += 1
        self.stream_clients_max = max(self.stream_clients_max, self.hub.clients)

    def snapshot(self, name="latest"):
        cv2.imwrite(str(self.output / f"{name}.png"), self.last_frame)
        p = self.grip_world()
        self.close_camera.set_pose(
            pos=(p + [0.45, -0.6, 0.35]).tolist(), lookat=p.tolist()
        )
        rgb = np.asarray(self.close_camera.render()[0], dtype=np.uint8)
        cv2.imwrite(
            str(self.output / f"{name}-close.png"), cv2.cvtColor(rgb, cv2.COLOR_RGB2BGR)
        )

    def move_arm(self, target, seconds=2.0):
        target = np.asarray(target, dtype=float)
        for n, q in zip(self.model.names, target):
            low, high = self.model.limits[n]
            if not np.isfinite(q) or not low <= q <= high:
                raise ValueError(f"Invalid target for {n}: {q}")
        start = self.q.copy()
        steps = max(1, round(seconds * 100))
        for i in range(steps):
            f = (i + 1) / steps
            blend = f * f * (3 - 2 * f)
            self.q = start + blend * (target - start)
            self.step(1)
        self.step(50)

    def navigate(self, goal, speed=0.12):
        """Collision-checked short planar path; the Nano base is holonomic."""
        goal = np.asarray(goal, dtype=float)
        if goal.shape != (2,) or not np.isfinite(goal).all() or not 0 < speed <= 0.15:
            raise ValueError("Expected finite XY goal and speed in (0, .15]")
        start = self.base[:2].copy()
        distance = np.linalg.norm(goal - start)
        steps = max(1, math.ceil(distance / (speed * 0.01)))
        # Furniture footprints inflated by the chassis radius. Validate the
        # entire segment before issuing its first movement.
        obstacles = [
            (0.55, 0.30, 0.90, 0.65),
            (0.25, 1.3, 1.95, 0.85),
            (-1.9, 1.1, 0.45, 0.5),
        ]
        obstacles += [
            (*b.get_pos()[:2], 0.04, 0.04)
            for b in self.cubes.values()
            if b.get_pos()[2] < 0.06
        ]
        radius = 0.26
        for f in np.linspace(0, 1, max(2, math.ceil(distance / 0.01))):
            p = start + f * (goal - start)
            if not (
                -2.4 + radius < p[0] < 2.4 - radius
                and -1.9 + radius < p[1] < 1.9 - radius
            ):
                raise ValueError("Navigation segment leaves room clearance")
            for x, y, width, depth in obstacles:
                closest = np.clip(
                    p, [x - width / 2, y - depth / 2], [x + width / 2, y + depth / 2]
                )
                if np.linalg.norm(p - closest) < radius:
                    raise ValueError(
                        "Navigation segment intersects furniture clearance"
                    )
        delta = (goal - start) / steps
        for i in range(steps):
            self.base[:2] = start + (i + 1) / steps * (goal - start)
            # Kinematic wheel rotations accompany the base displacement;
            # ground traction is deliberately not inferred from this model.
            for wheel, theta in enumerate([math.pi, math.pi / 3, -math.pi / 3], 1):
                self.q[self.model.names.index(f"wheel_{wheel}_joint")] += (
                    -math.sin(theta) * delta[0] + math.cos(theta) * delta[1]
                ) / 0.05
            self.step(1)
        self.step(50)

    def execute(self, command):
        self.commands.write(
            json.dumps({"before": self.status(), "command": command}) + "\n"
        )
        self.commands.flush()
        op = command["op"]
        self.phase = command.get("phase", op)
        if op == "select":
            self.cube = self.cubes[command["cube"]]
            self.active = command["cube"]
        elif op == "arm":
            target = self.q.copy()
            for n, value in command["joints"].items():
                target[self.model.names.index(n)] = value
            self.move_arm(target, command.get("seconds", 2.0))
        elif op == "reach":
            world = np.asarray(command["position"])
            r = Rotation.from_euler("z", self.base[2]).as_matrix()
            local = r.T @ (world - [*self.base[:2], self.base_z])
            target = self.model.solve(
                local, self.q, command.get("down", True), attempts=20
            )
            if command.get("cartesian", False):
                start, _ = self.model.grip(self.q)
                samples = max(2, round(command.get("seconds", 2.0) * 50))
                # Plan the complete path before executing any part of it.
                plan, seed = [], self.q.copy()
                for f in np.linspace(0, 1, samples):
                    seed = self.model.solve(
                        start + f * (local - start),
                        seed,
                        command.get("down", True),
                        attempts=1,
                    )
                    plan.append(seed)
                for q in plan:
                    self.q = q
                    self.step(2)
                self.step(50)
            else:
                self.move_arm(target, command.get("seconds", 2.0))
        elif op == "wait":
            self.step(round(command.get("seconds", 1) * 100))
        elif op == "navigate":
            self.navigate(command["position"], command.get("speed", 0.12))
        elif op == "contacts":
            contacts = self.scene.state.debug_contacts(self.engine.viewer)
            print(json.dumps({"contacts": contacts}, default=str), flush=True)
        elif op != "snapshot":
            raise ValueError(f"Unknown operation: {op}")
        self.snapshot(command.get("name", "latest"))
        result = self.status()
        expected = command.get("expect", {})
        if result["cube"][2] < expected.get("cube_min_z", -1):
            raise ValueError(f"Cube height check failed: {result['cube'][2]:.4f} m")
        self.commands.write(json.dumps({"after": result}) + "\n")
        self.commands.flush()
        return result

    def close(self):
        if self.hub:
            self.publish(finished=True)
            (self.output / "stream.json").write_text(
                json.dumps(
                    dict(
                        frames=self.stream_frames,
                        max_clients=self.stream_clients_max,
                        socket=str(self.hub.path),
                    ),
                    indent=2,
                )
                + "\n"
            )
            time.sleep(1)
            self.hub.close()
        self.video.release()
        self.commands.close()
        (self.output / "trajectory.json").write_text(json.dumps(self.rows) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--socket", type=Path, help="Studio external live preview socket"
    )
    parser.add_argument(
        "--aisle", type=Path, default=Path(__file__).resolve().parents[3] / "AISLE"
    )
    parser.add_argument(
        "--plan",
        type=Path,
        help="Replay a saved JSON list of Codex commands; stop on failure",
    )
    args = parser.parse_args()
    demo = Demo(args.output.resolve(), args.aisle.resolve(), args.socket)
    failed = False
    try:
        demo.snapshot("initial")
        print(json.dumps({"ready": True, **demo.status()}), flush=True)
        stream = (
            (json.dumps(c) for c in json.loads(args.plan.read_text()))
            if args.plan
            else sys.stdin
        )
        for line in stream:
            try:
                command = json.loads(line)
                if command["op"] == "finish":
                    break
                print(json.dumps({"ok": True, **demo.execute(command)}), flush=True)
            except Exception as e:
                failed = True
                demo.commands.write(
                    json.dumps({"error": str(e), "after": demo.status()}) + "\n"
                )
                demo.commands.flush()
                print(json.dumps({"ok": False, "error": str(e)}), flush=True)
                if args.plan:
                    break
    finally:
        demo.close()
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
