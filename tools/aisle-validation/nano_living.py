"""Living-room rigid-body scene and feedback navigation for the Nano Dora graph.

The controller commands wheel joints only. Root pose writes are forbidden during
rollout. Physics parameters are an explicit simulation model, not hardware calibration.
"""

from __future__ import annotations
import heapq
import math
from pathlib import Path
import numpy as np

CAMERAS = ("head", "left_wrist", "right_wrist")
WHEEL_RADIUS = 0.034  # effective rolling gain measured in the collision-mesh model
BASE_RADIUS = 0.16322
TURN_GAIN = 3.4  # wheel rad/s per base rad/s, measured rotation response
MAX_WHEEL_RATE = 5.0
CONTROL_DT = 0.2
ROBOT_CLEARANCE = 0.55
ROOM = (-5.7, 5.7, -4.7, 4.7)
# A stationary observation posture, reached through joint motors before capture.
# Equal joint values give mirrored arms in the local symmetric model. Wrists
# look ahead instead of at the floor; head looks into the room on this route.
NAVIGATION_POSTURE = {
    "head_pan_joint": 1.5,
    "head_tilt_joint": -0.22,
    "left_j1_yaw": 0.2,
    "right_j1_yaw": 0.2,
    "left_j3_pitch": 0.35,
    "right_j3_pitch": 0.35,
    "left_j4_pitch": -1.0,
    "right_j4_pitch": -1.0,
}


