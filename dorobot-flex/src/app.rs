//! DoRobot Flex - Main Application with Shell Layout
//!
//! Uses makepad-app-shell for a professional IDE-style layout with:
//! - Draggable panels with drag-and-drop
//! - Resizable sidebars
//! - Layout persistence (Save/Reset)
//! - Dark/light theme support

use makepad_widgets::*;
use makepad_app_shell::grid::panel_grid::PanelGridWidgetRefExt;
use makepad_app_shell::grid::footer_grid::FooterGridWidgetRefExt;
use makepad_app_shell::grid::{LayoutState, FooterLayoutState, FooterSlotState};
use crate::app_data::AppData;
use crate::data::LeRobotDataset;
use crate::sidebar_content::SidebarAction;
use crate::playback_controls::PlaybackAction;
use crate::widgets::timeline::TimelineAction;
use crate::widgets::time_series_plot::TimeSeriesPlotAction;
use crate::widgets::video_player::VideoPlayerWidgetRefExt;
use crate::widgets::robot_viewer::RobotViewerWidgetRefExt;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    // Import shell components
    use makepad_app_shell::shell::layout::ShellLayout;
    use makepad_app_shell::grid::panel_grid::PanelGrid;
    use makepad_app_shell::grid::footer_grid::FooterGrid;
    use makepad_app_shell::panel::panel::Panel;
    use makepad_app_shell::shell::sidebar::ShellSidebar;

    // Import our components
    use crate::shared::styles::*;
    use crate::sidebar_content::SidebarContent;
    use crate::episode_info_panel::EpisodeInfoPanel;
    use crate::playback_controls::PlaybackControls;
    use crate::footer_stack::FooterStack;
    use crate::widgets::video_player::VideoPlayer;
    use crate::widgets::robot_viewer::RobotViewer;
    use crate::widgets::time_series_plot::TimeSeriesPlot;
    use crate::widgets::timeline::Timeline;

    DoRobotApp = {{DoRobotApp}} {
        ui: <Root> {
            main_window = <Window> {
                window: {
                    title: "DoRobot Studio"
                    inner_size: vec2(1400, 900)
                }

                body = <ShellLayout> {
                    // Override the header title
                    main_container = {
                        header = {
                            title_label = { text: "DoRobot Studio" }
                        }

                        dock_wrapper = {
                            dock = {
                                // Override left sidebar content - Dataset (plain View to fill entire space)
                                left_sidebar_content = <View> {
                                    width: Fill, height: Fill
                                    flow: Down

                                    show_bg: true
                                    draw_bg: { color: (COLOR_BG_SIDEBAR) }

                                    // Header
                                    <View> {
                                        width: Fill, height: 40
                                        padding: { left: 16 }
                                        align: { y: 0.5 }
                                        show_bg: true
                                        draw_bg: { color: (COLOR_BG_HEADER) }
                                        <Label> {
                                            draw_text: {
                                                text_style: <FONT_SEMIBOLD> { font_size: 12.0 }
                                                color: (COLOR_TEXT_PRIMARY)
                                            }
                                            text: "Dataset"
                                        }
                                    }

                                    // Content fills remaining space
                                    sidebar_content = <SidebarContent> {
                                        width: Fill, height: Fill
                                    }
                                }

                                // Override center content with our panels (2x2 grid)
                                center_content = <PanelGrid> {
                                    window_container = {
                                        row1 = {
                                            s1_1 = {
                                                title_bar = { title = { text: "cam_high" } }
                                                content = {
                                                    video_main = <VideoPlayer> {}
                                                }
                                            }
                                            s1_2 = {
                                                title_bar = { title = { text: "3D Robot Viewer" } }
                                                content = {
                                                    robot_viewer = <RobotViewer> {}
                                                }
                                            }
                                            // Hide unused slots in row1
                                            s1_3 = { visible: false, width: 0, height: 0 }
                                            s1_4 = { visible: false, width: 0, height: 0 }
                                            s1_5 = { visible: false, width: 0, height: 0 }
                                            s1_6 = { visible: false, width: 0, height: 0 }
                                            s1_7 = { visible: false, width: 0, height: 0 }
                                            s1_8 = { visible: false, width: 0, height: 0 }
                                            s1_9 = { visible: false, width: 0, height: 0 }
                                        }
                                        row2 = {
                                            s2_1 = {
                                                title_bar = { title = { text: "cam_left_wrist" } }
                                                content = {
                                                    video_cam1 = <VideoPlayer> {}
                                                }
                                            }
                                            s2_2 = {
                                                title_bar = { title = { text: "cam_right_wrist" } }
                                                content = {
                                                    video_cam2 = <VideoPlayer> {}
                                                }
                                            }
                                            // Hide unused slots in row2
                                            s2_3 = { visible: false, width: 0, height: 0 }
                                            s2_4 = { visible: false, width: 0, height: 0 }
                                            s2_5 = { visible: false, width: 0, height: 0 }
                                            s2_6 = { visible: false, width: 0, height: 0 }
                                            s2_7 = { visible: false, width: 0, height: 0 }
                                            s2_8 = { visible: false, width: 0, height: 0 }
                                            s2_9 = { visible: false, width: 0, height: 0 }
                                        }
                                        // Hide entire row3
                                        row3 = { visible: false, height: 0 }
                                    }
                                }

                                // Override right sidebar content - Episode Info (plain View to fill entire space)
                                right_sidebar_content = <View> {
                                    width: Fill, height: Fill
                                    flow: Down

                                    show_bg: true
                                    draw_bg: { color: (COLOR_BG_SIDEBAR) }

                                    // Header
                                    <View> {
                                        width: Fill, height: 40
                                        padding: { left: 16 }
                                        align: { y: 0.5 }
                                        show_bg: true
                                        draw_bg: { color: (COLOR_BG_HEADER) }
                                        <Label> {
                                            draw_text: {
                                                text_style: <FONT_SEMIBOLD> { font_size: 12.0 }
                                                color: (COLOR_TEXT_PRIMARY)
                                            }
                                            text: "Episode Info"
                                        }
                                    }

                                    // Content fills remaining space
                                    episode_info = <EpisodeInfoPanel> {
                                        width: Fill, height: Fill
                                    }
                                }

                                // Override footer content
                                // Playback controls in left sidebar, 3 panels: State Plot, Action Plot, Timeline
                                footer_content = <FooterGrid> {
                                    initial_panels: 3

                                    dock = {
                                        // Left controller sidebar - Playback controls (plain View to fill entire space)
                                        controller_content = <View> {
                                            width: Fill, height: Fill
                                            flow: Down

                                            show_bg: true
                                            draw_bg: { color: (COLOR_BG_SIDEBAR) }

                                            // Header
                                            <View> {
                                                width: Fill, height: 40
                                                padding: { left: 16 }
                                                align: { y: 0.5 }
                                                show_bg: true
                                                draw_bg: { color: (COLOR_BG_HEADER) }
                                                <Label> {
                                                    draw_text: {
                                                        text_style: <FONT_SEMIBOLD> { font_size: 12.0 }
                                                        color: (COLOR_TEXT_PRIMARY)
                                                    }
                                                    text: "Playback"
                                                }
                                            }

                                            // Content fills remaining space
                                            playback = <PlaybackControls> {
                                                width: Fill, height: Fill
                                            }
                                        }

                                        // 3 footer panels stacked vertically: State Plot, Action Plot, Timeline
                                        panel_strip_content = {
                                            flow: Down
                                            f1_0 = {
                                                p0 = {
                                                    title_bar = { title = { text: "State Plot" } }
                                                    content = {
                                                        state_plot = <TimeSeriesPlot> {}
                                                    }
                                                }
                                            }
                                            f1_1 = {
                                                p0 = {
                                                    title_bar = { title = { text: "Action Plot" } }
                                                    content = {
                                                        action_plot = <TimeSeriesPlot> {}
                                                    }
                                                }
                                            }
                                            f1_2 = {
                                                p0 = {
                                                    title_bar = { title = { text: "Timeline" } }
                                                    content = {
                                                        timeline = <Timeline> {}
                                                    }
                                                }
                                            }
                                            // Hide unused footer slots
                                            f1_3 = { visible: false, width: 0 }
                                            f1_4 = { visible: false, width: 0 }
                                            f1_5 = { visible: false, width: 0 }
                                            f1_6 = { visible: false, width: 0 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Live, LiveHook)]
pub struct DoRobotApp {
    #[live]
    ui: WidgetRef,

    /// Shared app state passed through Scope
    #[rust]
    data: AppData,

    /// Playback timer (60 fps updates)
    #[rust]
    playback_timer: Timer,

    /// Track if videos need initialization
    #[rust]
    videos_initialized: bool,

    /// Track current episode to detect changes
    #[rust]
    last_episode: Option<u64>,
}

impl LiveRegister for DoRobotApp {
    fn live_register(cx: &mut Cx) {
        // Register Makepad widgets
        makepad_widgets::live_design(cx);

        // Register shell widgets
        makepad_app_shell::live_design(cx);

        // Register our modules
        crate::shared::live_design(cx);
        crate::widgets::live_design(cx);
        crate::sidebar_content::live_design(cx);
        crate::episode_info_panel::live_design(cx);
        crate::playback_controls::live_design(cx);
        crate::footer_stack::live_design(cx);
    }
}

impl MatchEvent for DoRobotApp {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // Start playback timer
        self.playback_timer = cx.start_interval(1.0 / 60.0);

        // Configure PanelGrid to show only 4 panels (2x2 grid)
        // panel_0 = cam_high, panel_1 = robot_viewer
        // panel_2 = cam_left_wrist, panel_3 = cam_right_wrist
        let panel_grid = self.ui.panel_grid(id!(center_content));
        panel_grid.set_layout_state(cx, LayoutState::with_panel_count(4));

        // Configure FooterGrid to show 3 panels (State Plot, Action Plot, Timeline)
        let footer_grid = self.ui.footer_grid(id!(footer_content));
        let footer_state = FooterLayoutState {
            slots: vec![
                FooterSlotState { visible: true, panel_ids: vec!["footer_panel_0".into()] },
                FooterSlotState { visible: true, panel_ids: vec!["footer_panel_1".into()] },
                FooterSlotState { visible: true, panel_ids: vec!["footer_panel_2".into()] },
            ],
            fullscreen_panel: None,
        };
        footer_grid.set_layout_state(cx, footer_state);

        // Try to load dataset from command line or default paths
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 {
            self.load_dataset(cx, &args[1]);
        } else {
            self.try_load_default_dataset(cx);
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        for action in actions.iter() {
            if let Some(widget_action) = action.as_widget_action() {
                // Handle sidebar actions (load dataset, episode selection)
                match widget_action.cast::<SidebarAction>() {
                    SidebarAction::LoadDataset => {
                        self.open_dataset_dialog(cx);
                    }
                    SidebarAction::EpisodeSelected(idx) => {
                        self.load_episode(cx, idx);
                    }
                    SidebarAction::None => {}
                }

                // Handle playback actions
                match widget_action.cast::<PlaybackAction>() {
                    PlaybackAction::Play => {
                        self.data.is_playing = true;
                    }
                    PlaybackAction::Pause => {
                        self.data.is_playing = false;
                    }
                    PlaybackAction::StepForward => {
                        self.step_frame(cx, 1);
                    }
                    PlaybackAction::StepBackward => {
                        self.step_frame(cx, -1);
                    }
                    PlaybackAction::None => {}
                }

                // Handle timeline actions
                match widget_action.cast::<TimelineAction>() {
                    TimelineAction::Seek(time) => {
                        self.seek_to(cx, time);
                    }
                    TimelineAction::Play => {
                        self.data.is_playing = true;
                    }
                    TimelineAction::Pause => {
                        self.data.is_playing = false;
                    }
                    _ => {}
                }

                // Handle plot cursor actions
                match widget_action.cast::<TimeSeriesPlotAction>() {
                    TimeSeriesPlotAction::CursorMoved(time) => {
                        self.seek_to(cx, time);
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_timer(&mut self, cx: &mut Cx, _event: &TimerEvent) {
        if self.data.is_playing {
            self.advance_time(cx, 1.0 / 60.0);
        }
    }
}

impl AppMain for DoRobotApp {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);

        // Check if episode changed - need to reinitialize videos
        if self.data.current_episode != self.last_episode {
            self.last_episode = self.data.current_episode;
            self.videos_initialized = false;
        }

        // Initialize/update videos
        if !self.videos_initialized && self.data.current_episode.is_some() {
            self.init_videos(cx);
            self.videos_initialized = true;
        }

        // Update video frames
        self.update_videos(cx);

        // Update robot viewer
        self.update_robot_viewer(cx);

        // Pass app data through scope to all widgets
        self.ui.handle_event(cx, event, &mut Scope::with_data(&mut self.data));
    }
}

impl DoRobotApp {
    fn try_load_default_dataset(&mut self, cx: &mut Cx) {
        let known_paths = [
            "dataset/aloha_mobile_cabinet",
            "dataset/xvla-soft-fold",
            "../dataset/aloha_mobile_cabinet",
        ];

        for path in &known_paths {
            if std::path::Path::new(path).join("meta").exists() {
                ::log::info!("Found dataset at {}", path);
                self.load_dataset(cx, path);
                return;
            }
        }

        ::log::info!("No default dataset found");
    }

    fn open_dataset_dialog(&mut self, cx: &mut Cx) {
        let dialog = rfd::FileDialog::new()
            .set_title("Select LeRobot Dataset Folder")
            .set_directory(std::env::current_dir().unwrap_or_default());

        if let Some(folder_path) = dialog.pick_folder() {
            if let Some(path_str) = folder_path.to_str() {
                self.load_dataset(cx, path_str);
            }
        }
    }

    fn load_dataset(&mut self, cx: &mut Cx, path: &str) {
        ::log::info!("Loading dataset from {}", path);

        match LeRobotDataset::open(path) {
            Ok(dataset) => {
                ::log::info!("Dataset loaded: {} episodes", dataset.num_episodes());

                // Extract name from path
                let name = std::path::Path::new(path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("dataset")
                    .to_string();

                // Build info string
                let info = format!(
                    "{} episodes | {} fps | {}",
                    dataset.info.total_episodes,
                    dataset.info.fps,
                    dataset.info.robot_type
                );

                // Build episode list
                let episodes: Vec<crate::widgets::episode_list::EpisodeInfo> = dataset.episodes
                    .iter()
                    .map(|ep| {
                        let task_desc = ep.tasks.first()
                            .and_then(|t| dataset.get_task(*t))
                            .unwrap_or("No task description")
                            .to_string();

                        crate::widgets::episode_list::EpisodeInfo {
                            index: ep.episode_index,
                            frame_count: ep.length,
                            duration_secs: ep.length as f64 / dataset.info.fps,
                            task_description: task_desc,
                            task_index: ep.tasks.first().copied().unwrap_or(0),
                        }
                    })
                    .collect();

                // Update app data
                self.data.dataset_name = name;
                self.data.dataset_info = info;
                self.data.episode_fps = dataset.info.fps;
                self.data.episodes = episodes;
                self.data.dataset = Some(dataset);

                // Auto-select first episode
                self.load_episode(cx, 0);
            }
            Err(e) => {
                ::log::error!("Failed to load dataset from {}: {}", path, e);
            }
        }
    }

    fn load_episode(&mut self, cx: &mut Cx, episode_idx: u64) {
        ::log::info!("Loading episode {}", episode_idx);

        if let Some(dataset) = &self.data.dataset {
            match dataset.load_episode(episode_idx) {
                Ok(episode_data) => {
                    ::log::info!(
                        "Episode {} loaded: {} frames, {} video paths",
                        episode_idx,
                        episode_data.frames.len(),
                        episode_data.video_paths.len()
                    );

                    self.data.current_episode = Some(episode_idx);
                    self.data.episode_frames = episode_data.frames;
                    self.data.video_paths = episode_data.video_paths;
                    self.data.video_frame_offset = episode_data.video_frame_offset;
                    self.data.episode_duration = self.data.episode_frames.len() as f64 / self.data.episode_fps;
                    self.data.current_time = 0.0;
                    self.data.is_playing = false;

                    self.ui.redraw(cx);
                }
                Err(e) => {
                    ::log::error!("Failed to load episode {}: {}", episode_idx, e);
                }
            }
        }
    }

    fn seek_to(&mut self, cx: &mut Cx, time: f64) {
        self.data.current_time = time.clamp(0.0, self.data.episode_duration);
        self.ui.redraw(cx);
    }

    fn advance_time(&mut self, cx: &mut Cx, delta: f64) {
        let new_time = self.data.current_time + delta * self.data.playback_speed;

        if new_time >= self.data.episode_duration {
            // End of episode
            self.data.is_playing = false;
            self.data.current_time = self.data.episode_duration;
        } else {
            self.data.current_time = new_time;
        }

        self.ui.redraw(cx);
    }

    fn step_frame(&mut self, cx: &mut Cx, frames: i32) {
        let frame_duration = 1.0 / self.data.episode_fps;
        let new_time = self.data.current_time + (frames as f64) * frame_duration;
        self.seek_to(cx, new_time);
    }

    fn init_videos(&mut self, cx: &mut Cx) {
        let total_frames = self.data.total_frames();

        // Initialize main video
        let video_main = self.ui.video_player(id!(video_main));
        video_main.set_camera_name(cx, "cam_high");
        video_main.set_episode_info(self.data.video_frame_offset, total_frames);
        if let Some(path) = self.data.video_paths.get("observation.images.cam_high") {
            let _ = video_main.load_video(cx, &path.to_string_lossy());
        } else {
            video_main.init_placeholder(640, 480, self.data.episode_fps, total_frames);
        }

        // Initialize cam1
        let video_cam1 = self.ui.video_player(id!(video_cam1));
        video_cam1.set_camera_name(cx, "cam_left_wrist");
        video_cam1.set_episode_info(self.data.video_frame_offset, total_frames);
        if let Some(path) = self.data.video_paths.get("observation.images.cam_left_wrist") {
            let _ = video_cam1.load_video(cx, &path.to_string_lossy());
        } else {
            video_cam1.init_placeholder(640, 480, self.data.episode_fps, total_frames);
        }

        // Initialize cam2
        let video_cam2 = self.ui.video_player(id!(video_cam2));
        video_cam2.set_camera_name(cx, "cam_right_wrist");
        video_cam2.set_episode_info(self.data.video_frame_offset, total_frames);
        if let Some(path) = self.data.video_paths.get("observation.images.cam_right_wrist") {
            let _ = video_cam2.load_video(cx, &path.to_string_lossy());
        } else {
            video_cam2.init_placeholder(640, 480, self.data.episode_fps, total_frames);
        }
    }

    fn update_videos(&mut self, cx: &mut Cx) {
        let frame_idx = self.data.current_frame_index();
        let total = self.data.total_frames();

        // Update main video
        let video_main = self.ui.video_player(id!(video_main));
        video_main.show_frame_at_time(cx, self.data.current_time);
        video_main.set_frame_info(cx, frame_idx, total);

        // Update cam1
        let video_cam1 = self.ui.video_player(id!(video_cam1));
        video_cam1.show_frame_at_time(cx, self.data.current_time);
        video_cam1.set_frame_info(cx, frame_idx, total);

        // Update cam2
        let video_cam2 = self.ui.video_player(id!(video_cam2));
        video_cam2.show_frame_at_time(cx, self.data.current_time);
        video_cam2.set_frame_info(cx, frame_idx, total);
    }

    fn update_robot_viewer(&mut self, cx: &mut Cx) {
        if let Some(frame) = self.data.current_frame() {
            let joint_angles: Vec<f64> = frame.state.iter().map(|&v| v as f64).collect();
            let robot_viewer = self.ui.robot_viewer(id!(robot_viewer));
            robot_viewer.set_joint_angles(cx, &joint_angles);
        }
    }
}

app_main!(DoRobotApp);
