//! STL Mesh rendering for Makepad
//!
//! This module provides custom geometry and draw shader for rendering STL meshes.

use makepad_widgets::*;
use std::io::BufReader;
use std::fs::File;
use std::path::Path;

// Vertex data for a mesh: pos(3) + normal(3) + uv(2) = 8 floats per vertex
// But to match Makepad's cube pattern: pos(3) + id(1) + normal(3) + uv(2) = 9 floats
const FLOATS_PER_VERTEX: usize = 9;

/// Mesh data loaded from STL
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<f32>,  // Interleaved: pos(3), id(1), normal(3), uv(2)
    pub indices: Vec<u32>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

impl MeshData {
    /// Load mesh from STL file
    pub fn from_stl<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file = File::open(path.as_ref())
            .map_err(|e| format!("Failed to open STL file: {}", e))?;
        let mut reader = BufReader::new(file);

        let stl = stl_io::read_stl(&mut reader)
            .map_err(|e| format!("Failed to parse STL: {}", e))?;

        let mut mesh = MeshData::default();
        mesh.bounds_min = [f32::MAX; 3];
        mesh.bounds_max = [f32::MIN; 3];

        // stl is IndexedMesh with vertices and faces (IndexedTriangle)
        // IndexedTriangle has: normal: Normal, vertices: [usize; 3] (indices)
        for (tri_idx, face) in stl.faces.iter().enumerate() {
            let normal = face.normal;

            for (local_idx, &vert_idx) in face.vertices.iter().enumerate() {
                // Get actual vertex position from index
                let vertex = &stl.vertices[vert_idx];

                // Position - Vertex implements Index<usize>
                mesh.vertices.push(vertex[0]);
                mesh.vertices.push(vertex[1]);
                mesh.vertices.push(vertex[2]);

                // Update bounds
                for i in 0..3 {
                    mesh.bounds_min[i] = mesh.bounds_min[i].min(vertex[i]);
                    mesh.bounds_max[i] = mesh.bounds_max[i].max(vertex[i]);
                }

                // ID (face index for potential face coloring)
                mesh.vertices.push(tri_idx as f32);

                // Normal - Normal implements Index<usize>
                mesh.vertices.push(normal[0]);
                mesh.vertices.push(normal[1]);
                mesh.vertices.push(normal[2]);

                // UV (simple planar mapping)
                mesh.vertices.push(vertex[0]);
                mesh.vertices.push(vertex[1]);

                // Add index (sequential for now)
                mesh.indices.push((tri_idx * 3 + local_idx) as u32);
            }
        }

