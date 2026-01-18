//! Embeddable Robot Viewer Widget
//!
//! A reusable Makepad widget for displaying and interacting with URDF robots.

use makepad_widgets::*;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use crate::mesh::{DrawMesh, DrawGrid, MeshData};

/// 3D Camera with perspective projection
#[derive(Clone, Debug)]
pub struct Camera3D {
    /// Camera position in world space
    pub position: glam::Vec3,
    /// Point the camera is looking at
    pub target: glam::Vec3,
    /// Up vector (usually Y-up)
    pub up: glam::Vec3,
    /// Vertical field of view in radians
    pub fov: f32,
    /// Near clipping plane distance
    pub near: f32,
    /// Far clipping plane distance
    pub far: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            position: glam::Vec3::new(0.0, 0.5, 3.0),
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            fov: std::f32::consts::FRAC_PI_4, // 45 degrees
            near: 0.01,
            far: 100.0,
        }
    }
}

impl Camera3D {
    /// Create a new camera with orbital parameters
    pub fn from_orbital(distance: f32, yaw: f32, pitch: f32, target: glam::Vec3) -> Self {
        // Convert orbital angles to position
        // yaw rotates around Y axis, pitch rotates around X axis
        let x = distance * pitch.cos() * yaw.sin();
        let y = distance * pitch.sin();
        let z = distance * pitch.cos() * yaw.cos();

        Self {
            position: target + glam::Vec3::new(x, y, z),
            target,
            up: glam::Vec3::Y,
            fov: std::f32::consts::FRAC_PI_4,
            near: 0.01,
            far: 100.0,
        }
    }

    /// Compute the view matrix (world to camera space)
    pub fn view_matrix(&self) -> glam::Mat4 {
        glam::Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// Compute the perspective projection matrix
    pub fn projection_matrix(&self, aspect_ratio: f32) -> glam::Mat4 {
        glam::Mat4::perspective_rh(self.fov, aspect_ratio, self.near, self.far)
    }

    /// Convert glam Mat4 to Makepad Mat4
    pub fn glam_to_makepad(m: glam::Mat4) -> Mat4 {
        Mat4 { v: m.to_cols_array() }
    }
}

/// Simple profiling stats for performance measurement
#[derive(Default, Clone)]
pub struct ProfilingStats {
    pub frame_times: Vec<f64>,      // Last N frame times in ms
    pub transform_times: Vec<f64>,  // Time spent on transforms in ms
    pub draw_times: Vec<f64>,       // Time spent on draw calls in ms
    pub frame_count: u64,
    max_samples: usize,
}

impl ProfilingStats {
    pub fn new(max_samples: usize) -> Self {
        Self {
            frame_times: Vec::with_capacity(max_samples),
            transform_times: Vec::with_capacity(max_samples),
            draw_times: Vec::with_capacity(max_samples),
            frame_count: 0,
            max_samples,
        }
    }

    pub fn record_frame(&mut self, frame_ms: f64, transform_ms: f64, draw_ms: f64) {
        self.frame_count += 1;

        if self.frame_times.len() >= self.max_samples {
            self.frame_times.remove(0);
            self.transform_times.remove(0);
            self.draw_times.remove(0);
        }

        self.frame_times.push(frame_ms);
        self.transform_times.push(transform_ms);
        self.draw_times.push(draw_ms);
    }

    pub fn avg_frame_time(&self) -> f64 {
        if self.frame_times.is_empty() { return 0.0; }
        self.frame_times.iter().sum::<f64>() / self.frame_times.len() as f64
    }

    pub fn avg_transform_time(&self) -> f64 {
        if self.transform_times.is_empty() { return 0.0; }
        self.transform_times.iter().sum::<f64>() / self.transform_times.len() as f64
    }

    pub fn avg_draw_time(&self) -> f64 {
        if self.draw_times.is_empty() { return 0.0; }
        self.draw_times.iter().sum::<f64>() / self.draw_times.len() as f64
    }

    pub fn print_stats(&self) {
        // Print every 30 frames (0.5 seconds at 60fps)
        if self.frame_count % 30 == 0 && self.frame_count > 0 {
            eprintln!(
                "[Frame {:>5}] Avg: {:>6.2}ms total | {:>6.3}ms transform | {:>6.2}ms draw | {:>5.1} FPS",
                self.frame_count,
                self.avg_frame_time(),
                self.avg_transform_time(),
                self.avg_draw_time(),
                1000.0 / self.avg_frame_time().max(0.001)
            );
        }
    }
}

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use crate::mesh::DrawMesh;
    use crate::mesh::DrawGrid;

    pub RobotView = {{RobotView}} {
        width: Fill
        height: Fill

        // Light blue sky background
        show_bg: true
        draw_bg: {
            fn pixel(self) -> vec4 {
                return vec4(0.53, 0.81, 0.92, 1.0);  // Light sky blue
            }
        }

        draw_mesh: {
            color: #e07020
        }

        draw_grid: {
            color: #408040  // Grid lines - darker green
            x_axis_color: #ff4040  // X axis - red
            z_axis_color: #4040ff  // Y axis - blue
            grid_spacing: 0.1  // 10cm grid spacing
            line_width: 0.004
        }
    }
}

