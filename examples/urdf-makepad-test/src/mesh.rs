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
        let mut total_vertices = 0;

        for file in &stl_files {
            let path = format!("{}/{}", assets_dir, file);
            match MeshData::from_stl(&path) {
                Ok(mesh) => {
                    total_vertices += mesh.vertices.len() / FLOATS_PER_VERTEX;
                    eprintln!("Loaded {}: {} vertices", file, mesh.vertices.len() / FLOATS_PER_VERTEX);
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

        eprintln!("Total: {} meshes, {} vertices", meshes.len(), total_vertices);
        let mut combined = MeshData::combine(meshes);
        combined.normalize();
        combined.make_double_sided();
        eprintln!("After double-sided: {} vertices", combined.vertices.len() / FLOATS_PER_VERTEX);
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

    pub DrawMesh = {{DrawMesh}} {
        geometry: <GeometryMesh3D> {}

        varying lit_color: vec4;
        varying world: vec4;

        fn vertex(self) -> vec4 {
            // Apply rotation transform
            let rotated = self.transform * vec4(self.geom_pos, 1.0);

            // Lighting from multiple directions
            let normal_rotated = (self.transform * vec4(self.geom_normal, 0.0)).xyz;
            let normal = normalize(normal_rotated);

            // Six lights from all directions
            let dp1 = abs(dot(normal, normalize(vec3(1.0, 1.0, 1.0))));
            let dp2 = abs(dot(normal, normalize(vec3(-1.0, 1.0, 1.0))));
            let dp3 = abs(dot(normal, normalize(vec3(0.0, -1.0, 1.0))));
            let dp4 = abs(dot(normal, normalize(vec3(0.0, 1.0, -1.0))));
            let dp5 = abs(dot(normal, normalize(vec3(1.0, 0.0, -1.0))));
            let dp6 = abs(dot(normal, normalize(vec3(-1.0, 0.0, -1.0))));
            let dp = max(max(max(dp1, dp2), max(dp3, dp4)), max(dp5, dp6));

            // High ambient + diffuse
            let ambient = vec3(0.4, 0.4, 0.4);
            let diffuse = self.color.xyz * dp * 0.6;
            self.lit_color = vec4(diffuse + ambient, 1.0);
            self.world = vec4(0.0, 0.0, 0.0, 1.0);

            // Scale and output with Z=0.0 (the working value)
            let scaled = rotated.xyz * 0.8;
            return vec4(scaled.x, scaled.y, 0.0, 1.0);
        }

        fn get_color(self, dp: float) -> vec4 {
            let ambient = vec3(0.2, 0.2, 0.2);
            let color = self.color.xyz * dp * self.color.w + ambient;
            return vec4(color, self.color.w);
        }

        fn pixel(self) -> vec4 {
            return self.lit_color;
        }

        fn fragment(self) -> vec4 {
            // Just return the lit color directly, no depth clipping effects
            return self.lit_color;
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
            eprintln!("GeometryMesh3D::after_apply[{}] - initializing default geometry", self.instance_id);
            let mesh = MeshData::test_cube(1.0);

            // Use instance_id in fingerprint for unique geometry per instance
            let mut fp = GeometryFingerprint::new(LiveType::of::<Self>());
            fp.push(self.instance_id as f32);

            self.geometry_ref = Some(cx.get_geometry_ref(fp));

            if let Some(ref gr) = self.geometry_ref {
                gr.0.update(cx, mesh.indices.clone(), mesh.vertices.clone());
                eprintln!("GeometryMesh3D::after_apply - geometry id: {:?}", gr.0.geometry_id());
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
        eprintln!("upload_mesh[{}]: {} vertices, {} indices",
            self.instance_id,
            mesh.vertices.len() / 9,
            mesh.indices.len()
        );
        if mesh.vertices.len() >= 9 {
            eprintln!("  first vertex pos: ({}, {}, {})",
                mesh.vertices[0], mesh.vertices[1], mesh.vertices[2]);
        }

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
            eprintln!("  geometry uploaded, id: {:?}", gr.0.geometry_id());
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
    #[calc] pub transform: Mat4,
    #[live(vec3(0.0, 0.0, 0.0))] pub mesh_pos: Vec3,
    #[live(vec3(1.0, 1.0, 1.0))] pub mesh_scale: Vec3,
    #[live(1.0)] pub depth_clip: f32,
}

impl LiveHook for DrawMesh {
    fn before_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        eprintln!("DrawMesh::before_apply - geom_id: {:?}", self.geometry.geometry_ref.as_ref().map(|gr| gr.0.geometry_id()));
        // Match DrawCube pattern: init shader in before_apply
        self.draw_vars.before_apply_init_shader(cx, apply, index, nodes, &self.geometry);
    }

    fn after_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        eprintln!("DrawMesh::after_apply - geom_id: {:?}", self.geometry.geometry_ref.as_ref().map(|gr| gr.0.geometry_id()));
        self.draw_vars.after_apply_update_self(cx, apply, index, nodes, &self.geometry);
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
            transform: Mat4::identity(),
            mesh_pos: vec3(0.0, 0.0, 0.0),
            mesh_scale: vec3(1.0, 1.0, 1.0),
            depth_clip: 1.0,
        }
    }

    /// Initialize the geometry and shader binding (call once after creation)
    pub fn init_link_geometry(&mut self, cx: &mut Cx) {
        if let Some(mesh_data) = self.geometry.mesh_data.take() {
            self.geometry.upload_mesh_data(cx, mesh_data);
        }
        // Update draw_vars to use our geometry via the LiveHook pattern
        // Call after_apply to update geometry_id
        self.draw_vars.after_apply_update_self(
            cx,
            &mut Apply::from(ApplyFrom::UpdateFromDoc { file_id: Default::default() }),
            0,
            &[],
            &self.geometry
        );
        eprintln!("init_link_geometry: can_instance={}", self.draw_vars.can_instance());
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
