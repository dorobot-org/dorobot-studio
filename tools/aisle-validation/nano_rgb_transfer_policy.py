"""Nano floor-to-table controller. Its pipe carries only onboard RGB and measured joint state."""

import io
import json
import math
from pathlib import Path
import struct
import sys

import cv2
import numpy as np

from nano_grasp_estimator import color_mask, parameters, predict
from nano_rgb_policy import CAMERAS, CUBE_CENTRE_Z, VisualServo
from nano_rgb_table import occupied_table_points, table_edge

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "nano-three-cubes"))
from model import Kinematics, UnreachableGrasp  # noqa: E402


def visible_in_jaws(rgb, color):
    mask = color_mask(cv2.cvtColor(rgb, cv2.COLOR_RGB2HSV), color)
    # Calibrated wrist ROI spans the gripper opening when the jaws are closed.
    h, w = mask.shape
    opening = mask[round(0.61 * h) : round(0.89 * h), round(0.17 * w) : round(0.48 * w)]
    return float(opening.mean()) >= 0.14 and float(mask.mean()) >= 0.04


# Reserve lateral space for rolling after the open finger withdraws. Moving the
# elevated base closer keeps the desired release point within the arm workspace.
RELEASE_INSET = 0.085
DOCK_DISTANCE = 0.385 - RELEASE_INSET


GRID_SPACING = 0.020
EXTRA_INSET = 0.020


def release_candidates(side, span):
    """Prefer the existing reachable slots, then scan the visible rim between them."""
    low, high = span
    for offset in (0, 0.08, -0.08, 0.115, -0.115, 0.15, -0.15):
        if low <= side + offset <= high:
            yield side + offset, RELEASE_INSET, "preferred"
    # Try a second strip farther inside the table when the original inset is occupied.
    lateral = sorted(
        np.arange(low + 0.009, high - 0.009, GRID_SPACING), key=lambda x: abs(x - side)
    )
    for inset in (RELEASE_INSET, RELEASE_INSET + EXTRA_INSET):
        for along in lateral:
            yield along, inset, "grid"


