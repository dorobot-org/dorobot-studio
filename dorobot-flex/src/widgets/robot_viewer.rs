//! 3D Robot Arm Viewer Widget
//!
//! Displays URDF robot models using kiss3d for rendering.
//! The kiss3d renderer runs in a background thread and streams
//! rendered frames as textures to this Makepad widget.

use makepad_widgets::*;
use crate::data::urdf_renderer::{
    CameraCommand, SharedRendererState, create_shared_state,
};

#[cfg(feature = "urdf")]
use crate::data::urdf_renderer::start_renderer_thread;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;

    // Individual joint angle display
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
                text: "Reset"
                width: 50
                height: 20
                padding: { left: 6, right: 6 }
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
            }
        }

        // 3D viewport - displays kiss3d rendered texture
        viewport = <View> {
            width: Fill
            height: Fill
            cursor: Hand

            show_bg: true
            draw_bg: { color: #1a1a22 }

            flow: Overlay

            // Rendered frame from kiss3d
            frame_image = <Image> {
                width: Fill
                height: Fill
                fit: Smallest
                visible: false
            }

            // Placeholder when no robot loaded
            placeholder = <View> {
                width: Fill
                height: Fill
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

            // Loading indicator
            loading = <View> {
                width: Fill
                height: Fill
                align: { x: 0.5, y: 0.5 }
                visible: false

                loading_label = <Label> {
                    draw_text: {
                        text_style: <TEXT_BODY> {}
                        color: (COLOR_TEXT_SECONDARY)
                    }
                    text: "Loading URDF..."
                }
            }

            // Error display
            error_view = <View> {
                width: Fill
                height: Fill
                align: { x: 0.5, y: 0.5 }
                visible: false

                error_label = <Label> {
                    draw_text: {
                        text_style: <TEXT_BODY> {}
                        color: #ff6666
                        wrap: Word
                    }
                    text: ""
                }
            }
        }

        // Joint angles display (bottom overlay)
        joint_info = <View> {
            width: Fill
            height: 50
            padding: 6
            flow: Right
            spacing: 8

            show_bg: true
            draw_bg: { color: (COLOR_BG_PANEL) }

            joint_0 = <JointDisplay> {}
            joint_1 = <JointDisplay> {}
            joint_2 = <JointDisplay> {}
            joint_3 = <JointDisplay> {}
            joint_4 = <JointDisplay> {}
            joint_5 = <JointDisplay> {}
        }
    }
}

#[derive(Live, Widget)]
pub struct RobotViewer {
    #[deref]
    view: View,

    // Renderer state (shared with background thread)
    #[rust]
    renderer_state: Option<SharedRendererState>,

    // Renderer thread handle
    #[rust]
    renderer_thread: Option<std::thread::JoinHandle<()>>,

    // Current URDF path
    #[rust]
    urdf_path: Option<String>,

    // Joint angles from dataset
    #[rust]
    joint_angles: Vec<f64>,

    // Texture for displaying rendered frame
    #[rust]
    frame_texture: Option<Texture>,

    // UI state
    #[rust]
    has_robot: bool,
    #[rust]
    is_loading: bool,

    // Interaction state
    #[rust]
    is_dragging: bool,
    #[rust]
    drag_button: usize, // 0 = left (orbit), 1 = right (pan)
    #[rust]
    last_mouse_pos: DVec2,

    // Timer for checking new frames
    #[rust]
    refresh_timer: Timer,

    // Initialization flag
    #[rust]
    initialized: bool,
}

impl LiveHook for RobotViewer {
    fn after_apply(&mut self, cx: &mut Cx, _apply: &mut Apply, _index: usize, _nodes: &[LiveNode]) {
        if !self.initialized {
            self.initialized = true;
            // Don't auto-start renderer - kiss3d requires main thread on macOS
            // Renderer will be started when load_urdf is called

            // Start refresh timer (30 fps)
            self.refresh_timer = cx.start_interval(0.033);
        }
    }
}

impl Widget for RobotViewer {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Handle timer - check for new rendered frames
        if self.refresh_timer.is_event(event).is_some() {
            self.check_for_new_frame(cx);
        }

        // Handle signal from renderer thread
        if let Event::Signal = event {
            self.check_for_new_frame(cx);
        }

