//! DoRobot Flex - Main Application with Shell Layout
//!
//! Uses makepad-app-shell for a professional IDE-style layout with:
//! - Draggable panels with drag-and-drop (content follows panel titles)
//! - Resizable sidebars
//! - Layout persistence (Save/Reset)
//! - Dark/light theme support
//!
//! ## Drag-and-Drop Architecture
//!
//! Each physical slot contains BOTH VideoPlayer and RobotView widgets.
//! When panels are dragged, `on_layout_changed()` swaps visibility based
//! on which panel_id is at each slot. See `DRAG_DROP_FIX.md` for details.

use std::time::Instant;

/// Playback timer frequency (frames per second)
const PLAYBACK_TIMER_FPS: f64 = 60.0;

/// Minimum interval between video decodes during scrubbing (milliseconds)
const SCRUB_RATE_LIMIT_MS: u64 = 100;

/// Minimum time change threshold to trigger video update
const TIME_EPSILON: f64 = 0.001;

/// Sliding window size for time series plots (seconds)
const PLOT_WINDOW_SIZE: f64 = 10.0;

/// Maximum number of plot channels to display
const MAX_PLOT_CHANNELS: usize = 14;
use makepad_widgets::*;
use makepad_app_shell::grid::panel_grid::PanelGridWidgetRefExt;
use makepad_app_shell::grid::footer_grid::FooterGridWidgetRefExt;
use makepad_app_shell::grid::{LayoutState, FooterLayoutState, FooterSlotState};
use makepad_app_shell::panel::PanelAction;
use makepad_app_shell::theme::get_global_dark_mode;
use crate::app_data::{AppData, PanelSlot, PanelContent};
use crate::data::LeRobotDataset;
use crate::sidebar_content::SidebarAction;
use crate::playback_controls::PlaybackAction;
use crate::widgets::timeline::{TimelineAction, TimelineWidgetRefExt};
use crate::widgets::time_series_plot::{TimeSeriesPlotAction, TimeSeriesPlotWidgetRefExt};
use crate::widgets::video_player::VideoPlayerWidgetRefExt;
use makepad_urdf_player::robot_view::RobotView;
use crate::widgets::episode_list::{DataSourceInfo, EpisodeListWidgetRefExt};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.flex.*
    use mod.widgets.shell.*

    startup() do #(DoRobotApp::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.title: "DoRobot Studio"
                window.inner_size: vec2(1400, 900)

                body +: {
                    width: Fill
                    height: Fill
                    ShellLayout{
                        // Override the header with logo and title
                        main_container +: {
                            header +: {
                                // Robot logo icon (Hugging Face style R)
                                logo_container +: {
                                    width: 28
                                    height: 28
                                    show_bg: true
                                    draw_bg +: {
                                        dark_mode: instance(0.0)
                                        pixel: fn() {
                                            let sdf = Sdf2d.viewport(self.pos * self.rect_size)

                                            // Scale SVG (24x24) to fit in 28x28 with padding
                                            let scale = self.rect_size.x / 24.0 * 0.9
                                            let ox = self.rect_size.x * 0.05  // offset for centering
                                            let oy = self.rect_size.y * 0.05

                                            // Small circle at top left (dot)
                                            sdf.circle(ox + 5.4 * scale, oy + 3.67 * scale, 3.0 * scale)

                                            // Main "R" shape - vertical stem
                                            sdf.box(ox + 9.0 * scale, oy + 5.5 * scale, 4.5 * scale, 18.0 * scale, 1.0)

                                            // "R" bowl at top (approximated as circle + box)
                                            sdf.box(ox + 9.0 * scale, oy + 0.9 * scale, 8.0 * scale, 5.0 * scale, 1.0)
                                            sdf.circle(ox + 15.5 * scale, oy + 5.5 * scale, 4.5 * scale)

                                            // "R" diagonal leg going down-right
                                            sdf.rotate(0.6, ox + 13.5 * scale, oy + 10.0 * scale)
                                            sdf.box(ox + 13.5 * scale, oy + 10.0 * scale, 4.5 * scale, 14.0 * scale, 1.0)
                                            sdf.rotate(-0.6, ox + 13.5 * scale, oy + 10.0 * scale)

                                            // Bottom left tail (the "L" part going to bottom-left)
                                            sdf.rotate(-0.15, ox + 4.0 * scale, oy + 17.5 * scale)
                                            sdf.box(ox + 0.0 * scale, oy + 17.5 * scale, 9.0 * scale, 5.8 * scale, 1.0)

                                            // Monochrome color based on theme
                                            let light_color = vec4(0.2, 0.2, 0.22, 1.0)
                                            let dark_color = vec4(0.85, 0.85, 0.88, 1.0)
                                            let color = mix(light_color, dark_color, self.dark_mode)

                                            return sdf.fill(color)
                                        }
                                    }
                                }
                                title_label +: { text: "DoRobot Studio" }
                            }

                            dock_wrapper +: {
                                dock +: {
                                    // Override left sidebar content - Dataset (themed, dark_mode responsive)
                                    left_sidebar_content: View{
                                        width: Fill
                                        height: Fill
                                        flow: Down

                                        show_bg: true
                                        draw_bg +: {
                                            dark_mode: instance(0.0)
                                            pixel: fn() {
                                                let light = vec4(0.973, 0.980, 0.988, 1.0)
                                                let dark = vec4(0.082, 0.082, 0.094, 1.0)
                                                return mix(light, dark, self.dark_mode)
                                            }
                                        }

                                        // Header
                                        left_sidebar_header := View{
                                            width: Fill
                                            height: 40
                                            padding: Inset{left: 16.}
                                            align: Align{y: 0.5}
                                            show_bg: true
                                            draw_bg +: {
                                                dark_mode: instance(0.0)
                                                pixel: fn() {
                                                    let light = vec4(0.945, 0.961, 0.976, 1.0)
                                                    let dark = vec4(0.133, 0.133, 0.157, 1.0)
                                                    return mix(light, dark, self.dark_mode)
                                                }
                                            }
                                            left_sidebar_title := Label{
                                                draw_text +: {
                                                    text_style: mod.widgets.flex.FONT_SEMIBOLD{font_size: 12.0}
                                                    dark_mode: instance(0.0)
                                                    get_color: fn() {
                                                        let light_text = vec4(0.1, 0.1, 0.12, 1.0)
                                                        let dark_text = vec4(0.878, 0.878, 0.878, 1.0)
                                                        return mix(light_text, dark_text, self.dark_mode)
                                                    }
                                                }
                                                text: "Dataset"
                                            }
                                        }

                                        // Content fills remaining space
                                        sidebar_content := SidebarContent{
                                            width: Fill
                                            height: Fill
                                        }
                                    }

                                    // Override center content with our panels (2x2 grid)
                                    // Each slot has BOTH video and robot view - we show/hide based on panel_id
                                    center_content: View{
                                        width: Fill
                                        height: Fill
                                        panel_grid := PanelGrid{
                                            width: Fill
                                            height: Fill
                                            window_container +: {
                                                row1 +: {
                                                    s1_1 +: {
                                                        title: "cam_high"
                                                        content +: {
                                                            video_slot0 := VideoPlayer{}
                                                            robot_slot0 := RobotView{ visible: false }
                                                        }
                                                    }
                                                    s1_2 +: {
                                                        title: "3D View"
                                                        content +: {
                                                            video_slot1 := VideoPlayer{ visible: false }
                                                            robot_slot1 := RobotView{}
                                                        }
                                                    }
                                                    // Hide unused slots in row1
                                                    s1_3 +: { visible: false width: 0 height: 0 }
                                                    s1_4 +: { visible: false width: 0 height: 0 }
                                                    s1_5 +: { visible: false width: 0 height: 0 }
                                                    s1_6 +: { visible: false width: 0 height: 0 }
                                                    s1_7 +: { visible: false width: 0 height: 0 }
                                                    s1_8 +: { visible: false width: 0 height: 0 }
                                                    s1_9 +: { visible: false width: 0 height: 0 }
                                                }
                                                row2 +: {
                                                    s2_1 +: {
                                                        title: "cam_left_wrist"
                                                        content +: {
                                                            video_slot2 := VideoPlayer{}
                                                            robot_slot2 := RobotView{ visible: false }
                                                        }
                                                    }
                                                    s2_2 +: {
                                                        title: "cam_right_wrist"
                                                        content +: {
                                                            video_slot3 := VideoPlayer{}
                                                            robot_slot3 := RobotView{ visible: false }
                                                        }
                                                    }
                                                    // Hide unused slots in row2
                                                    s2_3 +: { visible: false width: 0 height: 0 }
                                                    s2_4 +: { visible: false width: 0 height: 0 }
                                                    s2_5 +: { visible: false width: 0 height: 0 }
                                                    s2_6 +: { visible: false width: 0 height: 0 }
                                                    s2_7 +: { visible: false width: 0 height: 0 }
                                                    s2_8 +: { visible: false width: 0 height: 0 }
                                                    s2_9 +: { visible: false width: 0 height: 0 }
                                                }
                                                // Hide entire row3
                                                row3 +: { visible: false height: 0 }
                                            }
                                        }
                                    }

                                    // Override right sidebar content - Episode Info (themed, dark_mode responsive)
                                    right_sidebar_content: View{
                                        width: Fill
                                        height: Fill
                                        flow: Down

                                        show_bg: true
                                        draw_bg +: {
                                            dark_mode: instance(0.0)
                                            pixel: fn() {
                                                let light = vec4(0.973, 0.980, 0.988, 1.0)
                                                let dark = vec4(0.082, 0.082, 0.094, 1.0)
                                                return mix(light, dark, self.dark_mode)
                                            }
                                        }

                                        // Header
                                        right_sidebar_header := View{
                                            width: Fill
                                            height: 40
                                            padding: Inset{left: 16.}
                                            align: Align{y: 0.5}
                                            show_bg: true
                                            draw_bg +: {
                                                dark_mode: instance(0.0)
                                                pixel: fn() {
                                                    let light = vec4(0.945, 0.961, 0.976, 1.0)
                                                    let dark = vec4(0.133, 0.133, 0.157, 1.0)
                                                    return mix(light, dark, self.dark_mode)
                                                }
                                            }
                                            right_sidebar_title := Label{
                                                draw_text +: {
                                                    text_style: mod.widgets.flex.FONT_SEMIBOLD{font_size: 12.0}
                                                    dark_mode: instance(0.0)
                                                    get_color: fn() {
                                                        let light_text = vec4(0.1, 0.1, 0.12, 1.0)
                                                        let dark_text = vec4(0.878, 0.878, 0.878, 1.0)
                                                        return mix(light_text, dark_text, self.dark_mode)
                                                    }
                                                }
                                                text: "Episode Info"
                                            }
                                        }

                                        // Content fills remaining space
                                        episode_info := EpisodeInfoPanel{
                                            width: Fill
                                            height: Fill
                                        }
                                    }

                                    // Override footer content
                                    // Playback controls in left sidebar, 3 panels: State Plot, Action Plot, Timeline
                                    footer_content: View{
                                        width: Fill
                                        height: Fill
                                        footer_grid := FooterGrid{
                                            width: Fill
                                            height: Fill
                                            initial_panels: 3

                                            dock +: {
                                                // Left controller sidebar - Playback controls (themed, dark_mode responsive)
                                                controller_content: View{
                                                    width: Fill
                                                    height: Fill
                                                    flow: Down

                                                    show_bg: true
                                                    draw_bg +: {
                                                        dark_mode: instance(0.0)
                                                        pixel: fn() {
                                                            let light = vec4(0.973, 0.980, 0.988, 1.0)
                                                            let dark = vec4(0.082, 0.082, 0.094, 1.0)
                                                            return mix(light, dark, self.dark_mode)
                                                        }
                                                    }

                                                    // Header
                                                    footer_sidebar_header := View{
                                                        width: Fill
                                                        height: 40
                                                        padding: Inset{left: 16.}
                                                        align: Align{y: 0.5}
                                                        show_bg: true
                                                        draw_bg +: {
                                                            dark_mode: instance(0.0)
                                                            pixel: fn() {
                                                                let light = vec4(0.945, 0.961, 0.976, 1.0)
                                                                let dark = vec4(0.133, 0.133, 0.157, 1.0)
                                                                return mix(light, dark, self.dark_mode)
                                                            }
                                                        }
                                                        footer_sidebar_title := Label{
                                                            draw_text +: {
                                                                text_style: mod.widgets.flex.FONT_SEMIBOLD{font_size: 12.0}
                                                                dark_mode: instance(0.0)
                                                                get_color: fn() {
                                                                    let light_text = vec4(0.1, 0.1, 0.12, 1.0)
                                                                    let dark_text = vec4(0.878, 0.878, 0.878, 1.0)
                                                                    return mix(light_text, dark_text, self.dark_mode)
                                                                }
                                                            }
                                                            text: "Playback"
                                                        }
                                                    }

                                                    // Content fills remaining space
                                                    playback := PlaybackControls{
                                                        width: Fill
                                                        height: Fill
                                                    }
                                                }

                                                // 3 footer panels stacked vertically: State Plot, Action Plot, Timeline
                                                panel_strip_content +: {
                                                    flow: Down
                                                    f1_0 +: {
                                                        p0 +: {
                                                            title: "State Plot"
                                                            content +: {
                                                                state_plot := TimeSeriesPlot{}
                                                            }
                                                        }
                                                    }
                                                    f1_1 +: {
                                                        p0 +: {
                                                            title: "Action Plot"
                                                            content +: {
                                                                action_plot := TimeSeriesPlot{}
                                                            }
                                                        }
                                                    }
                                                    f1_2 +: {
                                                        p0 +: {
                                                            title: "Timeline"
                                                            content +: {
                                                                timeline := Timeline{}
                                                            }
                                                        }
                                                    }
                                                    // Hide unused footer slots
                                                    f1_3 +: { visible: false width: 0 }
                                                    f1_4 +: { visible: false width: 0 }
                                                    f1_5 +: { visible: false width: 0 }
                                                    f1_6 +: { visible: false width: 0 }
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
    }
}

#[derive(Script, ScriptHook)]
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

    /// Pending layout reset flag - layout will be reset on next event when widget is available
    #[rust]
    pending_layout_reset: bool,

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

impl MatchEvent for DoRobotApp {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // Start playback timer
        self.playback_timer = cx.start_interval(1.0 / PLAYBACK_TIMER_FPS);

        // Configure PanelGrid to show only 4 panels (2x2 grid)
        // panel_0 = cam_high, panel_1 = 3D View
        // panel_2 = cam_left_wrist, panel_3 = cam_right_wrist
        let panel_grid = self.ui.panel_grid(cx, ids!(panel_grid));
        panel_grid.set_layout_state(cx, LayoutState::with_panel_count(4));
        // Set panel titles (these persist across layout state changes)
        panel_grid.set_panel_titles(&[
            ("panel_0", "cam_high"),
            ("panel_1", "3D View"),
            ("panel_2", "cam_left_wrist"),
            ("panel_3", "cam_right_wrist"),
        ]);

        // Configure FooterGrid to show 3 panels (State Plot, Action Plot, Timeline)
        let footer_grid = self.ui.footer_grid(cx, ids!(footer_grid));
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

        // Open Dataset button (in the sidebar) — consume the Button's own click
        if self.ui.button(cx, ids!(dataset_section.load_btn)).clicked(actions) {
            log!("APP-DBG load_btn clicked");
            self.open_dataset_dialog(cx);
        }

        for action in actions {
            // Handle EpisodeListAction (emitted via cx.action() from EpisodeListItem)
            if let Some(EpisodeListAction::EpisodeSelected(idx)) = action.downcast_ref::<EpisodeListAction>() {
                log!("App: EpisodeListAction::EpisodeSelected({})", *idx);
                self.load_episode(cx, *idx);
            }

            // Handle SidebarAction (emitted via cx.action() from SidebarContent)
            if let Some(SidebarAction::LoadDataset) = action.downcast_ref::<SidebarAction>() {
                log!("APP-DBG LoadDataset action received, opening dialog");
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
                    TimelineAction::SpeedChanged(speed) => {
                        self.data.playback_speed = speed;
                    }
                    TimelineAction::None => {}
                }

                // Handle plot cursor actions
                match widget_action.cast::<TimeSeriesPlotAction>() {
                    TimeSeriesPlotAction::CursorMoved(time) => {
                        self.is_scrubbing = true;
                        self.seek_to(cx, time);
                        // Rate-limit video updates during scrubbing
                        self.update_videos_rate_limited(cx);
                    }
                    TimeSeriesPlotAction::None => {}
                }

                // Handle panel layout changes (drag-and-drop)
                match widget_action.cast::<PanelAction>() {
                    PanelAction::LayoutChanged(new_state) => {
                        self.on_layout_changed(cx, &new_state);
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_timer(&mut self, cx: &mut Cx, event: &TimerEvent) {
        if self.playback_timer.is_timer(event).is_some() {
            if self.data.is_playing {
                self.advance_time(cx, 1.0 / PLAYBACK_TIMER_FPS);
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
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        // Register Makepad widgets
        makepad_widgets::script_mod(vm);

        // Register XR scene support (required by the URDF player)
        makepad_urdf_player::makepad_xr::script_mod(vm);

        // Register URDF player widget
        makepad_urdf_player::script_mod(vm);

        // Register shell widgets
        makepad_app_shell::script_mod(vm);

        // Register our modules
        crate::shared::script_mod(vm);
        crate::widgets::script_mod(vm);
        crate::sidebar_content::script_mod(vm);
        crate::episode_info_panel::script_mod(vm);
        crate::playback_controls::script_mod(vm);
        crate::footer_stack::script_mod(vm);

        // The app module goes last: it defines the startup() root
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // Apply theme to custom elements
        self.apply_custom_theme(cx);

        // Apply pending layout reset if widget is now available
        self.apply_layout_reset(cx);

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
    /// Handle panel layout changes from drag-and-drop operations
    ///
    /// When panels are dragged, the panel_id moves but content widgets are static.
    /// This function updates which content is visible at each physical slot.
    fn on_layout_changed(&mut self, cx: &mut Cx, new_state: &LayoutState) {
        ::log::debug!("[on_layout_changed] Panel layout changed:");
        ::log::debug!("  row_assignments: {:?}", new_state.row_assignments);

        // Build new slot-to-panel mapping from row_assignments
        // Physical slots: 0=s1_1 (row0,col0), 1=s1_2 (row0,col1), 2=s2_1 (row1,col0), 3=s2_2 (row1,col1)
        let mut new_slot_mapping: [String; 4] = [
            String::new(), String::new(), String::new(), String::new()
        ];

        // Extract panel_ids from row_assignments
        if let Some(row0) = new_state.row_assignments.get(0) {
            if let Some(p) = row0.get(0) { new_slot_mapping[0] = p.clone(); }
            if let Some(p) = row0.get(1) { new_slot_mapping[1] = p.clone(); }
        }
        if let Some(row1) = new_state.row_assignments.get(1) {
            if let Some(p) = row1.get(0) { new_slot_mapping[2] = p.clone(); }
            if let Some(p) = row1.get(1) { new_slot_mapping[3] = p.clone(); }
        }

        ::log::debug!("  new_slot_mapping: {:?}", new_slot_mapping);

        // Check if mapping actually changed
        if new_slot_mapping == self.data.slot_to_panel {
            ::log::debug!("  No change in slot mapping");
            return;
        }

        // Update the stored mapping
        self.data.slot_to_panel = new_slot_mapping.clone();

        // Update visibility and content at each physical slot
        self.update_slot_content(cx, &new_slot_mapping);

        self.ui.redraw(cx);
    }

    /// Update content visibility and video sources at each physical slot
    fn update_slot_content(&mut self, cx: &mut Cx, slot_mapping: &[String; 4]) {
        // Physical slot widget IDs
        let video_slots = [
            ids!(video_slot0), ids!(video_slot1), ids!(video_slot2), ids!(video_slot3)
        ];
        let robot_slots = [
            ids!(robot_slot0), ids!(robot_slot1), ids!(robot_slot2), ids!(robot_slot3)
        ];

        for (slot_idx, panel_id) in slot_mapping.iter().enumerate() {
            if panel_id.is_empty() {
                continue;
            }

            // Determine if this panel_id should show video or robot view
            let panel_slot = PanelSlot::from_panel_id(panel_id);
            let is_robot = panel_slot == Some(PanelSlot::RobotView);

            ::log::debug!("  Slot {}: panel_id={}, is_robot={}", slot_idx, panel_id, is_robot);

            if is_robot {
                // Show robot view, hide video player
                self.ui.widget(cx, video_slots[slot_idx]).set_visible(cx, false);
                self.ui.widget(cx, robot_slots[slot_idx]).set_visible(cx, true);
            } else {
                // Show video player, hide robot view
                self.ui.widget(cx, video_slots[slot_idx]).set_visible(cx, true);
                self.ui.widget(cx, robot_slots[slot_idx]).set_visible(cx, false);

                // Load the correct video for this panel_id
                if let Some(ps) = panel_slot {
                    if let Some(video_key) = self.data.panel_registry.get_video_key(ps) {
                        if let Some(path) = self.data.video_paths.get(video_key) {
                            let player = self.ui.video_player(cx, video_slots[slot_idx]);
                            let _ = player.load_video(cx, &path.to_string_lossy());
                            ::log::debug!("    Loaded video {} into slot {}", video_key, slot_idx);
                        }
                    }
                }
            }
        }
    }

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
        log!("APP-DBG open_dataset_dialog: showing rfd folder picker");
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

        // Clear old state before loading new dataset
        self.clear_videos(cx);
        self.data.current_episode = None;
        self.data.episode_frames.clear();
        self.data.video_paths.clear();
        self.data.error_message = None;  // Clear any previous error
        self.data.robot_display_name = None;  // Clear robot name
        self.data.panel_registry.clear();  // Clear panel content registry
        self.videos_initialized = false;
        self.plots_initialized = false;

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
                            channel_names: feature.names.clone(),
                        }
                    })
                    .collect();

                // Update app data
                self.data.dataset_name = name;
                self.data.dataset_info = info;
                self.data.robot_type = dataset.info.robot_type.clone();
                self.data.episode_fps = dataset.info.fps;
                self.data.episodes = episodes.clone();
                self.data.dataset = Some(dataset);

                // Load URDF robot based on robot_type
                self.load_robot_urdf(cx);

                // Update episode list UI with data sources
                let episode_list = self.ui.episode_list(cx, ids!(episode_list));
                episode_list.set_data_sources(cx, data_sources);
                episode_list.set_episodes(cx, episodes);

                // Auto-select first episode
                self.load_episode(cx, 0);
            }
            Err(e) => {
                let error_msg = format!("Failed to load dataset: {}", e);
                ::log::error!("{}", error_msg);
                self.data.error_message = Some(error_msg);
                self.ui.redraw(cx);
            }
        }
    }

    fn load_episode(&mut self, cx: &mut Cx, episode_idx: u64) {
        let t_start = Instant::now();
        ::log::info!("Loading episode {}", episode_idx);

        if let Some(dataset) = &self.data.dataset {
            match dataset.load_episode(episode_idx) {
                Ok(episode_data) => {
                    let t_data = t_start.elapsed();
                    ::log::info!(
                        "Episode {} loaded: {} frames, {} video paths (data load: {:?})",
                        episode_idx,
                        episode_data.frames.len(),
                        episode_data.video_paths.len(),
                        t_data
                    );

                    self.data.current_episode = Some(episode_idx);
                    self.data.episode_frames = episode_data.frames;
                    self.data.video_paths = episode_data.video_paths;
                    self.data.video_frame_offset = episode_data.video_frame_offset;
                    self.data.episode_duration = self.data.episode_frames.len() as f64 / self.data.episode_fps;
                    self.data.current_time = 0.0;
                    self.data.is_playing = false;
                    self.data.error_message = None;  // Clear error on success

                    self.ui.redraw(cx);
                    let t_total = t_start.elapsed();
                    ::log::debug!("[Timing] load_episode total: {:?}", t_total);
                }
                Err(e) => {
                    let error_msg = format!("Failed to load episode {}: {}", episode_idx, e);
                    ::log::error!("{}", error_msg);
                    self.data.error_message = Some(error_msg);
                    self.ui.redraw(cx);
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
        let t_start = Instant::now();
        let total_frames = self.data.total_frames();
        let fps = self.data.episode_fps;
        let frame_offset = self.data.video_frame_offset;

        // Get sorted list of video keys (cloned to avoid borrow issues)
        let mut video_keys: Vec<String> = self.data.video_paths.keys().cloned().collect();
        video_keys.sort();
        let camera_count = video_keys.len();

        // Camera slot candidates (in order of preference)
        let main_candidates = ["observation.images.cam_high", "observation.images.top", "observation.image"];
        let cam1_candidates = ["observation.images.cam_left_wrist", "observation.images.wrist"];
        let cam2_candidates = ["observation.images.cam_right_wrist"];

        // Find video keys for each slot
        let main_key = Self::find_video_key_owned(&main_candidates, &video_keys)
            .or_else(|| video_keys.first().cloned());
        let cam1_key = Self::find_video_key_owned(&cam1_candidates, &video_keys)
            .or_else(|| video_keys.get(1).cloned());
        let cam2_key = Self::find_video_key_owned(&cam2_candidates, &video_keys)
            .or_else(|| video_keys.get(2).cloned());

        ::log::debug!("[init_videos] camera_count={}, fps={}, video_keys={:?}", camera_count, fps, video_keys);
        ::log::debug!("[init_videos] main_key={:?}, cam1_key={:?}, cam2_key={:?}", main_key, cam1_key, cam2_key);

        // Reset slot-to-panel mapping to default
        self.data.slot_to_panel = [
            "panel_0".to_string(),  // slot 0 (s1_1) -> video main
            "panel_1".to_string(),  // slot 1 (s1_2) -> robot view
            "panel_2".to_string(),  // slot 2 (s2_1) -> video cam1
            "panel_3".to_string(),  // slot 3 (s2_2) -> video cam2
        ];

        // Initialize video players at each slot based on default mapping
        // Slot 0: panel_0 (VideoMain) - show video
        let video_slot0 = self.ui.video_player(cx, ids!(video_slot0));
        Self::init_video_player_owned(cx, video_slot0, main_key.as_ref(), &self.data.video_paths, frame_offset, total_frames, fps);
        self.ui.widget(cx, ids!(video_slot0)).set_visible(cx, true);
        self.ui.widget(cx, ids!(robot_slot0)).set_visible(cx, false);
        let t_v1 = t_start.elapsed();

        // Slot 1: panel_1 (RobotView) - show robot, hide video
        let video_slot1 = self.ui.video_player(cx, ids!(video_slot1));
        video_slot1.clear(cx);
        self.ui.widget(cx, ids!(video_slot1)).set_visible(cx, false);
        self.ui.widget(cx, ids!(robot_slot1)).set_visible(cx, true);
        let t_v2 = t_start.elapsed();

        // Slot 2: panel_2 (VideoCam1) - show video
        let video_slot2 = self.ui.video_player(cx, ids!(video_slot2));
        Self::init_video_player_owned(cx, video_slot2, cam1_key.as_ref(), &self.data.video_paths, frame_offset, total_frames, fps);
        self.ui.widget(cx, ids!(video_slot2)).set_visible(cx, true);
        self.ui.widget(cx, ids!(robot_slot2)).set_visible(cx, false);

        // Slot 3: panel_3 (VideoCam2) - show video
        let video_slot3 = self.ui.video_player(cx, ids!(video_slot3));
        Self::init_video_player_owned(cx, video_slot3, cam2_key.as_ref(), &self.data.video_paths, frame_offset, total_frames, fps);
        self.ui.widget(cx, ids!(video_slot3)).set_visible(cx, true);
        self.ui.widget(cx, ids!(robot_slot3)).set_visible(cx, false);
        let t_v3 = t_start.elapsed();

        // ========================================
        // PHASE 1 FIX: Populate panel registry
        // ========================================
        self.data.panel_registry.clear();

        // Register video content for each slot
        if let Some(key) = &main_key {
            let display_name = Self::extract_camera_name(key);
            self.data.panel_registry.set_content(
                PanelSlot::VideoMain,
                PanelContent::Video { key: key.clone(), display_name }
            );
        } else {
            self.data.panel_registry.set_content(PanelSlot::VideoMain, PanelContent::Empty);
        }

        if let Some(key) = &cam1_key {
            let display_name = Self::extract_camera_name(key);
            self.data.panel_registry.set_content(
                PanelSlot::VideoCam1,
                PanelContent::Video { key: key.clone(), display_name }
            );
        } else {
            self.data.panel_registry.set_content(PanelSlot::VideoCam1, PanelContent::Empty);
        }

        if let Some(key) = &cam2_key {
            let display_name = Self::extract_camera_name(key);
            self.data.panel_registry.set_content(
                PanelSlot::VideoCam2,
                PanelContent::Video { key: key.clone(), display_name }
            );
        } else {
            self.data.panel_registry.set_content(PanelSlot::VideoCam2, PanelContent::Empty);
        }

        // Register robot view content
        let robot_title = self.get_robot_panel_title();
        self.data.panel_registry.set_content(
            PanelSlot::RobotView,
            PanelContent::RobotView { display_name: robot_title }
        );

        ::log::debug!("[init_videos] Panel registry populated: main={:?}, cam1={:?}, cam2={:?}",
            self.data.panel_registry.get_video_key(PanelSlot::VideoMain),
            self.data.panel_registry.get_video_key(PanelSlot::VideoCam1),
            self.data.panel_registry.get_video_key(PanelSlot::VideoCam2));

        // Configure panel layout using registry
        self.configure_panel_layout_from_registry(cx, camera_count);

        ::log::debug!("[Timing] init_videos: v1={:?}, v2={:?}, v3={:?}, panel_count={}",
            t_v1, t_v2, t_v3, camera_count.min(4));
    }

    /// Extract camera display name from video key
    fn extract_camera_name(key: &str) -> String {
        key.split('.').last().unwrap_or(key).to_string()
    }

    /// Get the robot panel title (uses robot_display_name if set)
    fn get_robot_panel_title(&self) -> String {
        self.data.robot_display_name
            .as_ref()
            .map(|n| format!("{} 3D View", n))
            .unwrap_or_else(|| "3D View".to_string())
    }

    /// Configure panel layout using the registry for titles
    ///
    /// PHASE 2 FIX: Titles are now included in LayoutState, eliminating the
    /// need for separate set_panel_titles() calls and the race condition workaround.
    fn configure_panel_layout_from_registry(&mut self, cx: &mut Cx, camera_count: usize) {
        let panel_grid = self.ui.panel_grid(cx, ids!(panel_grid));

        // Get display names from registry
        let main_name = self.data.panel_registry.get_display_name(PanelSlot::VideoMain);
        let robot_name = self.data.panel_registry.get_display_name(PanelSlot::RobotView);
        let cam1_name = self.data.panel_registry.get_display_name(PanelSlot::VideoCam1);
        let cam2_name = self.data.panel_registry.get_display_name(PanelSlot::VideoCam2);

        // Always lay out the full 2x2 grid; slots without a camera show as
        // empty placeholder panels ("Empty"), which keeps close/drag/reflow
        // testable regardless of how many cameras the dataset has.
        let _ = camera_count;
        let layout_state = {
            let mut state = LayoutState::with_panel_count(4);
            state.row_assignments = vec![vec!["panel_0".into(), "panel_1".into()], vec!["panel_2".into(), "panel_3".into()], vec![]];
            state.visible_panels = ["panel_0", "panel_1", "panel_2", "panel_3"].iter().map(|s| s.to_string()).collect();
            state.panel_titles.insert("panel_0".into(), main_name.to_string());
            state.panel_titles.insert("panel_1".into(), robot_name.to_string());
            state.panel_titles.insert("panel_2".into(), cam1_name.to_string());
            state.panel_titles.insert("panel_3".into(), cam2_name.to_string());
            state
        };

        ::log::debug!("[configure_panel_layout] Setting layout with titles: visible={:?}, titles={:?}",
            layout_state.visible_panels, layout_state.panel_titles);

        // Single atomic call - titles included in LayoutState
        panel_grid.set_layout_state(cx, layout_state);
    }

    /// Find a video key from candidates list (returns owned String)
    fn find_video_key_owned(candidates: &[&str], available: &[String]) -> Option<String> {
        for candidate in candidates {
            if let Some(key) = available.iter().find(|k| k.as_str() == *candidate) {
                return Some(key.clone());
            }
        }
        None
    }

    /// Initialize a single video player with the given video key (owned version)
    fn init_video_player_owned(
        cx: &mut Cx,
        player: crate::widgets::video_player::VideoPlayerRef,
        video_key: Option<&String>,
        video_paths: &std::collections::HashMap<String, std::path::PathBuf>,
        frame_offset: u64,
        total_frames: u64,
        dataset_fps: f64,
    ) {
        player.clear(cx);
        ::log::trace!("[init_videos] player exists: {}", player.borrow().is_some());

        if let Some(key) = video_key {
            ::log::debug!("[init_video_player] Setting up video: key={}, fps={}", key, dataset_fps);
            player.set_episode_info(frame_offset, total_frames);

            // Set FPS display from dataset (video decoder may override with actual fps)
            player.set_fps_display(cx, dataset_fps);
            ::log::debug!("[init_video_player] Called set_fps_display({})", dataset_fps);

            if let Some(path) = video_paths.get(key.as_str()) {
                ::log::debug!("[init_video_player] Loading video from: {}", path.display());
                match player.load_video(cx, &path.to_string_lossy()) {
                    Ok(_) => ::log::debug!("[init_video_player] Video loaded successfully"),
                    Err(e) => ::log::error!("[init_video_player] Failed to load video: {}", e),
                }
            } else {
                ::log::warn!("[init_video_player] No video path found for key: {}", key);
            }
        } else {
            ::log::debug!("[init_video_player] No video key, setting placeholder");
            player.set_placeholder_text(cx, "No camera");
        }
    }


    /// Clear all video players and mark layout for reset
    fn clear_videos(&mut self, cx: &mut Cx) {
        // Clear video players at all slots
        self.ui.video_player(cx, ids!(video_slot0)).clear(cx);
        self.ui.video_player(cx, ids!(video_slot1)).clear(cx);
        self.ui.video_player(cx, ids!(video_slot2)).clear(cx);
        self.ui.video_player(cx, ids!(video_slot3)).clear(cx);

        // Mark layout for reset - will be applied when widget is available
        self.pending_layout_reset = true;
        ::log::debug!("[clear_videos] Marked layout for reset");
    }

    /// Apply pending layout reset (called when widget is available)
    fn apply_layout_reset(&mut self, cx: &mut Cx) {
        if !self.pending_layout_reset {
            return;
        }

        let panel_grid = self.ui.panel_grid(cx, ids!(panel_grid));

        // Try to get current state to verify widget is available
        if panel_grid.layout_state().is_none() {
            ::log::warn!("[apply_layout_reset] Widget not yet available, will retry");
            return;
        }

        // Create reset state with 4 panels in 2x2 layout
        let mut reset_state = LayoutState::with_panel_count(4);
        reset_state.row_assignments = vec![
            vec!["panel_0".into(), "panel_1".into()],
            vec!["panel_2".into(), "panel_3".into()],
            vec![],
        ];
        reset_state.visible_panels = ["panel_0", "panel_1", "panel_2", "panel_3"]
            .iter().map(|s| s.to_string()).collect();

        ::log::debug!("[apply_layout_reset] Applying reset layout: visible={:?}", reset_state.visible_panels);
        panel_grid.set_layout_state(cx, reset_state);

        // Clear panel titles
        panel_grid.set_panel_titles(&[
            ("panel_0", "Loading..."),
            ("panel_1", "Loading..."),
            ("panel_2", "Loading..."),
            ("panel_3", "Loading..."),
        ]);

        self.pending_layout_reset = false;
        ::log::debug!("[apply_layout_reset] Layout reset complete");
    }

    /// Rate-limited video update - for scrubbing and playback
    /// During scrubbing: update at ~10fps to keep UI responsive
    /// During playback: update at video fps to match content
    fn update_videos_rate_limited(&mut self, cx: &mut Cx) {
        // Calculate minimum interval between video decodes
        // Scrubbing: 100ms (10fps) for UI responsiveness
        // Playback: match video fps (e.g., 33ms for 30fps)
        let min_interval_ms = if self.is_scrubbing {
            SCRUB_RATE_LIMIT_MS
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
    ///
    /// Updates video players at each physical slot based on the current
    /// slot-to-panel mapping. This handles drag-and-drop correctly.
    fn update_videos_now(&mut self, cx: &mut Cx) {
        // Only decode if time has changed
        let time_changed = (self.data.current_time - self.last_video_time).abs() > TIME_EPSILON;

        if time_changed {
            self.last_video_time = self.data.current_time;
            self.last_video_decode_instant = Some(Instant::now());

            let frame_idx = self.data.current_frame_index();
            let total = self.data.total_frames();
            let current_time = self.data.current_time;

            // Get panel visibility state
            let panel_grid = self.ui.panel_grid(cx, ids!(panel_grid));
            let layout_state = panel_grid.layout_state();

            // Video player widget IDs for each physical slot
            let video_slots = [
                ids!(video_slot0), ids!(video_slot1), ids!(video_slot2), ids!(video_slot3)
            ];

            // Update each physical slot's video player
            for (slot_idx, panel_id) in self.data.slot_to_panel.iter().enumerate() {
                // Check if panel is visible
                let is_visible = layout_state.as_ref()
                    .map(|s| s.visible_panels.contains(panel_id))
                    .unwrap_or(true);

                if !is_visible {
                    continue;
                }

                // Check if this slot shows video (not robot view)
                let panel_slot = PanelSlot::from_panel_id(panel_id);
                let is_video = panel_slot.map(|ps| ps != PanelSlot::RobotView).unwrap_or(false);

                if is_video {
                    let player = self.ui.video_player(cx, video_slots[slot_idx]);
                    player.show_frame_at_time(cx, current_time);
                    player.set_frame_info(cx, frame_idx, total);
                }
            }
        }

        // Always update timeline (lightweight)
        let timeline = self.ui.timeline(cx, ids!(timeline));
        timeline.set_duration(cx, self.data.episode_duration, self.data.episode_fps);
        timeline.set_current_time(cx, self.data.current_time);
        timeline.set_playing(cx, self.data.is_playing);
    }

    fn update_robot_viewer(&mut self, cx: &mut Cx) {
        if let Some(frame) = self.data.current_frame() {
            let joint_angles: Vec<f32> = frame.state.to_vec();

            // Update all robot view slots (only the visible one matters)
            let robot_slots = [
                ids!(robot_slot0), ids!(robot_slot1), ids!(robot_slot2), ids!(robot_slot3)
            ];
            for slot_id in &robot_slots {
                let robot_view = self.ui.widget(cx, *slot_id);
                if let Some(mut rv) = robot_view.borrow_mut::<RobotView>() {
                    rv.set_joint_angles(cx, &joint_angles);
                };
            }
        }
    }

    /// Load URDF robot based on robot_type from dataset
    fn load_robot_urdf(&mut self, cx: &mut Cx) {
        // Robot view slots
        let robot_slots = [
            ids!(robot_slot0), ids!(robot_slot1), ids!(robot_slot2), ids!(robot_slot3)
        ];

        if let Some((urdf_path, assets_dir, display_name)) = Self::get_urdf_path(&self.data.robot_type) {
            ::log::info!("Loading URDF for robot_type '{}': {}", self.data.robot_type, urdf_path);

            // Store robot display name for use in panel titles
            self.data.robot_display_name = Some(display_name.clone());

            // Load the robot model into ALL robot view slots
            for slot_id in &robot_slots {
                let robot_view = self.ui.widget(cx, *slot_id);
                if let Some(mut rv) = robot_view.borrow_mut::<RobotView>() {
                    rv.load_robot(&urdf_path, &assets_dir);
                }
                robot_view.redraw(cx);
            }

            // Update registry with robot name
            let title = format!("{} 3D View", display_name);
            self.data.panel_registry.set_content(
                PanelSlot::RobotView,
                PanelContent::RobotView { display_name: title.clone() }
            );

            // Update panel title
            let panel_grid = self.ui.panel_grid(cx, ids!(panel_grid));
            panel_grid.set_panel_titles(&[("panel_1", &title)]);
        } else {
            ::log::warn!("No URDF mapping found for robot_type: {}", self.data.robot_type);
            self.data.robot_display_name = None;

            // Update registry with default name
            self.data.panel_registry.set_content(
                PanelSlot::RobotView,
                PanelContent::RobotView { display_name: "3D View".to_string() }
            );

            // Reset to default title
            let panel_grid = self.ui.panel_grid(cx, ids!(panel_grid));
            panel_grid.set_panel_titles(&[("panel_1", "3D View")]);
        }
    }

    /// Map robot_type to URDF file path, assets directory, and display name
    /// Only returns a path if the URDF file actually exists
    fn get_urdf_path(robot_type: &str) -> Option<(String, String, String)> {
        // Known robot type mappings: (pattern, urdf_path, assets_dir, display_name)
        let mappings: &[(&str, &str, &str, &str)] = &[
            ("so101", "data/so100/so100.urdf", "data/so100", "SO-100"),
            ("so100", "data/so100/so100.urdf", "data/so100", "SO-100"),
            ("aimee", "data/so100/so100.urdf", "data/so100", "SO-100"),
            ("lekiwi", "data/lekiwi/lekiwi.urdf", "data/lekiwi", "LeKiwi"),
            ("moss", "data/moss/moss.urdf", "data/moss", "Moss"),
            ("koch", "data/koch/koch.urdf", "data/koch", "Koch"),
            ("vx300s", "data/vx300s/vx300s.urdf", "data/vx300s", "ViperX 300s"),
        ];

        let robot_lower = robot_type.to_lowercase();

        // Check known mappings - only return if file exists
        for (pattern, urdf, assets, display_name) in mappings {
            if robot_lower.contains(pattern) {
                if std::path::Path::new(urdf).exists() {
                    ::log::info!("Found URDF for '{}': {} ({})", robot_type, urdf, display_name);
                    return Some((urdf.to_string(), assets.to_string(), display_name.to_string()));
                }
            }
        }

        // Try robot_type as folder name: data/{robot_type}/{robot_type}.urdf
        let folder_urdf = format!("data/{}/{}.urdf", robot_lower, robot_lower);
        if std::path::Path::new(&folder_urdf).exists() {
            ::log::info!("Found URDF at: {}", folder_urdf);
            return Some((folder_urdf, format!("data/{}", robot_lower), robot_type.to_string()));
        }

        // Try robot_type as direct file: data/{robot_type}.urdf
        let direct_urdf = format!("data/{}.urdf", robot_lower);
        if std::path::Path::new(&direct_urdf).exists() {
            ::log::info!("Found URDF at: {}", direct_urdf);
            return Some((direct_urdf, "data".to_string(), robot_type.to_string()));
        }

        ::log::info!("No URDF file found for robot_type: {}", robot_type);
        None
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
        let state_plot = self.ui.time_series_plot(cx, ids!(state_plot));
        state_plot.set_title(cx, "observation.state");
        state_plot.set_time_range(0.0, self.data.episode_duration);
        state_plot.set_auto_scale_y(true);
        state_plot.set_window_size(PLOT_WINDOW_SIZE);

        for ch in 0..state_channels.min(MAX_PLOT_CHANNELS) {
            let plot_data: Vec<(f64, f64)> = frames.iter()
                .map(|f| (f.timestamp, f.state.get(ch).copied().unwrap_or(0.0) as f64))
                .collect();
            state_plot.set_channel_data(ch, &format!("state[{}]", ch), plot_data);
        }
        state_plot.recompute_scale(cx);

        // Configure and populate action plot
        let action_plot = self.ui.time_series_plot(cx, ids!(action_plot));
        action_plot.set_title(cx, "action");
        action_plot.set_time_range(0.0, self.data.episode_duration);
        action_plot.set_auto_scale_y(true);
        action_plot.set_window_size(PLOT_WINDOW_SIZE);

        for ch in 0..action_channels.min(MAX_PLOT_CHANNELS) {
            let plot_data: Vec<(f64, f64)> = frames.iter()
                .map(|f| (f.timestamp, f.action.get(ch).copied().unwrap_or(0.0) as f64))
                .collect();
            action_plot.set_channel_data(ch, &format!("action[{}]", ch), plot_data);
        }
        action_plot.recompute_scale(cx);
    }

    /// Update plot cursor positions (lightweight - just moves cursor)
    fn update_plots_cursor(&mut self, cx: &mut Cx) {
        let state_plot = self.ui.time_series_plot(cx, ids!(state_plot));
        state_plot.set_cursor_time(cx, self.data.current_time);

        let action_plot = self.ui.time_series_plot(cx, ids!(action_plot));
        action_plot.set_cursor_time(cx, self.data.current_time);
    }

    /// Apply theme to custom sidebar and header elements
    ///
    /// Widgets living inside lazily-created dock tab content may not exist yet
    /// on early events — skip empty refs (the rik apply_over was a silent no-op).
    fn apply_custom_theme(&mut self, cx: &mut Cx) {
        let dm = get_global_dark_mode();

        // Backgrounds (logo + sidebar/footer containers and headers).
        // The dock registers instantiated tab content under the tab item ids
        // (left_panel / right_panel / controller_tab), not the template names.
        for path in [
            ids!(logo_container),
            ids!(left_panel),
            ids!(left_sidebar_header),
            ids!(right_panel),
            ids!(right_sidebar_header),
            ids!(controller_tab),
            ids!(footer_sidebar_header),
        ] {
            let mut w = self.ui.widget(cx, path);
            if !w.is_empty() {
                script_apply_eval!(cx, w, {
                    draw_bg +: { dark_mode: #(dm) }
                });
            }
        }

        // Titles (text color follows theme)
        for path in [
            ids!(left_sidebar_title),
            ids!(right_sidebar_title),
            ids!(footer_sidebar_title),
        ] {
            let mut w = self.ui.widget(cx, path);
            if !w.is_empty() {
                script_apply_eval!(cx, w, {
                    draw_text +: { dark_mode: #(dm) }
                });
            }
        }
    }
}

app_main!(DoRobotApp);
