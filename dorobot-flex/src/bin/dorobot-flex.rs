// DoRobot Studio: real datasets and LeRobot capture, with explicit --demo fixtures.

use dorobot_studio_plugin::{PanelContext, PanelDescriptor, PluginRegistry};
use dorobot_flex::api::{files::FileBackend, mock::MockBackend, Backend, Intent, Screen};
use dorobot_flex::api::nexus::{NexusIntent, NexusKind};
use dorobot_flex::playback_controls::PlaybackAction;
use dorobot_flex::preferences;
use dorobot_flex::i18n::{self, Language, Localizer};
use dorobot_flex::ui::flex_layout::FlexPanelsWidgetRefExt;
use dorobot_flex::ui::{
    eval::EvalScreenWidgetRefExt, frame, hardware::HardwareScreenWidgetRefExt,
    library::LibraryScreenWidgetRefExt, play::{self, PlayScreenWidgetRefExt},
};
use dorobot_flex::widgets::time_series_plot::TimeSeriesPlotAction;
use dorobot_flex::widgets::timeline::TimelineAction;
use makepad_urdf_player::robot_view::RobotViewWidgetRefExt;
use makepad_widgets::*;

/// Transport tick. Matches the shipping player, and is independent of the
/// dataset's own fps: it advances a clock, and every view resolves itself
/// against that clock rather than counting frames of its own.
const PLAYBACK_TIMER_FPS: f64 = 60.0;

app_main!(App, font_assets: [INTER_FONT_ASSET]);

// Focused buttons own Space/Return; playback shortcuts must not perform a
// second, unrelated action when a person activates one with the keyboard.
fn focused_control(root: &WidgetRef, cx: &Cx) -> bool {
    if cx.has_key_focus(root.area())
        && (root.borrow::<Button>().is_some() || root.borrow::<TextInput>().is_some())
    {
        return true;
    }
    let mut found = false;
    root.children(&mut |_, child| {
        if !found {
            found = focused_control(&child, cx);
        }
    });
    found
}

/// `--dataset <dir>` opens a dataset or library. Fixtures require `--demo`.
fn make_backend() -> Box<dyn Backend> {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let args: Vec<String> = std::env::args().collect();
    let demo = args.iter().any(|a| a == "--demo")
        || std::env::current_exe()
            .ok()
            .is_some_and(|p| p.file_stem().is_some_and(|n| n == "ui_mock"));
    if demo {
        return Box::new(MockBackend::new());
    }
    let root = args
        .iter()
        .position(|a| a == "--dataset")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| std::env::var("DOROBOT_DATASETS").ok())
        .unwrap_or_else(|| dorobot_flex::runtime::datasets().display().to_string());
    let prefer = args
        .iter()
        .position(|a| a == "--open")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str);
    let mut backend = FileBackend::with_open(root, prefer);
    let profile = args
        .iter()
        .position(|a| a == "--rig")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| std::env::var("DOROBOT_RIG").ok());
    if let Some(p) = profile {
        backend.dispatch(Intent::ConfigureRig(p.into()));
    }
    if let Some(socket)=args.iter().position(|a|a=="--live-socket").and_then(|i|args.get(i+1)) {
        backend.connect_live(socket.into());
    }
    let mut zenoh = dorobot_flex::api::telemetry::ZenohOptions::default();
    for pair in args.windows(2) {
        match pair[0].as_str() {
            "--zenoh-connect" => zenoh.connect_endpoints.push(pair[1].clone()),
            "--zenoh-mode" => zenoh.mode.clone_from(&pair[1]),
            _ => {}
        }
    }
    backend.configure_telemetry(zenoh);
    Box::new(backend)
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.ux.*

    let WorkflowInput = TextInput{
        height: 32 min_height: 32 margin: 0.
        padding: Inset{left: 8. right: 8. top: 7. bottom: 7.}
        draw_text +: {text_style: mod.widgets.ux.TEXT_BODY{}}
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.5, 0.5, self.rect_size.x-1.0, self.rect_size.y-1.0, 4.0)
                sdf.fill_keep(mix(#x161618, #xFFFFFF, self.light))
                sdf.stroke(mix(mix(#x636366, #xAEAEB2, self.light), #x0A84FF, self.focus), 1.0)
                return sdf.result
            }
        }
    }
    startup() do #(App::script_component(vm)){
        ui: Root{

            main_window := Window{
                window.title: "DoRobot Studio"
                window.inner_size: vec2(1536, 1024)
                pass.clear_color: mod.theme.color_bg_app

                body +: {
                    flow: Right width: Fill height: Fill
                    show_bg: true draw_bg.color: mod.theme.color_bg_app
                    sidebar := View{
                        width: 196 height: Fill flow: Down padding: 12.0 spacing: 12.0
                        show_bg: true draw_bg.color: mod.theme.color_bg_container
                        Label{ width: Fill height: Fit margin: Inset{left: 8. top: 8. bottom: 12.} text: "DoRobot Studio"
                            draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_TITLE{}} }
                        Label{width: Fill text: "Plugins" padding: Inset{left: 12. top: 6. bottom: 0.}
                            draw_text +: {color: mod.theme.color_text_disabled text_style: mod.widgets.ux.TEXT_META{}}}
                        plugin_tools := View{width: Fill height: Fit flow: Down spacing: 4.}
                        View{width: Fill height: 1 show_bg: true draw_bg.color: mod.theme.color_bg_app}
                        data_workspace := mod.widgets.ux.QuietButton{
                            width: Fill height: 40 min_height: 40 text: "Data workspace"
                            align: Align{x: 0.0 y: 0.5} padding: Inset{left: 12. right: 12.}
                            draw_text +: {text_style: mod.widgets.ux.TEXT_NAV{}}
                        }
                        Filler{}
                        sidebar_tools := View{width: Fill height: 32 flow: Right spacing: 4.
                            language_toggle := mod.widgets.ux.QuietButton{
                                width: 32 height: 32 padding: 0. text: "EN"
                                a11y_name: "Switch language: English / 简体中文"
                            }
                            appearance := mod.widgets.ux.QuietButton{width: Fill height: 32 text: "Light appearance"}
                        }
                    }
                    sidebar_resize := View{width: 8 height: Fill cursor: MouseCursor.ColResize}
                    workspace := View{
                        width: Fill height: Fill flow: Down
                        page_header := View{
                            width: Fill height: 36 flow: Right align: Align{y: 0.5}
                            padding: Inset{left: 16. right: 12. top: 0. bottom: 0.} spacing: 16.0
                            page_title := Label{width: Fit height: Fit padding: 0.0 text: "Library"
                                draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.FONT_MEDIUM{font_size: 14.0}} }
                            status := Label{width: Fill height: Fit padding: 0.0 max_lines: 1 text_overflow: Ellipsis text: ""
                                draw_text +: {color: mod.theme.color_text_disabled text_style: mod.widgets.ux.TEXT_META{}} }
                            help_toggle := mod.widgets.ux.QuietButton{text: "Help"}
                        }
                        workspace_tabs := View{
                            width: Fill height: 42 flow: Right padding: Inset{left: 6. right: 12. bottom: 4.}
                            plugin_tabs := View{width: Fill height: Fill flow: Right spacing: 2.}
                            data_tabs := View{width: Fill height: Fill flow: Right spacing: 2.
                                tab_library := mod.widgets.ux.WorkspaceTab{text: "Library"}
                                tab_play := mod.widgets.ux.WorkspaceTab{text: "Replay"}
                                tab_eval := mod.widgets.ux.WorkspaceTab{text: "Evaluate"}
                                use_training := mod.widgets.ux.WorkspaceTab{text: "Use for training"}
                            }
                        }
                        hub_form := View{ visible: false width: Fill height: Fit flow: Right spacing: 8. padding: 12.
                            hub_repo := TextInput{width: Fill height: 44 empty_text: "Dataset repository: owner/name"}
                            hub_go := Button{ text: "Transfer" height: 44 }
                            hub_cancel := Button{ text: "Cancel" height: 44 }
                        }
                        capture_notice := mod.widgets.ux.Card{visible: false width: Fill height: 38 flow: Right padding: Inset{left: 10. right: 6. top: 3. bottom: 3.} spacing: 8. align: Align{y: 0.5}
                            capture_notice_text := Label{width: Fill padding: 0. max_lines: 1 text_overflow: Ellipsis text: "" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_BODY{}}}
                            capture_return := mod.widgets.ux.ToolButton{text: "Go to Record"}
                        }
                        workflow := mod.widgets.ux.Card{visible: false width: Fill height: Fit flow: Down padding: Inset{left: 16. right: 12. top: 10. bottom: 10.} spacing: 10.
                            source_row := View{width: Fill height: Fit flow: Right spacing: 8. align: Align{y: 0.5}
                                wf_sim := mod.widgets.ux.SegmentButton{text: "Simulation"}
                                wf_real := mod.widgets.ux.SegmentButton{text: "Real robot"}
                                wf_source := Label{width: Fill padding: Inset{left: 8.} max_lines: 1 text_overflow: Ellipsis text: "" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_TITLE{}}}
                                wf_start := mod.widgets.ux.PrimaryButton{text: "Start simulation"}
                                wf_operate := mod.widgets.ux.PrimaryButton{text: "Open controls"}
                                wf_stop := mod.widgets.ux.QuietButton{text: "Stop"}
                                wf_more := mod.widgets.ux.QuietButton{text: "More"}
                            }
                            wf_real_types := View{width: Fill height: Fit flow: Right spacing: 8.
                                wf_arm := mod.widgets.ux.SegmentButton{text: "SO-100 / SO-101"}
                                wf_nano := mod.widgets.ux.SegmentButton{text: "Adora Nano"}
                                Filler{}
                                wf_rig := mod.widgets.ux.QuietButton{text: "Load profile…"}
                                wf_edit := mod.widgets.ux.QuietButton{text: "Edit rig"}
                            }
                            wf_guide := Label{visible: false width: Fill height: Fit padding: Inset{top: 4. bottom: 4.} text: "" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_BODY{}}}
                            wf_detail := Label{width: Fill height: Fit padding: 0. text: "" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}}}
                            wf_setup_actions := View{width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 8.
                                wf_next := mod.widgets.ux.PrimaryButton{text: "Record →"}
                                wf_external := mod.widgets.ux.ToolButton{text: "External stream…"}
                                wf_back := mod.widgets.ux.PrimaryButton{text: "Robot setup →"}
                            }
                            wf_rig_form := View{width: Fill height: Fit flow: Down spacing: 6.
                                View{width: Fill height: Fit flow: Right spacing: 6.
                                    Label{width: 36 padding: 0. text: "Type" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}}}
                                    wf_type := WorkflowInput{width: 110 height: 32 min_height: 32 text: "so101" empty_text: "Robot type"}
                                    Label{width: 58 padding: 0. text: "Follower" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}}}
                                    wf_follower := WorkflowInput{width: Fill height: 32 min_height: 32 empty_text: "Follower serial port · /dev/…"}
                                    Label{width: 50 padding: 0. text: "Leader" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}}}
                                    wf_leader := WorkflowInput{width: Fill height: 32 min_height: 32 empty_text: "Leader serial port · /dev/…"}
                                }
                                View{width: Fill height: Fit flow: Right spacing: 6.
                                    Label{width: 90 padding: 0. text: "Cameras" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_BODY{}}}
                                    wf_cameras := WorkflowInput{width: Fill height: 32 min_height: 32 empty_text: "Camera indices · e.g. 0,1"}
                                    wf_create := mod.widgets.ux.PrimaryButton{text: "Use this rig"}
                                }
                            }
                            wf_capture_fields := View{width: Fill height: Fit flow: Down spacing: 6.
                                View{width: Fill height: Fit flow: Right spacing: 8. align: Align{y: 0.5}
                                    Label{width: 90 padding: 0. text: "Task" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_BODY{}}}
                                    wf_task := WorkflowInput{width: Fill height: 32 min_height: 32 empty_text: "Describe the task to capture"}
                                }
                                View{width: Fill height: Fit flow: Right spacing: 8. align: Align{y: 0.5}
                                    Label{width: 90 padding: 0. text: "Save folder" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_BODY{}}}
                                    wf_destination := WorkflowInput{width: Fill height: 32 min_height: 32 empty_text: "Absolute recording folder"}
                                    wf_browse := mod.widgets.ux.ToolButton{text: "Choose…"}
                                }
                            }
                            wf_capture := View{width: Fill height: Fit flow: Flow.Right{wrap: false} spacing: 8. align: Align{y: 0.5}
                                wf_record := mod.widgets.ux.PrimaryButton{text: "● Record"}
                                wf_finish := mod.widgets.ux.StopButton{text: "■ Stop recording"}
                                wf_save := mod.widgets.ux.PrimaryButton{text: "Save episode"}
                                wf_discard := mod.widgets.ux.ToolButton{text: "Discard take"}
                                wf_replay := mod.widgets.ux.ToolButton{text: "Replay →"}
                                wf_count := Label{width: Fill padding: 0. text: "" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}}}
                            }
                        }
                        live_bar := View{
                            visible: false width: Fill height: 40 flow: Right spacing: 8.0
                            align: Align{y: 0.5} padding: Inset{left: 6. right: 6. top: 4. bottom: 4.}
                            Label{text: "Dora source" padding: 0.0 draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}}}
                            live_source := TextInput{width: Fill height: 32 min_height: 32 margin: 0.0 text: "/tmp/nano-living.sock"
                                empty_text: "Local stream socket path"
                                padding: Inset{left: 8. right: 8. top: 7. bottom: 7.}
                                draw_text +: {text_style: mod.widgets.ux.TEXT_BODY{}}
                                draw_bg +: {border_radius: 4.0}
                            }
                            live_connect := mod.widgets.ux.ToolButton{text: "Connect"}
                            live_disconnect := mod.widgets.ux.ToolButton{text: "Disconnect"}
                            live_recording := mod.widgets.ux.ToolButton{text: "Open recording" visible: false}
                        }
                        gamepad_help := View{
                            visible: false width: Fill height: Fit flow: Right spacing: 8.0
                            align: Align{y: 0.5} padding: Inset{left: 10. right: 6. top: 4. bottom: 4.}
                            gamepad_mapping := Label{width: Fill height: Fit padding: 0.0 max_lines: 3 text_overflow: Ellipsis text: ""
                                draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}}}
                            gamepad_setup := mod.widgets.ux.PrimaryButton{text: "Robot setup"}
                        }
                        setup_page := View{
                                visible: false width: Fill height: Fit flow: Down padding: 6.0
                                setup_card := mod.widgets.ux.Card{
                                    width: Fill height: Fit flow: Down padding: 8.0 spacing: 6.0
                                    setup_title := Label{width: Fill padding: 0.0 text: "Set up your robot"
                                        draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_TITLE{}} }
                                    setup_description := Label{width: Fill height: Fit padding: 0.0 text: ""
                                        draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}} }
                                    setup_actions := View{width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 8.0
                                        setup_rig := mod.widgets.nx.Primary{height: 32 min_height: 32 padding: Inset{left: 12. right: 12. top: 0. bottom: 0.} text: "Robot setup →"}
                                        setup_rescan := mod.widgets.ux.ToolButton{text: "Check connections"}
                                        setup_replay := mod.widgets.ux.ToolButton{text: "Open replay"}
                                    }
                                }
                            }
                        pages := View{
                            width: Fill height: Fill flow: Overlay
                            page_library := LibraryScreen{}
                            page_play := PlayScreen{visible: false}
                            page_eval := EvalScreen{visible: false}

                        }
                    }
                    language_hint := Tooltip{
                        width: 0 height: 0
                        content +: {padding: 8.
                            tooltip_label +: {width: Fit text: "English / 简体中文"}
                        }
                    }
                }
            }
            replay_window := Window{
                window.title: "DoRobot Studio — Replay"
                window.inner_size: vec2(1280, 900)
                create_on_start: false
                body +: {flow: Down width: Fill height: Fill
                    monitor_bar := View{width: Fill height: 56 flow: Right padding: 6.0
                        monitor_title := Label{width: Fill text: "Replay · shared episode and playback" draw_text.color: mod.theme.color_text}
                        close_monitor := Button{height: 44 text: "Close window"}
                    }
                    monitor_play := PlayScreen{
                        upper +: {grid +: {persistence_key: "replay-monitor-layout.json"}}
                        panel_tools +: {new_window +: {visible: false}}
                    }
                }
            }
        }
    }
}

