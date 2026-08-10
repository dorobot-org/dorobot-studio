//! Main application module

use makepad_widgets::*;
use std::sync::Arc;

use dorobot_dora_bridge::{RerunLogger, SharedRobotState};
use dorobot_types::*;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // Color theme
    let DARK_BG = #x1a1a2f
    let PANEL_BG = #x162140
    let ACCENT_BLUE = #x0f3460
    let ACCENT_CYAN = #x00fff5
    let TEXT_PRIMARY = #xf0f0f0
    let TEXT_SECONDARY = #xa0a0a0
    let STATUS_OK = #x00ff88
    let STATUS_WARN = #xffaa00
    let STATUS_ERR = #xff4444

    startup() do #(App::script_component(vm)){
        ui: Window{
            pass.clear_color: DARK_BG

            body +: {
                flow: Down
                padding: 10
                spacing: 10

                // Top bar with title and controls
                View{
                    flow: Right
                    height: Fit
                    spacing: 20
                    align: Align{y: 0.5}

                    Label{
                        text: "DOROBOT"
                        draw_text +: {
                            color: ACCENT_CYAN
                            text_style +: { font_size: 24.0 }
                        }
                    }

                    Label{
                        text: "Robotics Visualization Dashboard"
                        draw_text +: {
                            color: TEXT_SECONDARY
                            text_style +: { font_size: 12.0 }
                        }
                    }

                    View{ width: Fill }

                    // Connection status
                    connection_status := Label{
                        text: "Demo Mode"
                        draw_text +: {
                            color: STATUS_WARN
                            text_style +: { font_size: 12.0 }
                        }
                    }

                    // Rerun toggle
                    rerun_btn := Button{
                        text: "Launch Rerun"
                        draw_text +: { color: TEXT_PRIMARY }
                        draw_bg +: {
                            color: ACCENT_BLUE
                            border_radius: 4.0
                        }
                    }
                }

                // Main content area
                View{
                    flow: Right
                    width: Fill
                    height: Fill
                    spacing: 10

                    // Left panel - Status and sensors
                    View{
                        flow: Down
                        width: 280
                        height: Fill
                        spacing: 10

                        // System status panel
                        View{
                            flow: Down
                            width: Fill
                            height: Fit
                            padding: 10
                            spacing: 8

                            draw_bg +: {
                                color: PANEL_BG
                                border_radius: 8.0
                            }

                            Label{
                                text: "SYSTEM STATUS"
                                draw_text +: {
                                    color: ACCENT_CYAN
                                    text_style +: { font_size: 11.0 }
                                }
                            }

                            View{
                                flow: Right
                                width: Fill
                                height: Fit
                                spacing: 8
                                align: Align{y: 0.5}

                                View{
                                    width: 12
                                    height: 12
                                    draw_bg +: {
                                        color: STATUS_OK
                                        border_radius: 6.0
                                    }
                                }

                                Label{
                                    text: "Overall: OK"
                                    draw_text +: {
                                        color: TEXT_PRIMARY
                                        text_style +: { font_size: 12.0 }
                                    }
                                }
                            }

                            uptime_label := Label{
                                text: "Uptime: 00:00:00"
                                draw_text +: {
                                    color: TEXT_SECONDARY
                                    text_style +: { font_size: 10.0 }
                                }
                            }
                        }

                        // Sensor gauges
                        View{
                            flow: Down
                            width: Fill
                            height: Fit
                            padding: 10
                            spacing: 8

                            draw_bg +: {
                                color: PANEL_BG
                                border_radius: 8.0
                            }

                            Label{
                                text: "SENSORS"
                                draw_text +: {
                                    color: ACCENT_CYAN
                                    text_style +: { font_size: 11.0 }
                                }
                            }

                            lidar_label := Label{
                                text: "LiDAR: OK"
                                draw_text +: {
                                    color: STATUS_OK
                                    text_style +: { font_size: 11.0 }
                                }
                            }

                            camera_label := Label{
                                text: "Camera: OK"
                                draw_text +: {
                                    color: STATUS_OK
                                    text_style +: { font_size: 11.0 }
                                }
                            }

                            imu_label := Label{
                                text: "IMU: OK"
                                draw_text +: {
                                    color: STATUS_OK
                                    text_style +: { font_size: 11.0 }
                                }
                            }
                        }

                        // System metrics
                        View{
                            flow: Down
                            width: Fill
                            height: Fit
                            padding: 10
                            spacing: 8

                            draw_bg +: {
                                color: PANEL_BG
                                border_radius: 8.0
                            }

                            Label{
                                text: "SYSTEM"
                                draw_text +: {
                                    color: ACCENT_CYAN
                                    text_style +: { font_size: 11.0 }
                                }
                            }

                            battery_label := Label{
                                text: "Battery: 85%"
                                draw_text +: {
                                    color: TEXT_PRIMARY
                                    text_style +: { font_size: 11.0 }
                                }
                            }

                            cpu_label := Label{
                                text: "CPU: 35%"
                                draw_text +: {
                                    color: TEXT_PRIMARY
                                    text_style +: { font_size: 11.0 }
                                }
                            }

                            memory_label := Label{
                                text: "Memory: 45%"
                                draw_text +: {
                                    color: TEXT_PRIMARY
                                    text_style +: { font_size: 11.0 }
                                }
                            }
                        }

                        View{ height: Fill }
                    }

                    // Center panel - Visualization area
                    View{
                        flow: Down
                        width: Fill
                        height: Fill
                        spacing: 10

                        // 2D Map view
                        View{
                            width: Fill
                            height: 300
                            padding: 10

                            draw_bg +: {
                                color: PANEL_BG
                                border_radius: 8.0
                            }

                            View{
                                flow: Down
                                width: Fill
                                height: Fill

                                View{
                                    flow: Right
                                    height: Fit
                                    spacing: 10

                                    Label{
                                        text: "2D MAP VIEW"
                                        draw_text +: {
                                            color: ACCENT_CYAN
                                            text_style +: { font_size: 11.0 }
                                        }
                                    }

                                    View{ width: Fill }

                                    pose_label := Label{
                                        text: "X: 0.00  Y: 0.00"
                                        draw_text +: {
                                            color: TEXT_SECONDARY
                                            text_style +: { font_size: 10.0 }
                                        }
                                    }
                                }

                                // Simple map placeholder
                                map_view := View{
                                    width: Fill
                                    height: Fill
                                    margin: Inset{top: 10.}

                                    draw_bg +: {
                                        pixel: fn() {
                                            // Dark grid background
                                            let uv = self.pos
                                            let grid = step(0.95, fract(uv.x * 20.0)) + step(0.95, fract(uv.y * 20.0))
                                            let color = mix(
                                                vec4(0.05, 0.08, 0.15, 1.0),
                                                vec4(0.1, 0.15, 0.25, 1.0),
                                                grid * 0.3
                                            )
                                            return color
                                        }
                                    }
                                }
                            }
                        }

                        // Info cards
                        View{
                            flow: Right
                            width: Fill
                            height: Fill
                            spacing: 10

                            // Point cloud stats
                            View{
                                flow: Down
                                width: Fill
                                height: Fill
                                padding: 10

                                draw_bg +: {
                                    color: PANEL_BG
                                    border_radius: 8.0
                                }

                                Label{
                                    text: "POINT CLOUD"
                                    draw_text +: {
                                        color: ACCENT_CYAN
                                        text_style +: { font_size: 11.0 }
                                    }
                                }

                                point_count_label := Label{
                                    text: "Points: 0"
                                    margin: Inset{top: 10.}
                                    draw_text +: {
                                        color: TEXT_PRIMARY
                                        text_style +: { font_size: 14.0 }
                                    }
                                }

                                frame_id_label := Label{
                                    text: "Frame: --"
                                    draw_text +: {
                                        color: TEXT_SECONDARY
                                        text_style +: { font_size: 11.0 }
                                    }
                                }

                                Label{
                                    text: "View in Rerun for 3D"
                                    margin: Inset{top: 10.}
                                    draw_text +: {
                                        color: TEXT_SECONDARY
                                        text_style +: { font_size: 10.0 }
                                    }
                                }
                            }

                            // Detections
                            View{
                                flow: Down
                                width: Fill
                                height: Fill
                                padding: 10

                                draw_bg +: {
                                    color: PANEL_BG
                                    border_radius: 8.0
                                }

                                Label{
                                    text: "DETECTIONS"
                                    draw_text +: {
                                        color: ACCENT_CYAN
                                        text_style +: { font_size: 11.0 }
                                    }
                                }

                                detection_count_label := Label{
                                    text: "Objects: 0"
                                    margin: Inset{top: 10.}
                                    draw_text +: {
                                        color: TEXT_PRIMARY
                                        text_style +: { font_size: 14.0 }
                                    }
                                }

                                detection_list_label := Label{
                                    text: "No detections"
                                    draw_text +: {
                                        color: TEXT_SECONDARY
                                        text_style +: { font_size: 11.0 }
                                    }
                                }
                            }
                        }
                    }

                    // Right panel - Logs
                    View{
                        flow: Down
                        width: 300
                        height: Fill
                        padding: 10

                        draw_bg +: {
                            color: PANEL_BG
                            border_radius: 8.0
                        }

                        Label{
                            text: "LOGS"
                            draw_text +: {
                                color: ACCENT_CYAN
                                text_style +: { font_size: 11.0 }
                            }
                        }

                        log_text := Label{
                            width: Fill
                            height: Fill
                            margin: Inset{top: 10.}
                            text: "[INFO] System started\n[INFO] Demo mode active"
                            draw_text +: {
                                color: TEXT_SECONDARY
                                text_style +: { font_size: 9.0 }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,

    #[rust]
    shared_state: Option<Arc<SharedRobotState>>,

    #[rust]
    rerun_logger: Option<RerunLogger>,

    #[rust]
    poll_timer: Timer,

    #[rust]
    frame_count: u64,

    #[rust]
    start_time: Option<std::time::Instant>,

    #[rust]
    log_messages: Vec<String>,
}

impl App {
    fn init(&mut self) {
        let state = SharedRobotState::new_shared();
        self.shared_state = Some(state.clone());
        self.start_time = Some(std::time::Instant::now());
        self.log_messages = vec![
            "[INFO] System started".to_string(),
            "[INFO] Demo mode active".to_string(),
        ];

        // Start demo data thread
        self.spawn_demo_data_thread(state);
    }

    fn spawn_demo_data_thread(&self, state: Arc<SharedRobotState>) {
        std::thread::spawn(move || {
            let mut frame = 0u64;
            let mut angle = 0.0f32;

            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));

                // Simulate robot movement
                angle += 0.05;
                let x = angle.cos() * 2.0;
                let y = angle.sin() * 2.0;

                let robot_state = RobotState {
                    pose: Pose3D::new(x, y, 0.0).with_quaternion(
                        0.0,
                        0.0,
                        (angle / 2.0).sin(),
                        (angle / 2.0).cos(),
                    ),
                    linear_velocity: [0.1, 0.0, 0.0],
                    angular_velocity: [0.0, 0.0, 0.05],
                    timestamp_ns: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as u64,
                };

                state.robot_state.set(robot_state);
                state.add_trajectory_point(Pose3D::new(x, y, 0.0));

                // Simulate point cloud
                let mut points = Vec::with_capacity(1000);
                for i in 0..1000 {
                    let a = (i as f32 / 1000.0) * std::f32::consts::PI * 2.0;
                    let r = 3.0 + (a * 5.0).sin() * 0.5;
                    points.push(Point3D::new(
                        x + r * a.cos(),
                        y + r * a.sin(),
                        (a * 3.0).sin() * 0.2,
                    ));
                }

                state.point_cloud.set(Some(
                    PointCloud::new("lidar")
                        .with_points(points)
                        .with_timestamp(robot_state.timestamp_ns),
                ));

                // Simulate system status
                let status = SystemStatus {
                    lidar_status: SensorStatus::Ok,
                    camera_status: SensorStatus::Ok,
                    imu_status: SensorStatus::Ok,
                    motor_status: SensorStatus::Ok,
                    battery_percent: 85.0 - (frame as f32 * 0.01) % 20.0,
                    cpu_usage: 35.0 + (frame as f32 * 0.1).sin() * 10.0,
                    memory_usage: 45.0 + (frame as f32 * 0.05).cos() * 5.0,
                    timestamp_ns: robot_state.timestamp_ns,
                };

                state.system_status.set(status);

                frame += 1;
            }
        });
    }

    fn poll_state(&mut self, cx: &mut Cx) {
        let Some(state) = &self.shared_state else {
            return;
        };

        // Poll robot state
        if let Some(robot_state) = state.robot_state.read_if_dirty() {
            let pose = robot_state.pose;
            self.ui.label(cx, ids!(pose_label)).set_text(
                cx,
                &format!("X: {:.2}  Y: {:.2}", pose.x, pose.y),
            );

            // Log to Rerun if available
            if let Some(logger) = &self.rerun_logger {
                let _ = logger.log_robot_pose(&robot_state, "robot/base");
            }
        }

        // Poll point cloud
        if let Some(Some(cloud)) = state.point_cloud.read_if_dirty() {
            self.ui
                .label(cx, ids!(point_count_label))
                .set_text(cx, &format!("Points: {}", cloud.points.len()));

            self.ui
                .label(cx, ids!(frame_id_label))
                .set_text(cx, &format!("Frame: {}", cloud.frame_id));

            // Log to Rerun
            if let Some(logger) = &self.rerun_logger {
                let _ = logger.log_point_cloud(&cloud);
            }
        }

        // Poll system status
        if let Some(status) = state.system_status.read_if_dirty() {
            self.ui
                .label(cx, ids!(battery_label))
                .set_text(cx, &format!("Battery: {:.0}%", status.battery_percent));
            self.ui
                .label(cx, ids!(cpu_label))
                .set_text(cx, &format!("CPU: {:.0}%", status.cpu_usage));
            self.ui
                .label(cx, ids!(memory_label))
                .set_text(cx, &format!("Memory: {:.0}%", status.memory_usage));

            // Log to Rerun
            if let Some(logger) = &self.rerun_logger {
                let _ = logger.log_status(&status, "robot/status");
            }
        }

        // Update uptime
        if let Some(start) = self.start_time {
            let elapsed = start.elapsed();
            let hours = elapsed.as_secs() / 3600;
            let minutes = (elapsed.as_secs() % 3600) / 60;
            let seconds = elapsed.as_secs() % 60;

            self.ui.label(cx, ids!(uptime_label)).set_text(
                cx,
                &format!("Uptime: {:02}:{:02}:{:02}", hours, minutes, seconds),
            );
        }

        self.frame_count += 1;
    }

    fn launch_rerun(&mut self, cx: &mut Cx) {
        if self.rerun_logger.is_some() {
            return;
        }

        match RerunLogger::spawn("dorobot") {
            Ok(logger) => {
                self.rerun_logger = Some(logger);
                self.ui.button(cx, ids!(rerun_btn)).set_text(cx, "Rerun Active");
                self.ui
                    .label(cx, ids!(connection_status))
                    .set_text(cx, "Rerun Connected");

                self.log_messages.push("[INFO] Rerun viewer launched".to_string());
                self.update_log_display(cx);
            }
            Err(e) => {
                self.log_messages.push(format!("[ERROR] Failed to launch Rerun: {}", e));
                self.update_log_display(cx);
            }
        }
    }

    fn update_log_display(&mut self, cx: &mut Cx) {
        let log_text = self.log_messages.iter().rev().take(20).cloned().collect::<Vec<_>>().join("\n");
        self.ui.label(cx, ids!(log_text)).set_text(cx, &log_text);
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        self.init();
        // Start polling timer (50ms = 20Hz)
        self.poll_timer = cx.start_interval(0.05);
    }

    fn handle_timer(&mut self, cx: &mut Cx, e: &TimerEvent) {
        if self.poll_timer.is_timer(e).is_some() {
            self.poll_state(cx);
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.ui.button(cx, ids!(rerun_btn)).clicked(actions) {
            self.launch_rerun(cx);
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

app_main!(App);
