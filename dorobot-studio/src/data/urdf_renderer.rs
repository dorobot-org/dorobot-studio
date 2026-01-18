//! URDF Robot Renderer
//!
//! Background thread that renders URDF robot models using kiss3d
//! and provides texture data for display in Makepad widgets.

use std::sync::{Arc, Mutex};

#[cfg(feature = "urdf")]
use std::path::Path;
#[cfg(feature = "urdf")]
use std::collections::HashMap;

#[cfg(feature = "urdf")]
use kiss3d::camera::ArcBall;
#[cfg(feature = "urdf")]
use kiss3d::light::Light;
#[cfg(feature = "urdf")]
use kiss3d::window::Window;
#[cfg(feature = "urdf")]
use kiss3d::scene::SceneNode;
#[cfg(feature = "urdf")]
use kiss3d::nalgebra::{Point3, Vector3, UnitQuaternion, Translation3, Isometry3};

/// Camera control commands sent from UI to renderer
#[derive(Clone, Debug)]
pub enum CameraCommand {
    /// Orbit camera around target (left mouse drag)
    Orbit { dx: f32, dy: f32 },
    /// Pan camera (right mouse drag)
    Pan { dx: f32, dy: f32 },
    /// Zoom in/out (scroll wheel)
    Zoom { delta: f32 },
    /// Reset camera to default view
    Reset,
}

/// Rendered frame data to send back to UI
#[derive(Clone)]
pub struct RenderedFrame {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>, // RGBA pixels
}

/// Shared state between UI thread and renderer thread
pub struct URDFRendererState {
    /// Path to URDF file to load
    pub urdf_path: Option<String>,
    /// Joint positions to apply
    pub joint_positions: Vec<f64>,
    /// Pending camera commands
    pub camera_commands: Vec<CameraCommand>,
    /// Latest rendered frame
    pub rendered_frame: Option<RenderedFrame>,
    /// Whether renderer is ready
    pub is_ready: bool,
    /// Error message if loading failed
    pub error: Option<String>,
    /// Joint names for UI display
    pub joint_names: Vec<String>,
}

impl Default for URDFRendererState {
    fn default() -> Self {
        Self {
            urdf_path: None,
            joint_positions: Vec::new(),
            camera_commands: Vec::new(),
            rendered_frame: None,
            is_ready: false,
            error: None,
            joint_names: Vec::new(),
        }
    }
}

/// Thread-safe handle to renderer state
pub type SharedRendererState = Arc<Mutex<URDFRendererState>>;

/// Create a new shared renderer state
pub fn create_shared_state() -> SharedRendererState {
    Arc::new(Mutex::new(URDFRendererState::default()))
}

/// Joint information
#[cfg(feature = "urdf")]
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
#[cfg(feature = "urdf")]
struct Robot {
    _root: SceneNode,
    joints: Vec<Joint>,
}

#[cfg(feature = "urdf")]
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

                        let scale = scale.as_ref()
                            .map(|s| Vector3::new(s[0] as f32, s[1] as f32, s[2] as f32))
                            .unwrap_or(Vector3::new(1.0, 1.0, 1.0));

                        if let Ok(node) = Self::load_mesh(&mut link_node, &mesh_path, scale) {
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

        Ok(Robot {
            _root: root,
            joints,
        })
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

        // Convert faces
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

        let mesh = kiss3d::resource::Mesh::new(vertices, faces, None, None, false);
        let node = parent.add_mesh(
            std::rc::Rc::new(std::cell::RefCell::new(mesh)),
            Vector3::new(1.0, 1.0, 1.0)
        );

        Ok(node)
    }

    /// Set joint position (clamped to limits)
    fn set_joint_position(&mut self, joint_idx: usize, position: f32) {
        use kiss3d::nalgebra as na;

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

    /// Get joint names
    fn joint_names(&self) -> Vec<String> {
        self.joints.iter().map(|j| j.name.clone()).collect()
    }

    /// Number of joints
    fn num_joints(&self) -> usize {
        self.joints.len()
    }
}

/// Start the URDF renderer in a background thread
#[cfg(feature = "urdf")]
pub fn start_renderer_thread(state: SharedRendererState) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        run_renderer_loop(state);
    })
}

