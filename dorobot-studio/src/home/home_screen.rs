//! Main Dashboard Home Screen with Adaptive Layout

use std::collections::HashMap;
use std::path::PathBuf;

use makepad_widgets::*;
use crate::widgets::timeline::{TimelineAction, TimelineWidgetExt};
use crate::widgets::time_series_plot::{TimeSeriesPlotAction, TimeSeriesPlotWidgetExt};
use crate::widgets::episode_list::{EpisodeListAction, EpisodeListWidgetExt};
use crate::widgets::video_player::VideoPlayerWidgetExt;
use crate::widgets::robot_viewer::RobotViewerWidgetExt;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.studio.*

    // ===========================================
    // APP HEADER (Full Width)
    // ===========================================

    let AppHeader = View{
        width: Fill
        height: 48
        padding: Inset{left: 16. right: 16.}
        flow: Right
        align: Align{y: 0.5}
        spacing: 16

        show_bg: true
        draw_bg.color: mod.widgets.studio.COLOR_BG_HEADER

        // App title
        Label{
            draw_text +: {
                text_style: mod.widgets.studio.TEXT_TITLE{}
                color: mod.widgets.studio.COLOR_TEXT_PRIMARY
            }
            text: "DoRobot Studio"
        }

        View{ width: Fill }  // Spacer

        // Current episode info (right side of header)
        episode_info_header := View{
            width: Fit
            height: Fit
            flow: Right
            spacing: 16
            align: Align{y: 0.5}

            current_episode_label := Label{
                draw_text +: {
                    text_style: mod.widgets.studio.TEXT_SUBTITLE{}
                    color: mod.widgets.studio.COLOR_TEXT_PRIMARY
                }
                text: ""
            }

            frame_info_label := Label{
                draw_text +: {
                    text_style: mod.widgets.studio.TEXT_SMALL{}
                    color: mod.widgets.studio.COLOR_TEXT_SECONDARY
                }
                text: ""
            }
        }
    }

    // ===========================================
    // SIDEBAR COMPONENT (Dataset + Episodes List)
    // ===========================================

    let Sidebar = View{
        width: 280
        height: Fill
        flow: Down

        show_bg: true
        draw_bg.color: mod.widgets.studio.COLOR_BG_SIDEBAR

        // Dataset section with Load button
        dataset_section := View{
            width: Fill
            height: Fit
            padding: 12
            flow: Down
            spacing: 8

            Label{
                draw_text +: {
                    text_style: mod.widgets.studio.TEXT_SMALL{}
                    color: mod.widgets.studio.COLOR_TEXT_MUTED
                }
                text: "DATASET"
            }

            dataset_name := Label{
                draw_text +: {
                    text_style: mod.widgets.studio.TEXT_SUBTITLE{}
                    color: mod.widgets.studio.COLOR_TEXT_PRIMARY
                }
                text: "No dataset loaded"
            }

            dataset_info := Label{
                draw_text +: {
                    text_style: mod.widgets.studio.TEXT_SMALL{}
                    color: mod.widgets.studio.COLOR_TEXT_SECONDARY
                }
                text: ""
            }

            load_btn := PrimaryButton{
                width: Fill
                text: "Load Dataset"
            }
        }

        Divider{}

        // Episode section header
        episode_header := View{
            width: Fill
            height: 36
            padding: Inset{left: 12. right: 12.}
            align: Align{y: 0.5}

            Label{
                draw_text +: {
                    text_style: mod.widgets.studio.TEXT_SMALL{}
                    color: mod.widgets.studio.COLOR_TEXT_MUTED
                }
                text: "EPISODES"
            }
        }

        // Episode list
        episode_list := EpisodeList{}
    }

    // ===========================================
    // MAIN CONTENT PANEL
    // ===========================================

    let MainContent = View{
        width: Fill
        height: Fill
        flow: Down

        show_bg: true
        draw_bg.color: mod.widgets.studio.COLOR_BG_APP

        // Top panel: Video and 3D viewers
        top_panel := View{
            width: Fill
            height: Fill
            flow: Right
            spacing: 4
            padding: 4

            // Video panel
            video_panel := View{
                width: Fill
                height: Fill
                flow: Down
                spacing: 4

                Panel{
                    width: Fill
                    height: Fill
                    primary_video := VideoPlayer{}
                }

                secondary_cameras := View{
                    width: Fill
                    height: 120
                    flow: Right
                    spacing: 4

                    Panel{
                        width: Fill
                        height: Fill
                        camera_1 := VideoPlayer{}
                    }

                    Panel{
                        width: Fill
                        height: Fill
                        camera_2 := VideoPlayer{}
                    }
                }
            }

            VDivider{}

            // 3D Robot viewer
            robot_panel := Panel{
                width: 350
                height: Fill
                robot_viewer := RobotViewer{}
            }
        }

        Divider{}

        // Bottom panel: Plots
        plots_panel := View{
            width: Fill
            height: 180
            flow: Right
            spacing: 4
            padding: 4

            Panel{
                width: Fill
                height: Fill
                state_plot := TimeSeriesPlot{}
            }

            Panel{
                width: Fill
                height: Fill
                action_plot := TimeSeriesPlot{}
            }
        }
    }

    // ===========================================
    // TABLET LAYOUT (Narrower, stacked panels)
    // ===========================================

    let TabletContent = View{
        width: Fill
        height: Fill
        flow: Down

        show_bg: true
        draw_bg.color: mod.widgets.studio.COLOR_BG_APP

        // Top: Video and 3D side by side (smaller)
        top_panel := View{
            width: Fill
            height: 300
            flow: Right
            spacing: 4
            padding: 4

            Panel{
                width: Fill
                height: Fill
                primary_video := VideoPlayer{}
            }

            Panel{
                width: 300
                height: Fill
                CachedWidget{
                    robot_viewer := RobotViewer{}
                }
            }
        }

        // Middle: Plots stacked vertically
        plots_panel := View{
            width: Fill
            height: 200
            flow: Down
            spacing: 4
            padding: 4

            Panel{
                width: Fill
                height: Fill
                state_plot := TimeSeriesPlot{}
            }

            Panel{
                width: Fill
                height: Fill
                action_plot := TimeSeriesPlot{}
            }
        }
    }

    // ===========================================
    // MOBILE LAYOUT (Stack navigation)
    // ===========================================

    let MobileContent = View{
        width: Fill
        height: Fill
        flow: Down

        show_bg: true
        draw_bg.color: mod.widgets.studio.COLOR_BG_APP

        // Full-screen video with tab switching
        content_area := PageFlip{
            width: Fill
            height: Fill
            active_page: @video_page

            video_page := View{
                width: Fill
                height: Fill

                primary_video := VideoPlayer{}
            }

            robot_page := View{
                width: Fill
                height: Fill

                CachedWidget{
                    robot_viewer := RobotViewer{}
                }
            }

            plots_page := View{
                width: Fill
                height: Fill
                flow: Down
                spacing: 4
                padding: 4

                Panel{
                    width: Fill
                    height: Fill
                    state_plot := TimeSeriesPlot{}
                }

                Panel{
                    width: Fill
                    height: Fill
                    action_plot := TimeSeriesPlot{}
                }
            }
        }

        // Tab bar for view switching
        tab_bar := View{
            width: Fill
            height: 48
            flow: Right

            show_bg: true
            draw_bg.color: mod.widgets.studio.COLOR_BG_HEADER

            video_tab := Button{
                width: Fill
                text: "Video"
            }

            robot_tab := Button{
                width: Fill
                text: "3D"
            }

            plots_tab := Button{
                width: Fill
                text: "Plots"
            }
        }

        // Compact timeline
        timeline_panel := View{
            width: Fill
            height: 60
            padding: 4

            CachedWidget{
                timeline := Timeline{}
            }
        }
    }

    // ===========================================
    // FOOTER TIMELINE PLAYER (Full Width with Mini Sidebar)
    // ===========================================

    let FooterTimeline = View{
        width: Fill
        height: 100
        flow: Right

        show_bg: true
        draw_bg.color: mod.widgets.studio.COLOR_BG_HEADER

        // Mini sidebar with playback controls
        mini_sidebar := View{
            width: 280
            height: Fill
            flow: Down
            padding: 8
            spacing: 4

            show_bg: true
            draw_bg.color: mod.widgets.studio.COLOR_BG_SIDEBAR

            // Playback controls row
            playback_controls := View{
                width: Fill
                height: 36
                flow: Right
                spacing: 8
                align: Align{x: 0.5 y: 0.5}

                step_back_btn := SecondaryButton{
                    width: 32
                    height: 32
                    padding: 0
                    text: "<<"
                    draw_text.text_style: mod.widgets.studio.TEXT_SMALL{}
                }

                play_btn := PrimaryButton{
                    width: 48
                    height: 32
                    padding: 0
                    text: "Play"
                    draw_text.text_style: mod.widgets.studio.TEXT_SMALL{}
                }

                step_fwd_btn := SecondaryButton{
                    width: 32
                    height: 32
                    padding: 0
                    text: ">>"
                    draw_text.text_style: mod.widgets.studio.TEXT_SMALL{}
                }
            }

            // Time display
            time_display := View{
                width: Fill
                height: Fit
                flow: Right
                spacing: 8
                align: Align{x: 0.5}

                current_time_label := Label{
                    draw_text +: {
                        text_style: mod.widgets.studio.TEXT_MONO{}
                        color: mod.widgets.studio.COLOR_TEXT_PRIMARY
                    }
                    text: "0:00.00"
                }

                Label{
                    draw_text +: {
                        text_style: mod.widgets.studio.TEXT_SMALL{}
                        color: mod.widgets.studio.COLOR_TEXT_MUTED
                    }
                    text: "/"
                }

                total_time_label := Label{
                    draw_text +: {
                        text_style: mod.widgets.studio.TEXT_MONO{}
                        color: mod.widgets.studio.COLOR_TEXT_SECONDARY
                    }
                    text: "0:00.00"
                }
            }

            // Speed control
            speed_control := View{
                width: Fill
                height: Fit
                flow: Right
                spacing: 4
                align: Align{x: 0.5}

                Label{
                    draw_text +: {
                        text_style: mod.widgets.studio.TEXT_SMALL{}
                        color: mod.widgets.studio.COLOR_TEXT_MUTED
                    }
                    text: "Speed:"
                }

                speed_label := Label{
                    draw_text +: {
                        text_style: mod.widgets.studio.TEXT_SMALL{}
                        color: mod.widgets.studio.COLOR_TEXT_PRIMARY
                    }
                    text: "1.0x"
                }
            }
        }

        VDivider{}

        // Timeline area (fills remaining width)
        timeline_area := View{
            width: Fill
            height: Fill
            padding: 8

            timeline := Timeline{}
        }
    }

    // ===========================================
    // MAIN HOME SCREEN WITH ADAPTIVE LAYOUT
    // ===========================================

    mod.widgets.HomeScreenBase = #(HomeScreen::register_widget(vm))
    mod.widgets.HomeScreen = set_type_default() do mod.widgets.HomeScreenBase{
        width: Fill
        height: Fill
        flow: Down

        show_bg: true
        draw_bg.color: #x1a1a1f

        // Full-width header (fixed)
        app_header := AppHeader{}

        Divider{}

        // Main content area - simplified for testing
        content_area := View{
            width: Fill
            height: Fill
            flow: Right

            // Sidebar
            sidebar := Sidebar{
                width: 280
            }

            VDivider{}

            // Main content
            main_content := MainContent{}
        }

        Divider{}

        // Full-width footer timeline (fixed height)
        footer_timeline := FooterTimeline{}
    }
}

