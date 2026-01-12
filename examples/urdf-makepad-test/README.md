# URDF Makepad Test - STL Mesh Rendering

A Makepad application that renders STL meshes from URDF robot models with interactive 3D rotation.

## Features

- Load and render STL mesh files
- Combine multiple meshes into a single renderable object
- Multi-directional lighting for visibility from all angles
- Double-sided geometry rendering
- Interactive camera rotation via mouse drag

## Architecture

### Module Structure

```
src/
├── main.rs      # Application entry point, UI, camera controls
└── mesh.rs      # STL loading, geometry, and shader definitions
```

### Key Components

#### `MeshData` (mesh.rs)

Data structure for mesh geometry:
- `vertices: Vec<f32>` - Interleaved vertex data: position(3), id(1), normal(3), uv(2) = 9 floats per vertex
- `indices: Vec<u32>` - Triangle indices
- `bounds_min/max: [f32; 3]` - Bounding box

Key methods:
- `from_stl(path)` - Load mesh from STL file
- `combine(meshes)` - Merge multiple meshes into one
- `load_robot_meshes(assets_dir)` - Load all robot STL files from a directory
- `normalize()` - Center mesh at origin and scale to unit cube
- `make_double_sided()` - Duplicate triangles with reversed winding for visibility from both sides

#### `GeometryMesh3D` (mesh.rs)

Custom Makepad geometry type implementing `GeometryFields`:
- Manages GPU geometry buffer via `GeometryRef`
- Vertex layout: `geom_pos` (vec3), `geom_id` (float), `geom_normal` (vec3), `geom_uv` (vec2)

Key methods:
- `load_robot(cx, assets_dir)` - Load all robot meshes
- `load_stl(cx, path)` - Load single STL file
- `upload_mesh(cx, mesh)` - Upload geometry to GPU

#### `DrawMesh` (mesh.rs)

Shader for mesh rendering with:
- Rotation transform via `self.transform` matrix
- 6-directional lighting from all angles
- High ambient light (0.4) to prevent dark areas
- Uses `abs(dot(normal, light))` to light both front and back faces

Shader output:
```glsl
// Vertex shader
let scaled = rotated.xyz * 0.8;
return vec4(scaled.x, scaled.y, 0.0, 1.0);  // Z=0.0 avoids depth clipping
```

#### `URDFViewer` (main.rs)

Main application widget:
- Camera controls: yaw, pitch, distance
- Mouse drag for rotation
- Scroll wheel for zoom
- Loads robot meshes on initialization

## Robot Parts Loaded

From `/examples/urdf-sample/assets/`:

| Part | STL File | Vertices |
|------|----------|----------|
| Base | Base.stl | 26,604 |
| Base Motor | Base_Motor.stl | 9,252 |
| Shoulder | Rotation_Pitch.stl | 20,850 |
| Shoulder Motor | Rotation_Pitch_Motor.stl | 9,252 |
| Upper Arm | Upper_Arm.stl | 12,906 |
| Upper Arm Motor | Upper_Arm_Motor.stl | 9,132 |
| Lower Arm | Lower_Arm.stl | 20,826 |
| Lower Arm Motor | Lower_Arm_Motor.stl | 9,108 |
| Wrist | Wrist_Pitch_Roll.stl | 20,496 |
| Wrist Motor | Wrist_Pitch_Roll_Motor.stl | 8,016 |
| Gripper | Fixed_Jaw.stl | 15,870 |
| Gripper Motor | Fixed_Jaw_Motor.stl | 9,108 |
| Jaw | Moving_Jaw.stl | 10,626 |

**Total: 182,046 vertices (364,092 with double-sided geometry)**

## Technical Notes

### Depth Handling

- Z output is set to 0.0 to avoid depth clipping issues
- This means no depth-based occlusion between triangles
- Works well for viewing the complete robot mesh

### Lighting

Six light sources from all directions ensure no dark areas:
```glsl
let dp1 = abs(dot(normal, normalize(vec3(1.0, 1.0, 1.0))));   // Front-top-right
let dp2 = abs(dot(normal, normalize(vec3(-1.0, 1.0, 1.0))));  // Front-top-left
let dp3 = abs(dot(normal, normalize(vec3(0.0, -1.0, 1.0))));  // Front-bottom
let dp4 = abs(dot(normal, normalize(vec3(0.0, 1.0, -1.0))));  // Back-top
let dp5 = abs(dot(normal, normalize(vec3(1.0, 0.0, -1.0))));  // Back-right
let dp6 = abs(dot(normal, normalize(vec3(-1.0, 0.0, -1.0)))); // Back-left
```

### Geometry Hook Timing

Important Makepad pattern discovered:
- `GeometryMesh3D::after_apply` - Initialize default geometry here
- `DrawMesh::before_apply` - Call `before_apply_init_shader` here
- `DrawMesh::after_apply` - Call `after_apply_update_self` here

### Geometry Fingerprint

Use consistent fingerprint to keep shader bindings valid when updating mesh:
```rust
let mut fp = GeometryFingerprint::new(LiveType::of::<Self>());
fp.push(1.0);  // Fixed marker
self.geometry_ref = Some(cx.get_geometry_ref(fp));
```

## Usage

```bash
cd examples/urdf-makepad-test
cargo run
```

Controls:
- **Mouse drag** - Rotate camera
- **Scroll wheel** - Zoom in/out

## Dependencies

- `makepad-widgets` - UI framework
- `stl_io` - STL file parsing

## Future Improvements

1. **Joint Transforms** - Position each mesh according to URDF joint hierarchy
2. **Joint Animation** - Keyboard controls to animate robot joints
3. **URDF Parsing** - Automatically parse URDF XML for mesh paths and transforms
4. **Depth Sorting** - Implement proper depth testing for correct occlusion
5. **Material Colors** - Use URDF material definitions for mesh colors