        Ok(mesh)
    }

    /// Create a ground plane mesh (grid pattern)
    pub fn ground_plane(size: f32, y_pos: f32) -> Self {
        let mut mesh = MeshData::default();
        let half = size / 2.0;
        let normal = [0.0, 1.0, 0.0];

        mesh.bounds_min = [-half, y_pos, -half];
        mesh.bounds_max = [half, y_pos, half];

        // Create grid cells for visual pattern
        let grid_size = 10;
        let cell_size = size / grid_size as f32;

        for i in 0..grid_size {
            for j in 0..grid_size {
                let x0 = -half + i as f32 * cell_size;
                let x1 = x0 + cell_size;
                let z0 = -half + j as f32 * cell_size;
                let z1 = z0 + cell_size;

                let face_idx = (i * grid_size + j) as f32;

                // Two triangles per cell
                let verts = [
                    [x0, y_pos, z0],
                    [x1, y_pos, z0],
                    [x1, y_pos, z1],
                    [x0, y_pos, z1],
                ];

                // Triangle 1
                for v in [&verts[0], &verts[1], &verts[2]] {
                    mesh.vertices.extend_from_slice(v);
                    mesh.vertices.push(face_idx);
                    mesh.vertices.extend_from_slice(&normal);
                    mesh.vertices.push(v[0] / size + 0.5);
                    mesh.vertices.push(v[2] / size + 0.5);
                }

                // Triangle 2
                for v in [&verts[0], &verts[2], &verts[3]] {
                    mesh.vertices.extend_from_slice(v);
                    mesh.vertices.push(face_idx);
                    mesh.vertices.extend_from_slice(&normal);
                    mesh.vertices.push(v[0] / size + 0.5);
                    mesh.vertices.push(v[2] / size + 0.5);
                }
            }
        }

        let num_vertices = mesh.vertices.len() / FLOATS_PER_VERTEX;
        mesh.indices = (0..num_vertices as u32).collect();

        mesh
    }

    /// Create a simple test cube mesh
    pub fn test_cube(size: f32) -> Self {
        let mut mesh = MeshData::default();
        let s = size / 2.0;

        // Define 6 faces, each with 2 triangles (6 vertices per face)
        let faces = [
            // Front face (z+)
            ([ s, -s,  s], [ s,  s,  s], [-s,  s,  s], [-s, -s,  s], [0.0, 0.0, 1.0]),
            // Back face (z-)
            ([-s, -s, -s], [-s,  s, -s], [ s,  s, -s], [ s, -s, -s], [0.0, 0.0, -1.0]),
            // Top face (y+)
            ([-s,  s, -s], [-s,  s,  s], [ s,  s,  s], [ s,  s, -s], [0.0, 1.0, 0.0]),
            // Bottom face (y-)
            ([-s, -s,  s], [-s, -s, -s], [ s, -s, -s], [ s, -s,  s], [0.0, -1.0, 0.0]),
            // Right face (x+)
            ([ s, -s,  s], [ s, -s, -s], [ s,  s, -s], [ s,  s,  s], [1.0, 0.0, 0.0]),
            // Left face (x-)
            ([-s, -s, -s], [-s, -s,  s], [-s,  s,  s], [-s,  s, -s], [-1.0, 0.0, 0.0]),
        ];

        mesh.bounds_min = [-s, -s, -s];
        mesh.bounds_max = [s, s, s];

        for (face_idx, (v0, v1, v2, v3, normal)) in faces.iter().enumerate() {
            // Triangle 1: v0, v1, v2
            for v in [v0, v1, v2] {
                mesh.vertices.extend_from_slice(v);
                mesh.vertices.push(face_idx as f32);
                mesh.vertices.extend_from_slice(normal);
                mesh.vertices.push(v[0]); // UV
                mesh.vertices.push(v[1]);
            }

            // Triangle 2: v0, v2, v3
            for v in [v0, v2, v3] {
                mesh.vertices.extend_from_slice(v);
                mesh.vertices.push(face_idx as f32);
                mesh.vertices.extend_from_slice(normal);
                mesh.vertices.push(v[0]);
                mesh.vertices.push(v[1]);
            }
        }

        // Create indices (0, 1, 2, 3, 4, 5, ...)
        let num_vertices = mesh.vertices.len() / FLOATS_PER_VERTEX;
        mesh.indices = (0..num_vertices as u32).collect();

        mesh
    }

    /// Combine multiple meshes into one
    pub fn combine(meshes: Vec<MeshData>) -> Self {
        let mut combined = MeshData::default();
        combined.bounds_min = [f32::MAX; 3];
        combined.bounds_max = [f32::MIN; 3];

        let mut vertex_offset = 0u32;

        for mesh in meshes {
            // Update bounds
            for i in 0..3 {
                combined.bounds_min[i] = combined.bounds_min[i].min(mesh.bounds_min[i]);
                combined.bounds_max[i] = combined.bounds_max[i].max(mesh.bounds_max[i]);
            }

            // Add vertices
            combined.vertices.extend_from_slice(&mesh.vertices);

            // Add indices with offset
            for idx in mesh.indices {
                combined.indices.push(idx + vertex_offset);
            }

            vertex_offset += (mesh.vertices.len() / FLOATS_PER_VERTEX) as u32;
        }

        combined
    }

    /// Load all robot STL files from a directory
    pub fn load_robot_meshes(assets_dir: &str) -> Result<Self, String> {
        let stl_files = [
            "Base.stl",
            "Base_Motor.stl",
            "Rotation_Pitch.stl",
            "Rotation_Pitch_Motor.stl",
            "Upper_Arm.stl",
            "Upper_Arm_Motor.stl",
            "Lower_Arm.stl",
            "Lower_Arm_Motor.stl",
            "Wrist_Pitch_Roll.stl",
            "Wrist_Pitch_Roll_Motor.stl",
            "Fixed_Jaw.stl",
            "Fixed_Jaw_Motor.stl",
            "Moving_Jaw.stl",
        ];

        let mut meshes = Vec::new();
        for file in &stl_files {
            let path = format!("{}/{}", assets_dir, file);
            match MeshData::from_stl(&path) {
                Ok(mesh) => {
                    meshes.push(mesh);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load {}: {}", file, e);
                }
            }
        }

        if meshes.is_empty() {
            return Err("No meshes loaded".to_string());
        }

        let mut combined = MeshData::combine(meshes);
        combined.normalize();
        combined.make_double_sided();
        Ok(combined)
    }

    /// Make mesh double-sided by duplicating triangles with reversed winding
    pub fn make_double_sided(&mut self) {
        let original_vertex_count = self.vertices.len() / FLOATS_PER_VERTEX;
        let original_index_count = self.indices.len();

        // Duplicate vertices with flipped normals
        let mut new_vertices = self.vertices.clone();
        for i in 0..original_vertex_count {
            let base = i * FLOATS_PER_VERTEX;
            // Copy position
            new_vertices.push(self.vertices[base + 0]);
            new_vertices.push(self.vertices[base + 1]);
            new_vertices.push(self.vertices[base + 2]);
            // Copy id
            new_vertices.push(self.vertices[base + 3]);
            // Flip normal
            new_vertices.push(-self.vertices[base + 4]);
            new_vertices.push(-self.vertices[base + 5]);
            new_vertices.push(-self.vertices[base + 6]);
            // Copy uv
            new_vertices.push(self.vertices[base + 7]);
            new_vertices.push(self.vertices[base + 8]);
        }
        self.vertices = new_vertices;

        // Duplicate indices with reversed winding order
        for i in 0..(original_index_count / 3) {
            let base = i * 3;
            // Reverse winding: 0,1,2 -> 0,2,1
            self.indices.push(self.indices[base] + original_vertex_count as u32);
            self.indices.push(self.indices[base + 2] + original_vertex_count as u32);
            self.indices.push(self.indices[base + 1] + original_vertex_count as u32);
        }
    }

    /// Apply uniform scale to all vertices
    pub fn apply_scale(&mut self, scale: f32) {
        let num_vertices = self.vertices.len() / FLOATS_PER_VERTEX;
        for i in 0..num_vertices {
            let base = i * FLOATS_PER_VERTEX;
            // Scale position only (normals stay the same)
            self.vertices[base + 0] *= scale;
            self.vertices[base + 1] *= scale;
            self.vertices[base + 2] *= scale;
        }
        // Update bounds
        for i in 0..3 {
            self.bounds_min[i] *= scale;
            self.bounds_max[i] *= scale;
        }
    }

    /// Apply a 4x4 transform matrix to all vertices
    pub fn apply_transform(&mut self, m: &Mat4) {
        let num_vertices = self.vertices.len() / FLOATS_PER_VERTEX;

        // Reset bounds
        self.bounds_min = [f32::MAX; 3];
        self.bounds_max = [f32::MIN; 3];

        for i in 0..num_vertices {
            let base = i * FLOATS_PER_VERTEX;

            // Transform position (indices 0, 1, 2)
            let px = self.vertices[base + 0];
            let py = self.vertices[base + 1];
            let pz = self.vertices[base + 2];

            // Matrix multiply: m * [px, py, pz, 1]
            // Makepad Mat4 is column-major: v[0..4] = col0, v[4..8] = col1, etc.
            let new_x = m.v[0] * px + m.v[4] * py + m.v[8] * pz + m.v[12];
            let new_y = m.v[1] * px + m.v[5] * py + m.v[9] * pz + m.v[13];
            let new_z = m.v[2] * px + m.v[6] * py + m.v[10] * pz + m.v[14];

            self.vertices[base + 0] = new_x;
            self.vertices[base + 1] = new_y;
            self.vertices[base + 2] = new_z;

            // Update bounds
            self.bounds_min[0] = self.bounds_min[0].min(new_x);
            self.bounds_min[1] = self.bounds_min[1].min(new_y);
            self.bounds_min[2] = self.bounds_min[2].min(new_z);
            self.bounds_max[0] = self.bounds_max[0].max(new_x);
            self.bounds_max[1] = self.bounds_max[1].max(new_y);
            self.bounds_max[2] = self.bounds_max[2].max(new_z);

            // Transform normal (indices 4, 5, 6) - direction only (w=0)
            let nx = self.vertices[base + 4];
            let ny = self.vertices[base + 5];
            let nz = self.vertices[base + 6];

            let new_nx = m.v[0] * nx + m.v[4] * ny + m.v[8] * nz;
            let new_ny = m.v[1] * nx + m.v[5] * ny + m.v[9] * nz;
            let new_nz = m.v[2] * nx + m.v[6] * ny + m.v[10] * nz;

            // Normalize
            let len = (new_nx * new_nx + new_ny * new_ny + new_nz * new_nz).sqrt();
            if len > 0.0001 {
                self.vertices[base + 4] = new_nx / len;
                self.vertices[base + 5] = new_ny / len;
                self.vertices[base + 6] = new_nz / len;
            }
        }
    }

    /// Center the mesh at origin and scale to fit in unit cube
    pub fn normalize(&mut self) {
        let center = [
            (self.bounds_min[0] + self.bounds_max[0]) / 2.0,
            (self.bounds_min[1] + self.bounds_max[1]) / 2.0,
            (self.bounds_min[2] + self.bounds_max[2]) / 2.0,
        ];

        let extent = [
            self.bounds_max[0] - self.bounds_min[0],
            self.bounds_max[1] - self.bounds_min[1],
            self.bounds_max[2] - self.bounds_min[2],
        ];

        let max_extent = extent[0].max(extent[1]).max(extent[2]).max(0.001);
        let scale = 1.0 / max_extent;

        // Transform all vertices
        let num_vertices = self.vertices.len() / FLOATS_PER_VERTEX;
        for i in 0..num_vertices {
            let base = i * FLOATS_PER_VERTEX;
            // Transform position
            self.vertices[base + 0] = (self.vertices[base + 0] - center[0]) * scale;
            self.vertices[base + 1] = (self.vertices[base + 1] - center[1]) * scale;
            self.vertices[base + 2] = (self.vertices[base + 2] - center[2]) * scale;
        }

        // Update bounds
        self.bounds_min = [-0.5, -0.5, -0.5];
        self.bounds_max = [0.5, 0.5, 0.5];
    }
}

