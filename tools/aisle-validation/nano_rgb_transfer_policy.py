"""Nano floor-to-table controller. Its pipe carries only onboard RGB and measured joint state."""

import io
import json
import math
from pathlib import Path
import struct
import sys

import cv2
import numpy as np

from nano_rgb_policy import CAMERAS, CUBE_CENTRE_Z, VisualServo
from nano_rgb_table import table_edge

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "nano-three-cubes"))
from model import Kinematics  # noqa: E402


def visible_in_jaws(rgb, color):
    hue, sat, val = cv2.cvtColor(rgb, cv2.COLOR_RGB2HSV).transpose(2, 0, 1)
    low, high = {"red": (-7, 7), "green": (45, 82), "blue": (106, 132)}[color]
    color_mask = (
        ((hue >= low) & (hue <= high))
        if low >= 0
        else ((hue <= high) | (hue >= 180 + low))
    )
    mask = color_mask & (sat > 125) & (val > 55)
    # Calibrated wrist ROI spans the gripper opening when the jaws are closed.
    h, w = mask.shape
    opening = mask[round(0.61 * h) : round(0.89 * h), round(0.17 * w) : round(0.48 * w)]
    return float(opening.mean()) >= 0.14 and float(mask.mean()) >= 0.04


# Reserve lateral space for rolling after the open finger withdraws. Moving the
# elevated base closer keeps the desired release point within the arm workspace.
RELEASE_INSET = 0.085
DOCK_DISTANCE = 0.385 - RELEASE_INSET


class Controller:
    def __init__(self, config):
        self.input = sys.stdin.buffer
        self.model = Kinematics(Path(config["model"]))
        self.names = config["joint_names"]
        if self.model.names != self.names:
            raise ValueError("Joint order differs from the URDF")
        self.colors = config["colors"]
        self.servo = VisualServo(config | {"color": self.colors[0], "standoff_m": 0.45})
        self.targets = None
        self.last_stamp = -1
        self.grip = "left_j6_gripper"
        self.wheel_pair = [self.names.index(f"wheel_{i}_joint") for i in (2, 3)]
        self.wheel_radius = config["rolling_radius_m"]

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
        self.act("wheel", speed=float(speed), turn=float(turn))

    def joints(self, values, steps):
        self.act("joint", values=values, steps=steps)
        for name, target in values.items():
            self.targets[self.names.index(name)] = target

    def phase(self, name, color):
        self.act("phase", name=name, color=color)

    def floor(self):
        return self.servo.command(self.sense())["xy"]

    def held_in_wrist_view(self, color, data=None):
        rgb = (self.sense() if data is None else data)["left_wrist"]
        visible = visible_in_jaws(rgb, color)
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

    def reach(self, target, steps, subdivisions, vertical=False, supported=False):
        q = self.targets.copy()
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
        for command in waypoints:
            self.joints(command, steps // subdivisions)

    def pick(self, xy):
        self.reach(np.r_[xy, 0.115], 250, 1, True)
        self.reach(np.r_[xy, CUBE_CENTRE_Z + 0.004], 250, 8, True)
        self.joints({self.grip: 0.55}, 180)
        self.reach(np.r_[xy, 0.085], 320, 8, True)

    def table(self, data=None):
        if data is None:
            data = self.sense()
        views = []
        for name in ("head", "right_wrist"):
            chain, indices, mount, k, _ = self.servo.cameras[name]
            p, r = chain.forward(data["encoders"][indices])
            pose = np.eye(4)
            pose[:3, 3], pose[:3, :3] = p, r
            views.append((data[name], pose @ mount, k))
        return table_edge(views)

    def dock(self, color):
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

    def back_away(self, color, exposure=None):
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
            for transport_attempt in range(2):
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
                        left_j6_gripper=0.72,
                    ),
                    320,
                )
                edge = self.dock(color)
                self.drive()
                if edge is not None and self.held_in_wrist_view(color):
                    break
                if transport_attempt:
                    raise ValueError(f"Lost {color} during two transport attempts")
                self.joints({self.grip: -0.25}, 120)
                self.back_away(color)
                self.joints(dict(head_pan_joint=0.25, head_tilt_joint=0.65), 160)
                self.phase("locate", color)
            self.phase("place", color)
            t = edge["along"]
            side = np.clip(
                t @ np.array([0.40, 0.155]),
                edge["span"][0] + 0.009,
                edge["span"][1] - 0.009,
            )
            place = (edge["distance"] + RELEASE_INSET) * edge["normal"] + side * t
            z = edge["height"]
            self.reach(np.r_[place, z + 0.078], 340, 12, supported=True)
            self.phase("release", color)
            self.joints({self.grip: -0.25}, 180)
            self.reach(np.r_[place - [0.07, 0], z + 0.10], 240, 9, supported=True)
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