/// A link in the robot
#[derive(Clone, Debug)]
pub struct RobotLink {
    pub name: String,
    pub mesh_data: Option<MeshData>,
    pub color: Option<[f32; 4]>,  // RGBA color from URDF material
}

/// A joint connecting links
#[derive(Clone, Debug)]
pub struct RobotJoint {
    pub name: String,
    pub parent_link: String,
    pub child_link: String,
    pub origin_xyz: glam::Vec3,
    pub origin_rpy: glam::Vec3,
    pub axis: glam::Vec3,
    pub angle: f32,
    pub limit_lower: f32,
    pub limit_upper: f32,
}

/// Robot structure with forward kinematics
#[derive(Clone)]
pub struct Robot {
    pub name: String,
    pub links: Vec<RobotLink>,
    pub joints: Vec<RobotJoint>,
    pub link_map: HashMap<String, usize>,
    pub root_link: String,
    pub link_transforms: Vec<glam::Mat4>,
    pub scale: f32,
    pub center: glam::Vec3,
}

impl Robot {
    pub fn from_urdf<P: AsRef<Path>>(urdf_path: P, assets_base: &str) -> Result<Self, String> {
        let urdf_content = std::fs::read_to_string(urdf_path.as_ref())
            .map_err(|e| format!("Failed to read URDF: {}", e))?;

        let urdf = urdf_rs::read_from_string(&urdf_content)
            .map_err(|e| format!("Failed to parse URDF: {}", e))?;

        let mut robot = Robot {
            name: urdf.name.clone(),
            links: Vec::new(),
            joints: Vec::new(),
            link_map: HashMap::new(),
            root_link: String::new(),
            link_transforms: Vec::new(),
            scale: 1.0,
            center: glam::Vec3::ZERO,
        };

        let mut bounds_min = glam::Vec3::splat(f32::MAX);
        let mut bounds_max = glam::Vec3::splat(f32::MIN);

        // Parse links
        for (idx, link) in urdf.links.iter().enumerate() {
            let mut robot_link = RobotLink {
                name: link.name.clone(),
                mesh_data: None,
                color: None,
            };

            let mut link_meshes = Vec::new();
            for visual in &link.visual {
                // Extract material color if present
                if robot_link.color.is_none() {
                    if let Some(ref material) = visual.material {
                        if let Some(ref color) = material.color {
                            robot_link.color = Some([
                                color.rgba[0] as f32,
                                color.rgba[1] as f32,
                                color.rgba[2] as f32,
                                color.rgba[3] as f32,
                            ]);
                        }
                    }
                }

                if let urdf_rs::Geometry::Mesh { filename, scale } = &visual.geometry {
                    let mesh_filename = Path::new(filename)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(filename);

                    let mesh_path = format!("{}/{}", assets_base, mesh_filename);

                    match MeshData::from_stl(&mesh_path) {
                        Ok(mut mesh) => {
                            // Apply mesh scale if specified in URDF
                            if let Some(s) = scale {
                                let scale_x = s.0[0] as f32;
                                let scale_y = s.0[1] as f32;
                                let scale_z = s.0[2] as f32;
                                // Use uniform scale (average) for simplicity
                                let uniform_scale = (scale_x + scale_y + scale_z) / 3.0;
                                if (uniform_scale - 1.0).abs() > 0.001 {
                                    mesh.apply_scale(uniform_scale);
                                }
                            }

                            let vis_xyz = glam::Vec3::new(
                                visual.origin.xyz.0[0] as f32,
                                visual.origin.xyz.0[1] as f32,
                                visual.origin.xyz.0[2] as f32,
                            );
                            let vis_rpy = glam::Vec3::new(
                                visual.origin.rpy.0[0] as f32,
                                visual.origin.rpy.0[1] as f32,
                                visual.origin.rpy.0[2] as f32,
                            );

                            if vis_xyz != glam::Vec3::ZERO || vis_rpy != glam::Vec3::ZERO {
                                let vis_rot = glam::Quat::from_euler(
                                    glam::EulerRot::XYZ,
                                    vis_rpy.x, vis_rpy.y, vis_rpy.z
                                );
                                let vis_transform = glam::Mat4::from_rotation_translation(vis_rot, vis_xyz);
                                let cols = vis_transform.to_cols_array();
                                let makepad_transform = Mat4 { v: cols };
                                mesh.apply_transform(&makepad_transform);
                            }

                            for i in 0..3 {
                                bounds_min[i] = bounds_min[i].min(mesh.bounds_min[i]);
                                bounds_max[i] = bounds_max[i].max(mesh.bounds_max[i]);
                            }
                            link_meshes.push(mesh);
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to load {}: {}", mesh_path, e);
                        }
                    }
                }
            }

            if !link_meshes.is_empty() {
                let mut combined = MeshData::combine(link_meshes);
                combined.make_double_sided();
                robot_link.mesh_data = Some(combined);
            }

            robot.link_map.insert(link.name.clone(), idx);
            robot.links.push(robot_link);
        }

        let center = (bounds_min + bounds_max) * 0.5;
        robot.scale = 1.0;
        robot.center = center;

        // Parse joints
        let mut child_links: std::collections::HashSet<String> = std::collections::HashSet::new();

        for joint in urdf.joints.iter() {
            let origin_xyz = glam::Vec3::new(
                joint.origin.xyz.0[0] as f32,
                joint.origin.xyz.0[1] as f32,
                joint.origin.xyz.0[2] as f32,
            );

            let origin_rpy = glam::Vec3::new(
                joint.origin.rpy.0[0] as f32,
                joint.origin.rpy.0[1] as f32,
                joint.origin.rpy.0[2] as f32,
            );

            let axis = glam::Vec3::new(
                joint.axis.xyz[0] as f32,
                joint.axis.xyz[1] as f32,
                joint.axis.xyz[2] as f32,
            ).normalize();

            let robot_joint = RobotJoint {
                name: joint.name.clone(),
                parent_link: joint.parent.link.clone(),
                child_link: joint.child.link.clone(),
                origin_xyz,
                origin_rpy,
                axis,
                angle: 0.0,
                limit_lower: joint.limit.lower as f32,
                limit_upper: joint.limit.upper as f32,
            };

            child_links.insert(joint.child.link.clone());
            robot.joints.push(robot_joint);
        }

        // Find root link
        for link in &robot.links {
            if !child_links.contains(&link.name) {
                robot.root_link = link.name.clone();
                break;
            }
        }

        robot.link_transforms = vec![glam::Mat4::IDENTITY; robot.links.len()];

        Ok(robot)
    }

