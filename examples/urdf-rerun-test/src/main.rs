//! 3D Robot Viewer - Rerun-style animation
//!
//! Uses glam quaternions like the rerun example for exact same behavior.
//!
//! Controls:
//!   - Mouse drag: Orbit camera
//!   - Scroll: Zoom
//!   - Arrow keys: Control joints (Left/Right to select, Up/Down to adjust)
//!   - A: Toggle animation
//!   - R: Reset to default pose

use makepad_widgets::*;
use std::collections::HashMap;
use std::path::Path;

mod mesh;

use mesh::{DrawMesh, MeshData};

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use crate::mesh::DrawMesh;

    URDFViewer = {{URDFViewer}} {
        width: Fill
        height: Fill
        flow: Down

        header = <View> {
            width: Fill
            height: 40
            padding: 8
            spacing: 16
            align: { y: 0.5 }
            show_bg: true
            draw_bg: { color: #2a2a35 }

            <Label> {
                draw_text: {
                    text_style: { font_size: 14.0 }
                    color: #fff
                }
                text: "SO100 Robot (Rerun-style) - Arrow keys control joints, A=animate"
            }

            <View> { width: Fill }

            reset_btn = <Button> {
                text: "Reset"
                draw_text: { color: #fff }
            }
        }

        viewport = <View> {
            width: Fill
            height: Fill
            show_bg: true
            draw_bg: { color: #f5f5dc }  // Light yellow/beige background
        }

        status_bar = <View> {
            width: Fill
            height: 36
            padding: 8
            align: { y: 0.5 }
            show_bg: true
            draw_bg: { color: #2a2a35 }

            joint_label = <Label> {
                draw_text: {
                    text_style: { font_size: 12.0 }
                    color: #888
                }
                text: "Joint 0: 0.00 rad"
            }
        }

        draw_mesh: {
            color: #e07020
        }
    }

    App = {{App}} {
        ui: <Window> {
            window: { title: "SO100 Robot Viewer (Rerun-style)" }
            show_bg: true
            draw_bg: { color: #1a1a1f }
            body = <URDFViewer> {}
        }
    }
}

/// A link in the robot
#[derive(Clone, Debug)]
struct RobotLink {
    name: String,
    mesh_data: Option<MeshData>,
}

/// A joint connecting links
#[derive(Clone, Debug)]
struct RobotJoint {
    name: String,
    parent_link: String,
    child_link: String,
    /// Origin translation (xyz)
    origin_xyz: glam::Vec3,
    /// Origin rotation (rpy)
    origin_rpy: glam::Vec3,
    /// Joint axis
    axis: glam::Vec3,
    /// Current joint angle
    angle: f32,
    /// Joint limits
    limit_lower: f32,
    limit_upper: f32,
}

/// Robot structure
struct Robot {
    name: String,
    links: Vec<RobotLink>,
    joints: Vec<RobotJoint>,
    link_map: HashMap<String, usize>,
    root_link: String,
    /// Global transforms for each link (computed via FK)
    link_transforms: Vec<glam::Mat4>,
    /// Scale factor for normalization
    scale: f32,
    /// Center offset for normalization
    center: glam::Vec3,
}

impl Robot {
    fn from_urdf<P: AsRef<Path>>(urdf_path: P, assets_base: &str) -> Result<Self, String> {
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

        // Track bounds for normalization
        let mut bounds_min = glam::Vec3::splat(f32::MAX);
        let mut bounds_max = glam::Vec3::splat(f32::MIN);

        // Parse links
        for (idx, link) in urdf.links.iter().enumerate() {
            let mut robot_link = RobotLink {
                name: link.name.clone(),
                mesh_data: None,
            };

            // Load meshes with visual origin transforms applied
            let mut link_meshes = Vec::new();
            for visual in &link.visual {
                if let urdf_rs::Geometry::Mesh { filename, .. } = &visual.geometry {
                    let mesh_filename = Path::new(filename)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(filename);

                    let mesh_path = format!("{}/{}", assets_base, mesh_filename);

                    match MeshData::from_stl(&mesh_path) {
                        Ok(mut mesh) => {
                            // Apply visual origin transform to position mesh in link frame
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

                            // Only apply if visual origin is not identity
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

        // Compute normalization - but DON'T apply scaling to meshes
        // Keep meshes in original URDF coordinates (meters)
        // This matches how rerun handles it
        let center = (bounds_min + bounds_max) * 0.5;
        let extent = bounds_max - bounds_min;
        let max_extent = extent.x.max(extent.y).max(extent.z).max(0.001);
        let _ = max_extent;  // unused but kept for reference
        robot.scale = 1.0;  // Don't scale meshes - keep in meters
        robot.center = center;

        // DON'T apply scale to meshes - keep them in original meters
        // The shader will handle view scaling

        // Parse joints
        let mut child_links: std::collections::HashSet<String> = std::collections::HashSet::new();

        for joint in urdf.joints.iter() {
            // Keep joint origins in original URDF units (meters) - no scaling
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

        // Initialize transforms
        robot.link_transforms = vec![glam::Mat4::IDENTITY; robot.links.len()];

        Ok(robot)
    }

    fn set_joint_angle(&mut self, idx: usize, angle: f32) {
        if idx < self.joints.len() {
            let joint = &mut self.joints[idx];
            joint.angle = angle.clamp(joint.limit_lower, joint.limit_upper);
        }
    }

    fn get_joint_info(&self, idx: usize) -> Option<(&str, f32, f32, f32)> {
        self.joints.get(idx).map(|j| (j.name.as_str(), j.angle, j.limit_lower, j.limit_upper))
    }

    fn num_joints(&self) -> usize {
        self.joints.len()
    }

    /// Compute forward kinematics using glam (same as rerun example)
    fn update_forward_kinematics(&mut self) {
        // Root link: just identity - let the robot be at its natural position
        // The mesh vertices are in link-local coordinates, FK positions them correctly
        // Don't use center offset - that was computed from combined mesh bounds which is meaningless
        let root_offset = glam::Mat4::IDENTITY;

        if let Some(&root_idx) = self.link_map.get(&self.root_link) {
            self.link_transforms[root_idx] = root_offset;
        }

        // Process joints multiple times to handle any ordering issues
        // (simple approach - iterate until all links are processed)
        let mut processed = std::collections::HashSet::new();
        processed.insert(self.root_link.clone());

        for _ in 0..self.joints.len() {
            for joint in &self.joints {
                // Skip if parent not yet processed
                if !processed.contains(&joint.parent_link) {
                    continue;
                }
                // Skip if already processed
                if processed.contains(&joint.child_link) {
                    continue;
                }

                let parent_transform = if let Some(&parent_idx) = self.link_map.get(&joint.parent_link) {
                    self.link_transforms[parent_idx]
                } else {
                    root_offset
                };

                // Compute rotation exactly like rerun:
                // rotation = origin_rotation * joint_rotation
                let origin_rotation = glam::Quat::from_euler(
                    glam::EulerRot::XYZ,
                    joint.origin_rpy.x,
                    joint.origin_rpy.y,
                    joint.origin_rpy.z,
                );

                let joint_rotation = glam::Quat::from_axis_angle(joint.axis, joint.angle);

                // Combined rotation
                let rotation = origin_rotation * joint_rotation;

                // Joint transform: translation * rotation
                // DEBUG: Try translation only to test if positions stack correctly
                // let joint_transform = glam::Mat4::from_translation(joint.origin_xyz);  // no rotation
                let joint_transform = glam::Mat4::from_rotation_translation(rotation, joint.origin_xyz);

                // Child global = parent * joint_transform
                let child_transform = parent_transform * joint_transform;

                if let Some(&child_idx) = self.link_map.get(&joint.child_link) {
                    self.link_transforms[child_idx] = child_transform;
                }

                processed.insert(joint.child_link.clone());
            }
        }
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct URDFViewer {
    #[deref] view: View,
    #[live] draw_mesh: DrawMesh,

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
    /// Original mesh data for each link (for CPU-side transforms)
    #[rust] original_meshes: Vec<MeshData>,
}

impl URDFViewer {
    fn update_status(&mut self, cx: &mut Cx) {
        let text = if let Some(ref robot) = self.robot {
            if let Some((name, angle, lower, upper)) = robot.get_joint_info(self.selected_joint) {
                format!(
                    "Joint {}: {} = {:.2} rad ({:.1}°) [{:.1}° to {:.1}°]",
                    self.selected_joint, name, angle, angle.to_degrees(),
                    lower.to_degrees(), upper.to_degrees()
                )
            } else {
                "No joints".to_string()
            }
        } else {
            "Loading robot...".to_string()
        };
        self.view.label(id!(status_bar.joint_label)).set_text(cx, &text);
    }
}

impl Widget for URDFViewer {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        // Animation timer - exactly like rerun example
        if self.anim_timer.is_event(event).is_some() && self.animating {
            self.anim_step += 1;
            if let Some(ref mut robot) = self.robot {
                for (joint_idx, joint) in robot.joints.iter_mut().enumerate() {
                    // Same animation formula as rerun example
                    let step = self.anim_step as f64;
                    let dynamic_angle = remap(
                        (step * (0.02 + joint_idx as f64 / 100.0)).sin(),
                        -1.0, 1.0,
                        joint.limit_lower as f64, joint.limit_upper as f64,
                    ) as f32;
                    joint.angle = dynamic_angle;
                }
            }
            self.update_status(cx);
            self.redraw(cx);
        }

        // Keyboard
        if let Event::KeyDown(ke) = event {
            let num_joints = self.robot.as_ref().map(|r| r.num_joints()).unwrap_or(6);
            match ke.key_code {
                KeyCode::ArrowUp => {
                    if let Some(ref mut robot) = self.robot {
                        let current = robot.joints.get(self.selected_joint).map(|j| j.angle).unwrap_or(0.0);
                        robot.set_joint_angle(self.selected_joint, current + 0.15);
                    }
                    self.update_status(cx);
                    self.redraw(cx);
                }
                KeyCode::ArrowDown => {
                    if let Some(ref mut robot) = self.robot {
                        let current = robot.joints.get(self.selected_joint).map(|j| j.angle).unwrap_or(0.0);
                        robot.set_joint_angle(self.selected_joint, current - 0.15);
                    }
                    self.update_status(cx);
                    self.redraw(cx);
                }
                KeyCode::ArrowLeft => {
                    if num_joints > 0 {
                        self.selected_joint = (self.selected_joint + num_joints - 1) % num_joints;
                    }
                    self.update_status(cx);
                    self.redraw(cx);
                }
                KeyCode::ArrowRight => {
                    if num_joints > 0 {
                        self.selected_joint = (self.selected_joint + 1) % num_joints;
                    }
                    self.update_status(cx);
                    self.redraw(cx);
                }
                KeyCode::KeyA => {
                    self.animating = !self.animating;
                    if self.animating {
                        self.anim_timer = cx.start_interval(0.033);
                    } else {
                        self.anim_timer = Timer::default();
                    }
                }
                KeyCode::KeyR => {
                    self.camera_distance = 1.0;
                    self.camera_yaw = 0.5;
                    self.camera_pitch = 0.3;
                    if let Some(ref mut robot) = self.robot {
                        for joint in &mut robot.joints {
                            joint.angle = 0.0;
                        }
                    }
                    self.update_status(cx);
                    self.redraw(cx);
                }
                _ => {}
            }
        }

        // Mouse for camera
        let viewport_area = self.view.view(id!(viewport)).area();
        match event.hits(cx, viewport_area) {
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
                self.camera_distance *= 1.0 - se.scroll.y * 0.001;
                self.camera_distance = self.camera_distance.clamp(0.5, 5.0);
                self.redraw(cx);
            }
            _ => {}
        }

        // Buttons
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });
        if self.view.button(id!(header.reset_btn)).clicked(&actions) {
            self.camera_distance = 1.0;
            self.camera_yaw = 0.5;
            self.camera_pitch = 0.3;
            if let Some(ref mut robot) = self.robot {
                for joint in &mut robot.joints {
                    joint.angle = 0.0;
                }
            }
            self.update_status(cx);
            self.redraw(cx);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if !self.initialized {
            self.initialized = true;
            self.camera_distance = 1.0;
            self.camera_yaw = 0.5;
            self.camera_pitch = 0.2;

            let urdf_path = "data/so100.urdf";
            let assets_dir = "data/assets";

            match Robot::from_urdf(urdf_path, assets_dir) {
                Ok(mut robot) => {
                    // Create DrawMesh for each link
                    for (_idx, link) in robot.links.iter().enumerate() {
                        if let Some(ref mesh_data) = link.mesh_data {
                            self.original_meshes.push(mesh_data.clone());
                            let mut draw = DrawMesh::new_for_link(cx.cx, mesh_data.clone(), &self.draw_mesh);
                            draw.init_link_geometry(cx.cx);
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
            self.update_status(cx.cx);
        }

        // Draw the view first to get layout
        let _ = self.view.draw_walk(cx, scope, walk);


        if let Some(ref mut robot) = self.robot {
            robot.update_forward_kinematics();

            let cam_yaw = self.camera_yaw as f32;
            let cam_pitch = self.camera_pitch as f32;

            // Z-up to Y-up conversion + orbital camera
            let base_rot = glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            let orbital_rot = glam::Mat4::from_euler(glam::EulerRot::YXZ, cam_yaw, cam_pitch, 0.0);
            let camera_rot = orbital_rot * base_rot;

            // apply_transform in mesh.rs expects column-major (same as glam)
            fn glam_to_makepad(m: glam::Mat4) -> Mat4 {
                Mat4 { v: m.to_cols_array() }
            }

            let robot_color = vec4(1.0, 0.65, 0.1, 1.0);  // Orange like rerun

            // TEST: Set to false to draw all meshes at origin (no FK)
            let use_fk = true;

            let mut drawer_idx = 0;
            for (link_idx, link) in robot.links.iter().enumerate() {
                if link.mesh_data.is_none() { continue; }
                if drawer_idx >= self.link_drawers.len() { break; }

                let transform = if use_fk {
                    let link_transform = robot.link_transforms[link_idx];
                    glam_to_makepad(camera_rot * link_transform)
                } else {
                    // No FK - all meshes at origin
                    glam_to_makepad(camera_rot)
                };

                let drawer = &mut self.link_drawers[drawer_idx];
                drawer.update_transformed_geometry(cx.cx, &self.original_meshes[drawer_idx], &transform);
                drawer.color = robot_color;

                drawer.begin_many_instances(cx);
                drawer.draw(cx);
                drawer.end_many_instances(cx);

                drawer_idx += 1;
            }
        }

        DrawStep::done()
    }
}

/// Remap value from one range to another (like emath::remap)
fn remap(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    let t = (value - from_min) / (from_max - from_min);
    to_min + t * (to_max - to_min)
}

#[derive(Live, LiveHook)]
pub struct App {
    #[live] ui: WidgetRef,
}

impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        makepad_widgets::live_design(cx);
        crate::mesh::live_design(cx);
    }
}

impl MatchEvent for App {}

impl AppMain for App {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

app_main!(App);

fn main() {
    app_main();
}
