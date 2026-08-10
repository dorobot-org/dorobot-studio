//! Design-validation harness for the LeRobot UX.
//!
//! Renders the new screens against [`MockBackend`] so they can be diffed
//! against `docs/ux/*.png` by `tools/vqa/`. Not the shipping app — the player
//! still lives in the `dorobot-flex` binary until each screen is signed off.
//!
//! `--screen <library|record|play|hardware|eval>` opens directly on a screen,
//! which is what the visual-diff runner uses.

use dorobot_flex::api::{files::FileBackend, mock::MockBackend, Backend, Intent, Screen};
use dorobot_flex::widgets::time_series_plot::TimeSeriesPlotAction;
use dorobot_flex::widgets::timeline::TimelineAction;
use dorobot_flex::playback_controls::PlaybackAction;
use dorobot_flex::ui::{frame, hardware::HardwareScreenWidgetRefExt, library::LibraryScreenWidgetRefExt, record::RecordScreenWidgetRefExt, play::PlayScreenWidgetRefExt, eval::EvalScreenWidgetRefExt};
use makepad_widgets::*;

/// Transport tick. Matches the shipping player, and is independent of the
/// dataset's own fps: it advances a clock, and every view resolves itself
/// against that clock rather than counting frames of its own.
const PLAYBACK_TIMER_FPS: f64 = 60.0;

app_main!(App);

