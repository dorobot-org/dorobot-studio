//! SO-ARM100 URDF Viewer
//!
//! A sample application demonstrating URDF robot visualization using kiss3d.
//! This serves as a proof-of-concept for integrating URDF visualization into DoRobot.
//!
//! Usage:
//!   cargo run --release
//!   cargo run --release -- --urdf path/to/robot.urdf
//!
//! Controls:
//!   - Left mouse drag: Rotate camera
//!   - Right mouse drag: Pan camera
//!   - Scroll: Zoom in/out
//!   - Arrow keys: Adjust selected joint
//!   - Tab: Select next joint
//!   - R: Reset joint positions
//!   - Q/Escape: Quit

use std::path::Path;
use std::collections::HashMap;

use clap::Parser;
use kiss3d::camera::ArcBall;
use kiss3d::light::Light;
use kiss3d::window::Window;
use kiss3d::scene::SceneNode;
use kiss3d::nalgebra::{Point3, Vector3, UnitQuaternion, Translation3, Isometry3};

// Use kiss3d's nalgebra to avoid version conflicts
use kiss3d::nalgebra as na;

#[derive(Parser, Debug)]
#[command(name = "urdf-sample")]
#[command(about = "SO-ARM100 URDF Viewer using kiss3d")]
struct Args {
    /// Path to URDF file
    #[arg(short, long, default_value = "so100.urdf")]
    urdf: String,
}

/// Represents a robot joint with its current position and limits
struct Joint {
    name: String,
    scene_node: SceneNode,
    axis: Vector3<f32>,
    origin: Isometry3<f32>,
    lower_limit: f32,
    upper_limit: f32,
    current_position: f32,
}

/// Robot model loaded from URDF
struct Robot {
    _root: SceneNode,
    joints: Vec<Joint>,
    link_nodes: HashMap<String, SceneNode>,
}

impl Robot {
    /// Load robot from URDF file
    fn from_urdf(window: &mut Window, urdf_path: &str) -> Result<Self, String> {
        let base_path = Path::new(urdf_path).parent().unwrap_or(Path::new("."));

        // Parse URDF
        let urdf = urdf_rs::read_file(urdf_path)
            .map_err(|e| format!("Failed to parse URDF: {}", e))?;

        log::info!("Loaded URDF: {} with {} links, {} joints",
            urdf.name, urdf.links.len(), urdf.joints.len());

        // Create root node
        let mut root = window.add_group();

        // Store link nodes for joint attachment
        let mut link_nodes: HashMap<String, SceneNode> = HashMap::new();

        // First pass: create all link nodes with their visual meshes
        for link in &urdf.links {
            let mut link_node = root.add_group();

            // Add visual geometries
            for visual in &link.visual {
                let mut visual_node = match &visual.geometry {
                    urdf_rs::Geometry::Mesh { filename, scale } => {
                        let mesh_path = base_path.join(filename);
                        log::debug!("Loading mesh: {:?}", mesh_path);

                        let scale = scale.as_ref().map(|s| Vector3::new(s[0] as f32, s[1] as f32, s[2] as f32))
                            .unwrap_or(Vector3::new(1.0, 1.0, 1.0));

                        if let Ok(node) = load_mesh(&mut link_node, &mesh_path, scale) {
                            node
                        } else {
                            log::warn!("Failed to load mesh: {:?}, using cube placeholder", mesh_path);
                            link_node.add_cube(0.02, 0.02, 0.02)
                        }
                    }
                    urdf_rs::Geometry::Box { size } => {
                        link_node.add_cube(size[0] as f32, size[1] as f32, size[2] as f32)
                    }
                    urdf_rs::Geometry::Cylinder { radius, length } => {
                        link_node.add_cylinder(*radius as f32, *length as f32)
                    }
                    urdf_rs::Geometry::Sphere { radius } => {
                        link_node.add_sphere(*radius as f32)
                    }
                    urdf_rs::Geometry::Capsule { radius, length } => {
                        // Approximate capsule with cylinder
                        link_node.add_cylinder(*radius as f32, *length as f32)
                    }
                };

                // Apply visual origin transform
                let origin = &visual.origin;
                let translation = Translation3::new(
                    origin.xyz[0] as f32,
                    origin.xyz[1] as f32,
                    origin.xyz[2] as f32,
                );
                let rotation = UnitQuaternion::from_euler_angles(
                    origin.rpy[0] as f32,
                    origin.rpy[1] as f32,
                    origin.rpy[2] as f32,
                );
                visual_node.set_local_translation(translation);
                visual_node.set_local_rotation(rotation);

                // Apply material color
                if let Some(material) = &visual.material {
                    if let Some(color) = &material.color {
                        visual_node.set_color(
                            color.rgba[0] as f32,
                            color.rgba[1] as f32,
                            color.rgba[2] as f32,
                        );
                    }
                }
            }

            link_nodes.insert(link.name.clone(), link_node);
        }

        // Second pass: set up joint hierarchy and transforms
        let mut joints = Vec::new();

        for joint in &urdf.joints {
            let parent_name = &joint.parent.link;
            let child_name = &joint.child.link;

            log::debug!("Joint {}: {} -> {} (type: {:?})",
                joint.name, parent_name, child_name, joint.joint_type);

            // Check parent exists, then get child node
            let parent_exists = link_nodes.contains_key(parent_name);
            let child_node = link_nodes.get_mut(child_name);

            if parent_exists {
            if let Some(child) = child_node {
                // Apply joint origin transform to child
                let origin = &joint.origin;
                let translation = Translation3::new(
                    origin.xyz[0] as f32,
                    origin.xyz[1] as f32,
                    origin.xyz[2] as f32,
                );
                let rotation = UnitQuaternion::from_euler_angles(
                    origin.rpy[0] as f32,
                    origin.rpy[1] as f32,
                    origin.rpy[2] as f32,
                );

                let origin_isometry = Isometry3::from_parts(translation, rotation);

                // For revolute/continuous joints, store joint info
                match joint.joint_type {
                    urdf_rs::JointType::Revolute | urdf_rs::JointType::Continuous => {
                        let axis = Vector3::new(
                            joint.axis.xyz[0] as f32,
                            joint.axis.xyz[1] as f32,
                            joint.axis.xyz[2] as f32,
                        );

                        let lower = joint.limit.lower as f32;
                        let upper = joint.limit.upper as f32;

                        joints.push(Joint {
                            name: joint.name.clone(),
                            scene_node: child.clone(),
                            axis,
                            origin: origin_isometry,
                            lower_limit: lower,
                            upper_limit: upper,
                            current_position: 0.0,
                        });
                    }
                    _ => {
                        // Fixed joint - just apply transform
                        child.set_local_translation(translation);
                        child.set_local_rotation(rotation);
                    }
                }
            }
            }
        }

        log::info!("Created {} controllable joints", joints.len());
        for (i, joint) in joints.iter().enumerate() {
            log::info!("  [{}] {}: limits [{:.2}, {:.2}]",
                i, joint.name, joint.lower_limit, joint.upper_limit);
        }

        Ok(Robot {
            _root: root,
            joints,
            link_nodes,
        })
    }

