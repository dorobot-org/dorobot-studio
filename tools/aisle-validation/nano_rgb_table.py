"""Estimate a horizontal tabletop edge from two calibrated onboard RGB views."""

import cv2
import numpy as np


def rim_pixels(rgb):
    hsv = cv2.cvtColor(rgb, cv2.COLOR_RGB2HSV)
    mask = cv2.inRange(hsv, (9, 65, 88), (26, 180, 195))
    height, width = mask.shape
    lines = []
    contours, _ = cv2.findContours(mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    for contour in contours:
        x, y, w, h = cv2.boundingRect(contour)
        if w < width * 0.22 or h < 5 or cv2.contourArea(contour) < w * 3:
            continue
        fill = np.zeros_like(mask)
        cv2.drawContours(fill, [contour], -1, 1, cv2.FILLED)
        u = np.arange(x + 2, x + w - 2, 2)
        v = fill[:, u].argmax(axis=0)
        valid = (v > 2) & (v < height - 4)
        u, v = u[valid], v[valid]
        if len(u) < 15:
            continue
        # A corner can expose two surfaces. Fit the longest straight part of the rim.
        samples = np.linspace(0, len(u) - 1, 13).astype(int)
        for i in samples:
            for j in samples:
                if u[j] - u[i] < width * 0.18:
                    continue
                slope = (v[j] - v[i]) / (u[j] - u[i])
                if abs(slope) > 0.65:
                    continue
                inside = np.abs(v - v[i] - slope * (u - u[i])) < 1.8
                if inside.sum() < width * 0.10:
                    continue
                lo, hi = u[inside].min(), u[inside].max()
                if inside.sum() * 2 < 0.72 * (hi - lo):
                    continue
                lines.append((int(inside.sum()), u[inside], v[inside]))
    if not lines:
        return None
    _, u, v = max(lines, key=lambda entry: entry[0])
    slope, offset = np.polyfit(u, v, 1)
    ends = np.array([u.min(), u.max()])
    return np.stack([ends, slope * ends + offset], axis=1)


def rays(pixels, pose, intrinsics):
    u, v = pixels.T
    local = np.stack(
        [
            (u - intrinsics[0, 2]) / intrinsics[0, 0],
            -(v - intrinsics[1, 2]) / intrinsics[1, 1],
            -np.ones(len(u)),
        ]
    )
    return (pose[:3, :3] @ local).T


def table_edge(views):
    """Triangulate an edge in base_link. Views supply (RGB, encoder-derived pose, K)."""
    planes = []
    sight = []
    for rgb, pose, intrinsics in views:
        pixels = rim_pixels(rgb)
        if pixels is None:
            return None
        a, b = rays(pixels, pose, intrinsics)
        normal = np.cross(a, b)
        normal /= np.linalg.norm(normal)
        origin = pose[:3, 3]
        planes.append((normal, normal @ origin))
        sight.append((origin, np.array([a, b])))
    normals = np.array([p[0] for p in planes])
    direction = np.cross(*normals)
    if np.linalg.norm(direction) < 0.04:
        return None
    direction /= np.linalg.norm(direction)
    if abs(direction[2]) > 0.08:
        return None
    point = np.linalg.lstsq(normals, [p[1] for p in planes], rcond=None)[0]
    along = direction[:2] / np.linalg.norm(direction[:2])
    normal = np.array([-along[1], along[0]])
    if normal @ point[:2] < 0:
        normal *= -1
    distance = normal @ point[:2]
    foot = distance * normal
    level = point[2] + np.dot(foot - point[:2], along) * direction[2] / np.linalg.norm(
        direction[:2]
    )
    if not 0.20 < level < 0.75 or not 0.18 < distance < 2.3:
        return None
    intervals = []
    for origin, ray in sight:
        denom = ray[:, :2] @ normal
        if np.min(denom) < 0.05:
            return None
        depth = (distance - origin[:2] @ normal) / denom
        if np.min(depth) <= 0:
            return None
        intervals.append(sorted((origin[:2] + depth[:, None] * ray[:, :2]) @ along))
    lo = max(i[0] for i in intervals)
    hi = min(i[1] for i in intervals)
    if hi - lo < 0.09:
        return None
    return dict(
        normal=normal,
        along=along,
        distance=distance,
        height=level,
        span=(min(i[0] for i in intervals), max(i[1] for i in intervals)),
    )