live_design! {
    use link::shaders::*;

    pub GeometryMesh3D = {{GeometryMesh3D}} {}

    pub DrawGrid = {{DrawGrid}} {
        geometry: <GeometryMesh3D> {}

        varying line_color: vec4;
        varying world_pos: vec3;

        fn vertex(self) -> vec4 {
            let pos = self.geom_pos;

            // Pass world position for grid pattern calculation in fragment shader
            self.world_pos = pos;

            // Scale grid to view
            let scaled = pos * 4.0;

            // Depth calculation matching DrawMesh
            let raw_depth = (pos.z + 2.0) * 0.2;
            let depth = clamp(raw_depth, 0.1, 0.9);

            // Color from uniform
            self.line_color = self.color;

            return vec4(scaled.x, scaled.y, depth, 1.0);
        }

        fn pixel(self) -> vec4 {
            // Light green background for earth
            let bg_color = vec4(0.7, 0.9, 0.7, 1.0);

            // After rotation, grid is in XY plane, so use x and y for grid lines
            let x_norm = self.world_pos.x / self.grid_spacing + 0.5;
            let y_norm = self.world_pos.y / self.grid_spacing + 0.5;
            let x_frac = x_norm - floor(x_norm);
            let y_frac = y_norm - floor(y_norm);
            let x_dist = abs(x_frac - 0.5) * self.grid_spacing;
            let y_dist = abs(y_frac - 0.5) * self.grid_spacing;

            // X axis (red) - runs along X, so y near 0
            if abs(self.world_pos.y) < self.line_width * 5.0 {
                return self.x_axis_color;
            }

            // Y axis (blue) - runs along Y, so x near 0
            if abs(self.world_pos.x) < self.line_width * 5.0 {
                return self.z_axis_color;
            }

            // Regular grid lines - darker green
            if x_dist < self.line_width || y_dist < self.line_width {
                return self.line_color;
            }

            // Light green background
            return bg_color;
        }

        fn fragment(self) -> vec4 {
            return self.pixel();
        }
    }

    pub DrawMesh = {{DrawMesh}} {
        geometry: <GeometryMesh3D> {}

        varying lit_color: vec4;
        varying world_pos: vec3;
        varying uv: vec2;

        fn vertex(self) -> vec4 {
            let pos = self.geom_pos;
            let normal = self.geom_normal;

            // Pass world position and UV for grid calculation
            self.world_pos = pos;
            self.uv = self.geom_uv;

            // Simple diffuse lighting
            let light_dir = normalize(vec3(0.3, 0.8, 0.5));
            let n = normalize(normal);
            let diff = max(0.0, dot(n, light_dir));
            let ambient = 0.4;
            let brightness = ambient + diff * 0.6;

            // Mix between top color and bottom color based on normal Y direction
            // normal.y < 0 means facing down (bottom)
            let bottom_blend = max(0.0, -n.y);  // 1.0 when facing straight down, 0.0 when facing up
            let base_color = mix(self.color.xyz, self.bottom_color.xyz, bottom_blend);

            self.lit_color = vec4(base_color * brightness, 1.0);

            // Scale to fill view
            let scaled = pos * 4.0;

            // Use compressed depth range to avoid near/far clipping (black fog)
            // Map depth to 0.1-0.9 range, clamped to stay safe
            let raw_depth = (pos.z + 2.0) * 0.2;  // Compress range
            let depth = clamp(raw_depth, 0.1, 0.9);

            return vec4(scaled.x, scaled.y, depth, 1.0);
        }

        fn pixel(self) -> vec4 {
            // Check if this is a grid plane (draw_grid_lines > 0)
            if self.draw_grid_lines > 0.5 {
                // Light yellow earth color (direct, not affected by lighting)
                let earth_color = vec4(0.96, 0.96, 0.88, 1.0);

                // Use UV coordinates for grid (preserves original world space for perspective)
                // UV is 0-1 across the grid, centered at 0.5
                let grid_lines = 10.0;  // Sparse grid - 10 lines across
                let u = self.uv.x - 0.5;  // Center at 0
                let v = self.uv.y - 0.5;  // Center at 0

                let u_scaled = u * grid_lines;
                let v_scaled = v * grid_lines;

                let u_frac = abs(u_scaled - floor(u_scaled + 0.5));
                let v_frac = abs(v_scaled - floor(v_scaled + 0.5));

                // Grid lines only (no separate axis lines) - very thin
                if u_frac < 0.0024 || v_frac < 0.0024 {
                    return vec4(0.82, 0.84, 0.72, 1.0);  // Slightly darker for grid lines
                }
                // Earth background - light yellow
                return earth_color;
            }
            return self.lit_color;
        }

        fn fragment(self) -> vec4 {
            return self.pixel();
        }
    }
}

