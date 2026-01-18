//! Makepad URDF Player - 3D Robot Viewer
//!
//! A Makepad-based URDF robot visualization application.
//!
//! Controls:
//!   - Mouse drag: Orbit camera
//!   - Scroll: Zoom
//!   - Arrow keys: Control joints (Left/Right to select, Up/Down to adjust)
//!   - A: Toggle animation
//!   - R: Reset to default pose

use makepad_widgets::*;

// Import from the library crate
use makepad_urdf_player::robot_view::{RobotViewAction, RobotViewWidgetExt};

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    // Import RobotView from library crate
    use makepad_urdf_player::robot_view::RobotView;

    URDFViewer = {{URDFViewer}} {
        width: Fill
        height: Fill
        flow: Down

        header = <View> {
            width: Fill
            height: 40
            padding: 8
            spacing: 16
            flow: Right
            align: { y: 0.5 }
            show_bg: true
            draw_bg: { color: #2a2a35 }

            open_btn = <Button> {
                text: "Open Robot..."
                draw_text: { color: #fff }
            }

            <View> { width: 16 }

            robot_name_label = <Label> {
                draw_text: {
                    text_style: { font_size: 14.0 }
                    color: #4080c0
                }
                text: "VX300s (ALOHA)"
            }

            <View> { width: 16 }

            <Label> {
                draw_text: {
                    text_style: { font_size: 12.0 }
                    color: #888
                }
                text: "Arrows=joints, +/-=zoom, A=animate, R=reset"
            }

            <View> { width: Fill }

            light_btn = <Button> {
                text: "Light: ON"
                draw_text: { color: #ff0 }
            }

            <View> { width: 8 }

            axes_btn = <Button> {
                text: "Axes: OFF"
                draw_text: { color: #888 }
            }

            <View> { width: 8 }

            xyz_btn = <Button> {
                text: "XYZ: OFF"
                draw_text: { color: #888 }
            }

            <View> { width: 8 }

            reset_btn = <Button> {
                text: "Reset"
                draw_text: { color: #fff }
            }
        }

        viewport = <View> {
            width: Fill
            height: Fill
            show_bg: true
            draw_bg: { color: #f0ddd4 }  // Fallback background

            robot_view = <RobotView> {
                width: Fill
                height: Fill
            }

            // Status overlay positioned at bottom of viewport
            status_overlay = <View> {
                width: Fill
                height: 36
                abs_pos: vec2(0.0, 0.0)  // Will be positioned in draw
                margin: { left: 8 }
                align: { y: 0.5 }
                show_bg: true
                draw_bg: { color: #2a2a3580 }  // Semi-transparent

                joint_label = <Label> {
                    draw_text: {
                        text_style: { font_size: 12.0 }
                        color: #ccc
                    }
                    text: "Joint 0: 0.00 rad"
                }
            }
        }

        // Robot selection modal
        robot_modal = <Modal> {
            content: <View> {
                width: 400
                height: 300
                padding: 20
                spacing: 16
                flow: Down
                show_bg: true
                draw_bg: { color: #2a2a35 }

                <Label> {
                    draw_text: {
                        text_style: { font_size: 18.0 }
                        color: #fff
                    }
                    text: "Select Robot Model"
                }

                // Robot selection buttons
                <View> {
                    width: Fill
                    height: Fit
                    flow: Down
                    spacing: 4

                    vx300s_btn = <Button> {
                        width: Fill
                        text: "VX300s (ALOHA) - ViperX 300 6DOF"
                        draw_text: { color: #fff }
                        draw_bg: { color: #4080c0 }
                    }

                    so100_btn = <Button> {
                        width: Fill
                        text: "SO100 - SO-ARM100 Robot"
                        draw_text: { color: #ccc }
                    }
                }

                robot_info = <Label> {
                    width: Fill
                    margin: { top: 12 }
                    draw_text: {
                        text_style: { font_size: 12.0 }
                        color: #888
                    }
                    text: "Click a robot above to load it"
                }

                <View> { height: Fill }

                <View> {
                    width: Fill
                    height: Fit
                    flow: Right
                    align: { x: 1.0 }

                    cancel_btn = <Button> {
                        text: "Cancel"
                    }
                }
            }
        }
    }

    App = {{App}} {
        ui: <Window> {
            window: { title: "VX300s (ALOHA) Robot Viewer" }
            show_bg: true
            draw_bg: { color: #f0ddd4 }  // Warm peachy to match gradient
            body = <URDFViewer> {}
        }
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct URDFViewer {
    #[deref] view: View,
    #[rust] animating: bool,
    #[rust] anim_timer: Timer,
    #[rust] anim_step: u64,
    #[rust] auto_started: bool,
}

impl URDFViewer {
    fn update_status(&mut self, cx: &mut Cx) {
        let robot_view = self.view.robot_view(id!(viewport.robot_view));
        let selected = robot_view.get_selected_joint();

        let text = if let Some((name, angle, lower, upper)) = robot_view.get_joint_info(selected) {
            format!(
                "Joint {}: {} = {:.2} rad ({:.1}°) [{:.1}° to {:.1}°]",
                selected, name, angle, angle.to_degrees(),
                lower.to_degrees(), upper.to_degrees()
            )
        } else {
            "Loading robot...".to_string()
        };
        self.view.label(id!(viewport.status_overlay.joint_label)).set_text(cx, &text);
    }
}

impl Widget for URDFViewer {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        // Animation timer - managed at URDFViewer level
        if self.anim_timer.is_event(event).is_some() && self.animating {
            self.anim_step += 1;
            let robot_view = self.view.robot_view(id!(viewport.robot_view));
            robot_view.animate_step(cx, self.anim_step);
            self.update_status(cx);
            self.redraw(cx);
        }

        // Handle keyboard at top level
        if let Event::KeyDown(ke) = event {
            let robot_view = self.view.robot_view(id!(viewport.robot_view));
            match ke.key_code {
                KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                    // Forward to RobotView - it handles these internally
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
                    robot_view.reset_view(cx);
                    self.update_status(cx);
                }
                _ => {}
            }
            self.update_status(cx);
        }

        // Handle actions from RobotView
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        for action in &actions {
            match action.as_widget_action().cast::<RobotViewAction>() {
                RobotViewAction::JointChanged { .. } => {
                    self.update_status(cx);
                }
                RobotViewAction::AnimationToggled(_) => {
                    self.update_status(cx);
                }
                _ => {}
            }
        }

        // Reset button
        if self.view.button(id!(header.reset_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(viewport.robot_view));
            robot_view.reset_view(cx);
            self.update_status(cx);
        }

        // Light toggle button
        if self.view.button(id!(header.light_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(viewport.robot_view));
            let enabled = robot_view.toggle_specular(cx);
            let text = if enabled { "Light: ON" } else { "Light: OFF" };
            self.view.button(id!(header.light_btn)).set_text(cx, text);
        }

        // Joint axes toggle button
        if self.view.button(id!(header.axes_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(viewport.robot_view));
            let enabled = robot_view.toggle_joint_axes(cx);
            let text = if enabled { "Axes: ON" } else { "Axes: OFF" };
            self.view.button(id!(header.axes_btn)).set_text(cx, text);
        }

        // World XYZ axes toggle button
        if self.view.button(id!(header.xyz_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(viewport.robot_view));
            let enabled = robot_view.toggle_world_axes(cx);
            let text = if enabled { "XYZ: ON" } else { "XYZ: OFF" };
            self.view.button(id!(header.xyz_btn)).set_text(cx, text);
        }

        // Open robot selection modal
        if self.view.button(id!(header.open_btn)).clicked(&actions) {
            println!("Opening modal...");
            self.view.view(id!(viewport)).set_visible(cx, false);
            self.view.modal(id!(robot_modal)).open(cx);
        }

        // Robot selection buttons in modal - click to load directly
        if self.view.button(id!(robot_modal.vx300s_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(viewport.robot_view));
            robot_view.reload_robot(cx, "data/vx300s/vx300s.urdf", "data/vx300s");
            self.view.label(id!(header.robot_name_label)).set_text(cx, "VX300s (ALOHA)");
            self.view.modal(id!(robot_modal)).close(cx);
            self.view.view(id!(viewport)).set_visible(cx, true);
            self.update_status(cx);
        }

        if self.view.button(id!(robot_modal.so100_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(viewport.robot_view));
            robot_view.reload_robot(cx, "data/so100.urdf", "data/assets");
            self.view.label(id!(header.robot_name_label)).set_text(cx, "SO100");
            self.view.modal(id!(robot_modal)).close(cx);
            self.view.view(id!(viewport)).set_visible(cx, true);
            self.update_status(cx);
        }

        // Cancel button in modal
        if self.view.button(id!(robot_modal.cancel_btn)).clicked(&actions) {
            self.view.modal(id!(robot_modal)).close(cx);
            self.view.view(id!(viewport)).set_visible(cx, true);
        }

        // Load button removed - clicking robot buttons loads directly

        // Modal dismissed (click outside or Escape)
        if self.view.modal(id!(robot_modal)).dismissed(&actions) {
            self.view.view(id!(viewport)).set_visible(cx, true);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Auto-start animation for profiling (after first frame)
        if !self.auto_started {
            self.auto_started = true;
            self.animating = true;
            self.anim_timer = cx.cx.start_interval(0.033);
            eprintln!("=== Auto-starting animation for profiling ===");
        }
        self.view.draw_walk(cx, scope, walk)
    }
}

#[derive(Live, LiveHook)]
pub struct App {
    #[live] ui: WidgetRef,
}

impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        // Order matters: dependencies first!
        makepad_widgets::live_design(cx);
        makepad_urdf_player::live_design(cx);  // Register library's live_design
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