    pub fn set_joint_angle(&mut self, idx: usize, angle: f32) {
        if idx < self.joints.len() {
            let joint = &mut self.joints[idx];
            joint.angle = angle.clamp(joint.limit_lower, joint.limit_upper);
        }
    }

    pub fn get_joint_info(&self, idx: usize) -> Option<(&str, f32, f32, f32)> {
        self.joints.get(idx).map(|j| (j.name.as_str(), j.angle, j.limit_lower, j.limit_upper))
    }

    pub fn num_joints(&self) -> usize {
        self.joints.len()
    }

    pub fn update_forward_kinematics(&mut self) {
        let root_offset = glam::Mat4::IDENTITY;

        if let Some(&root_idx) = self.link_map.get(&self.root_link) {
            self.link_transforms[root_idx] = root_offset;
        }

        let mut processed = std::collections::HashSet::new();
        processed.insert(self.root_link.clone());

        for _ in 0..self.joints.len() {
            for joint in &self.joints {
                if !processed.contains(&joint.parent_link) {
                    continue;
                }
                if processed.contains(&joint.child_link) {
                    continue;
                }

                let parent_transform = if let Some(&parent_idx) = self.link_map.get(&joint.parent_link) {
                    self.link_transforms[parent_idx]
                } else {
                    root_offset
                };

                let origin_rotation = glam::Quat::from_euler(
                    glam::EulerRot::XYZ,
                    joint.origin_rpy.x,
                    joint.origin_rpy.y,
                    joint.origin_rpy.z,
                );

                let joint_rotation = glam::Quat::from_axis_angle(joint.axis, joint.angle);
                let rotation = origin_rotation * joint_rotation;
                let joint_transform = glam::Mat4::from_rotation_translation(rotation, joint.origin_xyz);
                let child_transform = parent_transform * joint_transform;

                if let Some(&child_idx) = self.link_map.get(&joint.child_link) {
                    self.link_transforms[child_idx] = child_transform;
                }

                processed.insert(joint.child_link.clone());
            }
        }
    }
}

#[derive(Clone, Debug, DefaultNone)]
pub enum RobotViewAction {
    None,
    JointChanged { joint_idx: usize, angle: f32 },
    AnimationToggled(bool),
}

#[derive(Live, LiveHook, Widget)]
pub struct RobotView {
    #[redraw] #[live] draw_bg: DrawQuad,
    #[live] show_bg: bool,
    #[redraw] #[live] draw_mesh: DrawMesh,
    #[redraw] #[live] draw_grid: DrawGrid,
    #[walk] walk: Walk,
    #[layout] layout: Layout,

    #[rust] camera_distance: f64,
    #[rust] camera_yaw: f64,
    #[rust] camera_pitch: f64,
    #[rust] is_dragging: bool,
    #[rust] last_mouse: DVec2,

    #[rust] selected_joint: usize,
    #[rust] animating: bool,
    #[rust] anim_timer: Timer,
    #[rust] anim_step: u64,
    #[rust] initialized: bool,

    #[rust] robot: Option<Robot>,
    #[rust] link_drawers: Vec<DrawMesh>,
    #[rust] original_meshes: Vec<MeshData>,
    #[rust] grid_mesh: Option<MeshData>,
    #[rust] grid_drawer: Option<DrawMesh>,  // Use DrawMesh for grid instead
    #[rust] grid_initialized: bool,

