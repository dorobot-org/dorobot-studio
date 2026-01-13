//! Embeddable Robot Viewer Widget
//!
//! A reusable Makepad widget for displaying and interacting with URDF robots.

use makepad_widgets::*;
use std::collections::HashMap;
use std::path::Path;

use crate::mesh::{DrawMesh, DrawGrid, MeshData};

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
        self.camera_distance = 1.0;
        self.camera_yaw = 0.5;
        self.camera_pitch = 0.3;
        if let Some(ref mut robot) = self.robot {
            for joint in &mut robot.joints {
                joint.angle = 0.0;
            }
        }
        self.redraw(cx);
    }

    fn init_robot(&mut self, cx: &mut Cx) {
        if self.urdf_path.is_empty() {
            // Default to VX300s (ALOHA arm)
            self.urdf_path = "data/vx300s/vx300s.urdf".to_string();
            self.assets_dir = "data/vx300s".to_string();
        }

        match Robot::from_urdf(&self.urdf_path, &self.assets_dir) {
            Ok(mut robot) => {
                for link in robot.links.iter() {
                    if let Some(ref mesh_data) = link.mesh_data {
                        self.original_meshes.push(mesh_data.clone());
                        let mut draw = DrawMesh::new_for_link(cx, mesh_data.clone(), &self.draw_mesh);
                        draw.init_link_geometry(cx);
                        self.link_drawers.push(draw);
                    }
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
        cx.begin_turtle(walk, self.layout);

        // Draw sky background first
        if self.show_bg {
            self.draw_bg.draw_abs(cx, cx.turtle().rect());
        }

        if !self.initialized {
            self.initialized = true;
            self.camera_distance = 1.0;
            self.camera_yaw = 0.5;
            self.camera_pitch = 0.2;
            self.init_robot(cx.cx);
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

        if let Some(ref mut robot) = self.robot {
            robot.update_forward_kinematics();


            let cam_yaw = self.camera_yaw as f32;
            let cam_pitch = self.camera_pitch as f32;
            let cam_scale = 1.0 / self.camera_distance as f32;  // Closer = larger

            let base_rot = glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            let orbital_rot = glam::Mat4::from_euler(glam::EulerRot::YXZ, cam_yaw, cam_pitch, 0.0);
            let scale_mat = glam::Mat4::from_scale(glam::Vec3::splat(cam_scale));
            let camera_rot = scale_mat * orbital_rot * base_rot;

            fn glam_to_makepad(m: glam::Mat4) -> Mat4 {
                Mat4 { v: m.to_cols_array() }
            }

            // Draw grid first (behind robot) using DrawMesh
            if let (Some(ref grid_mesh), Some(ref mut grid_drawer)) = (&self.grid_mesh, &mut self.grid_drawer) {
                let grid_transform = glam_to_makepad(camera_rot);
                grid_drawer.update_transformed_geometry(cx.cx, grid_mesh, &grid_transform);
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

            let mut drawer_idx = 0;
            for (link_idx, link) in robot.links.iter().enumerate() {
                if link.mesh_data.is_none() { continue; }
                if drawer_idx >= self.link_drawers.len() { break; }

                let link_transform = robot.link_transforms[link_idx];
                let transform = glam_to_makepad(camera_rot * link_transform);

                let drawer = &mut self.link_drawers[drawer_idx];
                drawer.update_transformed_geometry(cx.cx, &self.original_meshes[drawer_idx], &transform);

                // Use URDF color if available, otherwise fall back to palette
                drawer.color = if let Some(c) = link.color {
                    vec4(c[0], c[1], c[2], c[3])
                } else {
                    link_colors[drawer_idx % link_colors.len()]
                };

                drawer.begin_many_instances(cx);
                drawer.draw(cx);
                drawer.end_many_instances(cx);

                drawer_idx += 1;
            }
        }

        cx.end_turtle_with_area(&mut self.area);
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
}

fn remap(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    let t = (value - from_min) / (from_max - from_min);
    to_min + t * (to_max - to_min)
}
