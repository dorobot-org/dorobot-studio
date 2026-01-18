# URDF Viewer Architecture Analysis & Refactoring Plan

## Current Architecture

### Third-Party Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `makepad-widgets` | git (rik branch) | UI framework, shaders, GPU geometry, events |
| `urdf-rs` | 0.9 | URDF XML parsing |
| `glam` | 0.29 | Linear algebra (quaternions, matrices, vectors) |
| `stl_io` | 0.7 | STL mesh file loading |

### Module Structure

```
src/
├── main.rs          # App, URDFViewer widget, Robot model (all in one file)
└── mesh.rs          # MeshData, GeometryMesh3D, DrawMesh
```

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         URDFViewer                              │
│  (Monolithic widget containing everything)                      │
├─────────────────────────────────────────────────────────────────┤
│  UI State                                                       │
│  ├── View (header, viewport, status_bar)                        │
│  ├── camera_yaw, camera_pitch, camera_distance                  │
│  ├── selected_joint, animating, anim_timer                      │
│  └── is_dragging, last_mouse                                    │
├─────────────────────────────────────────────────────────────────┤
│  Robot Model (private structs)                                  │
│  ├── Robot { links, joints, link_transforms, ... }              │
│  ├── RobotLink { name, mesh_data }                              │
│  └── RobotJoint { name, parent, child, origin, axis, angle }    │
├─────────────────────────────────────────────────────────────────┤
│  Rendering State                                                │
│  ├── draw_mesh: DrawMesh (template)                             │
│  ├── link_drawers: Vec<DrawMesh> (one per link)                 │
│  └── original_meshes: Vec<MeshData> (untransformed)             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                          mesh.rs                                │
├─────────────────────────────────────────────────────────────────┤
│  MeshData (CPU-side mesh representation)                        │
│  ├── vertices: Vec<f32>  [pos(3), id(1), normal(3), uv(2)]     │
│  ├── indices: Vec<u32>                                          │
│  ├── bounds_min/max: [f32; 3]                                   │
│  ├── from_stl() - load STL file                                 │
│  ├── apply_transform() - CPU vertex transformation              │
│  ├── combine() - merge meshes                                   │
│  └── make_double_sided() - duplicate with flipped normals       │
├─────────────────────────────────────────────────────────────────┤
│  GeometryMesh3D (GPU geometry buffer)                           │
│  ├── geometry_ref: GeometryRef                                  │
│  ├── instance_id: u64 (for unique fingerprints)                 │
│  ├── upload_mesh_data() - send to GPU                           │
│  └── impl GeometryFields (geom_pos, geom_normal, geom_uv)       │
├─────────────────────────────────────────────────────────────────┤
│  DrawMesh (shader + draw state)                                 │
│  ├── geometry: GeometryMesh3D                                   │
│  ├── draw_vars: DrawVars                                        │
│  ├── color: Vec4                                                │
│  ├── new_for_link() - create instance for a robot link          │
│  ├── update_transformed_geometry() - CPU transform + re-upload  │
│  └── Shader: vertex() computes lighting, pixel() returns color  │
└─────────────────────────────────────────────────────────────────┘
```

### Data Flow (Per Frame)

```
1. Animation timer fires
   │
2. Update joint angles (Robot.joints[i].angle)
   │
3. Compute forward kinematics
   │  └── Robot.update_forward_kinematics()
   │      └── Outputs: link_transforms: Vec<glam::Mat4>
   │
4. For each link with mesh:
   │
   ├── 4a. Clone original mesh (CPU)
   │       └── ~50k-70k vertices × 9 floats = ~2MB per link
   │
   ├── 4b. Apply transform on CPU
   │       └── MeshData.apply_transform(&Mat4)
   │       └── Loop over all vertices, matrix multiply each
   │
   ├── 4c. Upload transformed mesh to GPU
   │       └── GeometryMesh3D.upload_mesh_data()
   │       └── Full buffer re-upload every frame
   │
   └── 4d. Draw
           └── DrawMesh.draw()
```

**Total per frame**: ~364k vertices × 9 floats × 4 bytes = **13MB** cloned, transformed, and uploaded.

---

## Critical Issue: No GPU-side Matrix Transform

### The Problem

Makepad's shader system doesn't support `Mat4` as instance data. When attempting:

```rust
#[derive(Live, LiveRegister)]
#[repr(C)]
pub struct DrawMesh {
    #[live] pub transform: Mat4,  // FAILS - Mat4 not supported as instance data
    // ...
}
```

The shader compiler fails to recognize `Mat4` as a valid instance attribute type.

### What is Mat4?

A 4×4 transformation matrix (16 floats) that encodes rotation, translation, and scale:

```
┌                           ┐
│  r00  r01  r02  tx       │     r = rotation (3×3)
│  r10  r11  r12  ty       │     t = translation (x, y, z)
│  r20  r21  r22  tz       │
│  0    0    0    1        │
└                           ┘
```

Transforming a vertex: `new_pos = Mat4 × old_pos`

### Use Case: Robot Arm Forward Kinematics

```
Base ──[joint1]──▶ Shoulder ──[joint2]──▶ Upper Arm ──[joint3]──▶ ...
  │                    │                      │
  Mat4₀                Mat4₁                  Mat4₂
  (identity)           (parent × joint)       (parent × joint)
