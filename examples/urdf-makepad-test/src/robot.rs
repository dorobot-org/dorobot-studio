//! URDF Robot loader with forward kinematics
//!
//! Parses URDF files and computes joint transforms for articulated robots.

use makepad_widgets::*;
use std::collections::HashMap;
use std::path::Path;

use crate::mesh::MeshData;

/// Helper to multiply two Mat4 matrices
fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    Mat4::mul(a, b)
}

/// A single link in the robot with its mesh and transform
#[derive(Clone, Debug)]
pub struct RobotLink {
    pub name: String,
    pub mesh_data: Option<MeshData>,
    /// Local transform from URDF visual origin (applied before joint transform)
    pub visual_origin: Mat4,
    /// Global transform computed by forward kinematics
    pub global_transform: Mat4,
}

/// A joint connecting two links
#[derive(Clone, Debug)]
pub struct RobotJoint {
    pub name: String,
    pub parent_link: String,
    pub child_link: String,
    /// Transform from parent to child origin (fixed part)
    pub origin_transform: Mat4,
    /// Joint axis (for revolute joints)
    pub axis: Vec3,
    /// Current joint angle
    pub angle: f32,
    /// Joint limits
    pub limit_lower: f32,
    pub limit_upper: f32,
}

/// Parsed robot from URDF
#[derive(Clone, Debug)]
pub struct Robot {
    pub name: String,
    pub links: Vec<RobotLink>,
    pub joints: Vec<RobotJoint>,
    /// Map from link name to index
    pub link_map: HashMap<String, usize>,
    /// Map from joint name to index
    pub joint_map: HashMap<String, usize>,
    /// Root link (no parent joint)
    pub root_link: String,
    /// Bounding box for normalization
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    /// Root offset transform (for centering the robot in view)
    pub root_offset: Mat4,
}

impl Robot {
    /// Load robot from URDF file
    pub fn from_urdf<P: AsRef<Path>>(urdf_path: P, assets_base: &str) -> Result<Self, String> {
        let urdf_content = std::fs::read_to_string(urdf_path.as_ref())
            .map_err(|e| format!("Failed to read URDF: {}", e))?;

        let robot_urdf = urdf_rs::read_from_string(&urdf_content)
            .map_err(|e| format!("Failed to parse URDF: {}", e))?;

        let mut robot = Robot {
            name: robot_urdf.name.clone(),
            links: Vec::new(),
            joints: Vec::new(),
            link_map: HashMap::new(),
            joint_map: HashMap::new(),
            root_link: String::new(),
            bounds_min: [f32::MAX; 3],
            bounds_max: [f32::MIN; 3],
            root_offset: Mat4::identity(),
        };

        // Parse links
        for (idx, link) in robot_urdf.links.iter().enumerate() {
            let mut robot_link = RobotLink {
                name: link.name.clone(),
                mesh_data: None,
                visual_origin: Mat4::identity(),
                global_transform: Mat4::identity(),
            };

            // Load meshes from visual elements
            let mut link_meshes = Vec::new();
            for visual in &link.visual {
                if let urdf_rs::Geometry::Mesh { filename, .. } = &visual.geometry {
                    // Extract just the filename from the path
                    let mesh_filename = Path::new(filename)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(filename);

                    let mesh_path = format!("{}/{}", assets_base, mesh_filename);
                    eprintln!("Loading mesh for {}: {}", link.name, mesh_path);

                    match MeshData::from_stl(&mesh_path) {
                        Ok(mesh) => {
                            // Don't transform vertices here - apply transforms at render time
                            // Update bounds
                            for i in 0..3 {
                                robot.bounds_min[i] = robot.bounds_min[i].min(mesh.bounds_min[i]);
                                robot.bounds_max[i] = robot.bounds_max[i].max(mesh.bounds_max[i]);
                            }
                            link_meshes.push(mesh);
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to load {}: {}", mesh_path, e);
                        }
                    }
                }
            }

            // Combine all meshes for this link
            if !link_meshes.is_empty() {
                let mut combined = MeshData::combine(link_meshes);
                combined.make_double_sided();
                robot_link.mesh_data = Some(combined);
            }

            robot.link_map.insert(link.name.clone(), idx);
            robot.links.push(robot_link);
        }

        // Parse joints
        let mut child_links: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (idx, joint) in robot_urdf.joints.iter().enumerate() {
            let origin_transform = pose_to_mat4(&joint.origin);

            // Normalize axis (required for Rodrigues' formula)
            let ax = joint.axis.xyz[0] as f32;
            let ay = joint.axis.xyz[1] as f32;
            let az = joint.axis.xyz[2] as f32;
            let axis_len = (ax * ax + ay * ay + az * az).sqrt().max(0.0001);
            let axis = vec3(ax / axis_len, ay / axis_len, az / axis_len);

            let (limit_lower, limit_upper) = match &joint.limit {
                urdf_rs::JointLimit { lower, upper, .. } => (*lower as f32, *upper as f32),
            };

            let robot_joint = RobotJoint {
                name: joint.name.clone(),
                parent_link: joint.parent.link.clone(),
                child_link: joint.child.link.clone(),
                origin_transform,
                axis,
                angle: 0.0,
                limit_lower,
                limit_upper,
            };

            child_links.insert(joint.child.link.clone());
            robot.joint_map.insert(joint.name.clone(), idx);
            robot.joints.push(robot_joint);
        }

        // Find root link (not a child of any joint)
        for link in &robot.links {
            if !child_links.contains(&link.name) {
                robot.root_link = link.name.clone();
                break;
            }
        }

        eprintln!("Loaded robot '{}' with {} links, {} joints, root: '{}'",
            robot.name, robot.links.len(), robot.joints.len(), robot.root_link);

        // Normalize all meshes together
        robot.normalize_meshes();

        Ok(robot)
    }

