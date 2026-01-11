//! 3D Robot Arm Viewer Widget

use makepad_widgets::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;

    // Individual joint angle display - MUST be defined before RobotViewer
    JointDisplay = <View> {
        width: 60
        height: Fit
        flow: Down
        align: { x: 0.5 }

        name = <Label> {
            draw_text: {
                text_style: <TEXT_SMALL> {}
                color: (COLOR_TEXT_SECONDARY)
            }
            text: "J0"
        }

        value = <Label> {
            draw_text: {
                text_style: <TEXT_MONO> {}
                color: (COLOR_TEXT_PRIMARY)
            }
            text: "0.00"
        }
    }

    // Robot arm viewer widget
    pub RobotViewer = {{RobotViewer}} {
        width: Fill
        height: Fill

        flow: Down

        // Header with view controls
        header = <View> {
            width: Fill
            height: 32
            padding: { left: 8, right: 8 }
            spacing: 8
            align: { y: 0.5 }

            show_bg: true
            draw_bg: { color: (COLOR_BG_HEADER) }

            title = <Label> {
                draw_text: {
                    text_style: <TEXT_PANEL_TITLE> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
                text: "Robot 3D View"
            }

            <View> { width: Fill }

            // View controls
            reset_view_btn = <SecondaryButton> {
                text: "Reset View"
                width: 70
                height: 20
                padding: { left: 8, right: 8 }
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
            }

            show_trajectory_btn = <SecondaryButton> {
                text: "Trail"
                width: 40
                height: 20
                padding: { left: 6, right: 6 }
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
            }

            show_axes_btn = <SecondaryButton> {
                text: "Axes"
                width: 40
                height: 20
                padding: { left: 6, right: 6 }
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
            }
        }

        // 3D viewport
        viewport = <View> {
            width: Fill
            height: Fill
            cursor: Hand

            show_bg: true
            draw_bg: { color: #1a1a22 }

            // Grid floor placeholder
            grid_floor = <View> {
                width: Fill
                height: Fill
            }

            // Robot arm rendering would go here
            // In a full implementation, this would use DrawCube instances
            // for each robot link, transformed by forward kinematics

            // Placeholder message when no robot loaded
            placeholder = <View> {
                width: Fit
                height: Fit
                align: { x: 0.5, y: 0.5 }

                placeholder_label = <Label> {
                    draw_text: {
                        text_style: <TEXT_BODY> {}
                        color: (COLOR_TEXT_MUTED)
                        wrap: Word
                    }
                    text: "Load URDF to visualize robot"
                }
            }
        }

        // Joint angles display (bottom overlay)
        joint_info = <View> {
            width: Fill
            height: 60
            padding: 8
            flow: Right
            spacing: 16

            show_bg: true
            draw_bg: { color: (COLOR_BG_PANEL) }

            // Joint angle readouts - names are set programmatically
            joint_0 = <JointDisplay> {}
            joint_1 = <JointDisplay> {}
            joint_2 = <JointDisplay> {}
            joint_3 = <JointDisplay> {}
            joint_4 = <JointDisplay> {}
            joint_5 = <JointDisplay> {}
            joint_6 = <JointDisplay> {}
        }
    }
}

/// Orbit camera state
#[derive(Clone, Debug)]
pub struct OrbitCamera {
    pub target: DVec3,    // Look-at point
    pub distance: f64,    // Distance from target
    pub yaw: f64,         // Horizontal angle (radians)
    pub pitch: f64,       // Vertical angle (radians)
    pub fov: f64,         // Field of view (radians)
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: DVec3 { x: 0.0, y: 0.0, z: 0.0 },
            distance: 2.0,
            yaw: 0.5,
            pitch: 0.4,
            fov: std::f64::consts::PI / 4.0,
        }
    }
}

impl OrbitCamera {
    /// Compute camera position from orbit parameters
    pub fn position(&self) -> DVec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        DVec3 {
            x: self.target.x + x,
            y: self.target.y + y,
            z: self.target.z + z,
        }
    }

    /// Rotate camera around target
    pub fn rotate(&mut self, delta_yaw: f64, delta_pitch: f64) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-1.5, 1.5);
    }

    /// Zoom in/out
    pub fn zoom(&mut self, delta: f64) {
        self.distance = (self.distance * (1.0 - delta * 0.1)).clamp(0.5, 10.0);
    }

    /// Pan camera (move target)
    pub fn pan(&mut self, delta_x: f64, delta_y: f64) {
        // Pan in screen space
        let right_x = self.yaw.cos();
        let right_z = -self.yaw.sin();
        self.target.x += right_x * delta_x * self.distance * 0.01;
        self.target.z += right_z * delta_x * self.distance * 0.01;
        self.target.y += delta_y * self.distance * 0.01;
    }

    /// Reset to default view
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Live, Widget)]
pub struct RobotViewer {
    #[deref]
    view: View,

    // Camera
    #[rust]
    camera: OrbitCamera,

    // Robot state
    #[rust]
    joint_angles: Vec<f64>,
    #[rust]
    has_robot: bool,