    #[rust] urdf_path: String,
    #[rust] assets_dir: String,
    #[rust] area: Area,
    #[rust] profiling: ProfilingStats,
    #[rust] specular_enabled: bool,
    #[rust] show_joint_axes: bool,
    #[rust] axis_drawers: Vec<DrawMesh>,
    #[rust] axis_mesh: Option<MeshData>,
    #[rust] show_world_axes: bool,
    #[rust] world_axis_drawers: Vec<DrawMesh>,  // X, Y, Z axis cylinders
    #[rust] world_axes_initialized: bool,
}

impl RobotView {
    pub fn animate_step(&mut self, cx: &mut Cx, step: u64) {
        self.anim_step = step;
        if let Some(ref mut robot) = self.robot {
            for (joint_idx, joint) in robot.joints.iter_mut().enumerate() {
                let dynamic_angle = remap(
                    (step as f64 * (0.02 + joint_idx as f64 / 100.0)).sin(),
                    -1.0, 1.0,
                    joint.limit_lower as f64, joint.limit_upper as f64,
                ) as f32;
                joint.angle = dynamic_angle;
            }
        }
        self.redraw(cx);
    }

    pub fn load_robot(&mut self, cx: &mut Cx, urdf_path: &str, assets_dir: &str) {
        self.urdf_path = urdf_path.to_string();
        self.assets_dir = assets_dir.to_string();
        self.initialized = false;
        self.redraw(cx);
    }

    /// Reload robot with new URDF - clears current state and reinitializes
    pub fn reload_robot(&mut self, cx: &mut Cx, urdf_path: &str, assets_dir: &str) {
        // Clear current robot state
        self.robot = None;
        self.link_drawers.clear();
        self.original_meshes.clear();
        self.axis_drawers.clear();
        self.axis_mesh = None;
        self.selected_joint = 0;

        // Stop animation if running
        self.animating = false;
        self.anim_timer = Timer::default();
        self.anim_step = 0;

        // Set new paths and trigger reload
        self.urdf_path = urdf_path.to_string();
        self.assets_dir = assets_dir.to_string();
        self.initialized = false;

        self.redraw(cx);
    }

    pub fn set_joint_angle(&mut self, cx: &mut Cx, idx: usize, angle: f32) {
        if let Some(ref mut robot) = self.robot {
            robot.set_joint_angle(idx, angle);
            self.redraw(cx);
        }
    }

    pub fn get_joint_angles(&self) -> Vec<f32> {
        self.robot.as_ref()
            .map(|r| r.joints.iter().map(|j| j.angle).collect())
            .unwrap_or_default()
    }

    pub fn set_joint_angles(&mut self, cx: &mut Cx, angles: &[f32]) {
        if let Some(ref mut robot) = self.robot {
            for (i, &angle) in angles.iter().enumerate() {
                robot.set_joint_angle(i, angle);
            }
            self.redraw(cx);
        }
    }

    pub fn toggle_animation(&mut self, cx: &mut Cx) -> bool {
        self.animating = !self.animating;
        if self.animating {
            self.anim_timer = cx.start_interval(0.033);
        } else {
            self.anim_timer = Timer::default();
        }
        self.animating
    }

    pub fn reset_view(&mut self, cx: &mut Cx) {
        self.camera_distance = 3.0;  // Zoomed out to show entire robot
        self.camera_yaw = 0.5;
        self.camera_pitch = 0.3;
        if let Some(ref mut robot) = self.robot {
            for joint in &mut robot.joints {
                joint.angle = 0.0;
            }
        }
        self.redraw(cx);
    }

    /// Toggle specular lighting on/off
    pub fn toggle_specular(&mut self, cx: &mut Cx) -> bool {
        self.specular_enabled = !self.specular_enabled;
        self.redraw(cx);
        self.specular_enabled
    }

    /// Check if specular lighting is enabled
    pub fn is_specular_enabled(&self) -> bool {
        self.specular_enabled
    }

    /// Toggle joint axis visualization
    pub fn toggle_joint_axes(&mut self, cx: &mut Cx) -> bool {
        self.show_joint_axes = !self.show_joint_axes;
        self.redraw(cx);
        self.show_joint_axes
    }

    /// Check if joint axes are shown
    pub fn is_joint_axes_shown(&self) -> bool {
        self.show_joint_axes
    }

    /// Toggle world XYZ axis visualization
    pub fn toggle_world_axes(&mut self, cx: &mut Cx) -> bool {
        self.show_world_axes = !self.show_world_axes;
        self.redraw(cx);
        self.show_world_axes
    }

    /// Check if world axes are shown
    pub fn is_world_axes_shown(&self) -> bool {
        self.show_world_axes
    }