/// `--dataset <dir>` scans that directory for LeRobot datasets; without it the
/// app runs on the design fixtures.
fn make_backend() -> Box<dyn Backend> {
    let args: Vec<String> = std::env::args().collect();
    match args.iter().position(|a| a == "--dataset") {
        Some(i) => match args.get(i + 1) {
            Some(root) => {
                let prefer = args
                    .iter()
                    .position(|a| a == "--open")
                    .and_then(|j| args.get(j + 1))
                    .map(String::as_str);
                Box::new(FileBackend::with_open(root, prefer))
            }
            None => Box::new(MockBackend::new()),
        },
        None => Box::new(MockBackend::new()),
    }
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.ux.*

    startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.title: "DoRobot Studio"
                window.inner_size: vec2(1536, 1024)
                pass.clear_color: #x12151C

                body +: {
                    width: Fill
                    height: Fill
                    flow: Down

                    app_bar := mod.widgets.ux.AppBar{}

                    work := View{
                        width: Fill
                        height: Fill
                        flow: Right

                        nav := mod.widgets.ux.NavRail{}

                        pages := View{
                            width: Fill
                            height: Fill
                            flow: Overlay
                            page_library := LibraryScreen{}
                            page_hardware := HardwareScreen{ visible: false }
                            page_record := RecordScreen{ visible: false }
                            page_play := PlayScreen{ visible: false }
                            page_eval := EvalScreen{ visible: false }
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
    /// Real datasets when a root is supplied, mock fixtures otherwise.
    #[rust(make_backend())]
    backend: Box<dyn Backend>,
    #[rust(false)]
    started: bool,
    #[rust]
    playback_timer: Timer,
}

impl App {
    /// Repaint every screen from the current backend snapshot.
    fn sync(&mut self, cx: &mut Cx) {
        let light = frame::light_mode();
        frame::theme_chrome(cx, &self.ui, light);

        let nav = self.ui.widget(cx, ids!(nav));
        frame::sync_nav(cx, &nav, self.backend.screen());

        self.ui
            .library_screen(cx, ids!(page_library))
            .sync(cx, self.backend.library());
        self.ui
            .hardware_screen(cx, ids!(page_hardware))
            .sync(cx, self.backend.hardware());
        self.ui
            .record_screen(cx, ids!(page_record))
            .sync(cx, self.backend.record());
        self.ui
            .play_screen(cx, ids!(page_play))
            .sync(cx, self.backend.playback());
        self.ui
            .eval_screen(cx, ids!(page_eval))
            .sync(cx, self.backend.eval());

        // Overlay flow: exactly one page is visible at a time.
        let screen = self.backend.screen();
        for (path, s) in [
            (ids!(page_library) as &[LiveId], Screen::Library),
            (ids!(page_hardware), Screen::Hardware),
            (ids!(page_record), Screen::Record),
            (ids!(page_play), Screen::Play),
            (ids!(page_eval), Screen::Eval),
        ] {
            self.ui.widget(cx, path).set_visible(cx, s == screen);
        }
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // Optional deep-link so the diff runner can shoot one screen per launch.
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--screen") {
            if let Some(name) = args.get(i + 1) {
                let target = match name.as_str() {
                    "record" => Some(Screen::Record),
                    "play" => Some(Screen::Play),
                    "hardware" => Some(Screen::Hardware),
                    "eval" => Some(Screen::Eval),
                    "library" => Some(Screen::Library),
                    _ => None,
                };
                if let Some(t) = target {
                    self.backend.dispatch(Intent::Navigate(t));
                }
            }
        }
        // --light lets the visual-diff runner shoot both themes.
        if args.iter().any(|a| a == "--light") {
            frame::set_light_mode(1.0);
        }
        self.sync(cx);
        self.started = true;
        self.playback_timer = cx.start_interval(1.0 / PLAYBACK_TIMER_FPS);
    }

    fn handle_timer(&mut self, cx: &mut Cx, event: &TimerEvent) {
        if self.playback_timer.is_timer(event).is_none() {
            return;
        }
        if !self.backend.playback().is_playing {
            return;
        }
        let st = self.backend.playback();
        let next = st.current_time + st.speed / PLAYBACK_TIMER_FPS;
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
        }
        self.ui.redraw(cx);
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let mut dirty = false;

        let nav = self.ui.widget(cx, ids!(nav));
        for (path, screen) in frame::NAV_ITEMS {
            let item = nav.widget(cx, path);
            if item.is_empty() {
                continue;
            }
            if frame::view_clicked(actions, item.widget_uid()) {
                self.backend.dispatch(Intent::Navigate(screen));
                dirty = true;
            }
        }

        // Gear in the app bar toggles dark/light.
        let gear = self.ui.widget(cx, ids!(app_bar.actions.gear));
        if !gear.is_empty() {
            if frame::view_clicked(actions, gear.widget_uid()) {
                frame::toggle_light_mode();
                dirty = true;
            }
        }

        if self
            .ui
            .button(cx, ids!(page_library.left_col.header.btn_new))
            .clicked(actions)
        {
            self.backend.dispatch(Intent::NewRecordingSession);
            dirty = true;
        }
        if self
            .ui
            .button(cx, ids!(page_library.left_col.header.btn_pull))
            .clicked(actions)
        {
            self.backend.dispatch(Intent::PullFromHub);
            dirty = true;
        }

        // Library: open the clicked dataset.
        let lib = self.ui.library_screen(cx, ids!(page_library));
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

        // Play: episode selection and curation.
        let play = self.ui.play_screen(cx, ids!(page_play));
        if let Some(ep) = play.clicked_episode(cx, actions) {
            self.backend.dispatch(Intent::SelectEpisode(ep));
            dirty = true;
        }
        if let Some(sel) = self.backend.playback().selected {
            if let Some(intent) = play.curation_intent(cx, actions, sel) {
                self.backend.dispatch(intent);
                dirty = true;
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
        makepad_widgets::script_mod(vm);
        // RobotView, and the XR scene it draws into, back the Play 3D pane
        makepad_urdf_player::makepad_xr::script_mod(vm);
        makepad_urdf_player::script_mod(vm);
        // app-shell supplies the real draggable PanelGrid used by the Play screen
        dorobot_flex::makepad_app_shell::script_mod(vm);
        // TimeSeriesPlot lives in widgets and is used by the Play screen's plot
        // pane, so both must register before ui::script_mod evaluates that DSL.
        // shared goes first: it defines the mod.widgets.flex text styles the
        // widgets module builds on.
        dorobot_flex::shared::script_mod(vm);
        dorobot_flex::widgets::script_mod(vm);
        dorobot_flex::playback_controls::script_mod(vm);
        dorobot_flex::ui::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        // Scrolling the episode window produces no action, so it is picked up
        // here rather than in handle_actions, which would not run for it.
        if self.ui.play_screen(cx, ids!(page_play)).take_scrolled() {
            self.sync(cx);
            self.ui.redraw(cx);
        }
    }
}