def room_specs(seed=0):
    specs = []

    def add(name, size, pos, color, fixed=True):
        specs.append(
            dict(name=name, size=size, position=pos, color=[*color, 1.0], fixed=fixed)
        )

    add("floor", [12, 10, 0.1], [0, 0, -0.0843], [0.65, 0.62, 0.57])
    for name, size, pos in [
        ("north_wall", [12, 0.15, 2.7], [0, 5, 1.3157]),
        ("south_wall", [12, 0.15, 2.7], [0, -5, 1.3157]),
        ("east_wall", [0.15, 10, 2.7], [6, 0, 1.3157]),
        ("west_wall", [0.15, 10, 2.7], [-6, 0, 1.3157]),
    ]:
        add(name, size, pos, [0.8, 0.83, 0.85])
    for name, size, pos in [
        ("seat", [2.6, 0.9, 0.4], [0, 2.6, 0.3]),
        ("back", [2.6, 0.22, 0.9], [0, 3.1, 0.6]),
        ("left_arm", [0.22, 0.9, 0.7], [-1.3, 2.6, 0.45]),
        ("right_arm", [0.22, 0.9, 0.7], [1.3, 2.6, 0.45]),
    ]:
        add("sofa_" + name, size, pos, [0.20, 0.31, 0.43])
    add("coffee_table", [1.4, 0.8, 0.08], [0, 1, 0.5], [0.38, 0.23, 0.12])
    for x in [-0.6, 0.6]:
        for y in [0.7, 1.3]:
            add(
                f"table_leg_{x}_{y}",
                [0.08, 0.08, 0.5],
                [x, y, 0.21],
                [0.19, 0.14, 0.09],
            )
    colors = [
        [0.92, 0.18, 0.12],
        [0.12, 0.6, 0.9],
        [0.95, 0.7, 0.12],
        [0.1, 0.7, 0.35],
        [0.6, 0.2, 0.85],
        [0.9, 0.4, 0.1],
    ]
    rng = np.random.default_rng(seed)
    for i in range(12):
        if i < 6:
            pos = [-0.45 + (i % 3) * 0.45, 0.82 + (i // 3) * 0.35, 0.9]
        elif i == 6:
            pos = [0.72, 0, 0.3]  # deliberate bumper-contact task before navigation
        else:
            # Keep floor objects visible along the outer side of the route,
            # beyond the robot footprint, instead of clustering behind it.
            pos = [
                [3.2, -0.65, 0.3],
                [3.35, 0.6, 0.3],
                [3.35, 1.9, 0.3],
                [3.2, 3.25, 0.3],
                [-0.5, 4.7, 0.3],
            ][i - 7].copy()
        if i != 6:
            pos[0] += float(rng.uniform(-0.015, 0.015))
        add(f"cube_{i}", [0.12] * 3, pos, colors[i % 6], False)
    return specs


def obstacles(specs):
    return [
        (s["position"][0], s["position"][1], s["size"][0] / 2, s["size"][1] / 2)
        for s in specs
        if s["fixed"] and s["name"] != "floor"
    ]


def clear_point(point, boxes, radius=ROBOT_CLEARANCE):
    x, y = point
    if (
        not ROOM[0] + radius <= x <= ROOM[1] - radius
        or not ROOM[2] + radius <= y <= ROOM[3] - radius
    ):
        return False
    # Conservative square inflation also covers the folded arms and corner cuts.
    return all(
        abs(x - cx) > hx + radius or abs(y - cy) > hy + radius
        for cx, cy, hx, hy in boxes
    )


def clear_segment(a, b, boxes, radius=ROBOT_CLEARANCE):
    n = max(1, math.ceil(math.dist(a, b) / 0.025))
    return all(
        clear_point(
            [a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n], boxes, radius
        )
        for i in range(n + 1)
    )


def plan_path(start, goal, boxes, cell=0.15):
    if not clear_point(start, boxes) or not clear_point(goal, boxes):
        raise ValueError("Start or goal intersects the inflated scene")
    if clear_segment(start, goal, boxes):
        return [list(goal)]
    origin = (ROOM[0], ROOM[2])
    nx = round((ROOM[1] - ROOM[0]) / cell)
    ny = round((ROOM[3] - ROOM[2]) / cell)

    def point(n):
        return [origin[0] + n[0] * cell, origin[1] + n[1] * cell]

    def nearest(p):
        return (round((p[0] - origin[0]) / cell), round((p[1] - origin[1]) / cell))

    begin, end = nearest(start), nearest(goal)
    # A snapped endpoint must be connected safely to its exact endpoint.
    if not clear_segment(start, point(begin), boxes) or not clear_segment(
        point(end), goal, boxes
    ):
        raise ValueError("No safe grid endpoint")
    todo = [(0.0, begin)]
    costs = {begin: 0.0}
    prev = {}
    while todo:
        _, node = heapq.heappop(todo)
        if node == end:
            break
        for dx, dy in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ]:
            nxt = (node[0] + dx, node[1] + dy)
            if not (0 <= nxt[0] <= nx and 0 <= nxt[1] <= ny) or not clear_segment(
                point(node), point(nxt), boxes
            ):
                continue
            cost = costs[node] + math.hypot(dx, dy) * cell
            if cost >= costs.get(nxt, float("inf")):
                continue
            costs[nxt] = cost
            prev[nxt] = node
            heapq.heappush(todo, (cost + math.dist(point(nxt), goal), nxt))
    if end not in costs:
        raise ValueError("No collision-free path")
    path = [list(goal)]
    node = end
    while node != begin:
        path.append(point(node))
        node = prev[node]
    path.append(list(start))
    path.reverse()
    # Keep the furthest visible waypoint; never smooth through furniture.
    smooth = []
    i = 0
    while i < len(path) - 1:
        j = len(path) - 1
        while j > i + 1 and not clear_segment(path[i], path[j], boxes):
            j -= 1
        smooth.append(path[j])
        i = j
    return smooth


def wheel_rates(vx, vy, omega):
    rates = (
        np.array(
            [vy, math.sqrt(3) / 2 * vx - 0.5 * vy, -math.sqrt(3) / 2 * vx - 0.5 * vy]
        )
        / WHEEL_RADIUS
        - TURN_GAIN * omega
    )
    return rates * min(1.0, MAX_WHEEL_RATE / max(float(np.abs(rates).max()), 1e-9))


def yaw_from_quat(q):
    w, x, y, z = q
    return math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z))


