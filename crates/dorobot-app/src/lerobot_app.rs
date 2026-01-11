//! DoRobot Studio Application
//!
//! Main application entry point with event routing and state management.
//! Supports LeRobot v2.0 and v3.0 dataset formats.

use makepad_widgets::*;
use crate::data::LeRobotDataset;
use crate::home::home_screen::HomeScreenWidgetRefExt;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;
    use crate::home::home_screen::*;

    LeRobotApp = {{LeRobotApp}} {
        ui: <Root> {
            main_window = <Window> {
                window: {
                    title: "DoRobot Studio"
                    inner_size: vec2(1400, 900)
                }

                show_bg: true
                draw_bg: { color: (COLOR_BG_APP) }

                body = <HomeScreen> {}
            }
        }
    }
}

#[derive(Live, LiveHook)]
pub struct LeRobotApp {
    #[live]
    ui: WidgetRef,

    // Dataset state
    #[rust]
    dataset: Option<LeRobotDataset>,

    // Playback timer
    #[rust]
    update_timer: Timer,
}

impl LiveRegister for LeRobotApp {
    fn live_register(cx: &mut Cx) {
        // Register Makepad widgets
        makepad_widgets::live_design(cx);

        // Register our modules
        crate::shared::live_design(cx);
        crate::widgets::live_design(cx);
        crate::home::live_design(cx);

        // Link to dark theme
        cx.link(live_id!(theme), live_id!(theme_desktop_dark));
    }
}

impl MatchEvent for LeRobotApp {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // Start update timer for animations
        self.update_timer = cx.start_interval(1.0 / 60.0);

        // Try to load real dataset, fall back to demo data
        self.open_dataset_dialog(cx);
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        use crate::home::home_screen::HomeScreenAction;

        // Handle home screen actions
        for action in actions {
            // Try to get widget action and downcast it
            if let Some(widget_action) = action.as_widget_action() {
                match widget_action.cast::<HomeScreenAction>() {
                    HomeScreenAction::LoadDataset => {
                        self.open_dataset_dialog(cx);
                    }
                    HomeScreenAction::EpisodeSelected(idx) => {
                        self.load_episode(cx, idx as u64);
                    }
                    HomeScreenAction::TimeChanged(_time) => {
                        // Time changed - sync handled by HomeScreen
                    }
                    HomeScreenAction::PlaybackChanged(_playing) => {
                        // Playback state changed
                    }
                    HomeScreenAction::None => {}
                }
            }
        }
    }

    fn handle_timer(&mut self, cx: &mut Cx, _event: &TimerEvent) {
        // Could use for background updates, animations, etc.
        let _ = cx;
    }
}

impl AppMain for LeRobotApp {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

impl LeRobotApp {
    fn open_dataset_dialog(&mut self, cx: &mut Cx) {
        // Debug: write to file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[LeRobotApp] open_dataset_dialog called");
        }

        // Try common dataset locations
        let possible_paths = [
            "/Users/yuechen/home/dorobot/dataset/aloha_mobile_cabinet",
            "./data/lerobot_dataset",
            "../lerobot_dataset",
        ];

        for path in possible_paths {
            let exists = std::path::Path::new(path).exists();
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
                let _ = writeln!(f, "[LeRobotApp] Checking path {}: exists={}", path, exists);
            }
            if exists {
                self.load_dataset(cx, path);
                return;
            }
        }

        // Fall back to demo data
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[LeRobotApp] No dataset found, using demo data");
        }
        self.load_demo_dataset(cx);
    }

    /// Load a LeRobot dataset from a directory path
    pub fn load_dataset(&mut self, cx: &mut Cx, path: &str) {
        // Debug: write to file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[LeRobotApp] load_dataset called with path: {}", path);
        }

        match LeRobotDataset::open(path) {
            Ok(dataset) => {
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
                    let _ = writeln!(f, "[LeRobotApp] Dataset loaded: {} episodes", dataset.num_episodes());
                }
                ::log::info!("Loaded dataset from {}: {} episodes", path, dataset.num_episodes());

                let home = self.ui.home_screen(id!(body));

                // Set dataset info from loaded metadata
                let info_text = format!(
                    "{} episodes | {} fps | {}",
                    dataset.info.total_episodes,
                    dataset.info.fps,
                    dataset.info.robot_type
                );

                // Extract dataset name from path
                let name = std::path::Path::new(path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("dataset");

                home.set_dataset_info(cx, name, &info_text);

                // Convert episode metadata to EpisodeInfo
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

                home.set_episodes(cx, episodes);
                self.dataset = Some(dataset);

                // Auto-select first episode
                self.load_episode(cx, 0);
            }
            Err(e) => {
                ::log::error!("Failed to load dataset from {}: {}", path, e);
                // Fall back to demo data
                self.load_demo_dataset(cx);
            }
        }
    }

    /// Load episode data when an episode is selected
    fn load_episode(&mut self, cx: &mut Cx, episode_idx: u64) {
        // Debug: write to file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[LeRobotApp] load_episode called for episode {}, dataset={}", episode_idx, self.dataset.is_some());
        }

        if let Some(dataset) = &self.dataset {
            match dataset.load_episode(episode_idx) {
                Ok(episode_data) => {
                    ::log::info!(
                        "Loaded episode {}: {} frames, {} video paths",
                        episode_idx,
                        episode_data.frames.len(),
                        episode_data.video_paths.len()
                    );

                    let home = self.ui.home_screen(id!(body));

                    // Set episode metadata (duration, etc.)
                    home.set_episode_data(cx, episode_idx, episode_data.frames.len() as u64);

                    // Set the actual frame data for plots
                    home.set_frame_data(cx, &episode_data.frames);

                    // Load video files with episode frame offset
                    home.set_video_paths(cx, &episode_data.video_paths, episode_data.video_frame_offset);
                }
                Err(e) => {
                    ::log::error!("Failed to load episode {}: {}", episode_idx, e);
                }
            }
        } else {
            // Demo mode - just update the selection
            let home = self.ui.home_screen(id!(body));
            home.set_episode_data(cx, episode_idx, 150 + (episode_idx % 100));
        }
    }

    fn load_demo_dataset(&mut self, cx: &mut Cx) {
        let home = self.ui.home_screen(id!(body));

        home.set_dataset_info(
            cx,
            "aloha_sim_insertion_human",
            "500 episodes | 30 fps | SO-101 arm"
        );

        // Generate demo episode list
        let demo_episodes: Vec<crate::widgets::episode_list::EpisodeInfo> = (0..50)
            .map(|i| crate::widgets::episode_list::EpisodeInfo {
                index: i,
                frame_count: 150 + (i % 100),
                duration_secs: (150 + (i % 100)) as f64 / 30.0,
                task_description: format!("Insert the peg into the hole (trial {})", i),
                task_index: i % 3,
            })
            .collect();

        home.set_episodes(cx, demo_episodes);
        self.dataset = None;
    }
}

// Generate the app_main function using Makepad's macro
app_main!(LeRobotApp);