    fn init_robot(&mut self, cx: &mut Cx) {
        if self.urdf_path.is_empty() {
            // Default to VX300s (ALOHA arm)
            self.urdf_path = "data/vx300s/vx300s.urdf".to_string();
            self.assets_dir = "data/vx300s".to_string();
        }

        match Robot::from_urdf(&self.urdf_path, &self.assets_dir) {
            Ok(mut robot) => {
                // Create link drawers
                for link in robot.links.iter() {
                    if let Some(ref mesh_data) = link.mesh_data {
                        self.original_meshes.push(mesh_data.clone());
                        let mut draw = DrawMesh::new_for_link(cx, mesh_data.clone(), &self.draw_mesh);
                        draw.init_link_geometry(cx);
                        self.link_drawers.push(draw);
                    }
                }

                // Create axis mesh (thin cylinder along Y axis)
                let axis_mesh = MeshData::cylinder(0.005, 0.15, 8); // 5mm radius, 15cm length
                self.axis_mesh = Some(axis_mesh.clone());

                // Create axis drawer for each joint
                for _ in 0..robot.joints.len() {
                    let mut axis_draw = DrawMesh::new_for_link(cx, axis_mesh.clone(), &self.draw_mesh);
                    axis_draw.init_link_geometry(cx);
                    axis_draw.color = vec4(1.0, 0.2, 0.2, 1.0); // Red for joint axes
                    self.axis_drawers.push(axis_draw);
                }

                robot.update_forward_kinematics();
                self.robot = Some(robot);
            }
            Err(e) => {
                eprintln!("Failed to load robot: {}", e);
            }
        }
    }
}

