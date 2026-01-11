#!/usr/bin/env python3
"""
Simulated pose estimator node for Dora dataflow.

Generates synthetic robot pose (position + orientation) data.
"""

import os
import time
import math
import struct

try:
    from dora import Node
except ImportError:
    print("Warning: dora not installed, running in standalone mode")
    Node = None


class PoseSimulator:
    def __init__(self):
        self.x = 0.0
        self.y = 0.0
        self.z = 0.0
        self.yaw = 0.0

        self.linear_vel = 0.3  # m/s
        self.angular_vel = 0.1  # rad/s

        self.trajectory = []
        self.max_trajectory_points = 500

    def update(self, dt: float):
        """Update pose based on simulated motion."""
        # Circular motion pattern
        self.yaw += self.angular_vel * dt

        # Keep yaw in [-pi, pi]
        while self.yaw > math.pi:
            self.yaw -= 2 * math.pi
        while self.yaw < -math.pi:
            self.yaw += 2 * math.pi

        # Move forward in heading direction
        self.x += self.linear_vel * math.cos(self.yaw) * dt
        self.y += self.linear_vel * math.sin(self.yaw) * dt

        # Add some noise
        self.x += (hash(time.time_ns()) % 100 - 50) * 0.0001
        self.y += (hash(time.time_ns() + 1) % 100 - 50) * 0.0001

        # Record trajectory
        self.trajectory.append((self.x, self.y))
        if len(self.trajectory) > self.max_trajectory_points:
            self.trajectory.pop(0)

    def get_pose_bytes(self) -> bytes:
        """Serialize current pose."""
        timestamp_ns = int(time.time_ns())

        # Convert yaw to quaternion (rotation around Z axis)
        qw = math.cos(self.yaw / 2)
        qx = 0.0
        qy = 0.0
        qz = math.sin(self.yaw / 2)

        # Linear velocity in body frame
        vx = self.linear_vel
        vy = 0.0
        vz = 0.0

        # Angular velocity
        wx = 0.0
        wy = 0.0
        wz = self.angular_vel

        # Pack: [timestamp_ns:u64, x:f32, y:f32, z:f32, qx:f32, qy:f32, qz:f32, qw:f32,
        #        vx:f32, vy:f32, vz:f32, wx:f32, wy:f32, wz:f32]
        return struct.pack('<Qffffffffffff',
            timestamp_ns,
            self.x, self.y, self.z,
            qx, qy, qz, qw,
            vx, vy, vz,
            wx, wy, wz
        )

    def get_trajectory_bytes(self) -> bytes:
        """Serialize trajectory."""
        timestamp_ns = int(time.time_ns())

        # Pack: [timestamp_ns:u64, num_points:u32, points...]
        data = struct.pack('<QI', timestamp_ns, len(self.trajectory))
        for x, y in self.trajectory:
            data += struct.pack('<ff', x, y)

        return data


def main():
    update_rate = float(os.environ.get('UPDATE_RATE_HZ', '20'))
    interval = 1.0 / update_rate

    sim = PoseSimulator()
    last_time = time.time()

    if Node is not None:
        node = Node()

        for event in node:
            if event["type"] == "STOP":
                break

            current_time = time.time()
            dt = current_time - last_time
            last_time = current_time

            sim.update(dt)

            node.send_output("robot_pose", sim.get_pose_bytes())
            node.send_output("trajectory", sim.get_trajectory_bytes())

            time.sleep(interval)
    else:
        # Standalone mode
        while True:
            current_time = time.time()
            dt = current_time - last_time
            last_time = current_time

            sim.update(dt)

            pose = sim.get_pose_bytes()
            print(f"Pose: x={sim.x:.2f}, y={sim.y:.2f}, yaw={math.degrees(sim.yaw):.1f}°")

            time.sleep(interval)


if __name__ == "__main__":
    main()