        // Capture button actions (this also handles view events)
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Handle reset view button
        if self.view.button(id!(reset_view_btn)).clicked(&actions) {
            self.send_camera_command(CameraCommand::Reset);
        }

        // Handle viewport mouse interaction
        let viewport_area = self.view.view(id!(viewport)).area();
        match event.hits(cx, viewport_area) {
            Hit::FingerDown(fe) => {
                self.last_mouse_pos = fe.abs;
                self.is_dragging = true;
                // Right click or ctrl+click for pan, left click for orbit
                self.drag_button = if fe.modifiers.control { 1 } else { 0 };
            }
            Hit::FingerMove(fe) if self.is_dragging => {
                let delta = fe.abs - self.last_mouse_pos;
                self.last_mouse_pos = fe.abs;

                if self.drag_button == 0 {
                    // Orbit
                    self.send_camera_command(CameraCommand::Orbit {
                        dx: delta.x as f32,
                        dy: delta.y as f32,
                    });
                } else {
                    // Pan
                    self.send_camera_command(CameraCommand::Pan {
                        dx: delta.x as f32,
                        dy: delta.y as f32,
                    });
                }
            }
            Hit::FingerUp(_) => {
                self.is_dragging = false;
            }
            Hit::FingerScroll(se) => {
                self.send_camera_command(CameraCommand::Zoom {
                    delta: se.scroll.y as f32 * 0.01,
                });
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl RobotViewer {
    /// Initialize the renderer thread
    fn init_renderer(&mut self) {
        let state = create_shared_state();
        self.renderer_state = Some(state.clone());

        #[cfg(feature = "urdf")]
        {
            self.renderer_thread = Some(start_renderer_thread(state));
        }
    }

    /// Send camera command to renderer
    fn send_camera_command(&self, cmd: CameraCommand) {
        if let Some(state) = &self.renderer_state {
            if let Ok(mut state) = state.lock() {
                state.camera_commands.push(cmd);
            }
        }
    }

    /// Check for new rendered frame from background thread
    fn check_for_new_frame(&mut self, cx: &mut Cx) {
        let (frame, is_ready, error, joint_names) = {
            if let Some(state) = &self.renderer_state {
                if let Ok(mut state) = state.lock() {
                    let frame = state.rendered_frame.take();
                    let is_ready = state.is_ready;
                    let error = state.error.clone();
                    let joint_names = state.joint_names.clone();
                    (frame, is_ready, error, joint_names)
                } else {
                    (None, false, None, Vec::new())
                }
            } else {
                (None, false, None, Vec::new())
            }
        };

        // Update UI based on state
        if let Some(err) = error {
            self.show_error(cx, &err);
            self.is_loading = false;
        } else if is_ready && self.is_loading {
            self.is_loading = false;
            self.has_robot = true;

            // Update joint names
            self.update_joint_names(cx, &joint_names);

            // Hide loading, show viewport
            self.view.view(id!(viewport.loading)).set_visible(cx, false);
            self.view.view(id!(viewport.placeholder)).set_visible(cx, false);
            self.view.view(id!(viewport.error_view)).set_visible(cx, false);
        }

        // Update texture if we have a new frame
        if let Some(frame) = frame {
            self.update_frame_texture(cx, &frame);
        }
    }

    /// Update the displayed frame texture
    fn update_frame_texture(&mut self, cx: &mut Cx, frame: &crate::data::urdf_renderer::RenderedFrame) {
        // Convert RGBA to BGRA u32 for Makepad texture
        let bgra_data: Vec<u32> = frame.data.chunks(4)
            .map(|rgba| {
                let r = rgba[0] as u32;
                let g = rgba[1] as u32;
                let b = rgba[2] as u32;
                let a = rgba[3] as u32;
                (a << 24) | (r << 16) | (g << 8) | b
            })
            .collect();

        let texture = Texture::new_with_format(cx, TextureFormat::VecBGRAu8_32 {
            data: Some(bgra_data),
            width: frame.width,
            height: frame.height,
            updated: TextureUpdated::Full,
        });

        // Set texture on image widget
        let image = self.view.image(id!(viewport.frame_image));
        image.set_texture(cx, Some(texture));

        // Make image visible
        self.view.image(id!(viewport.frame_image)).apply_over(cx, live! {
            visible: true
        });

        self.view.redraw(cx);
    }

    /// Update joint name labels
    fn update_joint_names(&mut self, cx: &mut Cx, names: &[String]) {
        let name_ids = [
            id!(joint_info.joint_0.name),
            id!(joint_info.joint_1.name),
            id!(joint_info.joint_2.name),
            id!(joint_info.joint_3.name),
            id!(joint_info.joint_4.name),
            id!(joint_info.joint_5.name),
        ];

        for (i, name_id) in name_ids.iter().enumerate() {
            let name = names.get(i).map(|s| s.as_str()).unwrap_or("");
            // Shorten long names
            let short_name = if name.len() > 8 {
                &name[..8]
            } else {
                name
            };
            self.view.label(*name_id).set_text(cx, short_name);
        }
    }

    /// Show error message
    fn show_error(&mut self, cx: &mut Cx, error: &str) {
        self.view.label(id!(viewport.error_view.error_label)).set_text(cx, error);
        self.view.view(id!(viewport.error_view)).set_visible(cx, true);
        self.view.view(id!(viewport.loading)).set_visible(cx, false);
        self.view.view(id!(viewport.placeholder)).set_visible(cx, false);
        self.view.redraw(cx);
    }

    /// Load URDF file
    pub fn load_urdf(&mut self, cx: &mut Cx, path: &str) {
        self.urdf_path = Some(path.to_string());
        self.is_loading = true;
        self.has_robot = false;

        // Show loading indicator
        self.view.view(id!(viewport.loading)).set_visible(cx, true);
        self.view.view(id!(viewport.placeholder)).set_visible(cx, false);
        self.view.view(id!(viewport.error_view)).set_visible(cx, false);
        self.view.image(id!(viewport.frame_image)).apply_over(cx, live! {
            visible: false
        });

        // Initialize renderer if not already done
        if self.renderer_state.is_none() {
            self.init_renderer();
        }

        // Tell renderer to load URDF
        if let Some(state) = &self.renderer_state {
            if let Ok(mut state) = state.lock() {
                state.urdf_path = Some(path.to_string());
                state.is_ready = false;
                state.error = None;
            }
        }

        self.view.redraw(cx);
    }

    /// Set joint angles and update display
    pub fn set_joint_angles(&mut self, cx: &mut Cx, angles: &[f64]) {
        self.joint_angles = angles.to_vec();

        // Update renderer state
        if let Some(state) = &self.renderer_state {
            if let Ok(mut state) = state.lock() {
                state.joint_positions = angles.to_vec();
            }
        }

        // Update joint value displays
        let value_ids = [
            id!(joint_info.joint_0.value),
            id!(joint_info.joint_1.value),
            id!(joint_info.joint_2.value),
            id!(joint_info.joint_3.value),
            id!(joint_info.joint_4.value),
            id!(joint_info.joint_5.value),
        ];

        for (i, &value_id) in value_ids.iter().enumerate() {
            let value = angles.get(i).copied().unwrap_or(0.0);
            let text = format!("{:.1}°", value.to_degrees());
            self.view.label(value_id).set_text(cx, &text);
        }
    }

    /// Clear robot visualization
    pub fn clear(&mut self, cx: &mut Cx) {
        self.joint_angles.clear();
        self.has_robot = false;
        self.urdf_path = None;

        self.view.view(id!(viewport.placeholder)).set_visible(cx, true);
        self.view.view(id!(viewport.loading)).set_visible(cx, false);
        self.view.view(id!(viewport.error_view)).set_visible(cx, false);
        self.view.image(id!(viewport.frame_image)).apply_over(cx, live! {
            visible: false
        });

        self.view.redraw(cx);
    }

    /// Set placeholder text
    pub fn set_placeholder_text(&mut self, cx: &mut Cx, text: &str) {
        self.view.label(id!(viewport.placeholder.placeholder_label)).set_text(cx, text);
    }
}

impl RobotViewerRef {
    pub fn load_urdf(&self, cx: &mut Cx, path: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.load_urdf(cx, path);
        }
    }

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