    /// Set joint position (clamped to limits)
    fn set_joint_position(&mut self, joint_idx: usize, position: f32) {
        if let Some(joint) = self.joints.get_mut(joint_idx) {
            let clamped = position.clamp(joint.lower_limit, joint.upper_limit);
            joint.current_position = clamped;

            // Apply rotation around joint axis
            let rotation = UnitQuaternion::from_axis_angle(
                &na::Unit::new_normalize(joint.axis),
                clamped,
            );

            // Combine with origin transform
            let transform = joint.origin * Isometry3::from_parts(
                Translation3::identity(),
                rotation,
            );

            joint.scene_node.set_local_translation(Translation3::from(transform.translation.vector));
            joint.scene_node.set_local_rotation(transform.rotation);
        }
    }

    /// Get current joint positions
    fn get_joint_positions(&self) -> Vec<f32> {
        self.joints.iter().map(|j| j.current_position).collect()
    }

    /// Reset all joints to zero
    fn reset_joints(&mut self) {
        for i in 0..self.joints.len() {
            self.set_joint_position(i, 0.0);
        }
    }

    /// Number of joints
    fn num_joints(&self) -> usize {
        self.joints.len()
    }

    /// Get joint name
    fn joint_name(&self, idx: usize) -> &str {
        self.joints.get(idx).map(|j| j.name.as_str()).unwrap_or("unknown")
    }
}