/// Counter for unique geometry IDs
static GEOMETRY_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Custom geometry for STL meshes
#[derive(Live, LiveRegister, Clone)]
pub struct GeometryMesh3D {
    #[rust] pub geometry_ref: Option<GeometryRef>,
    #[rust] pub mesh_data: Option<MeshData>,
    /// Unique ID for this geometry instance
    #[rust] pub instance_id: u64,
}

impl GeometryMesh3D {
    /// Create a new empty instance (for cloning purposes)
    pub fn new_empty() -> Self {
        let id = GEOMETRY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self {
            geometry_ref: None,
            mesh_data: None,
            instance_id: id,
        }
    }
}

impl LiveHook for GeometryMesh3D {
    fn after_apply(&mut self, cx: &mut Cx, _apply: &mut Apply, _index: usize, _nodes: &[LiveNode]) {
        // Initialize with a default cube geometry for shader compilation
        // This geometry will be replaced when load_stl or load_test_cube is called
        if self.geometry_ref.is_none() {
            let mesh = MeshData::test_cube(1.0);

            // Use instance_id in fingerprint for unique geometry per instance
            let mut fp = GeometryFingerprint::new(LiveType::of::<Self>());
            fp.push(self.instance_id as f32);

            self.geometry_ref = Some(cx.get_geometry_ref(fp));

            if let Some(ref gr) = self.geometry_ref {
                gr.0.update(cx, mesh.indices.clone(), mesh.vertices.clone());
            }

            self.mesh_data = Some(mesh);
        }
    }
}

