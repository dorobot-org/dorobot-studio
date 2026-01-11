#!/usr/bin/env python3
"""
Simulated LiDAR driver node for Dora dataflow.

Generates synthetic point cloud and 2D scan data.
"""

import os
import time
import math
import struct
from typing import Callable

import numpy as np

# Dora node API
try:
    from dora import Node
except ImportError:
    print("Warning: dora not installed, running in standalone mode")
    Node = None


def generate_point_cloud(frame_id: int, num_points: int = 1000) -> bytes:
    """Generate a synthetic point cloud."""
    # Simulate a circular room with some objects
    points = []

    # Room walls (circular)
    wall_points = num_points // 2
    for i in range(wall_points):
        angle = 2 * math.pi * i / wall_points
        radius = 5.0 + 0.2 * math.sin(angle * 8)  # Slightly irregular walls
        x = radius * math.cos(angle)
        y = radius * math.sin(angle)
        z = np.random.uniform(0, 2.0)  # Height variation

        # Color based on height (grayscale)
        intensity = int(255 * z / 2.0)
        points.append((x, y, z, intensity, intensity, intensity))

    # Some random objects
    object_points = num_points - wall_points
    for _ in range(object_points):
        # Random position within room
        r = np.random.uniform(0.5, 4.0)
        angle = np.random.uniform(0, 2 * math.pi)
        x = r * math.cos(angle)
        y = r * math.sin(angle)
        z = np.random.uniform(0, 1.5)

        # Random color
        r_col = np.random.randint(100, 255)
        g_col = np.random.randint(100, 255)
        b_col = np.random.randint(100, 255)
        points.append((x, y, z, r_col, g_col, b_col))

    # Serialize as binary: [num_points:u32, frame_id:u64, timestamp_ns:u64, points...]
    # Each point: [x:f32, y:f32, z:f32, r:u8, g:u8, b:u8, padding:u8, intensity:f32]
    timestamp_ns = int(time.time_ns())

    data = struct.pack('<IQQ', len(points), frame_id, timestamp_ns)
    for x, y, z, r, g, b in points:
        data += struct.pack('<fffBBBBf', x, y, z, r, g, b, 0, 1.0)

    return data


def generate_scan_2d(frame_id: int, num_rays: int = 360) -> bytes:
    """Generate a synthetic 2D LiDAR scan."""
    angle_min = 0.0
    angle_max = 2 * math.pi
    angle_increment = (angle_max - angle_min) / num_rays
    range_min = 0.1
    range_max = 10.0

    ranges = []
    intensities = []

    for i in range(num_rays):
        angle = angle_min + i * angle_increment

        # Simulate room boundary with some objects
        base_range = 5.0 + 0.3 * math.sin(angle * 4)

        # Add some random obstacles
        if 0.5 < angle < 0.8 or 2.0 < angle < 2.3:
            base_range = min(base_range, 2.0 + np.random.uniform(-0.1, 0.1))

        ranges.append(base_range + np.random.uniform(-0.05, 0.05))
        intensities.append(np.random.uniform(0.5, 1.0))

    timestamp_ns = int(time.time_ns())

    # Serialize
    data = struct.pack('<IQffffffff',
        num_rays, timestamp_ns,
        angle_min, angle_max, angle_increment,
        range_min, range_max, 0.0, 0.0, 0.0  # padding
    )

    for r, intensity in zip(ranges, intensities):
        data += struct.pack('<ff', r, intensity)

    return data


def main():
    scan_rate = float(os.environ.get('SCAN_RATE_HZ', '10'))
    num_points = int(os.environ.get('NUM_POINTS', '1000'))

    interval = 1.0 / scan_rate
    frame_id = 0

    if Node is not None:
        node = Node()

        for event in node:
            if event["type"] == "INPUT":
                pass  # No inputs for this driver
            elif event["type"] == "STOP":
                break

            # Generate and send data
            point_cloud = generate_point_cloud(frame_id, num_points)
            scan_2d = generate_scan_2d(frame_id)

            node.send_output("point_cloud", point_cloud)
            node.send_output("scan_2d", scan_2d)

            frame_id += 1
            time.sleep(interval)
    else:
        # Standalone mode for testing
        while True:
            point_cloud = generate_point_cloud(frame_id, num_points)
            scan_2d = generate_scan_2d(frame_id)
            print(f"Frame {frame_id}: {len(point_cloud)} bytes point cloud, {len(scan_2d)} bytes scan")
            frame_id += 1
            time.sleep(interval)


if __name__ == "__main__":
    main()