class Controller:
    def __init__(self, config):
        self.input = sys.stdin.buffer
        self.model = Kinematics(Path(config["model"]))
        self.names = config["joint_names"]
        if self.model.names != self.names:
            raise ValueError("Joint order differs from the URDF")
        self.colors = config["colors"]
        self.servo = VisualServo(config | {"color": self.colors[0], "standoff_m": 0.45})
        self.max_forward_speed = config.get("max_forward_speed_m_s", 0.13)
        self.track_grasp = bool(config.get("track_grasp"))
        self.wrist_clearance = config.get("wrist_clearance", False)
        self.resolve_pickup_ambiguity = (
            config.get("grasp_rgb_association", "wrist-agreement-v1")
            == "wrist-agreement-v1"
        )
        self.targets = None
        self.last_stamp = -1
        self.grip = "left_j6_gripper"
        self.carry_detector = parameters(config.get("grasp_plan", {}))
        self.jaw_indices = [
            self.names.index(name) for name in (self.grip, "left_j5_roll")
        ]
        self.wheel_pair = [self.names.index(f"wheel_{i}_joint") for i in (2, 3)]
        self.wheel_radius = config["rolling_radius_m"]
        self.last_rim_distance = None

    def send(self, op, **fields):
        print(json.dumps(dict(op=op, **fields), allow_nan=False), flush=True)

    def act(self, op, **fields):
        self.send(op, **fields)
        reply = json.loads(self.input.readline())
        if reply != {"ok": True}:
            raise ValueError("Actuator rejected the requested movement")

    def sense(self):
        self.send("sense")
        (size,) = struct.unpack(">I", self.input.read(4))
        if not 0 < size <= 12_000_000:
            raise ValueError("Invalid observation length")
        with np.load(io.BytesIO(self.input.read(size)), allow_pickle=False) as data:
            if set(data.files) != {*CAMERAS, "stamp_ns", "encoders"}:
                raise ValueError("Unexpected sensor channels")
            packet = {k: data[k] for k in data.files}
        q = packet["encoders"]
        stamp = int(packet["stamp_ns"])
        if (
            stamp <= self.last_stamp
            or q.shape != (len(self.names),)
            or not np.isfinite(q).all()
        ):
            raise ValueError("Invalid encoder sample or timestamp")
        for name in CAMERAS:
            w, h = self.servo.cameras[name][-1]
            if packet[name].shape != (h, w, 3) or packet[name].dtype != np.uint8:
                raise ValueError("Invalid onboard RGB image")
        self.last_stamp = stamp
        if self.targets is None:
            self.targets = q.astype(float)
        return packet

    def drive(self, speed=0.0, turn=0.0):
        self.act(
            "wheel", speed=min(float(speed), self.max_forward_speed), turn=float(turn)
        )

    def joints(self, values, steps):
        self.act("joint", values=values, steps=steps)
        for name, target in values.items():
            self.targets[self.names.index(name)] = target

    def phase(self, name, color):
        self.act("phase", name=name, color=color)

    def floor(self):
        return self.servo.command(self.sense())["xy"]

    def held_in_wrist_view(self, color, data=None):
        observation = self.sense() if data is None else data
        rgb = observation["left_wrist"]
        if self.carry_detector is None:
            visible = visible_in_jaws(rgb, color)
        else:
            grip, roll = observation["encoders"][self.jaw_indices]
            visible = predict(self.carry_detector, rgb, color, grip, roll)
        self.act("jaw observation", color=color, visible=visible)
        return visible

    def align(self):
        for present_left_side in (False, True):
            for _ in range(180):
                xy = self.floor()
                if xy is None:
                    if present_left_side:
                        raise ValueError("Lost cube while aligning the left gripper")
                    self.drive(turn=0.35)
                    continue
                x, y = xy
                bearing = math.atan2(y, x) - (0.71 if present_left_side else 0.0)
                distance = math.hypot(x, y)
                speed = (
                    float(np.clip((distance - 0.345) * 0.85, -0.055, 0.12))
                    if not present_left_side and abs(bearing) < 0.075
                    else 0.0
                )
                self.drive(speed, np.clip(1.8 * bearing, -0.45, 0.45))
                if abs(bearing) < 0.020 and (
                    present_left_side or abs(distance - 0.345) < 0.009
                ):
                    break
            else:
                raise ValueError("Could not align the cube with the left arm")
        self.drive()
        self.drive()
        xy = self.floor()
        if xy is None:
            raise ValueError("Lost the pickup point after braking")
        return np.array(xy)

    def plan_reach(self, q, target, subdivisions, vertical=False, supported=False):
        q = q.copy()
        start, _ = self.model.grip(q)
        waypoints = []
        for p in np.linspace(start, target, subdivisions + 1)[1:]:
            if supported:
                q = self.model.solve_supported_grasp(p, q)
            elif vertical or subdivisions == 1:
                q = self.model.solve(p, q, down=vertical, attempts=2 if vertical else 4)
            else:
                q = self.model.solve_nearby(p, q)
            waypoints.append({j: float(q[self.names.index(j)]) for j in self.model.arm})
        return waypoints, q

    def follow(self, waypoints, steps):
        for command in waypoints:
            self.joints(command, steps // len(waypoints))

    def reach(self, target, steps, subdivisions, vertical=False, supported=False):
        waypoints, _ = self.plan_reach(
            self.targets, target, subdivisions, vertical, supported
        )
        self.follow(waypoints, steps)

    def release_paths(self, place, height):
        inbound, q = self.plan_reach(
            self.targets, np.r_[place, height + 0.078], 12, supported=True
        )
        q[self.names.index(self.grip)] = -0.25
        outbound, _ = self.plan_reach(
            q, np.r_[place - [0.07, 0], height + 0.10], 9, supported=True
        )
        return inbound, outbound

    def tracked_descent(self, xy):
        previous = None
        velocity = np.zeros(2)
        heights = np.linspace(0.115, CUBE_CENTRE_Z + 0.004, 9)[1:]
        for index, height in enumerate(heights):
            packet = self.sense()
            reading = self.servo.command(
                packet,
                preferred_camera="left_wrist",
                resolve_ambiguity=self.resolve_pickup_ambiguity,
            )
            point = reading["xy"]
            stamp_ns = int(packet["stamp_ns"])
            stamp = stamp_ns * 1e-9
            if point is not None:
                point = np.array(point)
                if previous is not None:
                    measured = (point - previous[1]) / (stamp - previous[0])
                    velocity = np.clip(0.35 * velocity + 0.65 * measured, -0.06, 0.06)
                previous = (stamp, point)
            if previous is None or stamp - previous[0] > 1.6:
                raise ValueError("Lost the pickup point in the onboard cameras")
            # Each descent segment takes 30 interpolation ticks and 20 settling
            # ticks. The final intercept also allows for finger closure.
            horizon = 0.8 if index == len(heights) - 1 else 0.5
            xy = previous[1] + velocity * (stamp - previous[0] + horizon)
            self.act(
                "grasp observation",
                color=self.servo.target,
                xy=reading["xy"],
                cameras=reading["cameras"],
                stamp_ns=stamp_ns,
                velocity_xy_m_s=velocity.tolist(),
                intercept_xy_m=xy.tolist(),
                height_m=float(height),
            )
            self.reach(np.r_[xy, height], 30, 1, True)
        return xy

    def pick(self, xy):
        self.reach(np.r_[xy, 0.115], 250, 1, True)
        if self.track_grasp:
            xy = self.tracked_descent(xy)
        else:
            self.reach(np.r_[xy, CUBE_CENTRE_Z + 0.004], 250, 8, True)
        self.joints({self.grip: 0.70}, 55 if self.track_grasp else 200)
        self.reach(np.r_[xy, 0.085], 320, 8, True)

    def table_views(self, data, cameras=("head", "right_wrist")):
        views = []
        for name in cameras:
            chain, indices, mount, k, _ = self.servo.cameras[name]
            p, r = chain.forward(data["encoders"][indices])
            pose = np.eye(4)
            pose[:3, 3], pose[:3, :3] = p, r
            views.append((data[name], pose @ mount, k))
        return views

    def table(self, data=None):
        return table_edge(self.table_views(self.sense() if data is None else data))

    def dock(self, color):
        self.last_rim_distance = None
        consecutive = 0
        elevated = False
        for _ in range(200):
            edge = self.table()
            if edge is None:
                consecutive = 0
                self.drive(turn=0.38)
                continue
            consecutive += 1
            if consecutive < 2:
                self.drive()
                continue
            angle = math.atan2(edge["normal"][1], edge["normal"][0])
            distance = self.last_rim_distance = edge["distance"]
            self.act(
                "table observation",
                distance_m=float(distance),
                height_m=float(edge["height"]),
                angle_rad=float(angle),
                span_m=[float(v) for v in edge["span"]],
            )
            if abs(angle) < 0.09 and not elevated:
                self.drive()
                self.reach(
                    [0.285, 0.225, edge["height"] + 0.090], 800, 1, supported=True
                )
                if not self.held_in_wrist_view(color):
                    return None
                elevated = True
            if elevated and abs(angle) < 0.04 and abs(distance - DOCK_DISTANCE) < 0.010:
                self.drive()
                self.drive()
                final = self.table()
                if final is not None:
                    self.last_rim_distance = final["distance"]
                    return final
                self.drive()
                continue
            speed = (
                np.clip((distance - DOCK_DISTANCE) * 0.55, -0.025, 0.055)
                if abs(angle) < 0.09
                else 0.0
            )
            self.drive(speed, np.clip(angle * 1.5, -0.4, 0.4))
        raise ValueError("Stereo table docking timed out")

    def release_slot(self, edge, occupied):
        along_axis = edge["along"]
        side = np.clip(
            along_axis @ np.array([0.40, 0.155]),
            edge["span"][0] + 0.009,
            edge["span"][1] - 0.009,
        )
        for along, inset, search in release_candidates(side, edge["span"]):
            place = (edge["distance"] + inset) * edge["normal"] + along * along_axis
            if any(np.linalg.norm(place - point) < 0.078 for point in occupied):
                continue
            try:
                inbound, outbound = self.release_paths(place, edge["height"])
            except UnreachableGrasp:
                continue
            return place, inbound, outbound, inset, search
        return None

    def release_observation(self, packet, edge, color):
        self.act(
            "release observation",
            stamp_ns=int(packet["stamp_ns"]),
            color=color,
            height_m=float(edge["height"]),
            distance_m=float(edge["distance"]),
            normal=edge["normal"].tolist(),
            along=edge["along"].tolist(),
            span_m=list(edge["span"]),
        )

    def release_landmarks(self, packet, edge, colors):
        """Use one complete, unambiguous silhouette per camera and color."""
        landmarks = {}
        for camera, view in zip(
            CAMERAS, self.table_views(packet, CAMERAS), strict=True
        ):
            for color in colors:
                points = occupied_table_points([view], edge, [color])
                if len(points) == 1:
                    landmarks[camera, color] = points[0]
        return landmarks

    def reposition_for_release(self, edge, occupied, packet, color, prior_colors):
        landmarks = self.release_landmarks(packet, edge, prior_colors)
        if not landmarks:
            raise ValueError(
                "Cannot identify an earlier cube before changing the placement pose"
            )
        # Turning changes the wrist field of view. Two distinct previously placed
        # colors give the turn multiple landmarks to register the new image.
        colors = {item_color for camera, item_color in landmarks}
        if len(colors) >= 2:
            return self.turn_for_release(
                edge, occupied, packet, color, prior_colors, landmarks
            )
        return self.shift_for_release(edge, occupied, packet, color, prior_colors)

    def shift_for_release(self, edge, occupied, packet, color, prior_colors):
        # Predict whether shifting along the docked base's y axis could bring
        # a clear part of the observed rim into the left arm's reachable area.
        for offset in (-0.08, 0.08, -0.14, 0.14):
            shift = np.array([0.0, offset])
            predicted = edge | {
                "distance": edge["distance"] - edge["normal"] @ shift,
                "span": np.asarray(edge["span"]) - edge["along"] @ shift,
            }
            if self.release_slot(predicted, [p - shift for p in occupied]) is not None:
                break
        else:
            raise ValueError(
                "No reachable clear release point in the observed rim or bounded sidestep"
            )

        wheel_ids = [self.names.index(f"wheel_{i}_joint") for i in (1, 2, 3)]
        start = packet["encoders"][wheel_ids].copy()

        def displacement(sample):
            w1, w2, w3 = sample["encoders"][wheel_ids] - start
            return self.wheel_radius * np.array(
                [
                    (w2 - w3) / math.sqrt(3),
                    (2 * w1 - w2 - w3) / 3,
                ]
            )

        def observed_rim(sample):
            current = self.table(sample)
            if current is None:
                raise ValueError(
                    "Could not see the table rim during the placement sidestep"
                )
            old_normal, new_normal = edge["normal"], current["normal"]
            heading_error = math.atan2(
                old_normal[0] * new_normal[1] - old_normal[1] * new_normal[0],
                old_normal @ new_normal,
            )
            drift = current["distance"] - edge["distance"]
            alignment = current["normal"] @ edge["normal"]
            self.act(
                "table observation",
                standoff_change_m=float(drift),
                normal_agreement=float(alignment),
                heading_error_rad=heading_error,
                lateral_encoder_m=float(displacement(sample)[1]),
                color=color,
            )
            if abs(drift) > 0.025 or alignment < math.cos(0.09):
                raise ValueError(
                    "Lost the measured table standoff during the placement sidestep"
                )
            return current, heading_error

        self.act("table observation", lateral_goal_m=offset, color=color)
        heading_error = 0.0
        try:
            for _ in range(72):
                remaining = offset - displacement(packet)[1]
                if abs(remaining) < 0.005:
                    break
                self.act(
                    "wheel",
                    speed=0.0,
                    turn=float(np.clip(3.0 * heading_error, -0.2, 0.2)),
                    lateral_m_s=float(np.clip(2 * remaining, -0.018, 0.018)),
                )
                packet = self.sense()
                _, heading_error = observed_rim(packet)
            else:
                raise ValueError(
                    "Wheel encoders did not confirm the bounded placement sidestep"
                )
        finally:
            self.drive()
        packet = self.sense()
        current, _ = observed_rim(packet)
        self.release_observation(packet, current, color)
        if not self.held_in_wrist_view(color, packet):
            raise ValueError(
                "Cannot confirm the carried cube after the placement sidestep"
            )
        self.last_rim_distance = current["distance"]
        seen = occupied_table_points(
            self.table_views(packet, CAMERAS),
            current,
            prior_colors,
            include_clipped=True,
        )
        # Keep earlier sightings even if they are now occluded. The newly measured
        # rim supplies heading and standoff, wheel encoders supply travel along it.
        motion = displacement(packet)
        tangent_before = np.array([-edge["normal"][1], edge["normal"][0]])
        tangent_after = np.array([-current["normal"][1], current["normal"][0]])
        kept = []
        for point in occupied:
            inset = point @ edge["normal"] - edge["distance"]
            along = (point - motion) @ tangent_before
            kept.append(
                (current["distance"] + inset) * current["normal"]
                + along * tangent_after
            )
        self.act(
            "table observation",
            fresh_occupants=len(seen),
            retained_occupants=len(kept),
            color=color,
        )
        return current, seen + kept

    def turn_for_release(self, edge, occupied, packet, color, prior_colors, landmarks):
        # A turn in place changes where the left arm can reach on the observed rim.
        # Try both directions before considering a larger change of heading.
        for angle in (0.20, -0.20, 0.30, -0.30):
            cs, sn = math.cos(angle), math.sin(angle)
            rotate = np.array([[cs, -sn], [sn, cs]])
            predicted = edge | {
                "normal": rotate @ edge["normal"],
                "along": rotate @ edge["along"],
            }
            if self.release_slot(predicted, [rotate @ p for p in occupied]) is not None:
                break
        else:
            raise ValueError(
                "No reachable clear release point within the bounded placement turn"
            )

        original = edge["normal"]
        tangent = np.array([-original[1], original[0]])

        def observed_rim(sample):
            rim = self.table(sample)
            if rim is None:
                raise ValueError("Lost the table rim during the placement turn")
            drift = rim["distance"] - edge["distance"]
            heading = math.atan2(rim["normal"] @ tangent, rim["normal"] @ original)
            self.act(
                "table observation",
                placement_yaw_rad=heading,
                standoff_change_m=float(drift),
                color=color,
            )
            if abs(drift) > 0.025 or abs(heading) > abs(angle) + 0.06:
                raise ValueError(
                    "Placement turn exceeded its observed standoff or heading limit"
                )
            return rim, heading

        self.act("table observation", placement_yaw_goal_rad=angle, color=color)
        heading = 0.0
        try:
            for _ in range(50):
                error = heading - angle
                if abs(error) < 0.012:
                    break
                self.drive(turn=np.clip(1.5 * error, -0.16, 0.16))
                packet = self.sense()
                _, heading = observed_rim(packet)
            else:
                raise ValueError(
                    "Could not align the arm workspace with the observed tabletop"
                )
        finally:
            self.drive()
        packet = self.sense()
        current, heading = observed_rim(packet)
        self.release_observation(packet, current, color)
        if abs(heading - angle) > 0.035 or not self.held_in_wrist_view(color, packet):
            raise ValueError(
                "Cannot confirm placement heading and carried cube after braking"
            )
        new_normal = current["normal"]
        new_tangent = np.array([-new_normal[1], new_normal[0]])
        fresh = self.release_landmarks(packet, current, prior_colors)
        matches = fresh.keys() & landmarks.keys()
        if not matches:
            raise ValueError(
                "No previous cube could be matched across the placement turn"
            )
        shifts = [
            fresh[key] @ new_tangent - landmarks[key] @ tangent
            for key in sorted(matches)
        ]
        shift = float(np.median(shifts))
        if abs(shift) > 0.05 or np.ptp(shifts) > 0.045:
            raise ValueError(
                "The observed cubes disagree with a bounded placement turn"
            )
        self.last_rim_distance = current["distance"]
        seen = occupied_table_points(
            self.table_views(packet, CAMERAS),
            current,
            prior_colors,
            include_clipped=True,
        )
        # Retain occluded detections using the measured rim and matched cube motion.
        kept = []
        for point in occupied:
            inset = point @ original - edge["distance"]
            along = point @ tangent + shift
            kept.append(
                (current["distance"] + inset) * new_normal + along * new_tangent
            )
        self.act(
            "table observation",
            fresh_occupants=len(seen),
            retained_occupants=len(kept),
            matched_landmarks=len(matches),
            observed_tangent_shift_m=shift,
            color=color,
        )
        return current, seen + kept

    def back_away(self, color, exposure=None):
        if self.last_rim_distance is None:
            raise ValueError("Retreat requires a rim observed during this approach")
        self.phase("back away", color)
        # Never lower the arm over the table when a wrist or cube occludes its rim.
        # Opposed wheel displacements cancel yaw during the straight retreat.
        packet = self.sense() if exposure is None else exposure
        start = packet["encoders"][self.wheel_pair]
        required = max(0.0, 0.57 - self.last_rim_distance)
        for _ in range(65):
            edge = self.table(packet)
            delta = packet["encoders"][self.wheel_pair] - start
            retreat = self.wheel_radius * (delta[1] - delta[0]) / math.sqrt(3)
            if (
                edge is not None
                and edge["distance"] > 0.57
                and retreat >= required - 0.07
            ) or (edge is None and retreat >= required + 0.06):
                self.drive()
                break
            self.drive(-0.065, 0)
            packet = self.sense()
        else:
            raise ValueError("Table clearance was not recovered")
        self.joints(
            dict(
                left_j1_yaw=0.2,
                left_j2_pitch=0.0,
                left_j3_pitch=0.35,
                left_j4_pitch=-1.0,
                left_j5_roll=0.0,
                right_j1_yaw=0.2,
                right_j2_pitch=0.0,
            ),
            340,
        )

    def run(self):
        self.sense()
        for index, color in enumerate(self.colors):
            self.servo.target = color
            self.phase("locate", color)
            if index:
                self.joints(dict(head_pan_joint=0.25, head_tilt_joint=0.65), 160)
            # Move once between consecutive sensor requests; each read is a fresh exposure.
            self.drive()
            for transport_attempt in range(3):
                for attempt in range(2):
                    xy = self.align()
                    self.phase("pick", color)
                    self.pick(xy)
                    if self.held_in_wrist_view(color):
                        break
                    if attempt:
                        raise ValueError(
                            f"Could not confirm {color} in the wrist camera after two grasps"
                        )
                    self.joints({self.grip: -0.25}, 120)
                    self.joints(
                        dict(
                            left_j1_yaw=0.2,
                            left_j2_pitch=0.0,
                            left_j3_pitch=0.35,
                            left_j4_pitch=-1.0,
                            left_j5_roll=0.0,
                        ),
                        320,
                    )
                    self.phase("locate", color)
                self.phase("carry", color)
                self.joints(
                    dict(
                        head_pan_joint=0.0,
                        head_tilt_joint=-0.50,
                        right_j1_yaw=-0.46,
                        right_j2_pitch=-0.55,
                        right_j3_pitch=0.35,
                        right_j4_pitch=-1.0,
                        left_j6_gripper=0.84,
                    ),
                    320,
                )
                if transport_attempt == 2:
                    # Two wrist-confirmed drops warrant a different carry posture.
                    # Turn the fixed finger beneath the cube before rotating the base.
                    self.reach([0.285, 0.225, 0.21], 600, 1, supported=True)
                edge = self.dock(color)
                self.drive()
                if edge is not None and self.held_in_wrist_view(color):
                    break
                if transport_attempt == 2:
                    raise ValueError(
                        f"Lost {color} after two normal carries and a supported carry"
                    )
                self.joints({self.grip: -0.25}, 120)
                self.back_away(color)
                # A dropped cube may lie beside a table leg. Fold the spare arm up
                # after clearing the rim, before turning to reacquire the cube.
                self.joints(
                    dict(
                        head_pan_joint=0.25,
                        head_tilt_joint=0.65,
                        right_j2_pitch=0.10,
                        right_j3_pitch=-1.45,
                        right_j4_pitch=-0.45,
                    ),
                    200,
                )
                self.phase("locate", color)
            self.phase("place", color)
            occupied = []
            if index:
                self.drive()
                cameras = CAMERAS if self.wrist_clearance else ("head", "right_wrist")
                observation = self.sense()
                if self.wrist_clearance:
                    self.release_observation(observation, edge, color)
                occupied = occupied_table_points(
                    self.table_views(observation, cameras),
                    edge,
                    self.colors[:index],
                    include_clipped=self.wrist_clearance,
                )
            selection = self.release_slot(edge, occupied)
            if selection is None and index and self.wrist_clearance:
                edge, occupied = self.reposition_for_release(
                    edge, occupied, observation, color, self.colors[:index]
                )
                selection = self.release_slot(edge, occupied)
            if selection is None:
                raise ValueError(
                    "No reachable clear release point on the visible tabletop"
                )
            place, inbound, outbound, inset, search = selection
            z = edge["height"]
            self.follow(inbound, 340)
            self.act(
                "table observation",
                release_xy_m=place.tolist(),
                observed_occupants=len(occupied),
                release_search=search,
                inset_m=inset,
            )
            self.phase("release", color)
            self.joints({self.grip: -0.25}, 180)
            self.follow(outbound, 240)
            for _ in range(10):
                self.drive()
            exposure = self.sense()
            if self.held_in_wrist_view(color, exposure):
                # An open jaw can still carry a cube on its lower fixed finger.
                # Return above the observed surface and tip the finger sideways.
                self.reach(np.r_[place, z + 0.078], 320, 10, supported=True)
                self.joints({"left_j5_roll": 0.15}, 160)
                self.reach(np.r_[place - [0.07, 0], z + 0.10], 240, 9, supported=True)
                for _ in range(8):
                    self.drive()
                exposure = self.sense()
                if self.held_in_wrist_view(color, exposure):
                    raise ValueError(f"{color} is still on the finger after release")
            self.back_away(color, exposure)
        self.act("finished")


if __name__ == "__main__":
    Controller(json.loads(sys.stdin.buffer.readline())).run()