    /// Normalize all meshes and joint transforms to fit in a unit cube centered at origin
    fn normalize_meshes(&mut self) {
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

        eprintln!("Normalizing meshes: center=({:.3}, {:.3}, {:.3}), scale={:.4}",
            center[0], center[1], center[2], scale);

        // Create scale-only transform for mesh vertices
        let scale_transform = Mat4 { v: [
            scale, 0.0, 0.0, 0.0,
            0.0, scale, 0.0, 0.0,
            0.0, 0.0, scale, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]};

        // Apply scale to all link meshes (no centering - preserve relative positions)
        for link in &mut self.links {
            if let Some(ref mut mesh) = link.mesh_data {
                mesh.apply_transform(&scale_transform);
            }
        }

        // Scale the joint translations (not offset - joints are relative transforms)
        for joint in &mut self.joints {
            joint.origin_transform.v[12] *= scale;
            joint.origin_transform.v[13] *= scale;
            joint.origin_transform.v[14] *= scale;
        }

        // Compute root offset: scale then center
        // This moves the scaled robot so its center is at origin
        self.root_offset = Mat4::translation(vec3(
            -center[0] * scale,
            -center[1] * scale,
            -center[2] * scale,
        ));

        // Update bounds
        self.bounds_min = [
            (self.bounds_min[0] - center[0]) * scale,
            (self.bounds_min[1] - center[1]) * scale,
            (self.bounds_min[2] - center[2]) * scale,
        ];
        self.bounds_max = [
            (self.bounds_max[0] - center[0]) * scale,
            (self.bounds_max[1] - center[1]) * scale,
            (self.bounds_max[2] - center[2]) * scale,
        ];
    }

    /// Get joint angle by name
    pub fn get_joint_angle(&self, name: &str) -> Option<f32> {
        self.joint_map.get(name).map(|&idx| self.joints[idx].angle)
    }

    /// Set joint angle by name (clamped to limits)
    pub fn set_joint_angle(&mut self, name: &str, angle: f32) {
        if let Some(&idx) = self.joint_map.get(name) {
            let joint = &mut self.joints[idx];
            joint.angle = angle.clamp(joint.limit_lower, joint.limit_upper);
        }
    }

    /// Set joint angle by index (clamped to limits)
    pub fn set_joint_angle_by_index(&mut self, idx: usize, angle: f32) {
        if idx < self.joints.len() {
            let joint = &mut self.joints[idx];
            joint.angle = angle.clamp(joint.limit_lower, joint.limit_upper);
        }
    }

    /// Get joint angle by index
    pub fn get_joint_angle_by_index(&self, idx: usize) -> f32 {
        if idx < self.joints.len() {
            self.joints[idx].angle
        } else {
            0.0
        }
    }

    /// Get joint info by index
    pub fn get_joint_info(&self, idx: usize) -> Option<(&str, f32, f32, f32)> {
        if idx < self.joints.len() {
            let j = &self.joints[idx];
            Some((&j.name, j.angle, j.limit_lower, j.limit_upper))
        } else {
            None
        }
    }

    /// Compute forward kinematics - updates global_transform for all links
    pub fn update_forward_kinematics(&mut self) {
        // Start from root link with root_offset (centers the robot in view)
        if let Some(&root_idx) = self.link_map.get(&self.root_link) {
            self.links[root_idx].global_transform = self.root_offset;
        }

        // Process joints in order (assuming URDF is in topological order)
        for joint in &self.joints {
            let parent_transform = if let Some(&parent_idx) = self.link_map.get(&joint.parent_link) {
                self.links[parent_idx].global_transform
            } else {
                self.root_offset
            };

            // Compute rotation around joint axis
            let joint_rotation = axis_angle_to_mat4(joint.axis, joint.angle);

            // Child transform = Parent * Origin * JointRotation
            let temp = mat4_mul(&parent_transform, &joint.origin_transform);
            let child_transform = mat4_mul(&temp, &joint_rotation);

            if let Some(&child_idx) = self.link_map.get(&joint.child_link) {
                self.links[child_idx].global_transform = child_transform;
            }
        }
    }

