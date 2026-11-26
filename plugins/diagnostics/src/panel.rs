//! Minimal run-diagnostics panel.
//!
//! Skeleton scope (D-03.3): list `runs/studio-*`, show the tail of the selected
//! `dora.log`, filter by level/keyword/source, raise the in-memory read limit
//! with "Load more", export the filtered view, copy the whole log with
//! "Export full log", and hand a recorded dataset to the existing Replay
//! screen.
//!
//! The run list uses ten fixed row slots instead of a virtualised list. That is
//! deliberate for the skeleton: it keeps the Makepad script small and avoids
//! mutating child widgets from `sync`. A real list widget can replace the slots
//! without changing `runs.rs`.

use std::path::PathBuf;

use dorobot_studio_core::{
    api::{Backend, Intent},
    runtime,
    ui::frame::{self, Themed},
};
use dorobot_studio_plugin::*;

use crate::runs::{self, LogFilter, LogLevel, LogLine, RunInfo};

const RUN_SLOTS: usize = 10;
/// A Label with 5 000 lines is not a usable viewer. The full filtered set is
/// still what export writes; only the on-screen tail is capped.
const DISPLAY_LINES: usize = 400;
const MAX_LINE_CHARS: usize = 600;

fn run_slot_ids() -> [&'static [LiveId]; RUN_SLOTS] {
    [
        ids!(run_0),
        ids!(run_1),
        ids!(run_2),
        ids!(run_3),
        ids!(run_4),
        ids!(run_5),
        ids!(run_6),
        ids!(run_7),
        ids!(run_8),
        ids!(run_9),
    ]
}

pub(crate) struct DiagnosticsPanel {
    root: WidgetRef,
    runs_dir: PathBuf,
    export_dir: PathBuf,
    runs: Vec<RunInfo>,
    selected: Option<usize>,
    /// Lines loaded for the selected run, before filtering.
    lines: Vec<LogLine>,
    filter: LogFilter,
    /// Current in-memory read limit for `dora.log`. Raised by "Load more".
    load_limit: usize,
    /// Whether older lines existed before the loaded window.
    log_truncated: bool,
    loaded: bool,
    light: Option<f64>,
    rows_key: Option<(usize, Option<usize>)>,
    levels_key: Option<LogLevel>,
    head_text: String,
    meta_text: String,
    log_status: String,
    load_error: Option<String>,
    notice: String,
}