enum FolderRequest {
    Dataset,
    Recording,
    Download(String),
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    language: Language,
    #[rust]
    localizer: Localizer,
    /// Production backend by default; generated fixtures require explicit demo mode.
    #[rust(make_backend())]
    backend: Box<dyn Backend>,
    #[rust(dorobot_flex::plugins::builtin_plugins())]
    plugins: PluginRegistry,
    #[rust(false)]
    started: bool,
    #[rust]
    rig_editing: bool,
    #[rust]
    external_stream: bool,
    #[rust]
    workflow_help: bool,
    #[rust]
    workflow_more: bool,
    #[rust]
    playback_timer: Timer,
    #[rust]
    last_tick: Option<std::time::Instant>,
    #[rust]
    last_refresh: Option<std::time::Instant>,
    #[rust]
    hub_upload: Option<bool>,
    #[rust]
    folder_request: Option<FolderRequest>,
    #[rust]
    monitor_open: bool,
    #[rust(196.0)]
    sidebar_width: f64,
    #[rust]
    resizing_sidebar: bool,
    #[rust(Screen::Library)]
    data_screen: Screen,
}

impl App {
    fn navigate(&mut self, screen: Screen) {
        self.plugins.follow_navigation();
        let target = if self.plugins.screen_available(screen) { screen } else { self.data_screen };
        self.backend.dispatch(Intent::Navigate(target));
    }
    fn open_panel(&mut self, panel: PanelDescriptor) {
        if let Some(screen) = panel.legacy_screen {
            self.navigate(screen);
        } else if let Err(error) = self.plugins.select(panel.id) {
            ::log::warn!("{error}");
        }
    }
    fn sync_workspace_navigation(&mut self, cx: &mut Cx) {
        if self.plugins.selected().is_none() && !self.plugins.screen_available(self.backend.screen()) {
            self.navigate(self.data_screen);
        }
        let screen = self.backend.screen();
        self.plugins.sync_navigation(cx, screen);
        let plugin = self.plugins.active_plugin(screen);
        let data = plugin.is_none() && self.plugins.selected().is_none()
            && matches!(screen, Screen::Library | Screen::Play | Screen::Eval);
        if data && matches!(screen, Screen::Library | Screen::Play | Screen::Eval) {
            self.data_screen = screen;
        }
        self.ui.label(cx, ids!(page_title)).set_text(cx, plugin.map_or("Data workspace", |p| p.title));
        self.ui.widget(cx, ids!(workspace_tabs.plugin_tabs)).set_visible(cx, !data);
        self.ui.widget(cx, ids!(workspace_tabs.data_tabs)).set_visible(cx, data);
        let light = frame::light_mode();
        frame::sync_nav_button(cx, &self.ui.widget(cx, ids!(data_workspace)), data, light);
        let data_tabs = self.ui.widget(cx, ids!(workspace_tabs.data_tabs));
        let nexus = self.backend.nexus();
        let use_training = data_tabs.button(cx, ids!(use_training));
        use_training.set_visible(cx, nexus.is_some() && self.backend.dataset_id().is_some());
        use_training.set_enabled(cx, nexus.is_some_and(|state| state.allows(&NexusIntent::UseOpenDataset)));
        frame::sync_nav_button(cx, &data_tabs.widget(cx, ids!(use_training)), false, light);
        for (path, target) in [(ids!(tab_library) as &[LiveId], Screen::Library), (ids!(tab_play), Screen::Play), (ids!(tab_eval), Screen::Eval)] {
            frame::sync_nav_button(cx, &data_tabs.widget(cx, path), data && screen == target, light);
        }
    }
    fn sync_plugins(&mut self, cx: &mut Cx) {
        let live_controls = self.backend.screen() != Screen::Live || self.external_stream
            || self.backend.hardware().nano.is_some()
            || self.backend.live_connection().is_some_and(|c| c.phase.active());
        self.plugins.sync(cx, &PanelContext { backend: &*self.backend, rig_editing: self.rig_editing,
            show_help: self.workflow_help, show_details: self.workflow_more, live_controls });
    }
    fn localize(&mut self, cx: &mut Cx) {
        let mut protected = std::collections::HashSet::new();
        let mut protect = |path: &str| {
            let path: Vec<_> = path.split('.').map(LiveId::from_str).collect();
            let widget = self.ui.widget(cx, &path);
            if !widget.is_empty() { protected.insert(widget.widget_uid()); }
        };
        // These captions are data, not UI messages, even if a user names their
        // dataset/task "Record". Editable values are excluded by Localizer too.
        for i in 0..6 {
            let row = if i < 3 { "row_a" } else { "row_b" };
            for suffix in ["head.title", "body.name"] {
                protect(&format!("page_library.left_col.grid.{row}.card_{i}.{suffix}"));
            }
            protect(&format!("page_record.body.left.traces.tr_body.trace_{i}.jn"));
            protect(&format!("page_hardware.content.camera_a_panel.content.calib.joints.joint_{i}.jname"));
        }
        for i in 0..2 {
            for suffix in ["dev_name", "dev_detail"] {
                protect(&format!("page_library.hardware_rail.hw_body.dev_{i}.text_col.{suffix}"));
            }
            if self.backend.record().cameras.get(i).is_some() {
                protect(&format!("page_record.body.left.cams.cam_{i}.head.title"));
            }
        }
        if !self.backend.record().profile_label.is_empty() { protect("page_record.sbar.profile.label"); }
        if !self.backend.record().task.is_empty() { protect("page_record.sbar.task"); }
        if !self.backend.eval().checkpoint.is_empty() { protect("page_eval.pbar.ckpt.label"); }
        for page in ["page_play", "hardware_teleop", "monitor_play"] {
            for path in play::custom_camera_caption_paths(self.backend.playback()) {
                protect(&format!("{page}.{path}"));
            }
            protect(&format!("{page}.upper.tree.tree_head.title"));
            if self.backend.playback().live.is_none() { protect(&format!("{page}.upper.side.side_body.task_t")); }
            for i in 0..3 { protect(&format!("{page}.upper.tree.tree_body.grp_{i}")); }
        }
        self.localizer.apply(cx, &self.ui, self.language, &protected);
    }

