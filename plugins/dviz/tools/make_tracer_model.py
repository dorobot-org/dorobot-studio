#!/usr/bin/env python3
"""Makes the Tracer Mini body model the dviz plugin draws in Rerun.

Reads the binary STL of the body (`base_link.STL` of tracer-mini-ros2), thins
it by vertex clustering until the output fits in `--max-bytes`, moves it so
its bounding box is centred on (`--center-x`, `--center-y`) with its lowest
point at `--floor-z`, and writes it as a binary STL. The mesh is not rotated
and not scaled. The same input always gives byte-identical output.

Vertex clustering: the source's distinct vertices are binned into cubes of
one size; each cube's vertices become their mean; triangles that lose a
corner, lose their area or repeat another triangle are dropped. The cube size
is the result of a bisection for a mesh that fits.

Needs only numpy. Usage (from the repository root):

    PYTHONNOUSERSITE=1 python3 plugins/dviz/tools/make_tracer_model.py \
        ~/nvme/tracer_ws/src/tracer-mini-ros2/tracer_mini_description/meshes/base_link.STL \
        plugins/dviz/assets/tracer_mini.stl
"""

import argparse
import sys

import numpy as np

TRIANGLE = np.dtype(
    [("normal", "<f4", (3,)), ("vertices", "<f4", (3, 3)), ("attribute", "<u2")]
)
HEADER = b"Tracer Mini base_link, vertex-clustered by make_tracer_model.py"
# The source is in metres; a horizontal extent outside this range means other
# units or another mesh, and nothing is written.
PLAUSIBLE_EXTENT = (0.3, 0.6)
BISECTION_STEPS = 40


def read_stl(path):
    """The triangles of a binary STL, as an (n, 3, 3) float64 array."""
    with open(path, "rb") as f:
        data = f.read()
    if len(data) < 84:
        sys.exit(f"{path}: {len(data)} bytes, shorter than a binary STL header")
    count = int.from_bytes(data[80:84], "little")
    if len(data) != 84 + 50 * count:
        sys.exit(f"{path}: {len(data)} bytes do not hold the {count} triangles its header names")
    triangles = np.frombuffer(data, dtype=TRIANGLE, count=count, offset=84)
    return triangles["vertices"].astype(np.float64), len(data)


def write_stl(path, triangles):
    """Writes (n, 3, 3) triangles as a binary STL with computed normals."""
    vertices = triangles.astype(np.float32)
    edges = np.cross(
        vertices[:, 1].astype(np.float64) - vertices[:, 0],
        vertices[:, 2].astype(np.float64) - vertices[:, 0],
    )
    normals = edges / np.linalg.norm(edges, axis=1)[:, None]
    out = np.zeros(len(vertices), dtype=TRIANGLE)
    out["normal"] = normals.astype(np.float32)
    out["vertices"] = vertices
    with open(path, "wb") as f:
        f.write(HEADER.ljust(80, b" "))
        f.write(len(out).to_bytes(4, "little"))
        f.write(out.tobytes())


class Clusterer:
    """Vertex clustering of one mesh at any cube size."""

    def __init__(self, triangles):
        corners = triangles.reshape(-1, 3)
        self.points, self.corner_of = np.unique(corners, axis=0, return_inverse=True)
        self.corner_of = self.corner_of.reshape(-1)
        self.low = self.points.min(axis=0)

    def triangles(self, cell):
        """The mesh with every cube of side `cell` merged into one vertex."""
        cubes = np.floor((self.points - self.low) / cell).astype(np.int64)
        dims = cubes.max(axis=0) + 1
        keys = (cubes[:, 2] * dims[1] + cubes[:, 1]) * dims[0] + cubes[:, 0]
        _, cluster = np.unique(keys, return_inverse=True)
        cluster = cluster.reshape(-1)
        members = np.bincount(cluster)
        centres = np.stack(
            [np.bincount(cluster, weights=self.points[:, axis]) for axis in range(3)],
            axis=1,
        ) / members[:, None]
        faces = cluster[self.corner_of].reshape(-1, 3)
        whole = (
            (faces[:, 0] != faces[:, 1])
            & (faces[:, 1] != faces[:, 2])
            & (faces[:, 0] != faces[:, 2])
        )
        faces = faces[whole]
        # One triangle per set of corners; the first one keeps its winding.
        _, first = np.unique(np.sort(faces, axis=1), axis=0, return_index=True)
        faces = faces[np.sort(first)]
        triangles = centres[faces]
        # As float32, as written, so no triangle ends up without a normal.
        single = triangles.astype(np.float32).astype(np.float64)
        area = np.linalg.norm(
            np.cross(single[:, 1] - single[:, 0], single[:, 2] - single[:, 0]), axis=1
        )
        return triangles[area > 0]