class Navigator:
    def __init__(self, specs, goals=None, continuous=False):
        self.boxes = obstacles(specs)
        self.goals = goals or [[1.25, 0.0], [2.2, 1.6], [2.2, 3.9], [-2.2, 3.9]]
        if continuous and len(self.goals) < 2:
            raise ValueError("Continuous navigation needs at least two goals")
        self.continuous = continuous
        self.direction = 1
        self.legs = 0
        self.goal_index = 0
        self.path = []
        self.completed = False
        self.plans = []

    def command(self, position, quat):
        p = np.asarray(position[:2], float)
        if not np.isfinite(p).all():
            raise ValueError("Invalid odometry")
        if self.completed:
            return np.zeros(3)
        goal = self.goals[self.goal_index]
        if math.dist(p, goal) < 0.10:
            self.goal_index += self.direction
            self.path = []
            if self.goal_index == len(self.goals) or self.goal_index < 0:
                self.legs += 1
                if not self.continuous:
                    self.completed = True
                    return np.zeros(3)
                self.direction *= -1
                self.goal_index = len(self.goals) - 2 if self.direction < 0 else 1
            goal = self.goals[self.goal_index]
        if not self.path:
            self.path = plan_path(p.tolist(), goal, self.boxes)
            self.plans.append(dict(start=p.tolist(), goal=goal, path=[*self.path]))
            if self.continuous:
                self.plans = self.plans[-128:]
        while len(self.path) > 1 and math.dist(p, self.path[0]) < 0.12:
            self.path.pop(0)
        target = np.array(self.path[0])
        delta = target - p
        distance = float(np.linalg.norm(delta))
        # The imported collision meshes are solid wheels, not independent
        # omni rollers. Use the measured forward/turn modes; lateral targets
        # produced slip and yaw in the calibration probe.
        yaw = yaw_from_quat(quat)
        heading = math.atan2(delta[1], delta[0])
        error = (heading - yaw + math.pi) % (2 * math.pi) - math.pi
        speed = (
            min(0.18, 0.8 * distance) * max(0.0, math.cos(error)) ** 3
            # Finish turning before driving: a loaded bumper and the folded
            # observation pose increase lateral slip during combined motion.
            if abs(error) < 0.20
            else 0.0
        )
        return wheel_rates(speed, 0.0, float(np.clip(1.5 * error, -0.45, 0.45)))


class WheelGuard:
    def __init__(self):
        self.last = np.zeros(3)
        self.next_seq = 0

    def accept(self, rates, seq):
        rates = np.asarray(rates, dtype=float)
        if seq != self.next_seq or rates.shape != (3,) or not np.isfinite(rates).all():
            raise ValueError("Invalid wheel command or sequence")
        if np.abs(rates).max() > MAX_WHEEL_RATE + 1e-6:
            raise ValueError("Wheel rate exceeds limit")
        # A physical wheel acceleration limit; brake commands can always stop.
        rates = np.clip(rates, self.last - 1.0, self.last + 1.0)
        self.last = rates
        self.next_seq += 1
        return rates