/// Stub for when urdf feature is not enabled
#[cfg(not(feature = "urdf"))]
pub fn start_renderer_thread(_state: SharedRendererState) -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {
        log::warn!("URDF feature not enabled. Build with --features urdf");
    })
}

/// Main renderer loop
#[cfg(feature = "urdf")]
fn run_renderer_loop(state: SharedRendererState) {
    use makepad_widgets::SignalToUI;

    // Create hidden window for offscreen rendering
    let mut window = Window::new_hidden("URDF Renderer");
    window.set_background_color(0.15, 0.15, 0.2);
    window.set_light(Light::StickToCamera);

    // Set up camera
    let eye = Point3::new(0.3, 0.3, 0.5);
    let target = Point3::new(0.0, 0.15, 0.0);
    let mut camera = ArcBall::new(eye, target);

    let mut robot: Option<Robot> = None;
    let mut current_urdf_path: Option<String> = None;

    // Render size
    let render_width = 640;
    let render_height = 480;

    loop {
        // Check for new URDF to load
        let urdf_to_load = {
            let state = state.lock().unwrap();
            if state.urdf_path != current_urdf_path {
                state.urdf_path.clone()
            } else {
                None
            }
        };

        if let Some(path) = urdf_to_load {
            current_urdf_path = Some(path.clone());

            match Robot::from_urdf(&mut window, &path) {
                Ok(r) => {
                    let joint_names = r.joint_names();
                    robot = Some(r);

                    let mut state = state.lock().unwrap();
                    state.is_ready = true;
                    state.error = None;
                    state.joint_names = joint_names;
                }
                Err(e) => {
                    log::error!("Failed to load URDF: {}", e);
                    robot = None;

                    let mut state = state.lock().unwrap();
                    state.is_ready = false;
                    state.error = Some(e);
                }
            }
        }

        // Process camera commands and joint positions
        {
            let mut state = state.lock().unwrap();

            // Apply camera commands using ArcBall's yaw/pitch interface
            for cmd in state.camera_commands.drain(..) {
                match cmd {
                    CameraCommand::Orbit { dx, dy } => {
                        // ArcBall uses yaw/pitch for rotation
                        let current_yaw = camera.yaw();
                        let current_pitch = camera.pitch();
                        camera.set_yaw(current_yaw + dx * 0.01);
                        camera.set_pitch(current_pitch + dy * 0.01);
                    }
                    CameraCommand::Zoom { delta } => {
                        let new_dist = camera.dist() * (1.0 - delta * 0.1);
                        camera.set_dist(new_dist.max(0.1));
                    }
                    CameraCommand::Pan { dx, dy } => {
                        // Simple pan by adjusting the at point
                        let current_at = camera.at();
                        camera.set_at(Point3::new(
                            current_at.x + dx * 0.005,
                            current_at.y - dy * 0.005,
                            current_at.z
                        ));
                    }
                    CameraCommand::Reset => {
                        camera.look_at(
                            Point3::new(0.3, 0.3, 0.5),
                            Point3::new(0.0, 0.15, 0.0)
                        );
                    }
                }
            }

            // Apply joint positions
            if let Some(ref mut r) = robot {
                for (i, &pos) in state.joint_positions.iter().enumerate() {
                    if i < r.num_joints() {
                        r.set_joint_position(i, pos as f32);
                    }
                }
            }
        }

        // Render frame
        if robot.is_some() {
            // Render one frame to the window
            window.render_with_camera(&mut camera);

            // Use snap() to capture the current frame
            let mut pixels = Vec::new();
            window.snap(&mut pixels);

            if !pixels.is_empty() {
                // Get window size for dimensions
                let size = window.size();
                let width = size.x as usize;
                let height = size.y as usize;

                // snap() returns RGB data, convert to RGBA
                let rgba: Vec<u8> = pixels.chunks(3)
                    .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
                    .collect();

                let frame = RenderedFrame {
                    width,
                    height,
                    data: rgba,
                };

                // Update shared state
                {
                    let mut state = state.lock().unwrap();
                    state.rendered_frame = Some(frame);
                }

                // Signal UI to update
                SignalToUI::set_ui_signal();
            }
        }

        // Small sleep to limit frame rate (~30 fps)
        std::thread::sleep(std::time::Duration::from_millis(33));
    }
}