def simplify(triangles, max_triangles):
    """The finest clustering (by bisection of the cube size) that leaves at
    most `max_triangles`, and its cube size (0: not simplified)."""
    if len(triangles) <= max_triangles:
        return triangles, 0.0
    clusterer = Clusterer(triangles)
    fine, coarse = 0.0, float((clusterer.points.max(axis=0) - clusterer.low).max())
    best = clusterer.triangles(coarse)
    for _ in range(BISECTION_STEPS):
        cell = (fine + coarse) / 2
        candidate = clusterer.triangles(cell)
        if len(candidate) <= max_triangles:
            coarse, best = cell, candidate
        else:
            fine = cell
    return best, coarse


def bounds(triangles):
    points = triangles.reshape(-1, 3)
    return points.min(axis=0), points.max(axis=0)


def describe(name, triangles, size):
    low, high = bounds(triangles)
    print(f"{name}: {len(triangles)} triangles, {size} bytes")
    for axis, label in enumerate("xyz"):
        print(
            f"  {label} {low[axis]:+.4f} .. {high[axis]:+.4f} m"
            f" (extent {high[axis] - low[axis]:.4f} m)"
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("source", help="binary STL of the body, in metres")
    parser.add_argument("output", help="binary STL to write")
    parser.add_argument("--max-bytes", type=int, default=1_000_000)
    parser.add_argument("--center-x", type=float, default=-0.11)
    parser.add_argument("--center-y", type=float, default=0.0)
    parser.add_argument("--floor-z", type=float, default=0.0)
    args = parser.parse_args()

    source, size = read_stl(args.source)
    describe("source", source, size)
    low, high = bounds(source)
    extent = high - low
    longer = "x" if extent[0] >= extent[1] else "y"
    print(
        f"  longer horizontal axis: {longer}"
        f" (x {extent[0]:.4f} m, y {extent[1]:.4f} m)"
    )
    low_limit, high_limit = PLAUSIBLE_EXTENT
    if not all(low_limit <= extent[axis] <= high_limit for axis in (0, 1)):
        sys.exit(
            f"{args.source}: x extent {extent[0]:.4f}, y extent {extent[1]:.4f};"
            f" both should be {low_limit}..{high_limit} m. Check its units."
            " Nothing written (the mesh is never scaled)."
        )

    max_triangles = (args.max_bytes - 84) // 50
    triangles, cell = simplify(source, max_triangles)
    if len(triangles) == 0:
        sys.exit(f"no triangles left within {args.max_bytes} bytes")
    low, high = bounds(triangles)
    offset = np.array(
        [
            args.center_x - (low[0] + high[0]) / 2,
            args.center_y - (low[1] + high[1]) / 2,
            args.floor_z - low[2],
        ]
    )
    write_stl(args.output, triangles + offset)

    written, size = read_stl(args.output)
    print(f"cube size: {cell:.5f} m")
    describe("output", written, size)
    low, high = bounds(written)
    print(
        f"  centre x {(low[0] + high[0]) / 2:+.4f}, y {(low[1] + high[1]) / 2:+.4f};"
        f" floor z {low[2]:+.4f}"
    )


if __name__ == "__main__":
    main()
