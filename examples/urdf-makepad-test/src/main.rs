//! 3D Robot Viewer with STL Mesh Rendering
//!
//! Controls:
//!   - Mouse drag: Orbit camera
//!   - Scroll: Zoom
//!   - Arrow keys: Control joints (Left/Right to select, Up/Down to adjust)
//!   - A: Toggle animation
//!   - R: Reset to default pose

use makepad_widgets::*;
use makepad_widgets::shader::draw_cube::DrawCube;

mod mesh;
mod robot;

use mesh::DrawMesh;
use robot::Robot;

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
                text: "3D Robot Viewer - Arrow keys control joints, A=animate"
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
            draw_bg: { color: #1e1e28 }
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

        draw_cube: {
            color: #d4a017
        }
        draw_mesh: {
            color: #e07020
        }
    }

    App = {{App}} {
        ui: <Window> {
            window: { title: "3D Robot Viewer" }
            show_bg: true
            draw_bg: { color: #1a1a1f }
            body = <URDFViewer> {}
        }
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct URDFViewer {
    #[deref] view: View,
    #[live] draw_cube: DrawCube,
    #[live] draw_mesh: DrawMesh,

    #[rust] camera_distance: f64,
    #[rust] camera_yaw: f64,
    #[rust] camera_pitch: f64,
    #[rust] is_dragging: bool,
    #[rust] last_mouse: DVec2,

    #[rust] selected_joint: usize,

    #[rust] animating: bool,
    #[rust] anim_timer: Timer,
    #[rust] time: f64,
    #[rust] initialized: bool,

    /// The loaded robot with URDF structure
    #[rust] robot: Option<Robot>,
    /// DrawMesh instances for each link (one per link)
    #[rust] link_drawers: Vec<DrawMesh>,
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

        // Animation timer
        if self.anim_timer.is_event(event).is_some() && self.animating {
            self.time += 0.03;
            if let Some(ref mut robot) = self.robot {
                for i in 0..robot.num_joints() {
                    let phase = i as f64 * 0.8;
                    let angle = ((self.time + phase).sin() * 0.6) as f32;
                    robot.set_joint_angle_by_index(i, angle);
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
                        let current = robot.get_joint_angle_by_index(self.selected_joint);
                        robot.set_joint_angle_by_index(self.selected_joint, current + 0.15);
                    }
                    self.update_status(cx);
                    self.redraw(cx);
                }
                KeyCode::ArrowDown => {
                    if let Some(ref mut robot) = self.robot {
                        let current = robot.get_joint_angle_by_index(self.selected_joint);
                        robot.set_joint_angle_by_index(self.selected_joint, current - 0.15);
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
                        for i in 0..robot.num_joints() {
                            robot.set_joint_angle_by_index(i, 0.0);
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
                for i in 0..robot.num_joints() {
                    robot.set_joint_angle_by_index(i, 0.0);
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
            self.camera_pitch = 0.3;

            // Load robot from URDF
            let urdf_path = "/Users/yuechen/home/dorobot/examples/urdf-sample/so100.urdf";
            let assets_dir = "/Users/yuechen/home/dorobot/examples/urdf-sample/assets";

            match Robot::from_urdf(urdf_path, assets_dir) {
                Ok(mut robot) => {
                    eprintln!("Loaded robot: {} with {} links, {} joints",
                        robot.name, robot.links.len(), robot.joints.len());

                    // Create DrawMesh instances for each link with mesh data
                    for (idx, link) in robot.links.iter().enumerate() {
                        if let Some(ref mesh_data) = link.mesh_data {
                            let mut draw = DrawMesh::new_for_link(cx.cx, mesh_data.clone(), &self.draw_mesh);
                            draw.init_link_geometry(cx.cx);
                            eprintln!("  Link {}: {} - {} vertices",
                                idx, link.name, mesh_data.vertices.len() / 9);
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

        // Draw UI
        let _ = self.view.draw_walk(cx, scope, walk);

        // Draw robot links with joint transforms
        if let Some(ref mut robot) = self.robot {
            // Update forward kinematics
            robot.update_forward_kinematics();

            let cam_yaw = self.camera_yaw as f32;
            let cam_pitch = self.camera_pitch as f32;
            let camera_rot = Mat4::rotation(vec3(cam_pitch, cam_yaw, 0.0));

            // Link colors for visibility
            let colors = [
                vec4(0.2, 0.5, 1.0, 1.0),   // base - blue
                vec4(1.0, 0.9, 0.2, 1.0),   // shoulder - yellow
                vec4(1.0, 0.85, 0.1, 1.0),  // upper_arm - yellow
                vec4(1.0, 0.8, 0.0, 1.0),   // lower_arm - yellow
                vec4(1.0, 0.75, 0.1, 1.0),  // wrist - yellow
                vec4(1.0, 0.2, 0.2, 1.0),   // gripper - red
                vec4(0.9, 0.1, 0.1, 1.0),   // jaw - red
            ];

            // Track which drawer to use (only links with meshes have drawers)
            let mut drawer_idx = 0;

            // Draw each link
            for (link_idx, link) in robot.links.iter().enumerate() {
                if link.mesh_data.is_none() {
                    continue;
                }

                if drawer_idx >= self.link_drawers.len() {
                    break;
                }

                // Compute final transform: camera rotation * link's global transform
                let transform = Mat4::mul(&camera_rot, &link.global_transform);

                // Get drawer for this link
                let drawer = &mut self.link_drawers[drawer_idx];

                // Set color based on link index
                drawer.color = colors.get(link_idx).copied().unwrap_or(vec4(0.5, 0.5, 0.5, 1.0));
                drawer.transform = transform;
                drawer.depth_clip = 1.0;
                drawer.draw(cx);

                drawer_idx += 1;
            }
        }

        DrawStep::done()
    }
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