    /// Get list of joint names
    pub fn joint_names(&self) -> Vec<&str> {
        self.joints.iter().map(|j| j.name.as_str()).collect()
    }

    /// Number of joints
    pub fn num_joints(&self) -> usize {
        self.joints.len()
    }
}

/// Convert URDF Pose to Mat4
fn pose_to_mat4(pose: &urdf_rs::Pose) -> Mat4 {
    let translation = vec3(
        pose.xyz.0[0] as f32,
        pose.xyz.0[1] as f32,
        pose.xyz.0[2] as f32,
    );

    let roll = pose.rpy.0[0] as f32;
    let pitch = pose.rpy.0[1] as f32;
    let yaw = pose.rpy.0[2] as f32;

    // Build rotation matrix from RPY (roll-pitch-yaw, XYZ order)
    let rotation = rpy_to_mat4(roll, pitch, yaw);

    // Translation * Rotation
    let trans_mat = Mat4::translation(translation);
    mat4_mul(&trans_mat, &rotation)
}

/// Convert roll-pitch-yaw angles to rotation matrix
fn rpy_to_mat4(roll: f32, pitch: f32, yaw: f32) -> Mat4 {
    // Rotation around X (roll)
    let cr = roll.cos();
    let sr = roll.sin();
    let rx = Mat4 { v: [
        1.0, 0.0, 0.0, 0.0,
        0.0, cr, sr, 0.0,
        0.0, -sr, cr, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]};

    // Rotation around Y (pitch)
    let cp = pitch.cos();
    let sp = pitch.sin();
    let ry = Mat4 { v: [
        cp, 0.0, -sp, 0.0,
        0.0, 1.0, 0.0, 0.0,
        sp, 0.0, cp, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]};

    // Rotation around Z (yaw)
    let cy = yaw.cos();
    let sy = yaw.sin();
    let rz = Mat4 { v: [
        cy, sy, 0.0, 0.0,
        -sy, cy, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]};

    // Combined rotation: Rz * Ry * Rx (apply roll first, then pitch, then yaw)
    let temp = mat4_mul(&rz, &ry);
    mat4_mul(&temp, &rx)
}

/// Convert axis-angle to rotation matrix
fn axis_angle_to_mat4(axis: Vec3, angle: f32) -> Mat4 {
    let c = angle.cos();
    let s = angle.sin();
    let t = 1.0 - c;

    let x = axis.x;
    let y = axis.y;
    let z = axis.z;

    // Rodrigues' rotation formula
    Mat4 { v: [
        t*x*x + c,     t*x*y + s*z,   t*x*z - s*y,   0.0,
        t*x*y - s*z,   t*y*y + c,     t*y*z + s*x,   0.0,
        t*x*z + s*y,   t*y*z - s*x,   t*z*z + c,     0.0,
        0.0,           0.0,           0.0,           1.0,
    ]}
}

impl MeshData {
    /// Apply a transformation matrix to all vertices
    pub fn apply_transform(&mut self, transform: &Mat4) {
        let num_vertices = self.vertices.len() / 9; // 9 floats per vertex

        // Reset bounds
        self.bounds_min = [f32::MAX; 3];
        self.bounds_max = [f32::MIN; 3];

        for i in 0..num_vertices {
            let base = i * 9;

            // Transform position (first 3 floats)
            let pos = vec4(
                self.vertices[base + 0],
                self.vertices[base + 1],
                self.vertices[base + 2],
                1.0,
            );
            let transformed_pos = transform.transform_vec4(pos);
            self.vertices[base + 0] = transformed_pos.x;
            self.vertices[base + 1] = transformed_pos.y;
            self.vertices[base + 2] = transformed_pos.z;

            // Update bounds
            for j in 0..3 {
                self.bounds_min[j] = self.bounds_min[j].min(self.vertices[base + j]);
                self.bounds_max[j] = self.bounds_max[j].max(self.vertices[base + j]);
            }

            // Transform normal (floats 4-6, index base+4 to base+6)
            let normal = vec4(
                self.vertices[base + 4],
                self.vertices[base + 5],
                self.vertices[base + 6],
                0.0, // w=0 for directions
            );
            let transformed_normal = transform.transform_vec4(normal);
            // Normalize the result
            let len = (transformed_normal.x * transformed_normal.x +
                      transformed_normal.y * transformed_normal.y +
                      transformed_normal.z * transformed_normal.z).sqrt();
            if len > 0.0001 {
                self.vertices[base + 4] = transformed_normal.x / len;
                self.vertices[base + 5] = transformed_normal.y / len;
                self.vertices[base + 6] = transformed_normal.z / len;
            }
        }
    }
}