/// Load STL mesh into scene
fn load_mesh(parent: &mut SceneNode, path: &Path, scale: Vector3<f32>) -> Result<SceneNode, String> {
    use mesh_loader::Loader;

    let loader = Loader::default();
    let scene = loader.load(path)
        .map_err(|e| format!("Failed to load mesh: {}", e))?;

    if scene.meshes.is_empty() {
        return Err("No meshes found in file".to_string());
    }

    let mesh = &scene.meshes[0];

    // Convert vertices
    let vertices: Vec<Point3<f32>> = mesh.vertices.iter()
        .map(|v| Point3::new(v[0] * scale.x, v[1] * scale.y, v[2] * scale.z))
        .collect();

    // Convert faces (mesh-loader uses u32 indices)
    let faces: Vec<Point3<u16>> = mesh.faces.iter()
        .filter_map(|f| {
            if f.len() >= 3 {
                Some(Point3::new(f[0] as u16, f[1] as u16, f[2] as u16))
            } else {
                None
            }
        })
        .collect();

    if vertices.is_empty() || faces.is_empty() {
        return Err("Empty mesh".to_string());
    }

    // Create kiss3d mesh
    let mesh = kiss3d::resource::Mesh::new(vertices, faces, None, None, false);
    let node = parent.add_mesh(std::rc::Rc::new(std::cell::RefCell::new(mesh)), Vector3::new(1.0, 1.0, 1.0));

    Ok(node)
}

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    log::info!("Starting URDF viewer...");
    log::info!("URDF file: {}", args.urdf);

    // Create window
    let mut window = Window::new("SO-ARM100 URDF Viewer");
    window.set_background_color(0.1, 0.1, 0.15);
    window.set_light(Light::StickToCamera);

    // Set up camera
    let eye = Point3::new(0.3, 0.3, 0.5);
    let target = Point3::new(0.0, 0.15, 0.0);
    let mut camera = ArcBall::new(eye, target);

    // Load robot
    let mut robot = match Robot::from_urdf(&mut window, &args.urdf) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to load robot: {}", e);
            log::error!("Make sure the URDF file and mesh assets are in the correct location.");
            log::error!("Expected: {} with assets/ folder containing STL files", args.urdf);
            std::process::exit(1);
        }
    };

    // Add ground plane
    let mut ground = window.add_quad(2.0, 2.0, 1, 1);
    ground.set_local_rotation(UnitQuaternion::from_axis_angle(
        &Vector3::x_axis(),
        -std::f32::consts::FRAC_PI_2,
    ));
    ground.set_color(0.3, 0.3, 0.35);

    // Add coordinate axes
    let axis_length = 0.1;
    let mut x_axis = window.add_cylinder(0.002, axis_length);
    x_axis.set_local_rotation(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), std::f32::consts::FRAC_PI_2));
    x_axis.set_local_translation(Translation3::new(axis_length / 2.0, 0.0, 0.0));
    x_axis.set_color(1.0, 0.0, 0.0);

    let mut y_axis = window.add_cylinder(0.002, axis_length);
    y_axis.set_local_translation(Translation3::new(0.0, axis_length / 2.0, 0.0));
    y_axis.set_color(0.0, 1.0, 0.0);

    let mut z_axis = window.add_cylinder(0.002, axis_length);
    z_axis.set_local_rotation(UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f32::consts::FRAC_PI_2));
    z_axis.set_local_translation(Translation3::new(0.0, 0.0, axis_length / 2.0));
    z_axis.set_color(0.0, 0.0, 1.0);

    // Joint control state
    let mut selected_joint: usize = 0;
    let joint_step = 0.05_f32;

    // Animation state
    let mut time: f32 = 0.0;
    let mut animate = false;

    log::info!("");
    log::info!("Controls:");
    log::info!("  Mouse drag: Rotate camera");
    log::info!("  Scroll: Zoom");
    log::info!("  Up/Down: Adjust selected joint");
    log::info!("  Tab: Select next joint");
    log::info!("  Shift+Tab: Select previous joint");
    log::info!("  A: Toggle animation");
    log::info!("  R: Reset joints");
    log::info!("  Q/Escape: Quit");
    log::info!("");
    log::info!("Selected joint [{}]: {}", selected_joint, robot.joint_name(selected_joint));

    // Main loop
    while window.render_with_camera(&mut camera) {
        // Handle keyboard input
        for event in window.events().iter() {
            match event.value {
                kiss3d::event::WindowEvent::Key(key, kiss3d::event::Action::Press, modifiers) => {
                    match key {
                        kiss3d::event::Key::Up => {
                            let pos = robot.get_joint_positions()[selected_joint];
                            robot.set_joint_position(selected_joint, pos + joint_step);
                            log::info!("Joint [{}] {}: {:.3}", selected_joint,
                                robot.joint_name(selected_joint),
                                robot.get_joint_positions()[selected_joint]);
                        }
                        kiss3d::event::Key::Down => {
                            let pos = robot.get_joint_positions()[selected_joint];
                            robot.set_joint_position(selected_joint, pos - joint_step);
                            log::info!("Joint [{}] {}: {:.3}", selected_joint,
                                robot.joint_name(selected_joint),
                                robot.get_joint_positions()[selected_joint]);
                        }
                        kiss3d::event::Key::Tab => {
                            if modifiers.contains(kiss3d::event::Modifiers::Shift) {
                                selected_joint = (selected_joint + robot.num_joints() - 1) % robot.num_joints();
                            } else {
                                selected_joint = (selected_joint + 1) % robot.num_joints();
                            }
                            log::info!("Selected joint [{}]: {}", selected_joint, robot.joint_name(selected_joint));
                        }
                        kiss3d::event::Key::R => {
                            robot.reset_joints();
                            log::info!("Reset all joints");
                        }
                        kiss3d::event::Key::A => {
                            animate = !animate;
                            log::info!("Animation: {}", if animate { "ON" } else { "OFF" });
                        }
                        kiss3d::event::Key::Q | kiss3d::event::Key::Escape => {
                            log::info!("Quitting...");
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Animate joints if enabled
        if animate {
            time += 0.016; // ~60fps
            for i in 0..robot.num_joints() {
                let phase = i as f32 * 0.5;
                let amplitude = 0.5;
                let pos = (time + phase).sin() * amplitude;
                robot.set_joint_position(i, pos);
            }
        }
    }
}
