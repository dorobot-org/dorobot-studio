//! DoRobot Studio Application
//!
//! Main application entry point with event routing and state management.
//! Supports LeRobot v2.0 and v3.0 dataset formats.

use makepad_widgets::*;
use makepad_app_shell::shell::layout::ShellLayoutWidgetRefExt;
use makepad_app_shell::grid::panel_grid::PanelGridWidgetRefExt;
use makepad_app_shell::grid::footer_grid::FooterGridWidgetRefExt;
use makepad_app_shell::grid::{LayoutState, FooterLayoutState, FooterSlotState};
use crate::data::LeRobotDataset;
use crate::home::home_screen::HomeScreenWidgetRefExt;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.studio.*

    startup() do #(DoRobotApp::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.title: "DoRobot Studio"
                window.inner_size: vec2(1400, 900)

                pass.clear_color: mod.widgets.studio.COLOR_BG_APP

                body +: {
                    home_screen := HomeScreen{}
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct DoRobotApp {
    #[live]
    ui: WidgetRef,

    // Dataset state
    #[rust]
    dataset: Option<LeRobotDataset>,

    // Playback timer
    #[rust]
    update_timer: Timer,
}

impl MatchEvent for DoRobotApp {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // The dorobot look is the dark one. Set through the widget rather than
        // `set_global_dark_mode`: the global is an *output* of
        // ShellLayout::apply_theme, recomputed from the widget's own state on
        // first draw, so setting it directly is overwritten a frame later.
        self.ui
            .shell_layout(cx, ids!(shell))
            .set_dark_mode(cx, true);

        // Titles live in the grid's layout state, which outlives the DSL
        // defaults — set them here or a restored layout shows "Panel".
        // The layout state establishes the panel ids the titles key on; setting
        // titles without it leaves them on whatever the previous layout had.
        let panel_grid = self.ui.panel_grid(cx, ids!(panel_grid));
        panel_grid.set_layout_state(cx, LayoutState::with_panel_count(4));
        panel_grid.set_panel_titles(&[
            ("panel_0", "Camera"),
            ("panel_1", "Robot 3D View"),
            ("panel_2", "Camera 1"),
            ("panel_3", "Camera 2"),
        ]);

        let footer_grid = self.ui.footer_grid(cx, ids!(footer_grid));
        footer_grid.set_layout_state(cx, FooterLayoutState {
            // All seven slots, so the state is authoritative: set_layout_state
            // replaces the vec wholesale, and any slot left out keeps whatever
            // the DSL gave it.
            slots: (0..7)
                .map(|i| FooterSlotState {
                    visible: i < 3,
                    panel_ids: vec![format!("footer_panel_{i}")],
                })
                .collect(),
            fullscreen_panel: None,
        });
        footer_grid.set_panel_title(cx, 0, 0, "observation.state");
        footer_grid.set_panel_title(cx, 1, 0, "action");
        footer_grid.set_panel_title(cx, 2, 0, "Timeline");

        // Start update timer for animations
        self.update_timer = cx.start_interval(1.0 / 60.0);

        // Try to load dataset from command line arg or known paths
        // NOTE: We cannot open file dialog here because it's a blocking modal dialog
        // that conflicts with Makepad's event loop (causes RefCell already borrowed panic)
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 {
            // Load from command line argument
            self.load_dataset(cx, &args[1]);
        } else {
            // Try known dataset paths
            let known_paths = [
                "dataset/aloha_mobile_cabinet",
                "dataset/xvla-soft-fold",
                "../dataset/aloha_mobile_cabinet",
            ];

            let mut loaded = false;
            for path in &known_paths {
                if std::path::Path::new(path).join("meta").exists() {
                    ::log::info!("Found dataset at {}", path);
                    self.load_dataset(cx, path);
                    loaded = true;
                    break;
                }
            }

            if !loaded {
                self.load_demo_dataset(cx);
            }
        }
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

impl AppMain for DoRobotApp {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        // Register Makepad widgets (mod.theme defaults to the dark theme)
        makepad_widgets::script_mod(vm);
        // Shared tokens before this app's styles, which alias onto them.
        dorobot_ux::script_mod(vm);

        // Register our modules
        crate::shared::script_mod(vm);

        // The panel shell, with this app's palette handed to it in the window
        // between app-shell's default tokens and the widgets that bind them.
        // Registering the theme anywhere else compiles and silently does
        // nothing — ShellLayout composes its children as this call returns.
        makepad_app_shell::script_mod_with_theme(vm, |vm| {
            crate::shared::shell_theme::script_mod(vm);
        });

        crate::widgets::script_mod(vm);
        crate::home::script_mod(vm);
        crate::app::script_mod(vm);

        // The app module goes last: it defines the startup() root
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

impl DoRobotApp {
    fn open_dataset_dialog(&mut self, cx: &mut Cx) {
        // Debug: write to file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[DoRobotApp] open_dataset_dialog called");
        }

        // Open native folder picker dialog
        // LeRobot datasets are directories containing meta/, data/, and videos/ subdirectories
        let dialog = rfd::FileDialog::new()
            .set_title("Select LeRobot Dataset Folder")
            .set_directory(std::env::current_dir().unwrap_or_default());

        if let Some(folder_path) = dialog.pick_folder() {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
                let _ = writeln!(f, "[DoRobotApp] User selected folder: {:?}", folder_path);
            }

            // Convert PathBuf to string and load the dataset
            if let Some(path_str) = folder_path.to_str() {
                self.load_dataset(cx, path_str);
                return;
            } else {
                ::log::error!("Invalid path encoding: {:?}", folder_path);
            }
        } else {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
                let _ = writeln!(f, "[DoRobotApp] User cancelled folder picker dialog");
            }
        }

        // Fall back to demo data if dialog cancelled or path invalid
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[DoRobotApp] Loading demo data");
        }
        self.load_demo_dataset(cx);
    }

    /// Load a LeRobot dataset from a directory path
    pub fn load_dataset(&mut self, cx: &mut Cx, path: &str) {
        // Debug: write to file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[DoRobotApp] load_dataset called with path: {}", path);
        }

        match LeRobotDataset::open(path) {
            Ok(dataset) => {
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
                    let _ = writeln!(f, "[DoRobotApp] Dataset loaded: {} episodes", dataset.num_episodes());
                }
                ::log::info!("Loaded dataset from {}: {} episodes", path, dataset.num_episodes());

                let home = self.ui.home_screen(cx, ids!(home_screen));

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
            let _ = writeln!(f, "[DoRobotApp] load_episode called for episode {}, dataset={}", episode_idx, self.dataset.is_some());
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

                    let home = self.ui.home_screen(cx, ids!(home_screen));

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
            let home = self.ui.home_screen(cx, ids!(home_screen));
            home.set_episode_data(cx, episode_idx, 150 + (episode_idx % 100));
        }
    }

    fn load_demo_dataset(&mut self, cx: &mut Cx) {
        let home = self.ui.home_screen(cx, ids!(home_screen));

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
app_main!(DoRobotApp);