class LivingSimulation:
    def __init__(
        self,
        model: Path,
        seed=0,
        cameras=True,
        resolution=(640, 480),
        *,
        specs=None,
        posture=None,
        room_settings=None,
    ):
        from aisle.sim.nexus_backend import _ensure_nexus, _new_scene, NexusRobot

        room_settings = room_settings or {}
        self.engine = _ensure_nexus("metal")
        self.scene = _new_scene(
            self.engine,
            {"sim": {"dt": 0.01, "gravity": [0, 0, -9.81]}},
            1,
            room_settings.get("ambient_rgb", [0.6] * 3),
        )
        self.specs = room_specs(seed) if specs is None else specs
        self.bodies = [
            self.scene.add_box(
                s["name"],
                s["size"],
                s["position"],
                fixed=s["fixed"],
                friction=room_settings.get("floor_friction", 0.7)
                if s["name"] == "floor"
                else 0.7,
                density=500,
                color=s["color"],
            )
            for s in self.specs
        ]
        nx = self.scene.nexus3d
        native = self.scene.state.load_urdf_robot(
            0,
            str(model),
            nx.UrdfLoaderOptions(make_roots_fixed=False, convex_hull=True),
        )
        self.scene.state.set_robot_joint_dynamics(
            native, armature=[0.01] * native.n_dofs, damping=[0.1] * native.n_dofs
        )
        from nano_visuals import register_visuals

        self.visuals = register_visuals(self.scene, native, model)
        self.robot = NexusRobot(self.scene, [native], "adora_nano")
        self.scene.robots.append(self.robot)
        import xml.etree.ElementTree as ET

        self.names = [
            j.get("name")
            for j in ET.parse(model).getroot().findall("joint")
            if j.get("type") not in ("fixed", "floating")
        ]
        self.dofs = [self.robot.get_joint(n).dofs_idx_local[0] for n in self.names]
        self.wheel_indices = [self.names.index(f"wheel_{i}_joint") for i in range(1, 4)]
        self.wheel_dofs = [self.dofs[i] for i in self.wheel_indices]
        self.rig = {}
        self.camera_specs = {}
        if cameras:
            from nano_cameras import create_rig

            self.rig, self.camera_specs = create_rig(
                self.scene,
                self.robot,
                resolution=resolution,
                calibration=room_settings.get("cameras"),
            )
            for spec in self.camera_specs.values():
                spec["fps"] = 1 / CONTROL_DT
        self.scene.build()
        park = NAVIGATION_POSTURE if posture is None else posture
        parked = np.array([park.get(name, 0.0) for name in self.names])
        for step in range(250):
            self.robot.control_dofs_position(
                (parked * min(1.0, (step + 1) / 150)).tolist(), self.dofs
            )
            self.scene.step()
        self.tick = 0
        self.targets = parked.copy()
        self.cube_start = self.object_positions()
        self.root_pose_writes = 0

    def room_readback(self):
        """Read imported fixed bodies from Nexus after physics has settled."""
        return {
            spec["name"]: dict(
                physics_body=int(self.scene.gpu_index(0, body.handles[0])),
                center_m=body.get_pos().tolist(),
            )
            for spec, body in zip(self.specs, self.bodies, strict=True)
            if spec["name"].startswith("room_")
        }

    def object_positions(self):
        return [np.asarray(b.get_pos()).tolist() for b in self.bodies]

    def advance(self, rates, capture_contacts=False):
        contacts = []
        for _ in range(round(CONTROL_DT / 0.01)):
            self.targets[self.wheel_indices] += np.asarray(rates) * 0.01
            self.robot.control_dofs_position(self.targets.tolist(), self.dofs)
            self.scene.step()
            self.tick += 1
            if capture_contacts:
                for c in self.scene.state.debug_contacts(self.engine.viewer):
                    contacts.append([self.tick, int(c["body_a"]), int(c["body_b"])])
        return np.asarray(contacts, dtype=np.int64).reshape(-1, 3)

    def observation(self):
        q = np.asarray(self.robot.get_qpos())[self.dofs]
        position = np.asarray(self.robot.get_pos())
        quat = np.asarray(self.robot.get_quat())
        if not np.isfinite(q).all() or not np.isfinite(position).all():
            raise ValueError("Nonfinite physics state")
        return np.concatenate((position, quat, q, self.targets))

    def snapshot(self):
        from nano_workspace import capture_objects

        obs = self.observation()
        return dict(
            sim_time_ns=self.tick * 10_000_000,
            base_position=obs[:3].tolist(),
            base_quat_wxyz=obs[3:7].tolist(),
            joints=obs[7:24].tolist(),
            targets=obs[24:41].tolist(),
            transforms=capture_objects(self.specs, self.bodies, self.tick * 10_000_000)[
                "transforms"
            ],
        )