impl GeometryFields for GeometryMesh3D {
    fn geometry_fields(&self, fields: &mut Vec<GeometryField>) {
        fields.push(GeometryField { id: live_id!(geom_pos), ty: ShaderTy::Vec3 });
        fields.push(GeometryField { id: live_id!(geom_id), ty: ShaderTy::Float });
        fields.push(GeometryField { id: live_id!(geom_normal), ty: ShaderTy::Vec3 });
        fields.push(GeometryField { id: live_id!(geom_uv), ty: ShaderTy::Vec2 });
    }

    fn get_geometry_id(&self) -> Option<GeometryId> {
        self.geometry_ref.as_ref().map(|gr| gr.0.geometry_id())
    }

    fn live_type_check(&self) -> LiveType {
        LiveType::of::<Self>()
    }
}

impl Default for GeometryMesh3D {
    fn default() -> Self {
        let id = GEOMETRY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self {
            geometry_ref: None,
            mesh_data: None,
            instance_id: id,
        }
    }
}

impl GeometryMesh3D {
    /// Load mesh from STL file
    pub fn load_stl(&mut self, cx: &mut Cx, path: &str) -> Result<(), String> {
        let mut mesh = MeshData::from_stl(path)?;
        mesh.normalize();
        self.upload_mesh(cx, mesh);
        Ok(())
    }