```

Each link needs its own Mat4 computed from the kinematic chain. The GPU should apply this transform to all vertices of that link's mesh.

### Current Workaround (Inefficient)

```rust
// Every frame, for each link:
fn update_transformed_geometry(&mut self, cx: &mut Cx, original: &MeshData, transform: &Mat4) {
    let mut transformed = original.clone();      // Clone ~50k vertices
    transformed.apply_transform(transform);       // CPU matrix multiply each vertex
    self.geometry.upload_mesh_data(cx, transformed);  // Re-upload to GPU
}
```

### Desired Solution (Efficient)

```rust
// At startup: upload mesh once
geometry.upload_mesh_data(cx, mesh);

// Every frame: just set the matrix
drawer.transform = link_transform;  // 64 bytes
drawer.draw(cx);                    // GPU transforms vertices in parallel
```

### Questions for Makepad Team

1. **Is `Mat4` supported as instance/uniform data?** If not, is this planned?

2. **Would 4× Vec4 work as a workaround?**
   ```rust
   #[live] transform_col0: Vec4,
   #[live] transform_col1: Vec4,
   #[live] transform_col2: Vec4,
   #[live] transform_col3: Vec4,
   ```
   Reconstruct in shader:
   ```rust
   fn vertex(self) -> vec4 {
       let m = mat4(self.transform_col0, self.transform_col1,
                    self.transform_col2, self.transform_col3);
       return m * vec4(self.geom_pos, 1.0);
   }
   ```

3. **Is there a uniform-based approach?** Set matrix uniform before each draw call?

4. **What's the recommended pattern for instanced 3D rendering with per-instance transforms?**

---

## Other Issues

### 1. Monolithic Widget Design

Everything is in `URDFViewer`:
- UI layout
- Camera controls
- Robot data model
- Animation logic
- Rendering

**Impact**: Can't reuse robot rendering in other apps without copy-pasting.

### 2. Hardcoded Paths

```rust
let urdf_path = "data/so100.urdf";  // Line 548
let assets_dir = "data/assets";     // Line 549
```

**Impact**: Can't load different robots without code changes.

### 3. No Projection Matrix

```rust
// Current: hardcoded scale and depth
let scaled = pos * 4.0;
let depth = 0.5 - scaled.z * 0.1;
return vec4(scaled.x, scaled.y, depth, 1.0);
```

**Impact**: No proper perspective, can't integrate with other 3D content.

### 4. Initialization in draw_walk

Robot loading happens in `draw_walk()` on first frame:

```rust
fn draw_walk(&mut self, cx: &mut Cx2d, ...) {
    if !self.initialized {
        self.initialized = true;
        // Load URDF, STL files, create geometry...
    }
}
```

**Impact**: Blocks rendering on first frame, no async loading.

---

## Development Plan

### Priority Levels
- **P0**: Blocker - Must resolve before other work (needs external help)
- **P1**: High - Required for integration with other Makepad apps
- **P2**: Medium - Important improvements
- **P3**: Low - Nice to have / Future work

---

### P0: Blockers (Needs Makepad Team Input)

- [ ] **P0.1** Investigate Mat4 as shader instance data
  - Try `#[live] transform: Mat4` in DrawMesh
  - Document exact compiler error message
  - **Owner**: Need Rik's guidance

- [ ] **P0.2** Try Vec4×4 workaround for matrix passing
  ```rust
  #[live] transform_col0: Vec4,
  #[live] transform_col1: Vec4,
  #[live] transform_col2: Vec4,
  #[live] transform_col3: Vec4,
  ```
  - Test if shader can reconstruct mat4 from 4 vec4s
  - **Depends on**: P0.1 findings

- [ ] **P0.3** Try uniform-based approach (if instance data fails)
  - Set matrix uniform before each draw call
  - Loses instancing benefits but still avoids CPU transform
  - **Depends on**: P0.2 outcome

---

### P1: Integration Requirements (High Priority)

#### P1.1: Code Organization
- [ ] **P1.1.1** Extract `Robot`, `RobotLink`, `RobotJoint` to `src/robot.rs`
- [ ] **P1.1.2** Extract URDF/STL loading to `src/urdf_loader.rs`
- [ ] **P1.1.3** Move `MeshData` to `src/mesh_data.rs`
- [ ] **P1.1.4** Move `GeometryMesh3D`, `DrawMesh` to `src/draw_mesh.rs`
- [ ] **P1.1.5** Create `src/lib.rs` with public exports

#### P1.2: Reusable Robot Model
- [ ] **P1.2.1** Make `Robot` struct public
- [ ] **P1.2.2** Public API for joint control:
  ```rust
  robot.set_joint_angles(&[f32]);
  robot.get_joint_angles() -> Vec<f32>;
  robot.num_joints() -> usize;
  robot.joint_limits(idx) -> (f32, f32);
  ```
- [ ] **P1.2.3** Public FK API:
  ```rust
  robot.update_forward_kinematics();
  robot.link_transform(idx) -> glam::Mat4;
  ```