    fn display_status(&self) -> &str {
        if self.plugins.selected().is_some_and(|p| p.starts_with("aisle_")) {
            return self.backend.status().filter(|s| !s.is_empty() && !s.starts_with("Choose Simulation or Real robot")).unwrap_or("AISLE · Nexus simulation");
        }
        if matches!(self.plugins.selected(), Some("hardware_live_mirror" | "nexus_setup" | "nexus_train" | "nexus_rollout"))
            || (self.backend.screen() == Screen::Record && self.nexus_record())
        {
            return "";
        }
        let status = self.backend.status().unwrap_or("Demo · no hardware connected");
        if status.starts_with("Simulation setup") { "Ready" }
        else if status.starts_with("Real robot setup") || status.starts_with("Choose a USB serial port") { "Not connected" }
        else { status }
    }
    fn sync_workflow(&self, cx: &mut Cx) {
        let state = self.backend.workflow();
        let screen = self.backend.screen();
        let mirror = self.plugins.selected() == Some("hardware_live_mirror");
        let setup = screen == Screen::Hardware && !mirror;
        let record = screen == Screen::Record && !mirror && !self.nexus_record();
        let simulation = state.simulation;
        let pending = if simulation { state.pending() } else { self.backend.record().recording || self.backend.record().last_take.is_some() };
        let nano = self.backend.hardware().nano.is_some();
        let has_profile = !self.backend.record().profile_label.is_empty();
        let real_form = setup && !simulation && !nano && (!has_profile || self.rig_editing);
        let capture_ready = simulation && (state.ready || state.pending() || state.phase == "error" || state.dataset.is_some());
        let fields = real_form || (record && capture_ready && state.phase != "recording");
        self.ui.widget(cx, ids!(capture_notice)).set_visible(cx, pending && !record);
        self.ui.label(cx, ids!(capture_notice_text)).set_text(cx, &if simulation { format!("{} · {} frames", if state.phase == "recording" { "● Recording" } else { "Unsaved take" }, state.frames) } else if self.backend.record().recording { "● Recording".into() } else { "Unsaved take".into() });
        self.ui.widget(cx, ids!(workflow)).set_visible(cx, setup || (record && (capture_ready || (!simulation && !nano && has_profile))));
        self.ui.widget(cx, ids!(source_row)).set_visible(cx, setup);
        self.ui.widget(cx, ids!(wf_real_types)).set_visible(cx, setup && !simulation);
        for (path, selected) in [(ids!(wf_sim),simulation),(ids!(wf_real),!simulation),(ids!(wf_arm),!nano),(ids!(wf_nano),nano)] {
            let mut button = self.ui.widget(cx,path);
            let selected = if selected { 1.0 } else { 0.0 };
            script_apply_eval!(cx, button, {draw_bg +: {on: #(selected)} draw_text +: {on: #(selected)}});
        }
        self.ui.label(cx, ids!(wf_source)).set_text(cx, if simulation { "Nano · Living room" } else if nano { "Adora Nano" } else { "Arm rig" });
        self.ui.widget(cx, ids!(help_toggle)).set_visible(cx, setup || (screen == Screen::Live && self.backend.playback().live.is_some()) || (record && (capture_ready || (!simulation && !nano && has_profile))));
        self.ui.button(cx, ids!(help_toggle)).set_text(cx, if self.workflow_help { "Hide help" } else { "Help" });
        self.ui.button(cx, ids!(wf_more)).set_text(cx, if self.workflow_more { "Less" } else { "More" });
        self.ui.widget(cx, ids!(wf_guide)).set_visible(cx, self.workflow_help);
        self.ui.label(cx, ids!(wf_guide)).set_text(cx, if simulation {
            if setup { "Model zeros and limits are preset. Restart to reset.\nPS4/PS5: hold L1 to move; release to stop. R1 changes control mode." }
            else { "Captures all 3 cameras, depth, joints and scene data. 5 fps in simulation time.\nStop → Save → Replay. Discard removes only the unsaved take." }
        } else if nano { "Connect the USB serial port to drive the base. Bus test checks commands without motors.\nArm/head calibration and camera capture are unavailable with this driver." }
        else { "Connect follower, leader and cameras, then save the rig.\nCalibrate each device before recording. Press Continue only when the calibration prompt asks." });
        let detail = if simulation {
            if state.phase == "saved" || (state.phase == "idle" && ((state.active && state.ready) || (!state.active && (state.detail.starts_with("Nano · Living room") || state.detail.starts_with("Simulation stopped.") || state.detail.starts_with("Unsaved take discarded."))))) { String::new() }
            else if state.phase == "recording" { "3 cameras · 17 joints · 5 sim fps".into() }
            else { state.detail.clone() }
        } else if nano { "Base only · arms, head and cameras unavailable".into() }
        else if state.calibrated { "Calibration complete".into() }
        else { String::new() };
        self.ui.label(cx, ids!(wf_detail)).set_text(cx,&detail);
        self.ui.widget(cx, ids!(wf_detail)).set_visible(cx,!detail.is_empty() && !(record && state.phase == "recording"));
        self.ui.widget(cx, ids!(wf_back)).set_visible(cx, false);
        self.ui.widget(cx, ids!(wf_rig_form)).set_visible(cx, real_form);
        self.ui.widget(cx, ids!(wf_capture_fields)).set_visible(cx, fields);
        for path in [ids!(wf_task), ids!(wf_destination), ids!(wf_browse)] {
            self.ui.widget(cx, path).set_disabled(cx, simulation && state.pending() && state.phase != "review");
        }
        self.ui.widget(cx, ids!(wf_start)).set_visible(cx, simulation && setup && !state.active);
        self.ui.widget(cx, ids!(wf_stop)).set_visible(cx, simulation && setup && state.active);
        self.ui.button(cx, ids!(wf_stop)).set_enabled(cx, !state.pending());
        self.ui.widget(cx, ids!(wf_operate)).set_visible(cx, setup && ((simulation && state.ready) || self.backend.hardware().nano.as_ref().is_some_and(|n|n.active)));
        self.ui.widget(cx, ids!(wf_edit)).set_visible(cx, setup && !simulation && has_profile && !nano);
        self.ui.button(cx, ids!(wf_edit)).set_text(cx, if self.rig_editing { "Cancel changes" } else { "Edit rig" });
        self.ui.widget(cx, ids!(wf_rig)).set_visible(cx, !simulation && setup && !nano);
        self.ui.widget(cx, ids!(wf_next)).set_visible(cx, setup && !simulation && state.calibrated);
        self.ui.widget(cx, ids!(wf_external)).set_visible(cx, setup && self.workflow_more);
        self.ui.widget(cx, ids!(wf_setup_actions)).set_visible(cx, setup && (self.workflow_more || (!simulation && state.calibrated)));
        self.ui.widget(cx, ids!(wf_capture)).set_visible(cx, record && (capture_ready || self.backend.record().saved > 0));
        self.ui.widget(cx, ids!(wf_record)).set_visible(cx, simulation && !state.pending());
        self.ui.widget(cx, ids!(wf_finish)).set_visible(cx, simulation && state.phase == "recording");
        self.ui.widget(cx, ids!(wf_save)).set_visible(cx, simulation && state.phase == "review");
        self.ui.widget(cx, ids!(wf_discard)).set_visible(cx, simulation && matches!(state.phase.as_str(), "review" | "error"));
        self.ui.button(cx, ids!(wf_record)).set_enabled(cx, state.can_record());
        self.ui.button(cx, ids!(wf_finish)).set_enabled(cx, state.phase == "recording");
        self.ui.button(cx, ids!(wf_save)).set_enabled(cx, state.phase == "review");
        self.ui.button(cx, ids!(wf_discard)).set_enabled(cx, matches!(state.phase.as_str(), "review" | "error"));
        self.ui.widget(cx, ids!(wf_replay)).set_visible(cx, if simulation { state.dataset.is_some() && !state.pending() } else { self.backend.record().saved > 0 });
        self.ui.label(cx, ids!(wf_count)).set_text(cx, &if simulation { format!("{} frames · {:.1}s · {} saved",state.frames,state.frames as f64 / 5.,state.saved) } else { format!("{} saved episodes",self.backend.record().saved) });
        frame::apply_light(cx, &self.ui, &[
            (ids!(help_toggle),frame::Themed::Both),(ids!(wf_more),frame::Themed::Both),(ids!(workflow),frame::Themed::Bg),(ids!(capture_notice),frame::Themed::Bg),(ids!(capture_return),frame::Themed::Both),
            (ids!(wf_edit),frame::Themed::Both),(ids!(wf_arm),frame::Themed::Both),(ids!(wf_nano),frame::Themed::Both),(ids!(wf_back),frame::Themed::Both),
            (ids!(wf_type),frame::Themed::Bg),(ids!(wf_follower),frame::Themed::Bg),(ids!(wf_leader),frame::Themed::Bg),(ids!(wf_cameras),frame::Themed::Bg),(ids!(wf_task),frame::Themed::Bg),(ids!(wf_destination),frame::Themed::Bg),
            (ids!(wf_sim),frame::Themed::Both),(ids!(wf_real),frame::Themed::Both),
            (ids!(wf_start),frame::Themed::Both),(ids!(wf_stop),frame::Themed::Both),
            (ids!(wf_operate),frame::Themed::Both),(ids!(wf_rig),frame::Themed::Both),
            (ids!(wf_next),frame::Themed::Both),(ids!(wf_external),frame::Themed::Both),
            (ids!(wf_create),frame::Themed::Both),(ids!(wf_browse),frame::Themed::Both),
            (ids!(wf_record),frame::Themed::Both),(ids!(wf_finish),frame::Themed::Both),
            (ids!(wf_save),frame::Themed::Both),(ids!(wf_discard),frame::Themed::Both),(ids!(wf_replay),frame::Themed::Both),
        ],frame::light_mode());
    }
    fn nexus_record(&self) -> bool {
        self.backend.nexus().is_some_and(|state| state.kind == Some(NexusKind::Capture) && state.busy)
    }
    fn sync_live_controls(&self, cx: &mut Cx) {
        if self.plugins.selected().is_some() {
            for path in [ids!(workflow), ids!(live_bar), ids!(gamepad_help)] {
                self.ui.widget(cx, path).set_visible(cx, false);
            }
            return;
        }
        self.sync_workflow(cx);
        let live = self.backend.screen() == Screen::Live;
        let teleop = matches!(self.backend.screen(), Screen::Live | Screen::Record) && self.backend.playback().live.is_some();
        self.ui.flex_panels(cx, ids!(hardware_teleop.upper.grid)).set_external_source(cx, self.external_stream);
        self.ui.widget(cx, ids!(live_bar)).set_visible(cx, live && self.external_stream && self.backend.hardware().nano.is_none());
        self.ui.widget(cx, ids!(gamepad_help)).set_visible(cx, teleop && !self.external_stream);
        self.ui.widget(cx,ids!(gamepad_setup)).set_visible(cx, live);
        self.ui.widget(cx,ids!(hardware_teleop.toolbar.camera_hint)).set_visible(cx, self.workflow_help);
        if teleop {
            self.ui.button(cx, ids!(gamepad_setup)).set_text(cx, if self.backend.screen() == Screen::Record { "Robot setup" } else { "Record →" });
            let phase = &self.backend.playback().stats.task;
            let mapping = if phase.starts_with("AISLE autonomous") {
                if self.language == Language::Chinese { "AISLE · 自主仿真" } else { "AISLE · Autonomous simulation" }
            } else if !self.workflow_help { "Hold L1 to move · Release to stop · R1 mode" } else if phase.contains("Left arm") || phase.contains("Right arm") {
                dorobot_flex::ui::play::ARM_CONTROL_MAP
            } else if phase.contains("Head") {
                "L1: hold to move · R1: change mode · Left stick: head pan / tilt · Options: pause"
            } else {
                "Hold L1 + left stick ↑/↓ to drive · Right stick ←/→ to turn · D-pad also drives / turns · R1: change mode · Options: pause"
            };
            // Keep every control visible, including J5, when other joints
            // reach their limits. Limit/recovery feedback lives in the 3D panel.
            self.ui.label(cx, ids!(gamepad_mapping)).set_text(cx, mapping);
        }
        let connection = self.backend.live_connection();
        let active = connection.is_some_and(|c| c.phase.active());
        self.ui.button(cx, ids!(live_connect)).set_text(cx, if active { "Reconnect" } else { "Connect" });
        self.ui.button(cx, ids!(live_connect)).set_enabled(cx, connection.is_some());
        self.ui.button(cx, ids!(live_disconnect)).set_enabled(cx, active);
        self.ui.widget(cx, ids!(live_recording)).set_visible(cx, connection.is_some_and(|c| c.dataset.is_some()));
        self.ui.label(cx, ids!(monitor_title)).set_text(cx, if live || teleop { "Live · synchronized cameras and robot" } else { "Replay · shared episode and playback" });
    }

    fn hide_monitor(&mut self, cx: &mut Cx) {
        self.monitor_open = false;
        self.ui
            .window(cx, ids!(replay_window))
            .set_visible(cx, false);
    }

    fn save_session(&self) {
        if std::env::args().any(|a| a == "--demo" || a == "--live-socket") {
            return;
        }
        if let Some(path) = preferences::path() {
            let mut state =
                preferences::Session::snapshot(&*self.backend, frame::light_mode() < 0.5);
            state.sidebar_width = self.sidebar_width;
            state.language = self.language;
            if let Err(error) = preferences::save(&path, &state) {
                ::log::warn!("Could not save UI preferences: {}", error.kind());
            }
        }
    }
    /// Repaint every screen from the current backend snapshot.
    fn sync(&mut self, cx: &mut Cx) {
        self.ui
            .label(cx, ids!(main_window.body.workspace.page_header.status))
            .set_text(
                cx,
                self.display_status(),
            );
        self.ui
            .widget(cx, ids!(hub_form))
            .set_visible(cx, self.hub_upload.is_some());
        if self.started {
            self.save_session();
        }
        let light = frame::light_mode();
        // A JIT reapply can retain the previous globe's loaded SVG.
        if let Some(mut button) = self.ui.widget(cx, ids!(language_toggle)).borrow_mut::<Button>() {
            button.draw_icon.svg = None;
            button.draw_icon.svg_doc = None;
        }
        self.ui.button(cx, ids!(language_toggle)).set_text(cx, match self.language {
            Language::English => "EN",
            Language::Chinese => "CN",
        });
        dorobot_ux::sync_appearance(cx, light > 0.5);
        self.ui.button(cx, ids!(appearance)).set_text(
            cx,
            if light > 0.5 {
                "Light appearance"
            } else {
                "Dark appearance"
            },
        );
        self.sync_workspace_navigation(cx);
        frame::theme_chrome(cx, &self.ui, light);
        frame::apply_light(
            cx,
            &self.ui,
            &[
                (ids!(appearance), frame::Themed::Both),
                (ids!(language_toggle), frame::Themed::Both),
                (ids!(setup_card), frame::Themed::Bg),
                (ids!(setup_rescan), frame::Themed::Both),
                (ids!(setup_replay), frame::Themed::Both),
                (ids!(live_connect), frame::Themed::Both),
                (ids!(live_disconnect), frame::Themed::Both),
                (ids!(live_recording), frame::Themed::Both),
                (ids!(gamepad_setup), frame::Themed::Both),
            ],
            light,
        );

        if let Some(mut sidebar) = self.ui.widget(cx, ids!(sidebar)).borrow_mut::<View>() {
            sidebar.walk.width = Size::Fixed(self.sidebar_width);
        }
        self.sync_live_controls(cx);

        self.ui
            .library_screen(cx, ids!(page_library))
            .sync(cx, self.backend.library());


        self.ui
            .play_screen(cx, ids!(page_play))
            .sync(cx, self.backend.playback());
        if matches!(self.backend.screen(), Screen::Live | Screen::Record) && self.backend.playback().live.is_some() {
            self.ui.play_screen(cx, ids!(hardware_teleop)).sync(cx, self.backend.playback());
        }
        self.ui
            .eval_screen(cx, ids!(page_eval))
            .sync(cx, self.backend.eval());

        if self.monitor_open {
            self.ui
                .play_screen(cx, ids!(monitor_play))
                .sync(cx, self.backend.playback());
        }
        // Overlay flow: exactly one page is visible at a time.
        let screen = self.backend.screen();
        let nexus_record = self.nexus_record();
        let setup = match screen {
            Screen::Record if nexus_record => false,
            Screen::Record => if self.backend.workflow().simulation { !self.backend.workflow().active } else { self.backend.record().profile_label.is_empty() || self.backend.hardware().nano.is_some() },
            Screen::Hardware => false,
            Screen::Live => !self.external_stream && self.backend.hardware().nano.is_none() && !self.backend.live_connection().is_some_and(|c|c.phase.active()),
            Screen::Eval => self.backend.eval().checkpoint.is_empty(),
            Screen::Play => self.backend.playback().episodes.is_empty(),
            _ => false,
        };
        self.ui.widget(cx, ids!(setup_page)).set_visible(cx, setup);
        if setup {
            let live_empty = screen == Screen::Live;
            let simulation_empty = screen == Screen::Record && self.backend.workflow().simulation;
            let nano_record = screen == Screen::Record && self.backend.hardware().nano.is_some();
            let evaluating = screen == Screen::Eval;
            let no_recordings = screen == Screen::Play;
            self.ui.button(cx, ids!(setup_replay)).set_text(
                cx,
                if no_recordings {
                    "Open library"
                } else {
                    "Open replay"
                },
            );
            self.ui.label(cx, ids!(setup_title)).set_text(
                cx,
                if live_empty { "No live session running" } else if simulation_empty { "Start a simulation to record" } else if nano_record { "Recording unavailable for the Nano base driver" } else if no_recordings {
                    "Choose a recording to replay"
                } else if evaluating {
                    "No rollout running"
                } else {
                    "Connect a robot to get started"
                },
            );
            self.ui.label(cx, ids!(setup_description)).set_text(cx, if live_empty { "Start a simulation or connect your robot in Robot setup." } else if simulation_empty { "Start Nano in Robot setup, then record its cameras and motion." } else if nano_record { "This driver supports the base only. Use Simulation to capture Nano’s cameras and arms." } else if no_recordings {
                "Choose a dataset from Library."
            } else if evaluating {
                "No policy runner connected. Recorded motion is available in Replay."
            } else {
                "Choose a rig profile, then check connections before recording. No hardware is connected."
            });
            self.ui
                .widget(cx, ids!(setup_rig))
                .set_visible(cx, !evaluating && !no_recordings);
            self.ui
                .widget(cx, ids!(setup_rescan))
                .set_visible(cx, !evaluating && !no_recordings && !simulation_empty && !nano_record && !live_empty);
        }
        for (path, s) in [
            (ids!(page_library) as &[LiveId], Screen::Library),
            (ids!(page_hardware), Screen::Hardware),
            (ids!(page_record), Screen::Record),
            (ids!(page_play), Screen::Play),
            (ids!(page_eval), Screen::Eval),
        ] {
            self.ui.widget(cx, path).set_visible(cx, s == screen || (s == Screen::Play && screen == Screen::Live));
        }
        if screen == Screen::Record && !nexus_record && (self.backend.workflow().simulation || self.backend.hardware().nano.is_some()) { self.ui.widget(cx, ids!(page_record)).set_visible(cx,false); }
        let teleop = (screen == Screen::Live || (screen == Screen::Record && !nexus_record)) && self.backend.playback().live.is_some();
        self.ui.widget(cx, ids!(hardware_teleop)).set_visible(cx, teleop);
        if teleop { self.ui.widget(cx, ids!(page_play)).set_visible(cx, false); self.ui.widget(cx, ids!(page_record)).set_visible(cx, false); }
        if setup && screen == Screen::Live { self.ui.widget(cx, ids!(page_play)).set_visible(cx,false); self.ui.widget(cx, ids!(hardware_teleop)).set_visible(cx,false); }
        if self.backend.hardware().nano.is_some() && screen == Screen::Live {
            self.ui.widget(cx, ids!(page_hardware)).set_visible(cx,true);
            self.ui.widget(cx, ids!(page_play)).set_visible(cx,false);
            self.ui.widget(cx, ids!(hardware_teleop)).set_visible(cx,false);
        }
        if self.plugins.selected().is_some() {
            for path in [ids!(page_library) as &[LiveId], ids!(page_play), ids!(page_eval), ids!(setup_page), ids!(workflow), ids!(live_bar), ids!(gamepad_help)] {
                self.ui.widget(cx, path).set_visible(cx, false);
            }
        }
        self.sync_plugins(cx);
        self.localize(cx);
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;

    fn app_with_disabled_control() -> App {
        let backend = Box::new(MockBackend::new());
        let mut plugins = dorobot_flex::plugins::builtin_plugins();
        plugins.set_enabled("dorobot.control", false, &*backend).unwrap();
        App {
            ui: WidgetRef::default(), language: Language::English, localizer: Localizer::default(),
            backend, plugins, started: false, rig_editing: false, external_stream: false,
            workflow_help: false, workflow_more: false, playback_timer: Timer::default(),
            last_tick: None, last_refresh: None, hub_upload: None, folder_request: None,
            monitor_open: false, sidebar_width: 196.0, resizing_sidebar: false, data_screen: Screen::Play,
        }
    }

    #[test]
    fn disabled_plugin_navigation_returns_to_the_recent_data_page() {
        let mut app = app_with_disabled_control();
        app.navigate(Screen::Live);
        assert_eq!(app.backend.screen(), Screen::Play);
    }

    #[test]
    fn disabled_plugin_current_screen_is_replaced_before_syncing_navigation() {
        let mut app = app_with_disabled_control();
        app.backend.dispatch(Intent::Navigate(Screen::Hardware));
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(|vm| {
            dorobot_studio_core::register_ui(vm);
            let value = script_eval!(vm, {
                use mod.prelude.widgets.* use mod.widgets.*
                View{
                    data_workspace := mod.widgets.ux.QuietButton{}
                    page_title := Label{}
                    workspace_tabs := View{
                        plugin_tabs := View{}
                        data_tabs := View{
                            tab_library := mod.widgets.ux.WorkspaceTab{}
                            tab_play := mod.widgets.ux.WorkspaceTab{}
                            tab_eval := mod.widgets.ux.WorkspaceTab{}
                        }
                    }
                }
            });
            app.ui = WidgetRef::script_from_value(vm, value);
        });
        app.sync_workspace_navigation(&mut cx);
        assert_eq!(app.backend.screen(), Screen::Play);
        cx.with_vm(|vm| assert!(vm.take_errors().is_empty()));
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        self.plugins.mount(cx, &self.ui.widget(cx, ids!(pages))).expect("插件界面挂载失败");
        self.plugins.mount_toolbox(cx, &self.ui.widget(cx, ids!(plugin_tools))).expect("插件工具箱挂载失败");
        self.plugins.mount_tabs(cx, &self.ui.widget(cx, ids!(workspace_tabs.plugin_tabs))).expect("插件标签栏挂载失败");
        let args: Vec<String> = std::env::args().collect();
        self.external_stream = args.iter().any(|a| a == "--live-socket");
        let saved = if args.iter().any(|a| a == "--demo" || a == "--live-socket") {
            None
        } else {
            preferences::path().and_then(|p| preferences::load(&p))
        };
        self.language = args.iter().position(|a| a == "--language").and_then(|i| args.get(i + 1))
            .map(|value| Language::parse(value))
            .unwrap_or_else(|| saved.as_ref().map_or(Language::English, |s| s.language));
        if let Some(saved) = &saved {
            if saved.sidebar_width.is_finite() {
                self.sidebar_width = saved.sidebar_width.clamp(176.0, 320.0);
            }
            saved.restore(
                &mut *self.backend,
                !args.iter().any(|a| a == "--open" || a == "--dataset"),
            );
        }
        // Match the iOS sheet on first draw; --dark selects its dark appearance.
        frame::set_light_mode(
            if args.iter().any(|a| a == "--dark") || saved.as_ref().is_some_and(|s| s.dark) {
                0.0
            } else {
                1.0
            },
        );

        // Optional deep-link so the diff runner can shoot one screen per launch.
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--screen") {
            if let Some(name) = args.get(i + 1) {
                let target = match name.as_str() {
                    "record" => Some(Screen::Record),
                    "play" if args.iter().any(|a| a == "--live-socket") => Some(Screen::Live),
                    "play" => Some(Screen::Play),
                    "live" => Some(Screen::Live),
                    "hardware" => Some(Screen::Hardware),
                    "eval" => Some(Screen::Eval),
                    "library" => Some(Screen::Library),
                    _ => None,
                };
                if let Some(t) = target {
                    self.navigate(t);
                } else if let Some(panel) = match name.as_str() {
                    "aisle-task" => Some("aisle_task"),
                    "aisle-scene" => Some("aisle_scene"),
                    "aisle-run" => Some("aisle_run"),
                    "aisle-evidence" => Some("aisle_evidence"),
                    "nexus-setup" => Some("nexus_setup"),
                    "nexus-training" => Some("nexus_train"),
                    "nexus-inference" => Some("nexus_rollout"),
                    _ => None,
                } {
                    if let Err(error) = self.plugins.select(panel) { ::log::warn!("{error}"); }
                }
            }
        }
        // --light lets the visual-diff runner shoot both themes.
        if args.iter().any(|a| a == "--light") {
            frame::set_light_mode(1.0);
        }
        if let Some(connection) = self.backend.live_connection() {
            self.ui.text_input(cx, ids!(live_source)).set_text(cx, &connection.source);
        }
        let flow = self.backend.workflow();
        self.ui.text_input(cx, ids!(wf_task)).set_text(cx, &flow.task);
        self.ui.text_input(cx, ids!(wf_destination)).set_text(cx, &flow.destination);
        self.sync(cx);
        self.started = true;
        self.playback_timer = cx.start_interval(1.0 / PLAYBACK_TIMER_FPS);
    }

    fn handle_timer(&mut self, cx: &mut Cx, event: &TimerEvent) {
        if self.playback_timer.is_timer(event).is_none() {
            return;
        }
        let now = std::time::Instant::now();
        let dt = self
            .last_tick
            .replace(now)
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        let live_before=self.backend.playback().live.as_ref().map(|v|(v.sequence,v.connected,v.finished));
        let was_live=self.backend.playback().live.is_some();
        let screen_before = self.backend.screen();
        let nexus_record_before = self.nexus_record();
        let nexus_busy_before = self.backend.nexus().is_some_and(|state| state.busy);
        self.backend.poll();
        let nexus_finished = nexus_busy_before && !self.backend.nexus().is_some_and(|state| state.busy);
        if nexus_record_before && !self.nexus_record() && self.backend.screen() == Screen::Record
            && self.plugins.selected().is_none()
        {
            if let Err(error) = self.plugins.select("nexus_setup") { ::log::warn!("{error}"); }
        }
        if nexus_finished || screen_before != self.backend.screen() || nexus_record_before != self.nexus_record() {
            self.sync(cx);
        } else if self.plugins.selected() == Some("hardware_live_mirror") {
            self.sync_plugins(cx);
        } else {
            let nexus_panel = match self.plugins.selected() {
                Some(panel @ ("aisle_task" | "aisle_scene" | "aisle_run" | "aisle_evidence" | "nexus_setup" | "nexus_train" | "nexus_rollout")) => Some(LiveId::from_str(panel)),
                None if self.backend.screen() == Screen::Record && self.nexus_record() => Some(live_id!(page_record)),
                _ => None,
            };
            if let Some(panel) = nexus_panel {
                self.sync_plugins(cx);
                if self.language == Language::Chinese {
                    let panel = self.ui.widget(cx, &[panel]);
                    self.localizer.apply_panel(cx, &panel, self.language);
                }
            }
        }
        let live_after=self.backend.playback().live.as_ref().map(|v|(v.sequence,v.connected,v.finished));
        if live_before!=live_after || (was_live && live_after.is_none()) {
            self.ui.play_screen(cx, ids!(page_play)).sync(cx, self.backend.playback());
            if matches!(self.backend.screen(), Screen::Live | Screen::Record) && self.backend.playback().live.is_some() {
                self.ui.play_screen(cx, ids!(hardware_teleop)).sync(cx, self.backend.playback());
            }
            if self.monitor_open { self.ui.play_screen(cx, ids!(monitor_play)).sync(cx, self.backend.playback()); }
            if was_live != live_after.is_some() { self.sync(cx); }
            self.ui.redraw(cx);
        }
        if self
            .last_refresh
            .is_none_or(|t| t.elapsed().as_millis() >= 100)
        {
            self.last_refresh = Some(now);
            self.sync_live_controls(cx);
            self.ui.play_screen(cx, ids!(page_play)).adapt_layout(cx);
            if matches!(self.backend.screen(), Screen::Live | Screen::Record) && self.backend.playback().live.is_some() {
                self.ui.play_screen(cx, ids!(hardware_teleop)).adapt_layout(cx);
            }
            if self.monitor_open {
                self.ui.play_screen(cx, ids!(monitor_play)).adapt_layout(cx);
            }
            if std::env::var_os("DOROBOT_UX_REPORT").is_some() {
                self.localize(cx);
                let play = self.backend.playback();
                let robot_widget = self
                    .ui
                    .widget(cx, if matches!(self.backend.screen(), Screen::Live | Screen::Record) && play.live.is_some() { ids!(hardware_teleop.upper.grid.robot_panel.stage_3d) } else { ids!(page_play.upper.grid.robot_panel.stage_3d) });
                let robot_probe = robot_widget.borrow::<makepad_urdf_player::robot_view::RobotView>().map(|rv| {
                    let links: std::collections::BTreeMap<_,_> = rv.robot().map(|robot|
                        robot.links.iter().enumerate().filter(|(_,link)|
                            ["left_tool_mount", "right_tool_mount", "head_tilt"].contains(&link.name.as_str()))
                        .map(|(i,link)| (link.name.clone(),robot.link_transforms[i].to_cols_array())).collect()
                    ).unwrap_or_default();
                    serde_json::json!({"names":rv.movable_joint_names(), "base_transform":rv.robot().map(|robot| robot.base_pose.to_cols_array()), "camera_target":rv.camera_target(), "camera_distance":rv.view_distance(), "colors":rv.robot().map(|robot| robot.links.iter().map(|link| (link.name.clone(),link.color)).collect::<std::collections::BTreeMap<_,_>>()), "links":links, "scene_items":rv.external_counts().1, "scene_transforms":rv.external_items().iter().map(|i| i.transform).collect::<Vec<_>>()})
                });
                let hardware_widget = self.ui.widget(cx, ids!(page_hardware.content.robot_panel.content.mirror.viewport));
                let hardware_colors = hardware_widget.borrow::<makepad_urdf_player::robot_view::RobotView>().map(|rv| {
                    rv.robot().map(|robot| robot.links.iter().map(|link| (link.name.clone(), link.color)).collect::<std::collections::BTreeMap<_,_>>()).unwrap_or_default()
                });
                let signals = self.ui.play_screen(cx, ids!(page_play)).signal_probe(cx);
                let monitor_signals = self.ui.play_screen(cx, ids!(monitor_play)).signal_probe(cx);
                let mut probe = serde_json::json!({
                        "screen": self.backend.screen().label(), "dark": frame::light_mode() < 0.5,
                        "workflow": {"calibrated":self.backend.workflow().calibrated,"nano":self.backend.hardware().nano.as_ref().map(|n|serde_json::json!({"active":n.active,"mock":n.mock,"phase":n.phase,"acks":n.acks})),"simulation":self.backend.workflow().simulation,"active":self.backend.workflow().active,"ready":self.backend.workflow().ready,"phase":self.backend.workflow().phase,"frames":self.backend.workflow().frames,"saved":self.backend.workflow().saved,"dataset":self.backend.workflow().dataset,"detail":self.backend.workflow().detail},
                        "episode": play.selected, "playing": play.is_playing, "time": play.current_time,
                        "recorded_joints": play.joints_now(),
                        "signals": signals,
                        "live":play.live.as_ref().map(|v|serde_json::json!({"sequence":v.sequence,"connected":v.connected,"finished":v.finished,"cameras":v.images.keys().collect::<Vec<_>>()})),
                        "connection":self.backend.live_connection().map(|c|serde_json::json!({"phase":c.phase.label(),"preview_fps":c.preview_fps,"frame_age_ms":c.frame_age_ms,"received_frames":c.received_frames,"recording_ready":c.dataset.is_some()})),
                        "base_pose":play.scene.as_ref().and_then(|s|s.frames.get(play.frame_index() as usize)).and_then(|f|f.base_pose.clone()),
                        "monitor_signals": monitor_signals,
                        "robot_probe": robot_probe,
                        "hardware_colors": hardware_colors,
                        "hardware_layout": self.ui.flex_panels(cx, ids!(page_hardware.content)).snapshot(),
                        "hardware_camera_layout": self.ui.flex_panels(cx, ids!(hardware_teleop.upper.grid)).snapshot(),
                        "control_model": self.ui.widget(cx, ids!(hardware_teleop.control_3d)).borrow::<makepad_urdf_player::robot_view::RobotView>().map(|v| serde_json::json!({"joints":v.joint_angles(),"highlights":v.control_highlights(),"angles":v.view_angles(),"distance":v.view_distance()})),
                        "hardware_joints_done": self.backend.hardware().joints_done(),
                        "nano_base": self.backend.hardware().nano.as_ref().map(|n| serde_json::json!({"active":n.active,"mock":n.mock,"phase":n.phase,"detail":n.detail,"command_ticks":n.command_ticks,"acks":n.acks})),
                        "hardware_joints_total": self.backend.hardware().joints.len(),
                        "hardware_steps": self.backend.hardware().steps.iter().map(|step| format!("{:?}", step.state)).collect::<Vec<_>>(),
                        "scene_objects": play.scene.as_ref().map(|s| s.objects.len()),
                        "viewer_joints": self.ui.robot_view(cx, ids!(page_play.upper.grid.robot_panel.stage_3d)).joint_angles(),
                        "speed": play.speed, "frames": play.stats.frames, "tag": play.selected.and_then(|id| play.episodes.iter().find(|e| e.index == id)).map(|e| format!("{:?}", e.tag)),
                        "monitor_open": self.monitor_open, "monitor_window": self.ui.window(cx, ids!(replay_window)).window_id().map(|w|w.id()),
                        "monitor_layout": self.ui.flex_panels(cx, ids!(monitor_play.upper.grid)).snapshot(),
                        "panel_layout": self.ui.flex_panels(cx, ids!(page_play.upper.grid)).snapshot(),
                        "recording": self.backend.record().recording, "saved": self.backend.record().saved, "discarded": self.backend.record().discarded,
                        "language": self.language.code(), "untranslated": self.localizer.untranslated(),
                        "folder_pending": self.folder_request.is_some(), "status": self.backend.status(), "hub_open": self.hub_upload.is_some(), "robot_animating": self.ui.robot_view(cx, ids!(page_play.stage_3d)).is_animating(), "signals_visible": self.ui.widget(cx, ids!(page_play.timeline.tl_body.lanes)).visible()});
                probe["aisle_panel"] = serde_json::json!(self.plugins.selected());
                probe["aisle"] = serde_json::json!(self.backend.aisle().map(|w|serde_json::json!({"robot":w.robot.name(),"busy":w.busy(),"worker_pid":w.worker_pid(),"status":w.status,"scene_joints":w.scene["joint_count"],"progress":w.progress,"controller":w.plan,"selected":w.selected,"live":w.live,"compliant":w.compliant,"revision":w.revision_index(),"dataset":w.dataset,"result":w.evidence})));
                dorobot_flex::ux_probe::write(cx,&self.ui,probe,
                    &[
                        "page_record.body.right.session",
                        "page_record.body.right.session.ss_body.cues",
                        "page_record.body.left.cams.camera_empty",
                        "page_record.body.left.cams.camera_empty.detail",
                        "page_record.body.left.cams.cam_0.stage",
                        "page_record.body.left.cams.cam_0.stage.waiting",
                        "page_record.review",
                        "page_record.review.rv_body.no_thumbnails",
                        "page_library.left_col.grid.row_a.card_0",
                        "page_play.timeline.tl_body.tl.track_area",
                        "page_play.timeline.tl_body.controls.speed_btn",
                        "page_hardware.content.camera_a_panel.content.calib.joints.joint_0.readout",
                        "main_window.body.workspace.page_header.status",
                        "main_window.body.workspace.page_header.page_title",
                        "main_window.body.workspace.page_header",
                        "page_play.toolbar",
                        "page_play.timeline",
                        "page_play.timeline.tl_head",
                        "workspace_tabs.plugin_tabs.plugin_tab_aisle_task", "workspace_tabs.plugin_tabs.plugin_tab_aisle_scene", "workspace_tabs.plugin_tabs.plugin_tab_aisle_run", "workspace_tabs.plugin_tabs.plugin_tab_aisle_evidence",
                        "aisle_task.nano", "aisle_task.g1", "aisle_task.instruction", "aisle_task.reference", "aisle_task.codex", "aisle_task.octos", "aisle_task.older", "aisle_task.newer", "aisle_task.revise", "aisle_scene.standard", "aisle_scene.compliant", "aisle_scene.inspect", "aisle_run.start_run", "aisle_run.stop_run", "aisle_evidence.previous", "aisle_evidence.next", "aisle_evidence.folder", "aisle_evidence.export", "aisle_evidence.replay",
                        "data_workspace", "plugin_tools", "plugin_tabs", "data_tabs",
                        "workspace_tabs.plugin_tabs.plugin_tab_page_hardware", "workspace_tabs.plugin_tabs.plugin_tab_hardware_teleop", "workspace_tabs.plugin_tabs.plugin_tab_page_record",
                        "workspace_tabs.data_tabs.tab_library", "workspace_tabs.data_tabs.tab_play", "workspace_tabs.data_tabs.tab_eval",
                        "appearance",
                        "language_toggle", "wf_task", "wf_destination",
                        "page_library.left_col.header.page_title", "page_library.left_col.grid.row_a.card_0.body.name",
                        "page_play.upper.side.side_body.task_t", "monitor_play.toolbar.panels_btn",
                        "sidebar",
                        "sidebar_resize",
                        "workflow", "source_row", "wf_rig_form", "wf_capture_fields", "wf_capture", "help_toggle", "wf_more", "wf_source", "capture_notice", "capture_return", "wf_edit", "wf_arm", "wf_nano", "wf_back", "wf_sim", "wf_real", "wf_start", "wf_stop", "wf_operate", "wf_rig", "wf_next", "wf_external", "wf_type", "wf_follower", "wf_leader", "wf_cameras", "wf_create", "wf_task", "wf_destination", "wf_browse", "wf_record", "wf_finish", "wf_save", "wf_discard", "wf_replay", "wf_guide", "wf_detail", "wf_count",
                        "setup_rig",
                        "setup_rescan",
                        "setup_replay",
                        "setup_page",
                        "page_play.timeline.tl_body.controls.frame_lbl",
                        "monitor_play.timeline.tl_body.controls.frame_lbl",
                        "new_window",
                        "close_monitor",
                        "monitor_play.panels_btn",
                        "monitor_play.layout_reset",
                        "monitor_play.play_btn",
                        "monitor_play.signals_btn",
                        "monitor_play.signal_prev",
                        "monitor_play.signal_next",
                        "monitor_play.upper.grid.robot_panel",
                        "monitor_play.upper.grid.error_panel",
                        "monitor_play.timeline.tl_body.controls.transport",
                        "panels_btn",
                        "panel_tools",
                        "robot_toggle",
                        "camera_a_toggle",
                        "camera_b_toggle",
                        "camera_c_toggle",
                        "error_toggle",
                        "layout_reset",
                        "layout_cycle",
                        "page_play.upper.grid",
                        "robot_panel",
                        "camera_a_panel",
                        "camera_b_panel",
                        "camera_c_panel",
                        "error_panel",
                        "stage_3d",
                        "stage_a",
                        "stage_b",
                        "stage_c",
                        "stage_plot",
                        "robot_panel.head",
                        "robot_panel.head.maximize",
                        "robot_panel.head.close",
                        "error_panel.head",
                        "error_panel.head.maximize",
                        "error_panel.head.close",
                        "camera_a_panel.head",
                        "camera_a_panel.head.maximize",
                        "camera_a_panel.head.close",
                        "camera_b_panel.head",
                        "camera_c_panel.head",
                        "camera_b_panel.head.maximize",
                        "camera_c_panel.head.maximize",
                        "camera_b_panel.head.close",
                        "camera_c_panel.head.close",
                        "page_hardware.content.camera_a_panel.content.calib.actions.btn_continue",
                        "page_hardware.content.camera_a_panel.content.calib.actions.btn_restart",
                        "page_hardware.hardware_reset",
                        "page_hardware.hardware_fit",
                        "page_hardware.stepper",
                        "page_hardware.layout_tools",
                        "page_hardware.content",
                        "page_hardware.content.robot_panel",
                        "page_hardware.content.robot_panel.head",
                        "page_hardware.content.robot_panel.head.maximize",
                        "page_hardware.content.robot_panel.head.close",
                        "page_hardware.content.robot_panel.content.mirror.viewport",
                        "page_hardware.content.camera_a_panel",
                        "page_hardware.content.camera_a_panel.head",
                        "page_hardware.content.camera_a_panel.head.maximize",
                        "page_hardware.content.camera_a_panel.head.close",
                        "page_hardware.content.camera_a_panel.content.calib.banner",
                        "page_hardware.content.camera_a_panel.content.calib.banner.ring",
                        "page_hardware.content.camera_a_panel.content.calib.banner.btext.instruction",
                        "page_hardware.content.camera_a_panel.content.calib.banner.btext.subline",
                        "page_hardware.content.camera_a_panel.content.calib.rows",
                        "page_hardware.content.camera_a_panel.content.calib.rows.joint_0.readout",
                        "page_hardware.content.camera_a_panel.content.calib.rows.joint_5.readout",
                        "page_hardware.stepper.step_0.dot",
                        "page_hardware.stepper.step_1.dot",
                        "page_hardware.stepper.step_2.dot",
                        "page_hardware.stepper.step_3.dot",
                        "page_hardware.stepper.step_4.dot",
                        "page_hardware.stepper.step_0.dot.num",
                        "page_hardware.stepper.step_1.dot.num",
                        "page_hardware.stepper.step_2.dot.num",
                        "page_hardware.stepper.step_3.dot.num",
                        "page_hardware.stepper.step_4.dot.num",
                        "page_eval.pbar.btn_stop",
                        "page_record.transport.btn_record",
                        "page_record.transport.btn_save",
                        "page_record.transport.btn_discard",
                        "page_record.transport.btn_rerecord",
                        "page_play.upper.side.side_body.btn_del",
                        "page_play.upper.side.side_body.btn_push",
                        "episodes_btn",
                        "details_btn",
                        "signals_btn",
                        "view_mode_btn",
                        "monitor_play.view_mode_btn",
                        "signal_prev",
                        "signal_next",
                        "page_play.timeline.tl_body.lanes.content.lane_0",
                        "page_play.timeline.tl_body.lanes.content.lane_5",
                        "page_play.upper.tree.tree_body.row_0",
                        "page_play.upper.tree.tree_body.row_1",
                        "page_play.upper.tree.tree_body.row_2",
                        "page_play.upper.side.side_body.btn_good",
                        "page_play.upper.side.side_body.btn_bad",
                        "page_play.timeline.tl_body.controls.transport",
                        "speed_btn",
                        "monitor_play.speed_btn",
                        "play_btn",
                        "step_back_btn",
                        "step_fwd_btn",
                        "page_play.timeline.tl_body.tl",
                        "page_library.left_col.header.btn_pull",
                        "page_library.left_col.header.btn_open",
                        "hub_repo",
                        "live_bar",
                        "live_source",
                        "live_connect",
                        "live_disconnect",
                        "live_recording",
                        "gamepad_help",
                        "gamepad_mapping",
                        "gamepad_setup",
                        "page_hardware.supported_robots.nano_select",
                        "page_hardware.supported_robots.simulation_select",
                        "page_hardware.supported_robots.rig_select",
                        "page_hardware.content.camera_a_panel.content.calib.nano_controls",
                        "page_hardware.content.camera_a_panel.content.calib.nano_controls.driver_state",
                        "page_hardware.content.camera_a_panel.content.calib.nano_controls.serial_row.port",
                        "page_hardware.content.camera_a_panel.content.calib.nano_controls.serial_row.scan",
                        "page_hardware.content.camera_a_panel.content.calib.nano_controls.connection_row.connect",
                        "page_hardware.content.camera_a_panel.content.calib.nano_controls.connection_row.test",
                        "page_hardware.content.camera_a_panel.content.calib.nano_controls.connection_row.stop",
                        "hardware_teleop",
                        "hardware_teleop.upper.grid.robot_panel",
                        "hardware_teleop.upper.grid.camera_a_panel",
                        "hardware_teleop.upper.grid.camera_b_panel",
                        "hardware_teleop.upper.grid.camera_c_panel",
                        "hardware_teleop.toolbar.signals_btn",
                        "hardware_teleop.toolbar.panels_btn",
                        "hardware_teleop.toolbar.camera_hint",
                        "hardware_teleop.panel_tools.layout_reset",
                        "hardware_teleop.panel_tools.layout_cycle",
                        "hardware_teleop.upper.grid.camera_a_panel.head.maximize",
                        "hardware_teleop.upper.grid.camera_b_panel.head.maximize",
                        "hardware_teleop.upper.grid.camera_c_panel.head.maximize",
                        "hardware_teleop.upper.grid.robot_panel.head.maximize",
                        "hardware_teleop.upper.grid.error_panel",
                        "hardware_teleop.upper.grid.error_panel.head.maximize",
                        "hardware_teleop.upper.grid.error_panel.control_body.control_3d",
                        "hardware_teleop.upper.grid.error_panel.control_body.control_feedback",
                        "hardware_teleop.upper.grid.error_panel.control_body.control_angles",
                        "hardware_teleop.panel_tools.error_toggle",
                        "hub_cancel",
                        "page_play.upper.side",
                        "page_play.upper.tree",
                    ],
                );
            }
            self.ui
                .label(cx, ids!(main_window.body.workspace.page_header.status))
                .set_text(
                    cx,
                    self.display_status(),
                );
            match self.backend.screen() {
                Screen::Library => self
                    .ui
                    .library_screen(cx, ids!(page_library))
                    .sync(cx, self.backend.library()),
                _ => {}
            }
            self.sync_plugins(cx);
        }
        if !self.backend.playback().is_playing {
            return;
        }
        let st = self.backend.playback();
        let next = st.current_time + st.speed * dt;
        // Stop at the tail rather than wrapping: the last frame is a result
        // worth looking at.
        if next >= st.stats.duration_s {
            self.backend.dispatch(Intent::Seek(st.stats.duration_s));
            self.backend.dispatch(Intent::TogglePlay);
            self.sync(cx);
        } else {
            self.backend.dispatch(Intent::Seek(next));
            // Only the playhead moved, so only the views it drives are
            // touched; a full sync here does not fit in a 60Hz frame.
            self.ui
                .play_screen(cx, ids!(page_play))
                .tick(cx, self.backend.playback());
            if self.monitor_open {
                self.ui
                    .play_screen(cx, ids!(monitor_play))
                    .tick(cx, self.backend.playback());
            }
        }
        self.ui.redraw(cx);
    }

    fn handle_key_down(&mut self, cx: &mut Cx, e: &KeyEvent) {
        if e.modifiers.logo || e.modifiers.control {
            let target = match e.key_code {
                KeyCode::Key1 => Some(Screen::Hardware),
                KeyCode::Key2 => Some(Screen::Live),
                KeyCode::Key3 => Some(Screen::Record),
                KeyCode::Key4 => Some(Screen::Play),
                KeyCode::Key5 => Some(Screen::Library),
                KeyCode::Key6 => Some(Screen::Eval),
                _ => None,
            };
            if let Some(target) = target {
                self.navigate(target);
                self.sync(cx);
                return;
            }
        }
        if e.key_code == KeyCode::Escape && self.hub_upload.is_some() {
            self.hub_upload = None;
            self.sync(cx);
            return;
        }
        if self.hub_upload.is_some() {
            return;
        }

        if self.plugins.selected().is_some() { return; }
        if e.key_code == KeyCode::Space && focused_control(&self.ui, cx) {
            return;
        }
        let intent = if self.backend.screen() == Screen::Play {
            match e.key_code {
                KeyCode::Space => Some(Intent::TogglePlay),
                KeyCode::ArrowLeft => Some(Intent::StepFrames(-1)),
                KeyCode::ArrowRight => Some(Intent::StepFrames(1)),
                KeyCode::Home => Some(Intent::Seek(0.0)),
                KeyCode::End => Some(Intent::Seek(self.backend.playback().stats.duration_s)),
                KeyCode::KeyZ if e.modifiers.control || e.modifiers.logo => {
                    Some(Intent::UndoExclude)
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some(intent) = intent {
            self.backend.dispatch(intent);
            self.sync(cx);
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let previous_screen = self.backend.screen();
        let previous_job = self.backend.nexus().and_then(|state| state.job_dir.clone());
        let opening_saved = (self.plugins.selected() == Some("nexus_setup")
            && self.ui.button(cx, ids!(nexus_setup.open_saved)).clicked(actions))
            || (self.plugins.selected() == Some("aisle_evidence") && self.ui.button(cx, ids!(aisle_evidence.replay)).clicked(actions));
        let mut dirty = self.plugins.handle_actions(cx, actions, &mut *self.backend);
        if dirty {
            if self.backend.screen() != previous_screen || (opening_saved && self.backend.screen() == Screen::Play) {
                self.plugins.follow_navigation();
            }
            if let Some(state) = self.backend.nexus() {
                if state.job_dir != previous_job {
                    match state.kind {
                        Some(NexusKind::Capture) => self.plugins.follow_navigation(),
                        Some(NexusKind::Rollout) => {
                            if let Err(error) = self.plugins.select("nexus_rollout") { ::log::warn!("{error}"); }
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some(panel) = self.plugins.toolbox_selection(cx, actions).or_else(|| self.plugins.tab_selection(actions)) {
            self.open_panel(panel);
            dirty = true;
        }
        if self.ui.button(cx,ids!(help_toggle)).clicked(actions) { self.workflow_help = !self.workflow_help; dirty = true; }
        if self.ui.button(cx,ids!(wf_more)).clicked(actions) { self.workflow_more = !self.workflow_more; dirty = true; }
        if self.ui.text_input(cx, ids!(wf_task)).changed(actions).is_some() || self.ui.text_input(cx, ids!(wf_destination)).changed(actions).is_some() {
            self.backend.dispatch(Intent::SetCapturePreferences { task: self.ui.text_input(cx, ids!(wf_task)).text(), destination: self.ui.text_input(cx, ids!(wf_destination)).text() });
            dirty = true;
        }
        for (path, intent) in [
            (ids!(capture_return), Intent::Navigate(Screen::Record)),
            (ids!(wf_arm), Intent::SelectNanoBase(false)), (ids!(wf_nano), Intent::SelectNanoBase(true)),
            (ids!(wf_sim), Intent::SelectSimulation(true)), (ids!(wf_real), Intent::SelectSimulation(false)),
            (ids!(wf_start), Intent::StartSimulation), (ids!(wf_stop), Intent::StopSimulation),
            (ids!(wf_back), Intent::Navigate(Screen::Hardware)), (ids!(wf_operate), Intent::Navigate(Screen::Live)), (ids!(wf_next), Intent::Navigate(Screen::Record)),
            (ids!(wf_replay), Intent::OpenSavedRecording),
        ] { if self.ui.button(cx,path).clicked(actions) {
            if matches!(intent, Intent::Navigate(_)) { self.plugins.follow_navigation(); }
            if matches!(intent, Intent::StartSimulation | Intent::SelectSimulation(_) | Intent::SelectNanoBase(_)) {
                self.external_stream = false;
            }
            self.backend.dispatch(intent); dirty = true;
        } }
        for (path, action) in [(ids!(wf_record), "record"), (ids!(wf_finish), "stop"), (ids!(wf_save), "save"), (ids!(wf_discard), "discard")] {
            if self.ui.button(cx,path).clicked(actions) {
                self.backend.dispatch(Intent::SimulationCapture { action: action.into(), task: self.ui.text_input(cx,ids!(wf_task)).text(), destination: self.ui.text_input(cx,ids!(wf_destination)).text() }); dirty = true;
            }
        }
        if self.ui.button(cx,ids!(wf_external)).clicked(actions) { self.external_stream = true; self.navigate(Screen::Live); dirty = true; }
        if self.ui.button(cx,ids!(wf_rig)).clicked(actions) {
            cx.open_select_file_dialog(FileDialog::new().set_id(live_id!(rig_profile)).set_title(i18n::text(self.language, "Choose a rig profile")).add_filter(i18n::text(self.language, "Rig profile"),vec!["json".into()]));
        }
        if self.ui.button(cx,ids!(wf_browse)).clicked(actions) {
            self.folder_request = Some(FolderRequest::Recording);
            cx.open_select_folder_dialog(FileDialog::new().set_id(live_id!(dataset_folder)).set_title(i18n::text(self.language, "Choose recording folder")));
        }
        if self.ui.button(cx,ids!(wf_edit)).clicked(actions) { self.rig_editing = !self.rig_editing; dirty = true; }
        if self.ui.button(cx,ids!(wf_create)).clicked(actions) {
            let previous = self.backend.rig_profile();
            self.backend.dispatch(Intent::CreateRig { robot_type:self.ui.text_input(cx,ids!(wf_type)).text(), follower:self.ui.text_input(cx,ids!(wf_follower)).text(), leader:self.ui.text_input(cx,ids!(wf_leader)).text(), cameras:self.ui.text_input(cx,ids!(wf_cameras)).text(), task:self.ui.text_input(cx,ids!(wf_task)).text(), destination:self.ui.text_input(cx,ids!(wf_destination)).text() });
            if self.backend.rig_profile() != previous { self.rig_editing = false; }
            dirty = true;
        }

        if self.ui.button(cx, ids!(gamepad_setup)).clicked(actions) {
            self.navigate(if self.backend.screen() == Screen::Record { Screen::Hardware } else { Screen::Record });
            dirty = true;
        }
        if self.ui.button(cx, ids!(live_connect)).clicked(actions) {
            let source = self.ui.text_input(cx, ids!(live_source)).text();
            self.backend.dispatch(Intent::ConnectLive(source.trim().into()));
            dirty = true;
        }
        if self.ui.button(cx, ids!(live_disconnect)).clicked(actions) {
            self.backend.dispatch(Intent::DisconnectLive);
            dirty = true;
        }
        if self.ui.button(cx, ids!(live_recording)).clicked(actions) {
            self.backend.dispatch(Intent::OpenLiveRecording);
            dirty = true;
        }
        // The HAL opens Cocoa panels on its main thread and returns asynchronously.
        // Keep the original operation until either selection or cancellation arrives.
        for action in actions {
            if let Some(dialog) = action.downcast_ref::<FileDialogAction>() {
                match dialog {
                    FileDialogAction::FileSelected { id, paths }
                        if *id == live_id!(rig_profile) =>
                    {
                        if let Some(path) = paths.first() {
                            self.backend.dispatch(Intent::ConfigureRig(path.clone()));
                            self.backend.dispatch(Intent::NewRecordingSession);
                            dirty = true;
                        }
                    }
                    FileDialogAction::FolderSelected(path) => {
                        match self.folder_request.take() {
                            Some(FolderRequest::Recording) => { self.ui.text_input(cx,ids!(wf_destination)).set_text(cx, &path.to_string_lossy()); self.backend.dispatch(Intent::SetCapturePreferences { task:self.ui.text_input(cx,ids!(wf_task)).text(), destination:path.to_string_lossy().into_owned() }); },
                            Some(FolderRequest::Dataset) => {
                                self.backend.dispatch(Intent::OpenFolder(path.clone()))
                            }
                            Some(FolderRequest::Download(repo)) => {
                                let name = repo.split('/').last().unwrap_or("dataset");
                                if !name.is_empty()
                                    && name != "."
                                    && name != ".."
                                    && !name.contains('\\')
                                {
                                    let destination = Some(path.join(name));
                                    self.backend
                                        .dispatch(Intent::HubTransfer { repo, destination });
                                    self.hub_upload = None;
                                }
                            }
                            None => {}
                        }
                        dirty = true;
                    }
                    FileDialogAction::FolderCancelled => {
                        self.folder_request = None;
                    }
                    _ => {}
                }
            }
        }
        if self
            .ui
            .button(cx, ids!(page_play.new_window))
            .clicked(actions) || self.ui.button(cx, ids!(hardware_teleop.new_window)).clicked(actions)
        {
            self.monitor_open = true;
            self.ui.window(cx, ids!(replay_window)).show(cx);
            dirty = true;
        }
        if self.ui.button(cx, ids!(close_monitor)).clicked(actions) {
            self.hide_monitor(cx);
        }
        for page in [ids!(page_play) as &[LiveId], ids!(monitor_play), ids!(hardware_teleop)] {
            let root = self.ui.widget(cx, page);
            let play = self.ui.play_screen(cx, page);
            let grid = root.flex_panels(cx, ids!(upper.grid));
            if root.button(cx, ids!(view_mode_btn)).clicked(actions) {
                play.toggle_room_view(cx, self.backend.playback());
                self.ui.redraw(cx);
            }
            if root.button(cx, ids!(panels_btn)).clicked(actions) {
                let tools = root.widget(cx, ids!(panel_tools));
                tools.set_visible(cx, !tools.visible());
                self.ui.redraw(cx);
            }
            for (id, panel) in [
                (ids!(robot_toggle), 0),
                (ids!(camera_a_toggle), 1),
                (ids!(camera_b_toggle), 2),
                (ids!(error_toggle), 3),
                (ids!(camera_c_toggle), 4),
            ] {
                if root.button(cx, id).clicked(actions) {
                    grid.toggle_panel(cx, panel);
                }
            }
            if root.button(cx, ids!(layout_reset)).clicked(actions) {
                grid.reset(cx);
            }
            if root.button(cx, ids!(speed_btn)).clicked(actions) {
                let speed = self.backend.playback().speed;
                let next = if speed < 0.5 {
                    0.5
                } else if speed < 1.0 {
                    1.0
                } else if speed < 2.0 {
                    2.0
                } else {
                    0.25
                };
                self.backend.dispatch(Intent::SetSpeed(next));
                dirty = true;
            }
            if root.button(cx, ids!(layout_cycle)).clicked(actions) {
                grid.cycle_layout(cx);
            }
            for (id, step) in [(ids!(signal_prev), -1), (ids!(signal_next), 1)] {
                if root.button(cx, id).clicked(actions) {
                    play.shift_signals(cx, step);
                }
            }
            for (id, pane) in [
                (ids!(episodes_btn), "episodes"),
                (ids!(details_btn), "details"),
                (ids!(signals_btn), "signals"),
            ] {
                if root.button(cx, id).clicked(actions) {
                    play.toggle_pane(cx, pane);
                }
            }
        }

        if self.ui.button(cx, ids!(setup_replay)).clicked(actions) {
            let target = if self.backend.screen() == Screen::Play {
                Screen::Library
            } else {
                Screen::Play
            };
            self.navigate(target);
            dirty = true;
        }
        if self.ui.button(cx, ids!(setup_rescan)).clicked(actions) {
            self.backend.dispatch(Intent::WizardRestartStep);
            dirty = true;
        }
        if self.ui.button(cx, ids!(data_workspace)).clicked(actions) {
            self.navigate(self.data_screen);
            dirty = true;
        }
        let data_tabs = self.ui.widget(cx, ids!(workspace_tabs.data_tabs));
        if data_tabs.visible() && data_tabs.button(cx, ids!(use_training)).clicked(actions) {
            self.backend.dispatch(Intent::Nexus(NexusIntent::UseOpenDataset));
            if let Err(error) = self.plugins.select("nexus_train") { ::log::warn!("{error}"); }
            dirty = true;
        }
        for (path, screen) in [(ids!(tab_library) as &[LiveId], Screen::Library), (ids!(tab_play), Screen::Play), (ids!(tab_eval), Screen::Eval)] {
            if data_tabs.visible() && data_tabs.button(cx, path).clicked(actions) {
                self.navigate(screen);
                dirty = true;
            }
        }

        if self.ui.button(cx, ids!(language_toggle)).clicked(actions) {
            self.language = match self.language {
                Language::English => Language::Chinese,
                Language::Chinese => Language::English,
            };
            dirty = true;
        }
        if self.ui.button(cx, ids!(appearance)).clicked(actions) {
            frame::toggle_light_mode();
            dirty = true;
        }

        if self
            .ui
            .button(cx, ids!(page_library.left_col.header.btn_new))
            .clicked(actions)
            || self.ui.button(cx, ids!(setup_rig)).clicked(actions)
        {
            self.navigate(Screen::Hardware); dirty = true;
        }
        if self
            .ui
            .button(cx, ids!(page_library.left_col.header.btn_pull))
            .clicked(actions)
        {
            self.hub_upload = Some(false);
            dirty = true;
        }

        if self
            .ui
            .button(cx, ids!(page_library.left_col.header.btn_open))
            .clicked(actions)
        {
            if self.folder_request.is_none() {
                self.folder_request = Some(FolderRequest::Dataset);
                cx.open_select_folder_dialog(
                    FileDialog::new().set_title(i18n::text(self.language, "Open LeRobot dataset or dataset library")),
                );
            }
        }
        if self.ui.button(cx, ids!(page_eval.pbar.btn_stop)).clicked(actions) {
            self.backend.dispatch(Intent::StopRollout);
            dirty = true;
        }

        if self
            .ui
            .button(cx, ids!(hub_form.hub_cancel))
            .clicked(actions)
        {
            self.hub_upload = None;
            dirty = true;
        }
        if self.ui.button(cx, ids!(hub_form.hub_go)).clicked(actions) {
            let repo = self
                .ui
                .widget(cx, ids!(hub_form.hub_repo))
                .text()
                .trim()
                .to_string();
            if !repo.is_empty() {
                if self.hub_upload == Some(true) {
                    self.backend.dispatch(Intent::HubTransfer {
                        repo,
                        destination: None,
                    });
                    self.hub_upload = None;
                } else if self.folder_request.is_none() {
                    self.folder_request = Some(FolderRequest::Download(repo));
                    cx.open_select_folder_dialog(
                        FileDialog::new().set_title(i18n::text(self.language, "Choose download parent folder")),
                    );
                }
                dirty = true;
            }
        }

        // Library: open the clicked dataset.
        let lib = self.ui.library_screen(cx, ids!(page_library));
        if lib.clicked_setup(cx, actions) {
            self.navigate(Screen::Hardware);
            dirty = true;
        }
        if let Some(id) = lib.clicked_dataset(cx, actions, self.backend.library()) {
            self.backend.dispatch(Intent::OpenDataset(id));
            dirty = true;
        }

        // Transport: the Timeline widget owns scrub, play/pause and step, and
        // reports them as actions rather than mutating anything itself.
        for a in actions {
            if let Some(a) = a.as_widget_action() {
                match a.cast::<PlaybackAction>() {
                    PlaybackAction::Play | PlaybackAction::Pause => {
                        self.backend.dispatch(Intent::TogglePlay);
                        dirty = true;
                    }
                    PlaybackAction::StepForward => {
                        self.backend.dispatch(Intent::StepFrames(1));
                        dirty = true;
                    }
                    PlaybackAction::StepBackward => {
                        self.backend.dispatch(Intent::StepFrames(-1));
                        dirty = true;
                    }
                    PlaybackAction::None => {}
                }
                match a.cast::<TimelineAction>() {
                    TimelineAction::Play | TimelineAction::Pause => {
                        self.backend.dispatch(Intent::TogglePlay);
                        dirty = true;
                    }
                    TimelineAction::Seek(t) => {
                        self.backend.dispatch(Intent::Seek(t));
                        dirty = true;
                    }
                    TimelineAction::StepForward => {
                        self.backend.dispatch(Intent::StepFrames(1));
                        dirty = true;
                    }
                    TimelineAction::StepBackward => {
                        self.backend.dispatch(Intent::StepFrames(-1));
                        dirty = true;
                    }
                    TimelineAction::SpeedChanged(v) => {
                        self.backend.dispatch(Intent::SetSpeed(v));
                        dirty = true;
                    }
                    TimelineAction::ScrubEnd | TimelineAction::None => {}
                }
                // Dragging the plot cursor scrubs too, so the trace and the
                // video stay one control.
                if let TimeSeriesPlotAction::CursorMoved(t) = a.cast::<TimeSeriesPlotAction>() {
                    self.backend.dispatch(Intent::Seek(t));
                    dirty = true;
                }
            }
        }

        for page in [ids!(page_play) as &[LiveId], ids!(monitor_play), ids!(hardware_teleop)] {
            // Play: episode selection and curation.
            let play = self.ui.play_screen(cx, page);
            if let Some(ep) = play.clicked_episode(cx, actions) {
                self.backend.dispatch(Intent::SelectEpisode(ep));
                dirty = true;
            }
            if let Some(sel) = self.backend.playback().selected {
                if let Some(intent) = play.curation_intent(cx, actions, sel) {
                    if matches!(intent, Intent::PushToHub) {
                        self.hub_upload = Some(true);
                    } else {
                        self.backend.dispatch(intent);
                    }
                    dirty = true;
                }
            }
        }

        if dirty {
            self.sync(cx);
            self.ui.redraw(cx);
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        dorobot_studio_core::register_ui(vm);
        dorobot_flex::plugins::builtin_plugins().register_widgets(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if matches!(event, Event::Shutdown | Event::WindowClosed(_)) {
            self.save_session();
        }
        match event {
            Event::MouseDown(e)
                if Some(e.window_id) == self.ui.window(cx, ids!(main_window)).window_id() =>
            {
                self.resizing_sidebar = self
                    .ui
                    .widget(cx, ids!(sidebar_resize))
                    .area()
                    .rect(cx)
                    .contains(e.abs);
            }
            Event::MouseMove(e) if self.resizing_sidebar => {
                self.sidebar_width = e.abs.x.clamp(176.0, 320.0);
                if let Some(mut sidebar) = self.ui.widget(cx, ids!(sidebar)).borrow_mut::<View>() {
                    sidebar.walk.width = Size::Fixed(self.sidebar_width);
                }
                self.ui.redraw(cx);
            }
            Event::MouseUp(_) if self.resizing_sidebar => {
                self.resizing_sidebar = false;
                self.save_session();
            }
            _ => {}
        }
        if let Event::WindowCloseRequested(e) = event {
            if Some(e.window_id) == self.ui.window(cx, ids!(replay_window)).window_id() {
                e.accept_close.set(false);
                self.hide_monitor(cx);
            }
        }
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        if let Event::MouseMove(e) = event {
            let anchor = self.ui.widget(cx, ids!(language_toggle)).area().rect(cx);
            let mut hint = self.ui.tooltip(cx, ids!(language_hint));
            if Some(e.window_id) == self.ui.window(cx, ids!(main_window)).window_id() && anchor.contains(e.abs) {
                hint.set_text(cx, match self.language {
                    Language::English => "Switch to 简体中文",
                    Language::Chinese => "切换为 English",
                });
                hint.show_anchored(cx, makepad_widgets::tooltip::TooltipAnchor {
                    rect: anchor, side: makepad_widgets::tooltip::TooltipPosition::Top,
                    gap: 6.0, natural_width: None,
                });
            } else {
                hint.hide(cx);
            }
        }
        if matches!(event, Event::LiveEdit | Event::ScriptReapply) && self.started {
            self.plugins.reapply(cx);
            let pages = self.ui.widget(cx, ids!(pages));
            let toolbox = self.ui.widget(cx, ids!(plugin_tools));
            let tabs = self.ui.widget(cx, ids!(workspace_tabs.plugin_tabs));
            if let Err(error) = self.plugins.mount(cx, &pages)
                .and_then(|_| self.plugins.mount_toolbox(cx, &toolbox))
                .and_then(|_| self.plugins.mount_tabs(cx, &tabs)) {
                ::log::error!("插件重载失败：{error}");
            }
            // Refresh material-only edits without reopening meshes or losing
            // the recorded pose. All Nano views use the same URDF palette.
            for (path, model) in [
                (ids!(page_hardware.content.robot_panel.content.mirror.viewport) as &[LiveId], self.backend.hardware().robot_urdf.as_ref()),
                (ids!(page_play.upper.grid.robot_panel.stage_3d), self.backend.playback().robot_urdf.as_ref()),
                (ids!(hardware_teleop.upper.grid.robot_panel.stage_3d), self.backend.playback().robot_urdf.as_ref()),
                (ids!(hardware_teleop.upper.grid.error_panel.control_body.control_3d), self.backend.playback().robot_urdf.as_ref()),
                (ids!(monitor_play.upper.grid.robot_panel.stage_3d), self.backend.playback().robot_urdf.as_ref()),
            ] {
                if let Some((urdf, _)) = model {
                    let widget = self.ui.widget(cx, path);
                    if let Some(mut viewer) = widget.borrow_mut::<makepad_urdf_player::robot_view::RobotView>() {
                        if let Err(error) = viewer.reload_materials(cx, urdf) {
                            ::log::warn!("Could not refresh robot materials: {error}");
                        }
                    };
                }
            }
            self.ui
                .play_screen(cx, ids!(page_play))
                .restore_panel_design(cx);
            self.ui.play_screen(cx, ids!(hardware_teleop)).restore_panel_design(cx);
            self.ui.hardware_screen(cx, ids!(page_hardware)).restore_view_distance(cx);
            self.sync(cx);
        }
        // Scrolling the episode window produces no action, so it is picked up
        // here rather than in handle_actions, which would not run for it.
        let play_scrolled = self.ui.play_screen(cx, ids!(page_play)).take_scrolled();
        let monitor_scrolled = self.ui.play_screen(cx, ids!(monitor_play)).take_scrolled();
        let hardware_scrolled = self.ui.play_screen(cx, ids!(hardware_teleop)).take_scrolled();
        if play_scrolled || monitor_scrolled || hardware_scrolled {
            self.sync(cx);
            self.ui.redraw(cx);
        }
        if self.started && matches!(event, Event::Timer(_)) { self.localize(cx); }
    }
}
