//! DoRobot Flex - Main Application with Shell Layout
//!
//! Uses makepad-app-shell for a professional IDE-style layout with:
//! - Draggable panels with drag-and-drop
//! - Resizable sidebars
//! - Layout persistence (Save/Reset)
//! - Dark/light theme support

use std::time::Instant;
use makepad_widgets::*;
use makepad_app_shell::grid::panel_grid::PanelGridWidgetRefExt;
use makepad_app_shell::grid::footer_grid::FooterGridWidgetRefExt;
use makepad_app_shell::grid::{LayoutState, FooterLayoutState, FooterSlotState};
use makepad_app_shell::theme::get_global_dark_mode;
use crate::app_data::AppData;
use crate::data::LeRobotDataset;
use crate::sidebar_content::SidebarAction;
use crate::playback_controls::PlaybackAction;
use crate::widgets::timeline::{TimelineAction, TimelineWidgetRefExt};
use crate::widgets::time_series_plot::{TimeSeriesPlotAction, TimeSeriesPlotWidgetRefExt};
use crate::widgets::video_player::VideoPlayerWidgetRefExt;
use crate::widgets::robot_viewer::RobotViewerWidgetRefExt;
use crate::widgets::episode_list::{DataSourceInfo, EpisodeListWidgetRefExt};

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
                    // Override the header with logo and title
                    main_container = {
                        header = {
                            // Robot logo icon (Hugging Face style R)
                            logo_container = <View> {
                                width: 28
                                height: 28
                                show_bg: true
                                draw_bg: {
                                    instance dark_mode: 0.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);

                                        // Scale SVG (24x24) to fit in 28x28 with padding
                                        let scale = self.rect_size.x / 24.0 * 0.9;
                                        let ox = self.rect_size.x * 0.05;  // offset for centering
                                        let oy = self.rect_size.y * 0.05;

                                        // Small circle at top left (dot)
                                        sdf.circle(ox + 5.4 * scale, oy + 3.67 * scale, 3.0 * scale);

                                        // Main "R" shape - vertical stem
                                        sdf.box(ox + 9.0 * scale, oy + 5.5 * scale, 4.5 * scale, 18.0 * scale, 1.0);

                                        // "R" bowl at top (approximated as circle + box)
                                        sdf.box(ox + 9.0 * scale, oy + 0.9 * scale, 8.0 * scale, 5.0 * scale, 1.0);
                                        sdf.circle(ox + 15.5 * scale, oy + 5.5 * scale, 4.5 * scale);

                                        // "R" diagonal leg going down-right
                                        sdf.rotate(0.6, ox + 13.5 * scale, oy + 10.0 * scale);
                                        sdf.box(ox + 13.5 * scale, oy + 10.0 * scale, 4.5 * scale, 14.0 * scale, 1.0);
                                        sdf.rotate(-0.6, ox + 13.5 * scale, oy + 10.0 * scale);

                                        // Bottom left tail (the "L" part going to bottom-left)
                                        sdf.rotate(-0.15, ox + 4.0 * scale, oy + 17.5 * scale);
                                        sdf.box(ox + 0.0 * scale, oy + 17.5 * scale, 9.0 * scale, 5.8 * scale, 1.0);

                                        // Monochrome color based on theme
                                        let light_color = vec4(0.2, 0.2, 0.22, 1.0);
                                        let dark_color = vec4(0.85, 0.85, 0.88, 1.0);
                                        let color = mix(light_color, dark_color, self.dark_mode);

                                        return sdf.fill(color);
                                    }
                                }
                            }
                            title_label = { text: "DoRobot Studio" }
                        }

                        dock_wrapper = {
                            dock = {
                                // Override left sidebar content - Dataset (themed, dark_mode responsive)
                                left_sidebar_content = <View> {
                                    width: Fill, height: Fill
                                    flow: Down

                                    show_bg: true
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        fn pixel(self) -> vec4 {
                                            let light = vec4(0.973, 0.980, 0.988, 1.0);
                                            let dark = vec4(0.082, 0.082, 0.094, 1.0);
                                            return mix(light, dark, self.dark_mode);
                                        }
                                    }

                                    // Header
                                    left_sidebar_header = <View> {
                                        width: Fill, height: 40
                                        padding: { left: 16 }
                                        align: { y: 0.5 }
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 {
                                                let light = vec4(0.945, 0.961, 0.976, 1.0);
                                                let dark = vec4(0.133, 0.133, 0.157, 1.0);
                                                return mix(light, dark, self.dark_mode);
                                            }
                                        }
                                        left_sidebar_title = <Label> {
                                            draw_text: {
                                                text_style: <FONT_SEMIBOLD> { font_size: 12.0 }
                                                instance dark_mode: 0.0
                                                fn get_color(self) -> vec4 {
                                                    let light_text = vec4(0.1, 0.1, 0.12, 1.0);
                                                    let dark_text = vec4(0.878, 0.878, 0.878, 1.0);
                                                    return mix(light_text, dark_text, self.dark_mode);
                                                }
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
                                                title: "cam_high"
                                                content = {
                                                    video_main = <VideoPlayer> {}
                                                }
                                            }
                                            s1_2 = {
                                                title: "3D View"
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
                                                title: "cam_left_wrist"
                                                content = {
                                                    video_cam1 = <VideoPlayer> {}
                                                }
                                            }
                                            s2_2 = {
                                                title: "cam_right_wrist"
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

                                // Override right sidebar content - Episode Info (themed, dark_mode responsive)
                                right_sidebar_content = <View> {
                                    width: Fill, height: Fill
                                    flow: Down

                                    show_bg: true
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        fn pixel(self) -> vec4 {
                                            let light = vec4(0.973, 0.980, 0.988, 1.0);
                                            let dark = vec4(0.082, 0.082, 0.094, 1.0);
                                            return mix(light, dark, self.dark_mode);
                                        }
                                    }

                                    // Header
                                    right_sidebar_header = <View> {
                                        width: Fill, height: 40
                                        padding: { left: 16 }
                                        align: { y: 0.5 }
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 {
                                                let light = vec4(0.945, 0.961, 0.976, 1.0);
                                                let dark = vec4(0.133, 0.133, 0.157, 1.0);
                                                return mix(light, dark, self.dark_mode);
                                            }
                                        }
                                        right_sidebar_title = <Label> {
                                            draw_text: {
                                                text_style: <FONT_SEMIBOLD> { font_size: 12.0 }
                                                instance dark_mode: 0.0
                                                fn get_color(self) -> vec4 {
                                                    let light_text = vec4(0.1, 0.1, 0.12, 1.0);
                                                    let dark_text = vec4(0.878, 0.878, 0.878, 1.0);
                                                    return mix(light_text, dark_text, self.dark_mode);
                                                }
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
                                        // Left controller sidebar - Playback controls (themed, dark_mode responsive)
                                        controller_content = <View> {
                                            width: Fill, height: Fill
                                            flow: Down

                                            show_bg: true
                                            draw_bg: {
                                                instance dark_mode: 0.0
                                                fn pixel(self) -> vec4 {
                                                    let light = vec4(0.973, 0.980, 0.988, 1.0);
                                                    let dark = vec4(0.082, 0.082, 0.094, 1.0);
                                                    return mix(light, dark, self.dark_mode);
                                                }
                                            }

                                            // Header
                                            footer_sidebar_header = <View> {
                                                width: Fill, height: 40
                                                padding: { left: 16 }
                                                align: { y: 0.5 }
                                                show_bg: true
                                                draw_bg: {
                                                    instance dark_mode: 0.0
                                                    fn pixel(self) -> vec4 {
                                                        let light = vec4(0.945, 0.961, 0.976, 1.0);
                                                        let dark = vec4(0.133, 0.133, 0.157, 1.0);
                                                        return mix(light, dark, self.dark_mode);
                                                    }
                                                }
                                                footer_sidebar_title = <Label> {
                                                    draw_text: {
                                                        text_style: <FONT_SEMIBOLD> { font_size: 12.0 }
                                                        instance dark_mode: 0.0
                                                        fn get_color(self) -> vec4 {
                                                            let light_text = vec4(0.1, 0.1, 0.12, 1.0);
                                                            let dark_text = vec4(0.878, 0.878, 0.878, 1.0);
                                                            return mix(light_text, dark_text, self.dark_mode);
                                                        }
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
                                                    title: "State Plot"
                                                    content = {
                                                        state_plot = <TimeSeriesPlot> {}
                                                    }
                                                }
                                            }
                                            f1_1 = {
                                                p0 = {
                                                    title: "Action Plot"
                                                    content = {
                                                        action_plot = <TimeSeriesPlot> {}
                                                    }
                                                }
                                            }
                                            f1_2 = {
                                                p0 = {
                                                    title: "Timeline"
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

    /// Track if plots need initialization
    #[rust]
    plots_initialized: bool,

    /// Track current episode to detect changes
    #[rust]
    last_episode: Option<u64>,

    /// Track last video update time to avoid redundant decoding
    #[rust]
    last_video_time: f64,

    /// Track last video decode instant for rate-limiting
    #[rust]
    last_video_decode_instant: Option<Instant>,

    /// Whether user is currently scrubbing the timeline
    #[rust]
    is_scrubbing: bool,

    /// Pending video update (deferred during fast scrubbing)
    #[rust]
    pending_video_update: bool,
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
        // panel_0 = cam_high, panel_1 = 3D View
        // panel_2 = cam_left_wrist, panel_3 = cam_right_wrist
        let panel_grid = self.ui.panel_grid(id!(center_content));
        panel_grid.set_layout_state(cx, LayoutState::with_panel_count(4));
        // Set panel titles (these persist across layout state changes)
        panel_grid.set_panel_titles(&[
            ("panel_0", "cam_high"),
            ("panel_1", "3D View"),
            ("panel_2", "cam_left_wrist"),
            ("panel_3", "cam_right_wrist"),
        ]);

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

        // Set footer panel titles explicitly via FooterGrid API
        footer_grid.set_panel_title(cx, 0, 0, "State Plot");
        footer_grid.set_panel_title(cx, 1, 0, "Action Plot");
        footer_grid.set_panel_title(cx, 2, 0, "Timeline");

        // Try to load dataset from command line or default paths
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 {
            self.load_dataset(cx, &args[1]);
        } else {
            self.try_load_default_dataset(cx);
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        use crate::widgets::episode_list::EpisodeListAction;

        for action in actions {
            // Handle EpisodeListAction (emitted via cx.action() from EpisodeListItem)
            if let Some(EpisodeListAction::EpisodeSelected(idx)) = action.downcast_ref::<EpisodeListAction>() {
                log!("App: EpisodeListAction::EpisodeSelected({})", *idx);
                self.load_episode(cx, *idx);
            }

            // Handle SidebarAction (emitted via cx.action() from SidebarContent)
            if let Some(SidebarAction::LoadDataset) = action.downcast_ref::<SidebarAction>() {
                self.open_dataset_dialog(cx);
            }

            // Handle widget actions (emitted via cx.widget_action())
            if let Some(widget_action) = action.as_widget_action() {
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
                        self.is_scrubbing = true;
                        self.seek_to(cx, time);
                        // Rate-limit video updates during scrubbing for responsiveness
                        self.update_videos_rate_limited(cx);
                    }
                    TimelineAction::ScrubEnd => {
                        self.is_scrubbing = false;
                        self.pending_video_update = false;
                        // Show final frame immediately when scrubbing ends
                        self.update_videos_now(cx);
                    }
                    TimelineAction::Play => {
                        self.is_scrubbing = false;
                        self.data.is_playing = true;
                    }
                    TimelineAction::Pause => {
                        self.is_scrubbing = false;
                        self.data.is_playing = false;
                        // Show final frame when pausing
                        self.update_videos_now(cx);
                    }
                    TimelineAction::StepForward | TimelineAction::StepBackward => {
                        self.is_scrubbing = false;
                    }
                    _ => {}
                }

                // Handle plot cursor actions
                match widget_action.cast::<TimeSeriesPlotAction>() {
                    TimeSeriesPlotAction::CursorMoved(time) => {
                        self.is_scrubbing = true;
                        self.seek_to(cx, time);
                        // Rate-limit video updates during scrubbing
                        self.update_videos_rate_limited(cx);
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_timer(&mut self, cx: &mut Cx, event: &TimerEvent) {
        if self.playback_timer.is_timer(event).is_some() {
            if self.data.is_playing {
                self.advance_time(cx, 1.0 / 60.0);
                // Update videos at rate-limited pace during playback
                self.update_videos_rate_limited(cx);
            } else if self.pending_video_update {
                // Process any pending scrub update when not playing
                self.pending_video_update = false;
                self.update_videos_now(cx);
            }
        }
    }
}

impl AppMain for DoRobotApp {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // Apply theme to custom elements
        self.apply_custom_theme(cx);

        // Check if episode changed - need to reinitialize videos and plots
        if self.data.current_episode != self.last_episode {
            self.last_episode = self.data.current_episode;
            self.videos_initialized = false;
            self.plots_initialized = false;
        }

        // Initialize videos once when episode loads
        if !self.videos_initialized && self.data.current_episode.is_some() {
            self.init_videos(cx);
            self.videos_initialized = true;
            // Show first frame immediately
            self.update_videos_now(cx);
        }

        // Initialize plots once when episode loads
        if !self.plots_initialized && !self.data.episode_frames.is_empty() {
            self.init_plots(cx);
            self.plots_initialized = true;
        }

        // Update plots cursor position
        self.update_plots_cursor(cx);

        // Update robot viewer (lightweight - no video decoding)
        self.update_robot_viewer(cx);

        // IMPORTANT: Let UI handle events first so widgets can emit actions,
        // then process those actions in match_event/handle_actions
        self.ui.handle_event(cx, event, &mut Scope::with_data(&mut self.data));

        // Now process any actions that were emitted during ui.handle_event
        self.match_event(cx, event);
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

                // Extract data sources from dataset features
                let data_sources: Vec<DataSourceInfo> = dataset.info.features.iter()
                    .map(|(name, feature)| {
                        let is_video = name.contains("images") || feature.dtype.contains("video");
                        DataSourceInfo {
                            name: name.clone(),
                            dtype: feature.dtype.clone(),
                            shape: feature.shape.clone(),
                            is_video,
                        }
                    })
                    .collect();

                // Update app data
                self.data.dataset_name = name;
                self.data.dataset_info = info;
                self.data.episode_fps = dataset.info.fps;
                self.data.episodes = episodes.clone();
                self.data.dataset = Some(dataset);

                // Update episode list UI with data sources
                let episode_list = self.ui.episode_list(id!(episode_list));
                episode_list.set_data_sources(cx, data_sources);
                episode_list.set_episodes(cx, episodes);

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
        // Single frame step - update immediately
        self.update_videos_now(cx);
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

    /// Rate-limited video update - for scrubbing and playback
    /// During scrubbing: update at ~10fps to keep UI responsive
    /// During playback: update at video fps to match content
    fn update_videos_rate_limited(&mut self, cx: &mut Cx) {
        // Calculate minimum interval between video decodes
        // Scrubbing: 100ms (10fps) for UI responsiveness
        // Playback: match video fps (e.g., 33ms for 30fps)
        let min_interval_ms = if self.is_scrubbing {
            100 // 10 fps during scrubbing for fast response
        } else {
            (1000.0 / self.data.episode_fps.max(1.0)) as u64
        };

        let now = Instant::now();
        let should_decode = match self.last_video_decode_instant {
            Some(last) => now.duration_since(last).as_millis() as u64 >= min_interval_ms,
            None => true,
        };

        if should_decode {
            self.update_videos_now(cx);
        } else {
            // Mark that we have a pending update for when scrubbing stops
            self.pending_video_update = true;
        }
    }

    /// Immediate video update - bypasses rate limiting
    fn update_videos_now(&mut self, cx: &mut Cx) {
        // Only decode if time has changed
        let time_changed = (self.data.current_time - self.last_video_time).abs() > 0.001;

        if time_changed {
            self.last_video_time = self.data.current_time;
            self.last_video_decode_instant = Some(Instant::now());

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

        // Always update timeline (lightweight)
        let timeline = self.ui.timeline(id!(timeline));
        timeline.set_duration(cx, self.data.episode_duration, self.data.episode_fps);
        timeline.set_current_time(cx, self.data.current_time);
        timeline.set_playing(cx, self.data.is_playing);
    }

    fn update_robot_viewer(&mut self, cx: &mut Cx) {
        if let Some(frame) = self.data.current_frame() {
            let joint_angles: Vec<f64> = frame.state.iter().map(|&v| v as f64).collect();
            let robot_viewer = self.ui.robot_viewer(id!(robot_viewer));
            robot_viewer.set_joint_angles(cx, &joint_angles);
        }
    }

    /// Initialize plots with episode data
    fn init_plots(&mut self, cx: &mut Cx) {
        let frames = &self.data.episode_frames;
        if frames.is_empty() {
            return;
        }

        let state_channels = frames.first().map(|f| f.state.len()).unwrap_or(0);
        let action_channels = frames.first().map(|f| f.action.len()).unwrap_or(0);

        // Configure and populate state plot
        let state_plot = self.ui.time_series_plot(id!(state_plot));
        state_plot.set_title(cx, "observation.state");
        state_plot.set_time_range(0.0, self.data.episode_duration);
        state_plot.set_auto_scale_y(true);
        state_plot.set_window_size(10.0);  // 10 second sliding window

        for ch in 0..state_channels.min(14) {
            let plot_data: Vec<(f64, f64)> = frames.iter()
                .map(|f| (f.timestamp, f.state.get(ch).copied().unwrap_or(0.0) as f64))
                .collect();
            state_plot.set_channel_data(ch, &format!("state[{}]", ch), plot_data);
        }
        state_plot.recompute_scale(cx);

        // Configure and populate action plot
        let action_plot = self.ui.time_series_plot(id!(action_plot));
        action_plot.set_title(cx, "action");
        action_plot.set_time_range(0.0, self.data.episode_duration);
        action_plot.set_auto_scale_y(true);
        action_plot.set_window_size(10.0);  // 10 second sliding window

        for ch in 0..action_channels.min(14) {
            let plot_data: Vec<(f64, f64)> = frames.iter()
                .map(|f| (f.timestamp, f.action.get(ch).copied().unwrap_or(0.0) as f64))
                .collect();
            action_plot.set_channel_data(ch, &format!("action[{}]", ch), plot_data);
        }
        action_plot.recompute_scale(cx);
    }

    /// Update plot cursor positions (lightweight - just moves cursor)
    fn update_plots_cursor(&mut self, cx: &mut Cx) {
        let state_plot = self.ui.time_series_plot(id!(state_plot));
        state_plot.set_cursor_time(cx, self.data.current_time);

        let action_plot = self.ui.time_series_plot(id!(action_plot));
        action_plot.set_cursor_time(cx, self.data.current_time);
    }

    /// Apply theme to custom sidebar and header elements
    fn apply_custom_theme(&mut self, cx: &mut Cx) {
        let dm = get_global_dark_mode();

        // Robot logo in header
        self.ui.view(id!(logo_container)).apply_over(cx, live! {
            draw_bg: { dark_mode: (dm) }
        });

        // Left sidebar (Dataset) header and background
        self.ui.view(id!(left_sidebar_content)).apply_over(cx, live! {
            draw_bg: { dark_mode: (dm) }
        });
        self.ui.view(id!(left_sidebar_header)).apply_over(cx, live! {
            draw_bg: { dark_mode: (dm) }
        });
        self.ui.label(id!(left_sidebar_title)).apply_over(cx, live! {
            draw_text: { dark_mode: (dm) }
        });

        // Right sidebar (Episode Info) header and background
        self.ui.view(id!(right_sidebar_content)).apply_over(cx, live! {
            draw_bg: { dark_mode: (dm) }
        });
        self.ui.view(id!(right_sidebar_header)).apply_over(cx, live! {
            draw_bg: { dark_mode: (dm) }
        });
        self.ui.label(id!(right_sidebar_title)).apply_over(cx, live! {
            draw_text: { dark_mode: (dm) }
        });

        // Footer sidebar (Playback) header and background
        self.ui.view(id!(footer_sidebar_content)).apply_over(cx, live! {
            draw_bg: { dark_mode: (dm) }
        });
        self.ui.view(id!(footer_sidebar_header)).apply_over(cx, live! {
            draw_bg: { dark_mode: (dm) }
        });
        self.ui.label(id!(footer_sidebar_title)).apply_over(cx, live! {
            draw_text: { dark_mode: (dm) }
        });
    }
}

app_main!(DoRobotApp);