#[derive(Clone, Debug, Default)]
pub enum HomeScreenAction {
    LoadDataset,
    EpisodeSelected(u64),
    TimeChanged(f64),
    PlaybackChanged(bool),
    #[default]
    None,
}

#[derive(Script, ScriptHook, Widget)]
pub struct HomeScreen {
    #[deref]
    view: View,

    // Playback state
    #[rust]
    current_time: f64,
    #[rust]
    is_playing: bool,
    /// Playback speed multiplier - default to 1.0
    #[rust(1.0)]
    playback_speed: f64,

    // Episode state
    #[rust]
    current_episode: Option<u64>,
    #[rust]
    episode_duration: f64,
    /// Episode FPS - default to 30.0 to avoid division by zero
    #[rust(30.0)]
    episode_fps: f64,

    // Playback timer
    #[rust]
    playback_timer: Timer,

    // Episode frame data for robot pose updates
    #[rust]
    episode_frames: Vec<crate::data::EpisodeFrame>,
}

impl Widget for HomeScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Handle playback timer
        if self.playback_timer.is_event(event).is_some() && self.is_playing {
            self.advance_time(cx, 1.0 / 60.0);  // 60 fps update
        }

        // Handle widget actions from the current event cycle
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        self.handle_widget_actions(cx, &actions, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl HomeScreen {
    fn handle_widget_actions(&mut self, cx: &mut Cx, actions: &Actions, _scope: &mut Scope) {
        // Handle timeline actions
        for action in actions {
            // Get widget action and cast its inner action
            if let Some(widget_action) = action.as_widget_action() {
                // Cast the inner action to TimelineAction
                match widget_action.cast::<TimelineAction>() {
                    TimelineAction::Seek(time) => {
                        self.seek_to(cx, time);
                    }
                    TimelineAction::Play => {
                        self.set_playing(cx, true);
                    }
                    TimelineAction::Pause => {
                        self.set_playing(cx, false);
                    }
                    TimelineAction::StepForward => {
                        self.step_frame(cx, 1);
                    }
                    TimelineAction::StepBackward => {
                        self.step_frame(cx, -1);
                    }
                    _ => {}
                }

                // Time series plot cursor moved
                if let TimeSeriesPlotAction::CursorMoved(time) = widget_action.cast() {
                    self.seek_to(cx, time);
                }
            }

            // Episode list actions (from cx.action())
            if let Some(EpisodeListAction::EpisodeSelected(idx)) = action.downcast_ref::<EpisodeListAction>() {
                self.select_episode(cx, *idx);
                // Fire action to parent to load episode data
                cx.widget_action(
                    self.widget_uid(),
                    HomeScreenAction::EpisodeSelected(*idx),
                );
            }
            if let Some(EpisodeListAction::EpisodeDoubleClicked(idx)) = action.downcast_ref::<EpisodeListAction>() {
                self.load_episode(cx, *idx);
            }
        }

        // Handle load button click
        if self.view.button(cx, ids!(load_btn)).clicked(actions) {
            cx.widget_action(
                self.widget_uid(),
                HomeScreenAction::LoadDataset,
            );
        }

        // Handle footer timeline buttons (play/pause, step buttons)
        if self.view.button(cx, ids!(footer_timeline.mini_sidebar.playback_controls.play_btn)).clicked(actions) {
            self.is_playing = !self.is_playing;
            self.set_playing(cx, self.is_playing);

            // Update button text
            let text = if self.is_playing { "Pause" } else { "Play" };
            self.view.button(cx, ids!(footer_timeline.mini_sidebar.playback_controls.play_btn))
                .set_text(cx, text);
        }

        if self.view.button(cx, ids!(footer_timeline.mini_sidebar.playback_controls.step_back_btn)).clicked(actions) {
            self.step_frame(cx, -1);
        }

        if self.view.button(cx, ids!(footer_timeline.mini_sidebar.playback_controls.step_fwd_btn)).clicked(actions) {
            self.step_frame(cx, 1);
        }
    }

    /// Set custom variant selector for responsive breakpoints
    /// Note: AdaptiveView currently disabled due to rendering issues
    pub fn setup_adaptive_layout(&mut self, _cx: &mut Cx) {
        // Adaptive layout disabled - using fixed desktop layout
    }

    /// Seek to a specific time
    fn seek_to(&mut self, cx: &mut Cx, time: f64) {
        self.current_time = time.clamp(0.0, self.episode_duration);

        // Update all synchronized components
        self.view.timeline(cx, ids!(footer_timeline.timeline_area.timeline))
            .set_current_time(cx, self.current_time);

        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.state_plot))
            .set_cursor_time(cx, self.current_time);

        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.action_plot))
            .set_cursor_time(cx, self.current_time);

        // Update footer time display
        self.view.label(cx, ids!(footer_timeline.mini_sidebar.time_display.current_time_label))
            .set_text(cx, &Self::format_time(self.current_time));
        self.view.label(cx, ids!(footer_timeline.mini_sidebar.time_display.total_time_label))
            .set_text(cx, &Self::format_time(self.episode_duration));

        // Update video frame
        let frame_idx = (self.current_time * self.episode_fps) as u64;
        self.update_video_frame(cx, frame_idx);

        // Update robot pose
        self.update_robot_pose(cx);
    }

    /// Format time as M:SS.CC
    fn format_time(seconds: f64) -> String {
        let mins = (seconds / 60.0) as u32;
        let secs = seconds % 60.0;
        format!("{}:{:05.2}", mins, secs)
    }

    /// Advance time by delta (for playback)
    fn advance_time(&mut self, cx: &mut Cx, delta: f64) {
        let new_time = self.current_time + delta * self.playback_speed;

        if new_time >= self.episode_duration {
            // End of episode
            self.set_playing(cx, false);
            self.seek_to(cx, self.episode_duration);
        } else {
            self.seek_to(cx, new_time);
        }
    }

    /// Step forward/backward by frames
    fn step_frame(&mut self, cx: &mut Cx, frames: i32) {
        let frame_duration = 1.0 / self.episode_fps;
        let new_time = self.current_time + (frames as f64) * frame_duration;
        self.seek_to(cx, new_time.clamp(0.0, self.episode_duration));
    }

    /// Set playing state
    fn set_playing(&mut self, cx: &mut Cx, playing: bool) {
        // Debug: log play state changes
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[HomeScreen] set_playing called: playing={}", playing);
        }

        self.is_playing = playing;

        self.view.timeline(cx, ids!(footer_timeline.timeline_area.timeline))
            .set_playing(cx, playing);

        if playing {
            self.playback_timer = cx.start_interval(1.0 / 60.0);  // 60 fps
        } else {
            self.playback_timer = Timer::default();
        }
    }

    /// Select an episode (highlight in list and show details)
    fn select_episode(&mut self, cx: &mut Cx, episode_idx: u64) {
        self.current_episode = Some(episode_idx);
        let frame_count = 150 + (episode_idx % 100);
        let fps = 30.0;

        // Update episode info in header
        self.view.label(cx, ids!(app_header.episode_info_header.current_episode_label))
            .set_text(cx, &format!("Episode {}", episode_idx));
        self.view.label(cx, ids!(app_header.episode_info_header.frame_info_label))
            .set_text(cx, &format!("{} frames", frame_count));

        // Update video player labels and show episode indicator
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.primary_video))
            .set_camera_name(cx, "cam_high");
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.primary_video))
            .set_frame_info(cx, 0, frame_count);

        // Update secondary cameras
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_1))
            .set_camera_name(cx, "cam_left_wrist");
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_1))
            .set_frame_info(cx, 0, frame_count);
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_2))
            .set_camera_name(cx, "cam_right_wrist");
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_2))
            .set_frame_info(cx, 0, frame_count);

        // Initialize placeholder decoders for video playback (demo mode)
        // Primary camera: higher resolution
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.primary_video))
            .init_placeholder(640, 480, fps, frame_count);
        // Secondary cameras: smaller resolution
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_1))
            .init_placeholder(320, 240, fps, frame_count);
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_2))
            .init_placeholder(320, 240, fps, frame_count);

        // Show first frame immediately
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.primary_video))
            .show_frame_at_time(cx, 0.0);
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_1))
            .show_frame_at_time(cx, 0.0);
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_2))
            .show_frame_at_time(cx, 0.0);

        // Set timeline duration
        self.episode_duration = frame_count as f64 / 30.0;
        self.episode_fps = 30.0;
        self.current_time = 0.0;

        self.view.timeline(cx, ids!(footer_timeline.timeline_area.timeline))
            .set_duration(cx, self.episode_duration, self.episode_fps);
        self.view.timeline(cx, ids!(footer_timeline.timeline_area.timeline))
            .set_current_time(cx, 0.0);

        // Update plot titles
        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.state_plot))
            .set_title(cx, "observation.state");
        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.action_plot))
            .set_title(cx, "action");

        // Update plot placeholder text using the new method
        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.state_plot))
            .set_placeholder_text(cx, &format!("Episode {} - 14 state channels\nJoint positions, velocities", episode_idx));
        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.action_plot))
            .set_placeholder_text(cx, &format!("Episode {} - 14 action channels\nTarget joint positions", episode_idx));

        // Update time range for plots
        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.state_plot))
            .set_time_range(0.0, self.episode_duration);
        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.action_plot))
            .set_time_range(0.0, self.episode_duration);

        // Update robot viewer placeholder
        self.view.robot_viewer(cx, ids!(content_area.main_content.top_panel.robot_panel.robot_viewer))
            .set_placeholder_text(cx, &format!("Episode {} - ALOHA Robot\n7 DOF dual arm", episode_idx));

        self.view.redraw(cx);
    }

    /// Load and display an episode (double-click)
    fn load_episode(&mut self, cx: &mut Cx, episode_idx: u64) {
        // Same as select for now, but could trigger video loading
        self.select_episode(cx, episode_idx);
    }

    /// Update video frame (called on time change)
    fn update_video_frame(&mut self, cx: &mut Cx, frame_idx: u64) {
        let total_frames = (self.episode_duration * self.episode_fps) as u64;

        // Update frame info display
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.primary_video))
            .set_frame_info(cx, frame_idx, total_frames);
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_1))
            .set_frame_info(cx, frame_idx, total_frames);
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_2))
            .set_frame_info(cx, frame_idx, total_frames);

        // Show frame at current time (uses placeholder decoder)
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.primary_video))
            .show_frame_at_time(cx, self.current_time);
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_1))
            .show_frame_at_time(cx, self.current_time);
        self.view.video_player(cx, ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_2))
            .show_frame_at_time(cx, self.current_time);
    }

    /// Update robot pose (called on time change)
    fn update_robot_pose(&mut self, cx: &mut Cx) {
        if self.episode_frames.is_empty() {
            return;
        }

        // Find the frame closest to current_time
        let frame_idx = self.episode_frames
            .iter()
            .position(|f| f.timestamp >= self.current_time)
            .unwrap_or(self.episode_frames.len() - 1);

        // Get joint angles from the state data
        if let Some(frame) = self.episode_frames.get(frame_idx) {
            // Convert state to f64 slice for the robot viewer
            let joint_angles: Vec<f64> = frame.state.iter().map(|&v| v as f64).collect();

            self.view.robot_viewer(cx, ids!(content_area.main_content.top_panel.robot_panel.robot_viewer))
                .set_joint_angles(cx, &joint_angles);
        }
    }

    /// Set dataset info
    pub fn set_dataset_info(&mut self, cx: &mut Cx, name: &str, info: &str) {
        self.view.label(cx, ids!(content_area.sidebar.dataset_section.dataset_name))
            .set_text(cx, name);
        self.view.label(cx, ids!(content_area.sidebar.dataset_section.dataset_info))
            .set_text(cx, info);
    }

    /// Initialize plots with channel names
    pub fn init_plots(&mut self, cx: &mut Cx, state_channels: &[&str], action_channels: &[&str]) {
        // State plot
        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.state_plot))
            .set_title(cx, "observation.state");

        for (i, name) in state_channels.iter().enumerate() {
            // Would set channel data here
            let _ = (i, name);
        }

        // Action plot
        self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.action_plot))
            .set_title(cx, "action");

        for (i, name) in action_channels.iter().enumerate() {
            let _ = (i, name);
        }
    }

    /// Set the episode list
    pub fn set_episodes(&mut self, cx: &mut Cx, episodes: Vec<crate::widgets::episode_list::EpisodeInfo>) {
        self.view.episode_list(cx, ids!(content_area.sidebar.episode_list))
            .set_episodes(cx, episodes);
    }

    /// Set episode data when an episode is loaded
    pub fn set_episode_data(&mut self, cx: &mut Cx, episode_idx: u64, frame_count: u64) {
        self.current_episode = Some(episode_idx);
        self.episode_duration = frame_count as f64 / self.episode_fps;
        self.current_time = 0.0;

        // Update timeline duration
        self.view.timeline(cx, ids!(footer_timeline.timeline_area.timeline))
            .set_duration(cx, self.episode_duration, self.episode_fps);

        // Reset to start
        self.seek_to(cx, 0.0);

        ::log::info!("Episode {} loaded: {} frames, {:.2}s duration",
            episode_idx, frame_count, self.episode_duration);
    }

    /// Set the time series data for plots from episode frames
    pub fn set_frame_data(&mut self, cx: &mut Cx, frames: &[crate::data::EpisodeFrame]) {
        if frames.is_empty() {
            return;
        }

        // Store frames for robot pose updates
        self.episode_frames = frames.to_vec();

        // Get number of state and action channels
        let state_channels = frames.first().map(|f| f.state.len()).unwrap_or(0);
        let action_channels = frames.first().map(|f| f.action.len()).unwrap_or(0);

        // Update time range
        let duration = frames.last().map(|f| f.timestamp).unwrap_or(0.0);
        self.episode_duration = duration;

        // Populate state plot
        let state_plot = self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.state_plot));
        state_plot.set_title(cx, "observation.state");
        state_plot.set_time_range(0.0, duration);
        state_plot.set_auto_scale_y(true);
        // Set sliding window: show 10 seconds of data centered on cursor
        // (must be smaller than episode duration to allow sliding)
        state_plot.set_window_size(10.0);

        for ch in 0..state_channels.min(14) {
            let data: Vec<(f64, f64)> = frames.iter()
                .map(|f| (f.timestamp, f.state.get(ch).copied().unwrap_or(0.0) as f64))
                .collect();
            let name = format!("state[{}]", ch);
            state_plot.set_channel_data(ch, &name, data);
        }
        // Force recompute Y-axis scale after all channels are loaded
        state_plot.recompute_scale(cx);

        // Populate action plot
        let action_plot = self.view.time_series_plot(cx, ids!(content_area.main_content.plots_panel.action_plot));
        action_plot.set_title(cx, "action");
        action_plot.set_time_range(0.0, duration);
        action_plot.set_auto_scale_y(true);
        // Set sliding window: show 10 seconds of data centered on cursor
        action_plot.set_window_size(10.0);

        for ch in 0..action_channels.min(14) {
            let data: Vec<(f64, f64)> = frames.iter()
                .map(|f| (f.timestamp, f.action.get(ch).copied().unwrap_or(0.0) as f64))
                .collect();
            let name = format!("action[{}]", ch);
            action_plot.set_channel_data(ch, &name, data);
        }
        // Force recompute Y-axis scale after all channels are loaded
        action_plot.recompute_scale(cx);

        // Update timeline
        self.view.timeline(cx, ids!(footer_timeline.timeline_area.timeline))
            .set_duration(cx, duration, self.episode_fps);

        self.view.redraw(cx);
    }

    /// Set video paths for the cameras
    ///
    /// # Arguments
    /// * `video_paths` - Map of camera name to video file path
    /// * `frame_offset` - Starting frame index in the video file (for v3.0 concatenated videos)
    pub fn set_video_paths(&mut self, cx: &mut Cx, video_paths: &HashMap<String, PathBuf>, frame_offset: u64) {
        // Debug: write to file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[HomeScreen] set_video_paths called with {} paths, frame_offset={}", video_paths.len(), frame_offset);
            for (name, path) in video_paths.iter() {
                let _ = writeln!(f, "[HomeScreen]   {}: {}", name, path.display());
            }
        }
        let fps = self.episode_fps;
        let frame_count = (self.episode_duration * fps) as u64;

        // Primary camera - cam_high
        self.load_camera_video(
            cx,
            video_paths.get("observation.images.cam_high"),
            ids!(content_area.main_content.top_panel.video_panel.primary_video),
            "cam_high",
            fps,
            frame_count,
            frame_offset,
        );

        // Secondary camera 1 - cam_left_wrist
        self.load_camera_video(
            cx,
            video_paths.get("observation.images.cam_left_wrist"),
            ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_1),
            "cam_left_wrist",
            fps,
            frame_count,
            frame_offset,
        );

        // Secondary camera 2 - cam_right_wrist
        self.load_camera_video(
            cx,
            video_paths.get("observation.images.cam_right_wrist"),
            ids!(content_area.main_content.top_panel.video_panel.secondary_cameras.camera_2),
            "cam_right_wrist",
            fps,
            frame_count,
            frame_offset,
        );
    }

    fn load_camera_video(
        &mut self,
        cx: &mut Cx,
        video_path: Option<&PathBuf>,
        widget_id: &[LiveId],
        camera_name: &str,
        fps: f64,
        frame_count: u64,
        frame_offset: u64,
    ) {
        let video_player = self.view.video_player(cx, widget_id);
        video_player.set_camera_name(cx, camera_name);
        video_player.set_episode_info(frame_offset, frame_count);

        if let Some(path) = video_path {
            let path_str = path.to_string_lossy();
            ::log::info!("Loading video for {}: {} (offset: {})", camera_name, path_str, frame_offset);

            if let Err(e) = video_player.load_video(cx, &path_str) {
                ::log::warn!("Failed to load video {}: {}", path_str, e);
                // Fall back to placeholder
                video_player.init_placeholder(640, 480, fps, frame_count);
                video_player.show_frame_at_time(cx, 0.0);
            } else {
                // Show first frame of the episode
                video_player.show_frame_at_time(cx, 0.0);
            }
        } else {
            ::log::info!("No video path for {}, using placeholder", camera_name);
            video_player.init_placeholder(640, 480, fps, frame_count);
            video_player.show_frame_at_time(cx, 0.0);
        }
    }
}

impl HomeScreenRef {
    pub fn set_dataset_info(&self, cx: &mut Cx, name: &str, info: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_dataset_info(cx, name, info);
        }
    }

    pub fn setup_adaptive_layout(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.setup_adaptive_layout(cx);
        }
    }

    pub fn set_episodes(&self, cx: &mut Cx, episodes: Vec<crate::widgets::episode_list::EpisodeInfo>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_episodes(cx, episodes);
        }
    }

    pub fn set_episode_data(&self, cx: &mut Cx, episode_idx: u64, frame_count: u64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_episode_data(cx, episode_idx, frame_count);
        }
    }

    pub fn set_frame_data(&self, cx: &mut Cx, frames: &[crate::data::EpisodeFrame]) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_frame_data(cx, frames);
        }
    }

    pub fn set_video_paths(&self, cx: &mut Cx, video_paths: &HashMap<String, PathBuf>, frame_offset: u64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_video_paths(cx, video_paths, frame_offset);
        }
    }
}
