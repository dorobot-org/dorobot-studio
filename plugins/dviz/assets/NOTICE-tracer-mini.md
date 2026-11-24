# Tracer Mini body model

`tracer_mini.stl` in this directory is derived from
`tracer_mini_description/meshes/base_link.STL` of tracer-mini-ros2
(<https://github.com/23garyd/tracer-mini-ros2>, branch
`feature/tracer-mini-ros2`, commit 8020dcf; the file last changed in cc68ca0;
SHA-256 `fa96600e9f36e481574575e5e2b7eadd8b8964d1c9186920f60a32a6bb38589e`).
That package's `tracer_mini_description/package.xml` declares the Apache
License 2.0.

The mesh was first published by AgileX Robotics in ugv_gazebo_sim
(<https://github.com/agilexrobotics/ugv_gazebo_sim>,
`tracer_mini/meshes/base_link.STL`, commit 9d56f12, 2022), from which
tracer-mini-ros2 descends. That repository has no licence file, and its
`tracer_mini/package.xml` gives the licence as `TODO`.

## Changes

Made by `plugins/dviz/tools/make_tracer_model.py` (deterministic; running it
again on the same source gives the same bytes):

- simplified by vertex clustering (3.75 mm cubes) from 85,014 to 19,581
  triangles, 4,250,784 to 979,134 bytes;
- moved, not rotated or scaled, so that its bounding box is centred on
  x = −0.11 m, y = 0 m of the robot's `base_footprint` and its lowest point is
  at z = 0;
- normals recomputed, new 80-byte header.

## Licence

Apache License 2.0, <https://www.apache.org/licenses/LICENSE-2.0>, as declared by
tracer-mini-ros2's `tracer_mini_description/package.xml`; the same licence as the
rest of `plugins/dviz`.