    /// Load all robot meshes from assets directory
    pub fn load_robot(&mut self, cx: &mut Cx, assets_dir: &str) -> Result<(), String> {
        let mesh = MeshData::load_robot_meshes(assets_dir)?;
        self.upload_mesh(cx, mesh);
        Ok(())
    }

    /// Load test cube
    pub fn load_test_cube(&mut self, cx: &mut Cx, size: f32) {
        let mesh = MeshData::test_cube(size);
        self.upload_mesh(cx, mesh);
    }

    /// Upload pre-built mesh data to GPU (for use by Robot loader)
    pub fn upload_mesh_data(&mut self, cx: &mut Cx, mesh: MeshData) {
        self.upload_mesh(cx, mesh);
    }

    /// Upload mesh data to GPU
    /// Uses unique fingerprint per instance to allow multiple geometries
    fn upload_mesh(&mut self, cx: &mut Cx, mesh: MeshData) {

        // If we don't have a geometry reference yet, create one with unique fingerprint
        if self.geometry_ref.is_none() {
            let mut fp = GeometryFingerprint::new(LiveType::of::<Self>());
            // Use instance_id to create unique fingerprint for each geometry
            fp.push(self.instance_id as f32);
            self.geometry_ref = Some(cx.get_geometry_ref(fp));
        }

        // Upload new geometry data to the existing geometry buffer
        if let Some(ref gr) = self.geometry_ref {
            gr.0.update(cx, mesh.indices.clone(), mesh.vertices.clone());
        }

        self.mesh_data = Some(mesh);
    }
}

/// Draw shader for mesh rendering
#[derive(Live, LiveRegister)]
#[repr(C)]
pub struct DrawMesh {
    #[rust] pub many_instances: Option<ManyInstances>,
    #[live] pub geometry: GeometryMesh3D,
    #[deref] pub draw_vars: DrawVars,
    #[live] pub color: Vec4,
    #[live] pub bottom_color: Vec4,  // Color for bottom-facing surfaces
    #[live(vec3(0.0, 0.0, 0.0))] pub mesh_pos: Vec3,
    #[live(vec3(1.0, 1.0, 1.0))] pub mesh_scale: Vec3,
    #[live(1.0)] pub depth_clip: f32,
    #[live(0.0)] pub draw_grid_lines: f32,  // Set to 1.0 to draw grid lines
}

impl LiveHook for DrawMesh {
    fn before_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.before_apply_init_shader(cx, apply, index, nodes, &self.geometry);
    }

    fn after_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.after_apply_update_self(cx, apply, index, nodes, &self.geometry);
    }
}

/// Draw shader for grid rendering
#[derive(Live, LiveRegister)]
#[repr(C)]
pub struct DrawGrid {
    #[rust] pub many_instances: Option<ManyInstances>,
    #[live] pub geometry: GeometryMesh3D,
    #[deref] pub draw_vars: DrawVars,
    #[live] pub color: Vec4,
    #[live] pub x_axis_color: Vec4,
    #[live] pub z_axis_color: Vec4,
    #[live(0.05)] pub grid_spacing: f32,
    #[live(0.002)] pub line_width: f32,
}

impl LiveHook for DrawGrid {
    fn before_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.before_apply_init_shader(cx, apply, index, nodes, &self.geometry);
    }

    fn after_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.after_apply_update_self(cx, apply, index, nodes, &self.geometry);
    }
}

impl DrawGrid {
    /// Create a ground plane grid at specified Y position
    pub fn create_ground_plane(&mut self, cx: &mut Cx, size: f32, y_pos: f32) {
        let mesh = MeshData::ground_plane(size, y_pos);
        self.geometry.upload_mesh_data(cx, mesh);
        // Re-bind draw_vars to geometry after upload
        self.draw_vars.after_apply_update_self(
            cx,
            &mut Apply::from(ApplyFrom::UpdateFromDoc { file_id: Default::default() }),
            0,
            &[],
            &self.geometry
        );
    }

