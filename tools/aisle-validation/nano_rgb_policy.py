"""Wheel servo for a visible 40 mm floor cube, using nominal onboard RGB calibration."""

import io
import json
import math
from pathlib import Path
import struct
import sys

import cv2
import numpy as np
from aisle.kinematics import load_urdf_chain

GRASP_RGB_ASSOCIATION = "wrist-agreement-v1"
CAMERAS = ("head", "left_wrist", "right_wrist")
COLORS = ("red", "green", "blue")
# Nominal wheel support plane in the extended Nano URDF's base_link frame.
CUBE_CENTRE_Z = -0.0343 + 0.020


def detect(rgb, name, intrinsics, pose):
    channel = COLORS.index(name)
    dominant = rgb[:, :, channel].astype(np.int16)
    others = np.delete(rgb, channel, axis=2).max(axis=2)
    mask = ((dominant > 75) & (dominant > others * 1.4 + 15)).astype(np.uint8)
    _, _, components, centres = cv2.connectedComponentsWithStats(mask)
    height, width = rgb.shape[:2]
    points = []
    for (x, y, w, h, area), (u, v) in zip(components[1:], centres[1:]):
        if area < 10 or min(w, h) < 3 or max(w, h) > 3 * min(w, h):
            continue
        if x <= 0 or y <= 0 or x + w >= width or y + h >= height:
            continue
        fx, fy, cx, cy = (
            intrinsics[0, 0],
            intrinsics[1, 1],
            intrinsics[0, 2],
            intrinsics[1, 2],
        )
        ray = pose[:3, :3] @ [(u - cx) / fx, -(v - cy) / fy, -1.0]
        if ray[2] >= -0.04:
            continue
        depth = (CUBE_CENTRE_Z - pose[2, 3]) / ray[2]
        if not 0.018 < w * depth / fx < 0.090:
            continue
        points.append((pose[:3, 3] + depth * ray)[:2])
    return points


class VisualServo:
    def __init__(self, config):
        self.target = config["color"]
        self.distance = config["standoff_m"]
        self.names = config["joint_names"]
        self.cameras = {}
        for name in CAMERAS:
            spec = config["cameras"][name]
            chain = load_urdf_chain(
                Path(config["model"]), "base_link", spec["parent_link"]
            )
            self.cameras[name] = (
                chain,
                [self.names.index(joint) for joint in chain.joint_names],
                np.array(spec["link_from_camera_gl"]),
                np.array(spec["intrinsics"]),
                tuple(spec["resolution"]),
            )
        self.last_stamp = -1

    def command(self, packet, *, preferred_camera=None, resolve_ambiguity=False):
        if set(packet) != {*CAMERAS, "stamp_ns", "encoders"}:
            raise ValueError("unexpected observation channels")
        stamp = packet["stamp_ns"]
        q = packet["encoders"]
        if (
            stamp.shape != ()
            or stamp.dtype.kind not in "iu"
            or int(stamp) <= self.last_stamp
        ):
            raise ValueError("nonmonotonic sensor timestamp")
        if q.shape != (len(self.names),) or not np.isfinite(q).all():
            raise ValueError("invalid encoder vector")
        self.last_stamp = int(stamp)
        candidates = {}
        for name, (chain, indices, mount, intrinsics, (w, h)) in self.cameras.items():
            rgb = packet[name]
            if rgb.dtype != np.uint8 or rgb.shape != (h, w, 3):
                raise ValueError("invalid RGB frame")
            pos, rot = chain.forward(q[indices])
            pose = np.eye(4)
            pose[:3, 3], pose[:3, :3] = pos, rot
            views = detect(rgb, self.target, intrinsics, pose @ mount)
            if len(views) > 1 and not resolve_ambiguity:
                return self.stop("ambiguous")
            if views:
                candidates[name] = views
        if not candidates:
            return self.stop("lost")
        points = []
        used = list(candidates)
        for name, views in candidates.items():
            if len(views) == 1:
                points.append(views[0])
                continue
            # During a tracked descent a single wrist candidate can resolve a
            # second camera's alternatives if exactly one lies within 35 mm.
            wrist = candidates.get(preferred_camera, [])
            if len(wrist) != 1:
                return self.stop("ambiguous")
            matches = [p for p in views if np.linalg.norm(p - wrist[0]) <= 0.035]
            if len(matches) != 1:
                return self.stop("ambiguous")
            points.append(matches[0])
        point = np.median(points, axis=0)
        if any(np.linalg.norm(p - point) > 0.06 for p in points):
            return self.stop("inconsistent views")
        if preferred_camera in used:
            point = points[used.index(preferred_camera)]
        bearing = math.atan2(point[1], point[0])
        error = np.linalg.norm(point) - self.distance
        aligned = abs(bearing) < 0.08
        reached = aligned and abs(error) < 0.022
        speed = min(0.13, max(-0.045, 0.85 * error)) if aligned and not reached else 0.0
        omega = 0.0 if reached else float(np.clip(1.8 * bearing, -0.5, 0.5))
        return dict(
            state="reached" if reached else "approaching",
            xy=point.tolist(),
            cameras=used,
            speed=speed,
            yaw_rate=omega,
        )

    @staticmethod
    def stop(state):
        return dict(state=state, xy=None, cameras=[], speed=0.0, yaw_rate=0.0)


def main():
    stream = sys.stdin.buffer
    servo = VisualServo(json.loads(stream.readline()))
    while header := stream.read(4):
        (size,) = struct.unpack(">I", header)
        if not 0 < size <= 12_000_000:
            raise ValueError("observation exceeds frame limit")
        payload = stream.read(size)
        if len(payload) != size:
            raise EOFError("incomplete observation")
        with np.load(io.BytesIO(payload), allow_pickle=False) as packet:
            print(json.dumps(servo.command(packet), allow_nan=False), flush=True)


if __name__ == "__main__":
    main()