impl Widget for RobotView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Animation timer (now handled at URDFViewer level)

        // Keyboard
        if let Event::KeyDown(ke) = event {
            let num_joints = self.robot.as_ref().map(|r| r.num_joints()).unwrap_or(6);
            match ke.key_code {
                KeyCode::ArrowUp => {
                    if let Some(ref mut robot) = self.robot {
                        let current = robot.joints.get(self.selected_joint).map(|j| j.angle).unwrap_or(0.0);
                        robot.set_joint_angle(self.selected_joint, current + 0.15);
                        let new_angle = robot.joints[self.selected_joint].angle;
                        cx.widget_action(
                            self.widget_uid(),
                            &scope.path,
                            RobotViewAction::JointChanged { joint_idx: self.selected_joint, angle: new_angle }
                        );
                    }
                    self.redraw(cx);
                }
                KeyCode::ArrowDown => {
                    if let Some(ref mut robot) = self.robot {
                        let current = robot.joints.get(self.selected_joint).map(|j| j.angle).unwrap_or(0.0);
                        robot.set_joint_angle(self.selected_joint, current - 0.15);
                        let new_angle = robot.joints[self.selected_joint].angle;
                        cx.widget_action(
                            self.widget_uid(),
                            &scope.path,
                            RobotViewAction::JointChanged { joint_idx: self.selected_joint, angle: new_angle }
                        );
                    }
                    self.redraw(cx);
                }
                KeyCode::ArrowLeft => {
                    if num_joints > 0 {
                        self.selected_joint = (self.selected_joint + num_joints - 1) % num_joints;
                    }
                    self.redraw(cx);
                }
                KeyCode::ArrowRight => {
                    if num_joints > 0 {
                        self.selected_joint = (self.selected_joint + 1) % num_joints;
                    }
                    self.redraw(cx);
                }
                KeyCode::KeyA => {
                    // Animation handled at URDFViewer level
                }
                KeyCode::KeyR => {
                    self.reset_view(cx);
                }
                // Zoom with =/- keys (= is + on US keyboards)
                KeyCode::Equals => {
                    // Zoom in (decrease camera distance)
                    self.camera_distance *= 0.9;
                    self.camera_distance = self.camera_distance.clamp(0.2, 10.0);
                    self.redraw(cx);
                }
                KeyCode::Minus => {
                    // Zoom out (increase camera distance)
                    self.camera_distance *= 1.1;
                    self.camera_distance = self.camera_distance.clamp(0.2, 10.0);
                    self.redraw(cx);
                }
                _ => {}
            }
        }

        // Mouse for camera
        match event.hits(cx, self.area) {
            Hit::FingerDown(fe) => {
                self.is_dragging = true;
                self.last_mouse = fe.abs;
            }
            Hit::FingerMove(fe) if self.is_dragging => {
                let delta = fe.abs - self.last_mouse;
                self.last_mouse = fe.abs;
                self.camera_yaw += delta.x * 0.01;
                self.camera_pitch += delta.y * 0.01;
                self.camera_pitch = self.camera_pitch.clamp(-1.4, 1.4);
                self.redraw(cx);
            }
            Hit::FingerUp(_) => {
                self.is_dragging = false;
            }
            Hit::FingerScroll(se) => {
                // Two-finger scroll on trackpad for zoom
                // scroll.y > 0 = scroll up = zoom in (closer)
                // scroll.y < 0 = scroll down = zoom out (farther)
                let zoom_factor = 1.0 - se.scroll.y * 0.01;  // Increased sensitivity
                self.camera_distance *= zoom_factor;
                self.camera_distance = self.camera_distance.clamp(0.1, 15.0);
                self.redraw(cx);
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let frame_start = Instant::now();

        cx.begin_turtle(walk, self.layout);

        // Draw sky background first
        if self.show_bg {
            self.draw_bg.draw_abs(cx, cx.turtle().rect());
        }

        if !self.initialized {
            self.initialized = true;
            self.camera_distance = 3.0;  // Zoomed out to show entire robot
            self.camera_yaw = 0.5;
            self.camera_pitch = 0.3;
            self.profiling = ProfilingStats::new(120);  // Track last 120 frames (2 seconds at 60fps)
            self.specular_enabled = true;  // Specular lighting on by default
            self.init_robot(cx.cx);
            eprintln!("=== URDF Viewer Initialized - GPU Transform Profiling Enabled ===");
        }

        // Initialize grid if needed
        if !self.grid_initialized {
            self.grid_initialized = true;
            // Create large grid plane to fill earth area
            let grid_size = 20.0;  // 20 meter grid - much larger to fill screen
            // Create grid slightly below robot base (y=-0.01) so robot renders on top
            let mut grid = MeshData::ground_plane(grid_size, -0.01);
            // Rotate grid 90 degrees around X to align with robot base
            let rot_mat = glam::Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);
            let rot_makepad = Mat4 { v: rot_mat.to_cols_array() };
            grid.apply_transform(&rot_makepad);
            grid.make_double_sided();
            self.grid_mesh = Some(grid.clone());
            // Use DrawMesh for the grid with grid lines enabled
            let mut grid_draw = DrawMesh::new_for_link(cx.cx, grid, &self.draw_mesh);
            grid_draw.init_link_geometry(cx.cx);
            grid_draw.color = vec4(0.95, 0.95, 0.85, 1.0);  // Light light yellow for earth
            grid_draw.draw_grid_lines = 1.0;  // Enable grid line rendering
            self.grid_drawer = Some(grid_draw);
        }

        // Initialize world XYZ axes if needed (thin black lines on ground)
        if !self.world_axes_initialized || self.world_axis_drawers.is_empty() {
            self.world_axes_initialized = true;
            self.world_axis_drawers.clear();
            let axis_length = 0.15;  // 15cm axes
            let axis_radius = 0.001;  // 1mm thin line
            let axis_mesh = MeshData::cylinder(axis_radius, axis_length, 6);

            // X axis (black thin line)
            let mut x_axis = DrawMesh::new_for_link(cx.cx, axis_mesh.clone(), &self.draw_mesh);
            x_axis.init_link_geometry(cx.cx);
            x_axis.color = vec4(0.1, 0.1, 0.1, 1.0);  // Dark black
            self.world_axis_drawers.push(x_axis);

            // Y axis (black thin line) - this will be vertical
            let mut y_axis = DrawMesh::new_for_link(cx.cx, axis_mesh.clone(), &self.draw_mesh);
            y_axis.init_link_geometry(cx.cx);
            y_axis.color = vec4(0.1, 0.1, 0.1, 1.0);
            self.world_axis_drawers.push(y_axis);

            // Z axis (black thin line)
            let mut z_axis = DrawMesh::new_for_link(cx.cx, axis_mesh, &self.draw_mesh);
            z_axis.init_link_geometry(cx.cx);
            z_axis.color = vec4(0.1, 0.1, 0.1, 1.0);
            self.world_axis_drawers.push(z_axis);
        }

        let mut transform_time_ms = 0.0;
        let mut draw_time_ms = 0.0;

        if let Some(ref mut robot) = self.robot {
            robot.update_forward_kinematics();

            // Camera parameters
            let cam_yaw = self.camera_yaw as f32;
            let cam_pitch = self.camera_pitch as f32;
            let cam_scale = 1.0 / self.camera_distance as f32;  // Closer = larger

            // Build combined transform: scale * orbital_rotation * base_rotation
            let base_rot = glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            let orbital_rot = glam::Mat4::from_euler(glam::EulerRot::YXZ, cam_yaw, cam_pitch, 0.0);
            let scale_mat = glam::Mat4::from_scale(glam::Vec3::splat(cam_scale));
            let camera_transform = scale_mat * orbital_rot * base_rot;

            // Camera position for specular lighting (approximate from orbital params)
            let cam_dist = self.camera_distance as f32;
            let cam_x = cam_dist * cam_pitch.cos() * cam_yaw.sin();
            let cam_y = cam_dist * cam_pitch.sin() + 0.3;
            let cam_z = cam_dist * cam_pitch.cos() * cam_yaw.cos();
            let camera_pos = vec3(cam_x, cam_y, cam_z);

            // Specular strength based on toggle
            let specular_strength = if self.specular_enabled { 0.5 } else { 0.0 };

            // Draw grid first (behind robot)
            if let Some(ref mut grid_drawer) = self.grid_drawer {
                let grid_transform = Camera3D::glam_to_makepad(camera_transform);
                grid_drawer.set_transform(&grid_transform);
                grid_drawer.set_camera_position(camera_pos);
                grid_drawer.set_specular_strength(specular_strength);
                grid_drawer.begin_many_instances(cx);
                grid_drawer.draw(cx);
                grid_drawer.end_many_instances(cx);
            }

            // Color palette for different robot parts (maintains shading)
            let link_colors = [
                vec4(0.3, 0.3, 0.35, 1.0),   // Base - dark gray
                vec4(0.2, 0.4, 0.8, 1.0),    // Shoulder - blue
                vec4(0.2, 0.6, 0.9, 1.0),    // Upper arm - light blue
                vec4(1.0, 0.9, 0.2, 1.0),    // Upper forearm - yellow
                vec4(0.15, 0.15, 0.15, 1.0), // Lower forearm - black
                vec4(0.9, 0.6, 0.2, 1.0),    // Wrist - orange
                vec4(0.8, 0.3, 0.3, 1.0),    // Gripper - red
                vec4(0.7, 0.7, 0.7, 1.0),    // Gripper bar - silver
                vec4(0.6, 0.6, 0.65, 1.0),   // Left finger - gray
                vec4(0.6, 0.6, 0.65, 1.0),   // Right finger - gray
            ];

            // ===== PROFILING: Transform phase =====
            let transform_start = Instant::now();

            let mut drawer_idx = 0;
            for (link_idx, link) in robot.links.iter().enumerate() {
                if link.mesh_data.is_none() { continue; }
                if drawer_idx >= self.link_drawers.len() { break; }

                // Combined transform = camera_transform * link_transform
                let link_transform = robot.link_transforms[link_idx];
                let combined_transform = Camera3D::glam_to_makepad(camera_transform * link_transform);

                let drawer = &mut self.link_drawers[drawer_idx];
                // Set combined transform and camera position for specular
                drawer.set_transform(&combined_transform);
                drawer.set_camera_position(camera_pos);
                drawer.set_specular_strength(specular_strength);

                // Use URDF color if available, otherwise fall back to palette
                drawer.color = if let Some(c) = link.color {
                    vec4(c[0], c[1], c[2], c[3])
                } else {
                    link_colors[drawer_idx % link_colors.len()]
                };

                drawer_idx += 1;
            }

            transform_time_ms = transform_start.elapsed().as_secs_f64() * 1000.0;

            // ===== PROFILING: Draw phase =====
            let draw_start = Instant::now();

            for drawer in &mut self.link_drawers {
                drawer.begin_many_instances(cx);
                drawer.draw(cx);
                drawer.end_many_instances(cx);
            }

            // Draw world XYZ axes if enabled (thin black lines beside robot)
            if self.show_world_axes && self.world_axis_drawers.len() >= 3 {
                // Position axes beside robot (right side)
                let base_x = 0.25;  // Right of robot
                let base_z = 0.0;   // Aligned with robot
                let ground_y = 0.01;
                let half_len = 0.075;  // Half of 15cm axis length

                // X axis - horizontal line along X on ground (rotate Y cylinder to X)
                let x_rot = glam::Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2);
                let x_pos = glam::Vec3::new(base_x + half_len, ground_y, base_z);
                let x_transform = camera_transform * glam::Mat4::from_rotation_translation(x_rot, x_pos);
                self.world_axis_drawers[0].set_transform(&Camera3D::glam_to_makepad(x_transform));
                self.world_axis_drawers[0].set_camera_position(camera_pos);
                self.world_axis_drawers[0].set_specular_strength(0.0);  // No specular for flat look
                self.world_axis_drawers[0].begin_many_instances(cx);
                self.world_axis_drawers[0].draw(cx);
                self.world_axis_drawers[0].end_many_instances(cx);

                // Y axis - vertical line (default cylinder orientation)
                let y_pos = glam::Vec3::new(base_x, half_len + ground_y, base_z);
                let y_transform = camera_transform * glam::Mat4::from_translation(y_pos);
                self.world_axis_drawers[1].set_transform(&Camera3D::glam_to_makepad(y_transform));
                self.world_axis_drawers[1].set_camera_position(camera_pos);
                self.world_axis_drawers[1].set_specular_strength(0.0);
                self.world_axis_drawers[1].begin_many_instances(cx);
                self.world_axis_drawers[1].draw(cx);
                self.world_axis_drawers[1].end_many_instances(cx);

                // Z axis - horizontal line along Z on ground (rotate Y cylinder to Z)
                let z_rot = glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                let z_pos = glam::Vec3::new(base_x, ground_y, base_z + half_len);
                let z_transform = camera_transform * glam::Mat4::from_rotation_translation(z_rot, z_pos);
                self.world_axis_drawers[2].set_transform(&Camera3D::glam_to_makepad(z_transform));
                self.world_axis_drawers[2].set_camera_position(camera_pos);
                self.world_axis_drawers[2].set_specular_strength(0.0);
                self.world_axis_drawers[2].begin_many_instances(cx);
                self.world_axis_drawers[2].draw(cx);
                self.world_axis_drawers[2].end_many_instances(cx);
            }

            // Draw joint axes if enabled
            if self.show_joint_axes {
                for (joint_idx, joint) in robot.joints.iter().enumerate() {
                    if joint_idx >= self.axis_drawers.len() { break; }

                    // Get parent link transform
                    let parent_transform = if let Some(&parent_idx) = robot.link_map.get(&joint.parent_link) {
                        robot.link_transforms[parent_idx]
                    } else {
                        glam::Mat4::IDENTITY
                    };

                    // Joint position in parent frame
                    let joint_pos = joint.origin_xyz;

                    // Create rotation to align Y-axis with joint axis
                    let axis = joint.axis.normalize();
                    let up = glam::Vec3::Y;
                    let axis_rotation = if (axis - up).length() < 0.001 {
                        glam::Quat::IDENTITY
                    } else if (axis + up).length() < 0.001 {
                        glam::Quat::from_rotation_x(std::f32::consts::PI)
                    } else {
                        glam::Quat::from_rotation_arc(up, axis)
                    };

                    // Combine: camera * parent * joint_origin * axis_rotation
                    let joint_transform = glam::Mat4::from_rotation_translation(
                        glam::Quat::from_euler(glam::EulerRot::XYZ,
                            joint.origin_rpy.x, joint.origin_rpy.y, joint.origin_rpy.z) * axis_rotation,
                        joint_pos
                    );
                    let world_transform = camera_transform * parent_transform * joint_transform;
                    let axis_transform = Camera3D::glam_to_makepad(world_transform);

                    let drawer = &mut self.axis_drawers[joint_idx];
                    drawer.set_transform(&axis_transform);
                    drawer.set_camera_position(camera_pos);
                    drawer.set_specular_strength(0.3);

                    // Color based on joint index (cycle through colors)
                    let colors = [
                        vec4(1.0, 0.2, 0.2, 1.0), // Red
                        vec4(0.2, 1.0, 0.2, 1.0), // Green
                        vec4(0.2, 0.2, 1.0, 1.0), // Blue
                        vec4(1.0, 1.0, 0.2, 1.0), // Yellow
                        vec4(1.0, 0.2, 1.0, 1.0), // Magenta
                        vec4(0.2, 1.0, 1.0, 1.0), // Cyan
                    ];
                    drawer.color = colors[joint_idx % colors.len()];

                    drawer.begin_many_instances(cx);
                    drawer.draw(cx);
                    drawer.end_many_instances(cx);
                }
            }

            draw_time_ms = draw_start.elapsed().as_secs_f64() * 1000.0;
        }

        cx.end_turtle_with_area(&mut self.area);

        // Record profiling stats
        let frame_time_ms = frame_start.elapsed().as_secs_f64() * 1000.0;
        self.profiling.record_frame(frame_time_ms, transform_time_ms, draw_time_ms);
        self.profiling.print_stats();

        DrawStep::done()
    }
}