#### P1.3: Configurable Widget
- [ ] **P1.3.1** Add `#[live]` path properties:
  ```rust
  #[live] urdf_path: String,
  #[live] assets_dir: String,
  ```
- [ ] **P1.3.2** Add `#[live]` appearance properties:
  ```rust
  #[live] robot_color: Vec4,
  #[live] scale: f32,
  ```
- [ ] **P1.3.3** Method to load robot programmatically:
  ```rust
  fn load_robot(&mut self, cx: &mut Cx, urdf: &Path, assets: &Path);
  ```

#### P1.4: Embeddable Widget
- [ ] **P1.4.1** Create `RobotView` widget (separate from App)
- [ ] **P1.4.2** External joint control API:
  ```rust
  fn set_joint_angle(&mut self, idx: usize, angle: f32);
  ```
- [ ] **P1.4.3** External camera control API:
  ```rust
  fn set_camera(&mut self, yaw: f32, pitch: f32, distance: f32);
  ```
- [ ] **P1.4.4** Widget actions for state changes:
  ```rust
  enum RobotViewAction {
      JointChanged { index: usize, angle: f32 },
      CameraChanged { yaw: f32, pitch: f32 },
  }
  ```

---

### P2: Important Improvements (Medium Priority)

#### P2.1: GPU Performance (After P0 resolved)
- [ ] **P2.1.1** Implement GPU-side vertex transformation
- [ ] **P2.1.2** Upload mesh geometry only once at load time
- [ ] **P2.1.3** Remove CPU transform + re-upload code path
- [ ] **P2.1.4** Benchmark: target <5ms frame time (currently ~16ms)

#### P2.2: Proper 3D Camera
- [ ] **P2.2.1** Create `Camera3D` struct with position/target/fov
- [ ] **P2.2.2** Compute proper view matrix
- [ ] **P2.2.3** Compute proper projection matrix
- [ ] **P2.2.4** Handle aspect ratio from viewport size

#### P2.3: Better Shader
- [ ] **P2.3.1** Use proper MVP matrix pipeline:
  ```
  clip_pos = projection × view × model × vertex
  ```
- [ ] **P2.3.2** Proper depth buffer range [0, 1]
- [ ] **P2.3.3** Add specular lighting component
- [ ] **P2.3.4** Support per-link colors

---

### P3: Nice to Have (Low Priority)

#### P3.1: Loading & Error Handling
- [ ] **P3.1.1** Async URDF/mesh loading (don't block first frame)
- [ ] **P3.1.2** Progress indicator while loading
- [ ] **P3.1.3** User-visible error messages for missing files
- [ ] **P3.1.4** Fallback geometry for missing meshes

#### P3.2: Additional Features
- [ ] **P3.2.1** Support OBJ mesh format
- [ ] **P3.2.2** Support GLTF/GLB mesh format
- [ ] **P3.2.3** Support collision meshes (not just visual)
- [ ] **P3.2.4** Add grid/ground plane option
- [ ] **P3.2.5** Joint axis visualization
- [ ] **P3.2.6** Joint limit visualization
- [ ] **P3.2.7** Link frame axes visualization

#### P3.3: Documentation
- [ ] **P3.3.1** Integration example in README
- [ ] **P3.3.2** API documentation
- [ ] **P3.3.3** URDF format requirements doc

---

## Task Dependencies

```
P0.1 ──▶ P0.2 ──▶ P0.3 ──▶ P2.1 (GPU transforms)
                              │
P1.1 (code org) ─────────────┼──▶ P1.4 (embeddable widget)
         │                    │
         ▼                    │
P1.2 (robot model) ──────────┘
         │
         ▼
P1.3 (configurable) ──▶ P2.2 (camera) ──▶ P2.3 (shader)
```

---

## Immediate Next Steps

1. **Ask Rik about P0.1-P0.3** - GPU matrix transform support
2. **Start P1.1** - Code organization (can proceed in parallel)
3. **Start P1.2** - Robot model API (can proceed in parallel)

---

## File Structure After Refactoring

```
src/
├── lib.rs              # Public API exports
├── robot.rs            # Robot, RobotLink, RobotJoint
├── urdf_loader.rs      # URDF parsing, STL loading
├── mesh_data.rs        # MeshData (CPU mesh representation)
├── draw_mesh.rs        # GeometryMesh3D, DrawMesh (GPU rendering)
├── camera.rs           # Camera3D, view/projection matrices
├── robot_view.rs       # RobotView widget (embeddable)
└── main.rs             # Standalone app entry point
```

---

## Performance Targets

| Metric | Current | Target |
|--------|---------|--------|
| Frame time (7 links) | ~16ms | <5ms |
| GPU upload per frame | 13MB | 0 (one-time) |
| CPU transform time | ~8ms | 0 |
| Memory (mesh clones) | 13MB/frame | 0 |

---

## Related Files

- `src/main.rs` - Current monolithic implementation
- `src/mesh.rs` - Current mesh/shader code
- `DEVELOPMENT.md` - Development history and challenges
- `Cargo.toml` - Dependencies
