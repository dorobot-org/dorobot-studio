import unittest

import cv2
import numpy as np

from nano_rgb_table import occupied_table_points, table_edge


def camera(origin, target):
    forward = target - origin
    forward /= np.linalg.norm(forward)
    right = np.cross(forward, [0.0, 0.0, 1.0])
    right /= np.linalg.norm(right)
    pose = np.eye(4)
    pose[:3, :3] = np.column_stack([right, np.cross(right, forward), -forward])
    pose[:3, 3] = origin
    return pose


def view(origin, corners):
    intrinsics = np.array([[260.0, 0, 240], [0, 260.0, 180], [0, 0, 1.0]])
    pose = camera(np.array(origin), np.mean(corners, axis=0))
    local = (corners - pose[:3, 3]) @ pose[:3, :3]
    uv = local[:, :2] / (-local[:, 2:])
    uv *= np.array([260.0, -260.0])
    uv += [240, 180]
    image = np.full((360, 480, 3), 210, np.uint8)
    cv2.fillConvexPoly(image, np.rint(uv).astype(np.int32), (119, 89, 54))
    return image, pose, intrinsics


class TableTests(unittest.TestCase):
    def setUp(self):
        self.corners = np.array(
            [
                [0.62, -0.26, 0.53],
                [0.62, 0.34, 0.53],
                [0.62, 0.34, 0.45],
                [0.62, -0.26, 0.45],
            ]
        )
        self.head = view([0.11, 0, 0.42], self.corners)
        self.wrist = view([0.14, -0.21, 0.31], self.corners)

    def test_recovers_unknown_edge_height_and_standoff(self):
        edge = table_edge([self.head, self.wrist])
        self.assertIsNotNone(edge)
        np.testing.assert_allclose(edge["normal"], [1.0, 0.0], atol=0.015)
        self.assertAlmostEqual(edge["distance"], 0.62, delta=0.008)
        self.assertAlmostEqual(edge["height"], 0.53, delta=0.006)
        self.assertGreater(np.ptp(edge["span"]), 0.15)

    def test_partial_cube_in_overhead_wrist_view_reserves_release_space(self):
        # A cube on the tabletop is visible along the top image boundary.
        rgb = np.zeros((100, 100, 3), dtype=np.uint8)
        rgb[:12, 45:65, 1] = 180
        pose = np.eye(4)
        pose[:3, 3] = [0.4, 0.20, 0.75]
        k = np.array([[100, 0, 50], [0, 100, 50], [0, 0, 1]])
        edge = dict(height=0.53, normal=np.array([1.0, 0.0]), distance=0.30)
        occupied = occupied_table_points([(rgb, pose, k)], edge, ["green"])
        self.assertEqual(len(occupied), 1)
        np.testing.assert_allclose(occupied[0], [0.409, 0.289], atol=0.001)
        self.assertEqual(occupied_table_points([(rgb, pose, k)], edge, ["red"]), [])

    def test_no_depth_from_one_camera_or_absent_surface(self):
        self.assertIsNone(table_edge([self.head, self.head]))
        blank, pose, k = self.wrist
        self.assertIsNone(table_edge([self.head, (np.zeros_like(blank), pose, k)]))