    // Display options
    #[rust]
    show_trajectory: bool,
    #[rust]
    show_axes: bool,

    // Trajectory history
    #[rust]
    end_effector_trail: Vec<DVec3>,

    // Interaction state
    #[rust]
    is_rotating: bool,
    #[rust]
    is_panning: bool,
    #[rust]
    last_mouse_pos: DVec2,

    // Initialization flag
    #[rust]
    initialized: bool,
}

impl LiveHook for RobotViewer {
    fn after_apply(&mut self, cx: &mut Cx, _apply: &mut Apply, _index: usize, _nodes: &[LiveNode]) {
        if !self.initialized {
            self.initialized = true;
            // Set joint names
            let joint_names = ["J0", "J1", "J2", "J3", "J4", "J5", "Grip"];
            let name_ids = [
                id!(joint_info.joint_0.name),
                id!(joint_info.joint_1.name),
                id!(joint_info.joint_2.name),
                id!(joint_info.joint_3.name),
                id!(joint_info.joint_4.name),
                id!(joint_info.joint_5.name),
                id!(joint_info.joint_6.name),
            ];
            for (i, &name_id) in name_ids.iter().enumerate() {
                self.view.label(name_id).set_text(cx, joint_names[i]);
            }
        }
    }
}

impl Widget for RobotViewer {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        // Capture actions from this event cycle
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Handle button clicks
        if self.view.button(id!(reset_view_btn)).clicked(&actions) {
            self.camera.reset();
            self.view.redraw(cx);
        }

        if self.view.button(id!(show_trajectory_btn)).clicked(&actions) {
            self.show_trajectory = !self.show_trajectory;
            self.view.redraw(cx);
        }

        if self.view.button(id!(show_axes_btn)).clicked(&actions) {
            self.show_axes = !self.show_axes;
            self.view.redraw(cx);
        }

        // Handle viewport interaction
        let viewport_area = self.view.view(id!(viewport)).area();
        match event.hits(cx, viewport_area) {
            Hit::FingerDown(fe) => {
                self.last_mouse_pos = fe.abs;
                if fe.modifiers.control {
                    self.is_panning = true;
                } else {
                    self.is_rotating = true;
                }
            }
            Hit::FingerMove(fe) => {
                let delta = fe.abs - self.last_mouse_pos;
                self.last_mouse_pos = fe.abs;

                if self.is_rotating {
                    self.camera.rotate(delta.x * 0.01, -delta.y * 0.01);
                    self.view.redraw(cx);
                } else if self.is_panning {
                    self.camera.pan(-delta.x, delta.y);
                    self.view.redraw(cx);
                }
            }
            Hit::FingerUp(_) => {
                self.is_rotating = false;
                self.is_panning = false;
            }
            Hit::FingerScroll(se) => {
                self.camera.zoom(se.scroll.y * 0.01);
                self.view.redraw(cx);
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
        // Note: Full 3D rendering would be done here using DrawCube instances
        // with computed transforms from forward kinematics
    }
}

impl RobotViewer {
    /// Set joint angles and update display
    pub fn set_joint_angles(&mut self, cx: &mut Cx, angles: &[f64]) {
        self.joint_angles = angles.to_vec();
        self.has_robot = true;

        // Update joint displays
        let joint_ids = [
            id!(joint_info.joint_0.value),
            id!(joint_info.joint_1.value),
            id!(joint_info.joint_2.value),
            id!(joint_info.joint_3.value),
            id!(joint_info.joint_4.value),
            id!(joint_info.joint_5.value),
            id!(joint_info.joint_6.value),
        ];

        for (i, &joint_id) in joint_ids.iter().enumerate() {
            let value = angles.get(i).copied().unwrap_or(0.0);
            let text = format!("{:.2}", value.to_degrees());
            self.view.label(joint_id).set_text(cx, &text);
        }

        // Update trajectory trail
        // In a full implementation, compute end-effector position via FK
        // and add to trail

        self.view.view(id!(viewport.placeholder)).set_visible(cx, false);
        self.view.redraw(cx);
    }

    /// Clear robot visualization
    pub fn clear(&mut self, cx: &mut Cx) {
        self.joint_angles.clear();
        self.has_robot = false;
        self.end_effector_trail.clear();
        self.view.view(id!(viewport.placeholder)).set_visible(cx, true);
        self.view.redraw(cx);
    }

    /// Clear trajectory trail
    pub fn clear_trail(&mut self) {
        self.end_effector_trail.clear();
    }

    /// Set placeholder text
    pub fn set_placeholder_text(&mut self, cx: &mut Cx, text: &str) {
        self.view.label(id!(viewport.placeholder.placeholder_label)).set_text(cx, text);
    }
}

impl RobotViewerRef {
    pub fn set_joint_angles(&self, cx: &mut Cx, angles: &[f64]) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_joint_angles(cx, angles);
        }
    }

    pub fn clear(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.clear(cx);
        }
    }

    pub fn set_placeholder_text(&self, cx: &mut Cx, text: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_placeholder_text(cx, text);
        }
    }
}