impl DiagnosticsPanel {
    pub(crate) fn create(vm: &mut ScriptVm) -> Self {
        let value = script_eval!(vm, {
            use mod.prelude.widgets.*
            use mod.widgets.*

            View{width: Fill height: Fill flow: Down padding: 12.0 spacing: 8.0
                View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                    Label{text: "Run diagnostics"
                        draw_text +: {text_style: mod.widgets.ux.TEXT_TITLE{}}}
                    head_status := Label{width: Fill max_lines: 1 text: ""
                        draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                    refresh := mod.widgets.ux.ToolButton{text: "Refresh"}
                }
                View{width: Fill height: Fit flow: Right spacing: 6.0 align: Align{y: 0.5}
                    Label{text: "Level"}
                    lv_all := mod.widgets.ux.ToolButton{text: "> All"}
                    lv_debug := mod.widgets.ux.ToolButton{text: "Debug"}
                    lv_info := mod.widgets.ux.ToolButton{text: "Info"}
                    lv_warn := mod.widgets.ux.ToolButton{text: "Warn"}
                    lv_error := mod.widgets.ux.ToolButton{text: "Error"}
                    keyword := TextInput{width: 220 height: 32 min_height: 32
                        empty_text: "Filter keyword"}
                    source := TextInput{width: 160 height: 32 min_height: 32
                        empty_text: "Source / target"}
                }
                body := View{width: Fill height: Fill flow: Right spacing: 8.0
                    runs_card := mod.widgets.ux.Card{width: 320 height: Fill flow: Down
                        padding: 10.0 spacing: 4.0
                        Label{text: "Runs" draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                        ScrollYView{width: Fill height: Fill flow: Down spacing: 2.0
                            run_0 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            run_1 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            run_2 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            run_3 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            run_4 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            run_5 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            run_6 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            run_7 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            run_8 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            run_9 := mod.widgets.ux.QuietButton{width: Fill height: 34 min_height: 34 text: ""}
                            runs_more := Label{width: Fill max_lines: 2 text: ""
                                draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                        }
                    }
                    log_card := mod.widgets.ux.Card{width: Fill height: Fill flow: Down
                        padding: 10.0 spacing: 4.0
                        log_head := Label{width: Fill max_lines: 1 text: "dora.log"
                            draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                        ScrollYView{width: Fill height: Fill flow: Down
                            log_view := Label{width: Fill max_lines: 0 text: ""}
                        }
                        log_status := Label{width: Fill max_lines: 2 text: ""
                            draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                    }
                }
                View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                    run_meta := Label{width: Fill max_lines: 2 text: ""
                        draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                    replay := mod.widgets.ux.ToolButton{text: "Open replay"}
                    load_more := mod.widgets.ux.ToolButton{text: "Load more"}
                    export := mod.widgets.ux.PrimaryButton{text: "Export filtered"}
                    export_full := mod.widgets.ux.ToolButton{text: "Export full log"}
                }
                notice := Label{width: Fill max_lines: 2 text: ""
                    draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
            }
        });
        let runs_dir = runtime::runs();
        Self {
            root: WidgetRef::script_from_value(vm, value),
            export_dir: runs_dir.join("exports"),
            runs_dir,
            runs: Vec::new(),
            selected: None,
            lines: Vec::new(),
            filter: LogFilter::default(),
            load_limit: runs::DEFAULT_TAIL_LINES,
            log_truncated: false,
            loaded: false,
            light: None,
            rows_key: None,
            levels_key: None,
            head_text: String::new(),
            meta_text: String::new(),
            log_status: String::new(),
            load_error: None,
            notice: String::new(),
        }
    }

    fn reload(&mut self, cx: &mut Cx) {
        // `runtime::runs()` resolves to the installed app's state directory, or
        // to the development checkout's tools directory when unfundled.
        self.runs_dir = runtime::runs();
        self.export_dir = self.runs_dir.join("exports");
        let previous = self
            .selected
            .and_then(|index| self.runs.get(index))
            .map(|run| run.run_id.clone());
        self.runs = runs::discover_runs(&self.runs_dir);
        self.selected = previous
            .and_then(|run_id| self.runs.iter().position(|run| run.run_id == run_id))
            .or_else(|| (!self.runs.is_empty()).then_some(0));
        // A rescan starts each run back at the default window; "Load more" only
        // changes the current in-memory view.
        self.load_limit = runs::DEFAULT_TAIL_LINES;
        self.head_text = format!(
            "{} run{} · {}",
            self.runs.len(),
            if self.runs.len() == 1 { "" } else { "s" },
            self.runs_dir.display()
        );
        self.root
            .label(cx, ids!(head_status))
            .set_text(cx, &self.head_text);
        self.rows_key = None;
        self.notice.clear();
        self.load_selected(cx);
    }

    fn load_selected(&mut self, cx: &mut Cx) {
        self.lines.clear();
        self.load_error = None;
        self.log_truncated = false;
        if let Some(run) = self.selected.and_then(|index| self.runs.get(index)) {
            match runs::load_run_log(run, self.load_limit) {
                Ok(loaded) => {
                    self.lines = loaded.lines;
                    self.log_truncated = loaded.truncated;
                    self.log_status = if loaded.truncated {
                        format!(
                            "Showing the tail of {} ({} bytes on disk)",
                            run.run_id, loaded.total_bytes
                        )
                    } else {
                        format!("{} lines from {}", self.lines.len(), run.run_id)
                    };
                }
                Err(error) => {
                    self.load_error =
                        Some(format!("Cannot read {}: {error}", run.log_path.display()));
                }
            }
        }
        self.render_log(cx);
        self.render_meta(cx);
    }

    fn render_rows(&mut self, cx: &mut Cx) {
        let slots = run_slot_ids();
        for (index, path) in slots.iter().enumerate() {
            let button = self.root.button(cx, path);
            if let Some(run) = self.runs.get(index) {
                button.set_visible(cx, true);
                let marker = if self.selected == Some(index) {
                    "> "
                } else {
                    "  "
                };
                let dataset = if run.dataset_ready() {
                    " · dataset"
                } else {
                    ""
                };
                button.set_text(
                    cx,
                    &format!("{marker}{}{dataset}  {}", run.run_id, run.status.label()),
                );
            } else {
                button.set_visible(cx, false);
            }
        }
        let extra = self.runs.len().saturating_sub(RUN_SLOTS);
        self.root.label(cx, ids!(runs_more)).set_text(
            cx,
            &if extra > 0 {
                format!("+{extra} older run(s) not listed")
            } else {
                String::new()
            },
        );
    }

    fn render_levels(&mut self, cx: &mut Cx) {
        for (path, level, label) in [
            (ids!(lv_all) as &[LiveId], LogLevel::Trace, "All"),
            (ids!(lv_debug), LogLevel::Debug, "Debug"),
            (ids!(lv_info), LogLevel::Info, "Info"),
            (ids!(lv_warn), LogLevel::Warn, "Warn"),
            (ids!(lv_error), LogLevel::Error, "Error"),
        ] {
            let text = if self.filter.min_level == level {
                format!("> {label}")
            } else {
                label.to_owned()
            };
            self.root.button(cx, path).set_text(cx, &text);
        }
    }

    fn render_meta(&mut self, cx: &mut Cx) {
        let text = match self.selected.and_then(|index| self.runs.get(index)) {
            Some(run) => {
                let started = run
                    .started_utc()
                    .map(|stamp| format!("started {stamp} UTC"))
                    .unwrap_or_else(|| "start time unknown".to_owned());
                let dataset = run
                    .dataset
                    .as_ref()
                    .map(|path| {
                        if run.dataset_ready() {
                            format!("dataset {}", path.display())
                        } else {
                            format!("dataset missing: {}", path.display())
                        }
                    })
                    .unwrap_or_else(|| "no dataset recorded".to_owned());
                format!(
                    "{} · {} · {started} · log {}\n{dataset}",
                    run.run_id,
                    run.status.label(),
                    human_bytes(run.log_bytes)
                )
            }
            None => "Select a run. This panel only reads local files.".to_owned(),
        };
        self.meta_text = text;
        self.root
            .label(cx, ids!(run_meta))
            .set_text(cx, &self.meta_text);
        self.root.label(cx, ids!(log_head)).set_text(
            cx,
            &match self.selected.and_then(|index| self.runs.get(index)) {
                Some(run) => format!("{} · dora.log", run.run_id),
                None => "dora.log".to_owned(),
            },
        );
        self.root.button(cx, ids!(replay)).set_enabled(
            cx,
            self.selected
                .and_then(|i| self.runs.get(i))
                .is_some_and(RunInfo::dataset_ready),
        );
    }

    fn render_log(&mut self, cx: &mut Cx) {
        let filtered = runs::filter_lines(&self.lines, &self.filter);
        let text = if self.selected.is_none() {
            format!(
                "No studio-* runs found under {}.\nStart a simulation to create one; this panel only reads sessions that already exist.",
                self.runs_dir.display()
            )
        } else if self.lines.is_empty() {
            "dora.log is missing or empty for this run.\nThe runtime creates it when Dora starts; an early launch failure may only leave stderr in the launcher log.".to_owned()
        } else if filtered.is_empty() {
            "No lines match the current level / keyword / source filter.".to_owned()
        } else {
            let start = filtered.len().saturating_sub(DISPLAY_LINES);
            filtered[start..]
                .iter()
                .map(|line| display_line(line))
                .collect::<Vec<_>>()
                .join("\n")
        };
        self.root.label(cx, ids!(log_view)).set_text(cx, &text);
        if let Some(error) = &self.load_error {
            self.log_status = error.clone();
        } else if self.selected.is_some() {
            let shown = filtered.len().min(DISPLAY_LINES);
            let more = if self.log_truncated {
                if self.load_limit >= runs::MAX_TAIL_LINES {
                    " · maximum load limit reached"
                } else {
                    " · more available"
                }
            } else {
                ""
            };
            self.log_status = format!(
                "{} of {} loaded lines match · showing last {shown} · loaded up to {} lines{more}",
                filtered.len(),
                self.lines.len(),
                self.load_limit
            );
        } else {
            self.log_status.clear();
        }
        self.root
            .label(cx, ids!(log_status))
            .set_text(cx, &self.log_status);
        self.root
            .button(cx, ids!(export))
            .set_enabled(cx, !filtered.is_empty());
        self.root.button(cx, ids!(load_more)).set_enabled(
            cx,
            self.log_truncated && self.load_limit < runs::MAX_TAIL_LINES && self.selected.is_some(),
        );
        self.root.button(cx, ids!(export_full)).set_enabled(
            cx,
            self.selected
                .and_then(|index| self.runs.get(index))
                .is_some_and(|run| run.log_path.is_file()),
        );
    }

    /// Raise the in-memory window by one step (cap: `MAX_TAIL_LINES`) and
    /// re-read the tail. This never rewrites `dora.log`.
    fn load_more(&mut self, cx: &mut Cx) {
        let Some(run) = self.selected.and_then(|index| self.runs.get(index)) else {
            self.notice = "Select a run before loading more lines.".to_owned();
            self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
            return;
        };
        if self.load_limit >= runs::MAX_TAIL_LINES {
            self.notice = format!(
                "Already at the maximum in-memory limit ({} lines).",
                runs::MAX_TAIL_LINES
            );
            self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
            return;
        }
        let run_id = run.run_id.clone();
        self.load_limit = (self.load_limit + runs::TAIL_STEP_LINES).min(runs::MAX_TAIL_LINES);
        self.load_selected(cx);
        self.notice = match &self.load_error {
            Some(error) => error.clone(),
            None => format!(
                "Loaded up to {} lines from {} ({} in memory).",
                self.load_limit,
                run_id,
                self.lines.len()
            ),
        };
        self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
    }

    /// Stream-copy the whole `dora.log` to `<runs>/exports`, independent of the
    /// loaded/filtered window. Complements "Export filtered".
    fn export_full(&mut self, cx: &mut Cx) {
        let Some(run) = self.selected.and_then(|index| self.runs.get(index)) else {
            self.notice = "Select a run before exporting.".to_owned();
            self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
            return;
        };
        if !run.log_path.is_file() {
            self.notice = format!(
                "Cannot copy the full log: {} is missing.",
                run.log_path.display()
            );
            self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
            return;
        }
        let run_id = run.run_id.clone();
        let bytes = run.log_bytes;
        match runs::export_full_log(run, &self.export_dir, runs::now_ms()) {
            Ok(path) => {
                self.notice = format!(
                    "Copied the full dora.log of {run_id} ({}) to {}",
                    human_bytes(bytes),
                    path.display()
                );
            }
            Err(error) => self.notice = format!("Full log export failed: {error}"),
        }
        self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
    }

    fn export(&mut self, cx: &mut Cx) {
        let run = self.selected.and_then(|index| self.runs.get(index));
        let Some(run) = run else {
            self.notice = "Select a run before exporting.".to_owned();
            return;
        };
        let run_id = run.run_id.clone();
        let directory = self.export_dir.clone();
        let filtered = runs::filter_lines(&self.lines, &self.filter);
        if filtered.is_empty() {
            self.notice = "Nothing to export: no lines match the filter.".to_owned();
            return;
        }
        match runs::export_filtered(&filtered, &run_id, &directory, runs::now_ms()) {
            Ok(path) => {
                self.notice = format!("Exported {} lines to {}", filtered.len(), path.display())
            }
            Err(error) => self.notice = format!("Export failed: {error}"),
        }
        self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
    }

    fn open_replay(&mut self, cx: &mut Cx, backend: &mut dyn Backend) -> bool {
        let Some(run) = self.selected.and_then(|index| self.runs.get(index)) else {
            self.notice = "Select a run before opening replay.".to_owned();
            self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
            return true;
        };
        let Some(dataset) = run.dataset.clone() else {
            self.notice = format!(
                "{} has no recorded dataset. Save a take in the simulation, then Refresh.",
                run.run_id
            );
            self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
            return true;
        };
        if !dataset.is_dir() {
            self.notice = format!("The dataset is no longer at {}", dataset.display());
            self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
            return true;
        }
        // Reuses the Library/Replay path: the host rescans the folder, opens the
        // first dataset and switches to Screen::Play.
        backend.dispatch(Intent::OpenFolder(dataset.clone()));
        self.notice = format!("Opening {} in Replay", dataset.display());
        self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
        true
    }
}

impl PluginPanel for DiagnosticsPanel {
    fn root(&self) -> WidgetRef {
        self.root.clone()
    }

    fn sync(&mut self, cx: &mut Cx, _context: &PanelContext<'_>) {
        let light = frame::light_mode();
        if self.light != Some(light) {
            frame::apply_light(
                cx,
                &self.root,
                &[
                    (ids!(runs_card), Themed::Bg),
                    (ids!(log_card), Themed::Bg),
                    (ids!(refresh), Themed::Both),
                    (ids!(lv_all), Themed::Both),
                    (ids!(lv_debug), Themed::Both),
                    (ids!(lv_info), Themed::Both),
                    (ids!(lv_warn), Themed::Both),
                    (ids!(lv_error), Themed::Both),
                    (ids!(export), Themed::Both),
                    (ids!(export_full), Themed::Both),
                    (ids!(load_more), Themed::Both),
                    (ids!(replay), Themed::Both),
                ],
                light,
            );
            self.light = Some(light);
        }
        if !self.loaded {
            self.loaded = true;
            self.reload(cx);
        }
        if self.rows_key != Some((self.runs.len(), self.selected)) {
            self.rows_key = Some((self.runs.len(), self.selected));
            self.render_rows(cx);
        }
        if self.levels_key != Some(self.filter.min_level) {
            self.levels_key = Some(self.filter.min_level);
            self.render_levels(cx);
        }
        self.root.button(cx, ids!(replay)).set_enabled(
            cx,
            self.selected
                .and_then(|i| self.runs.get(i))
                .is_some_and(RunInfo::dataset_ready),
        );
        self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
    }

    fn handle_actions(
        &mut self,
        cx: &mut Cx,
        actions: &Actions,
        backend: &mut dyn Backend,
    ) -> bool {
        if self.root.button(cx, ids!(refresh)).clicked(actions) {
            self.reload(cx);
            self.notice = format!("Rescanned {}", self.runs_dir.display());
            self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);
            return true;
        }

        for (path, level) in [
            (ids!(lv_all) as &[LiveId], LogLevel::Trace),
            (ids!(lv_debug), LogLevel::Debug),
            (ids!(lv_info), LogLevel::Info),
            (ids!(lv_warn), LogLevel::Warn),
            (ids!(lv_error), LogLevel::Error),
        ] {
            if self.root.button(cx, path).clicked(actions) {
                self.filter.min_level = level;
                self.levels_key = None;
                self.render_levels(cx);
                self.render_log(cx);
                return true;
            }
        }

        let keyword_changed = self
            .root
            .text_input(cx, ids!(keyword))
            .changed(actions)
            .is_some();
        let source_changed = self
            .root
            .text_input(cx, ids!(source))
            .changed(actions)
            .is_some();
        if keyword_changed || source_changed {
            self.filter.keyword = self.root.text_input(cx, ids!(keyword)).text();
            self.filter.source = self.root.text_input(cx, ids!(source)).text();
            self.render_log(cx);
            return true;
        }

        if self.root.button(cx, ids!(export)).clicked(actions) {
            self.export(cx);
            return true;
        }

        if self.root.button(cx, ids!(load_more)).clicked(actions) {
            self.load_more(cx);
            return true;
        }

        if self.root.button(cx, ids!(export_full)).clicked(actions) {
            self.export_full(cx);
            return true;
        }

        if self.root.button(cx, ids!(replay)).clicked(actions) {
            return self.open_replay(cx, backend);
        }

        for (index, path) in run_slot_ids().iter().enumerate() {
            if self.root.button(cx, *path).clicked(actions) {
                self.selected = Some(index);
                // Switching runs starts from the default window again; the
                // previous run's "Load more" level does not carry over.
                self.load_limit = runs::DEFAULT_TAIL_LINES;
                self.rows_key = None;
                self.load_selected(cx);
                self.render_rows(cx);
                return true;
            }
        }
        false
    }
}

fn display_line(line: &LogLine) -> String {
    let text = line.display();
    if text.chars().count() > MAX_LINE_CHARS {
        let mut shortened: String = text.chars().take(MAX_LINE_CHARS).collect();
        shortened.push_str("...");
        shortened
    } else {
        text
    }
}

fn human_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