impl RobotViewRef {
    pub fn animate_step(&self, cx: &mut Cx, step: u64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.animate_step(cx, step);
        }
    }

    pub fn load_robot(&self, cx: &mut Cx, urdf_path: &str, assets_dir: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.load_robot(cx, urdf_path, assets_dir);
        }
    }

    pub fn reload_robot(&self, cx: &mut Cx, urdf_path: &str, assets_dir: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.reload_robot(cx, urdf_path, assets_dir);
        }
    }

    pub fn set_joint_angle(&self, cx: &mut Cx, idx: usize, angle: f32) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_joint_angle(cx, idx, angle);
        }
    }

    pub fn get_joint_angles(&self) -> Vec<f32> {
        if let Some(inner) = self.borrow() {
            inner.get_joint_angles()
        } else {
            vec![]
        }
    }

    pub fn set_joint_angles(&self, cx: &mut Cx, angles: &[f32]) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_joint_angles(cx, angles);
        }
    }

    pub fn toggle_animation(&self, cx: &mut Cx) -> bool {
        if let Some(mut inner) = self.borrow_mut() {
            inner.toggle_animation(cx)
        } else {
            false
        }
    }

    pub fn reset_view(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.reset_view(cx);
        }
    }

    pub fn get_selected_joint(&self) -> usize {
        if let Some(inner) = self.borrow() {
            inner.selected_joint
        } else {
            0
        }
    }

    pub fn get_joint_info(&self, idx: usize) -> Option<(String, f32, f32, f32)> {
        if let Some(inner) = self.borrow() {
            inner.robot.as_ref()
                .and_then(|r| r.get_joint_info(idx))
                .map(|(name, angle, lower, upper)| (name.to_string(), angle, lower, upper))
        } else {
            None
        }
    }

    pub fn toggle_specular(&self, cx: &mut Cx) -> bool {
        if let Some(mut inner) = self.borrow_mut() {
            inner.toggle_specular(cx)
        } else {
            false
        }
    }

    pub fn is_specular_enabled(&self) -> bool {
        if let Some(inner) = self.borrow() {
            inner.is_specular_enabled()
        } else {
            true
        }
    }

    pub fn toggle_joint_axes(&self, cx: &mut Cx) -> bool {
        if let Some(mut inner) = self.borrow_mut() {
            inner.toggle_joint_axes(cx)
        } else {
            false
        }
    }

    pub fn is_joint_axes_shown(&self) -> bool {
        if let Some(inner) = self.borrow() {
            inner.is_joint_axes_shown()
        } else {
            false
        }
    }

    pub fn toggle_world_axes(&self, cx: &mut Cx) -> bool {
        if let Some(mut inner) = self.borrow_mut() {
            inner.toggle_world_axes(cx)
        } else {
            false
        }
    }

    pub fn is_world_axes_shown(&self) -> bool {
        if let Some(inner) = self.borrow() {
            inner.is_world_axes_shown()
        } else {
            false
        }
    }
}

fn remap(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    let t = (value - from_min) / (from_max - from_min);
    to_min + t * (to_max - to_min)
}