    /// Update geometry with transformed vertices
    pub fn update_transformed_geometry(&mut self, cx: &mut Cx, original_mesh: &MeshData, transform: &Mat4) {
        let mut transformed = original_mesh.clone();
        transformed.apply_transform(transform);
        self.geometry.upload_mesh_data(cx, transformed);
        self.draw_vars.after_apply_update_self(
            cx,
            &mut Apply::from(ApplyFrom::UpdateFromDoc { file_id: Default::default() }),
            0,
            &[],
            &self.geometry
        );
    }

    pub fn draw(&mut self, cx: &mut Cx2d) {
        if let Some(mi) = &mut self.many_instances {
            mi.instances.extend_from_slice(self.draw_vars.as_slice());
        } else if self.draw_vars.can_instance() {
            let new_area = cx.add_instance(&self.draw_vars);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }

    pub fn begin_many_instances(&mut self, cx: &mut Cx2d) {
        self.many_instances = cx.begin_many_instances(&self.draw_vars);
    }

    pub fn end_many_instances(&mut self, cx: &mut Cx2d) {
        if let Some(mi) = self.many_instances.take() {
            let new_area = cx.end_many_instances(mi);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }
}

impl DrawMesh {
    /// Create a new DrawMesh instance with separate geometry for a robot link
    /// Uses template's draw_vars (including shader) but with new geometry
    pub fn new_for_link(_cx: &mut Cx, mesh_data: MeshData, template: &DrawMesh) -> Self {
        let mut geom = GeometryMesh3D::new_empty();
        // Don't upload yet - will be done when init is called
        geom.mesh_data = Some(mesh_data);

        // Copy draw_vars from template (including shader)
        let draw_vars = template.draw_vars.clone();

        DrawMesh {
            many_instances: None,
            geometry: geom,
            draw_vars,
            color: vec4(1.0, 0.65, 0.1, 1.0), // default orange
            bottom_color: vec4(0.2, 0.2, 0.25, 1.0), // default dark gray for bottom
            mesh_pos: vec3(0.0, 0.0, 0.0),
            mesh_scale: vec3(1.0, 1.0, 1.0),
            depth_clip: 1.0,
            draw_grid_lines: 0.0,
        }
    }

    /// Update geometry with transformed vertices
    pub fn update_transformed_geometry(&mut self, cx: &mut Cx, original_mesh: &MeshData, transform: &Mat4) {
        let mut transformed = original_mesh.clone();
        transformed.apply_transform(transform);
        self.geometry.upload_mesh_data(cx, transformed);
        // Re-bind draw_vars to geometry after upload
        self.draw_vars.after_apply_update_self(
            cx,
            &mut Apply::from(ApplyFrom::UpdateFromDoc { file_id: Default::default() }),
            0,
            &[],
            &self.geometry
        );
    }

    /// Initialize the geometry and shader binding (call once after creation)
    pub fn init_link_geometry(&mut self, cx: &mut Cx) {
        if let Some(mesh_data) = self.geometry.mesh_data.take() {
            self.geometry.upload_mesh_data(cx, mesh_data);
        }
        // Update draw_vars to use our geometry via the LiveHook pattern
        self.draw_vars.after_apply_update_self(
            cx,
            &mut Apply::from(ApplyFrom::UpdateFromDoc { file_id: Default::default() }),
            0,
            &[],
            &self.geometry
        );
    }

    pub fn draw(&mut self, cx: &mut Cx2d) {
        if let Some(mi) = &mut self.many_instances {
            mi.instances.extend_from_slice(self.draw_vars.as_slice());
        } else if self.draw_vars.can_instance() {
            let new_area = cx.add_instance(&self.draw_vars);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }

    pub fn new_draw_call(&self, cx: &mut Cx2d) {
        cx.new_draw_call(&self.draw_vars);
    }

    pub fn begin_many_instances(&mut self, cx: &mut Cx2d) {
        self.many_instances = cx.begin_many_instances(&self.draw_vars);
    }

    pub fn end_many_instances(&mut self, cx: &mut Cx2d) {
        if let Some(mi) = self.many_instances.take() {
            let new_area = cx.end_many_instances(mi);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }
}
