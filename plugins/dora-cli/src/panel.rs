//! Read-only component debug panel backed by the `dora` CLI.
//!
//! Layout: CLI/coordinator header, topic list + one stream output in the
//! middle, node/param inspection on the right. Every action maps to a
//! read-only CLI command from [`crate::cli`]; the panel never starts, stops or
//! mutates a dataflow.
//!
//! Streams are polled instead of pushed: [`StreamSession`] owns the child and a
//! bounded ring, and `sync` copies only when the ring's revision changes. A
//! second stream cancels the first before spawning.

use std::collections::VecDeque;

use dorobot_studio_core::{
    api::Backend,
    ui::frame::{self, Themed},
};
use dorobot_studio_plugin::*;

use crate::cli::{
    parse_ids, CliOutput, DoraCli, IdKind, StreamSession, StreamStats, DEFAULT_RING_LINES,
    HZ_DURATION_SECONDS,
};

const SLOTS: usize = 8;
/// A Label with thousands of stream lines is unusable; the ring keeps 5 000,
/// the panel renders the newest 400.
const DISPLAY_LINES: usize = 400;
const MAX_LINE_CHARS: usize = 600;
const DETAIL_CHARS: usize = 1_500;
const HISTORY_LINES: usize = 20;
const HISTORY_SHOWN: usize = 4;

fn topic_slot_ids() -> [&'static [LiveId]; SLOTS] {
    [
        ids!(topic_0),
        ids!(topic_1),
        ids!(topic_2),
        ids!(topic_3),
        ids!(topic_4),
        ids!(topic_5),
        ids!(topic_6),
        ids!(topic_7),
    ]
}

fn node_slot_ids() -> [&'static [LiveId]; SLOTS] {
    [
        ids!(node_0),
        ids!(node_1),
        ids!(node_2),
        ids!(node_3),
        ids!(node_4),
        ids!(node_5),
        ids!(node_6),
        ids!(node_7),
    ]
}

fn param_slot_ids() -> [&'static [LiveId]; SLOTS] {
    [
        ids!(param_0),
        ids!(param_1),
        ids!(param_2),
        ids!(param_3),
        ids!(param_4),
        ids!(param_5),
        ids!(param_6),
        ids!(param_7),
    ]
}

#[derive(Clone, Copy)]
enum StreamKind {
    Echo,
    Hz,
}

impl StreamKind {
    fn label(self) -> &'static str {
        match self {
            Self::Echo => "echo",
            Self::Hz => "hz",
        }
    }
}

pub(crate) struct DoraCliPanel {
    root: WidgetRef,
    cli: Option<DoraCli>,
    cli_path: String,
    version: String,
    coordinator: String,
    topics: Vec<String>,
    topic_status: String,
    selected_topic: Option<usize>,
    nodes: Vec<String>,
    node_status: String,
    node_info: String,
    selected_node: Option<usize>,
    params: Vec<String>,
    param_status: String,
    param_value: String,
    selected_param: Option<usize>,
    stream: Option<StreamSession>,
    stream_running: bool,
    stream_exit_reported: bool,
    stream_revision: u64,
    stream_stats: StreamStats,
    stream_tail: Vec<String>,
    stream_note: String,
    history: VecDeque<String>,
    notice: String,
    light: Option<f64>,
    connected: bool,
    dirty: bool,
}

impl DoraCliPanel {
    pub(crate) fn create(vm: &mut ScriptVm) -> Self {
        let value = script_eval!(vm, {
            use mod.prelude.widgets.*
            use mod.widgets.*

            View{width: Fill height: Fill flow: Down padding: 12.0 spacing: 8.0
                View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                    Label{text: "Component debug"
                        draw_text +: {text_style: mod.widgets.ux.TEXT_TITLE{}}}
                    cli_meta := Label{width: Fill max_lines: 2 text: ""
                        draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                    cli_refresh := mod.widgets.ux.ToolButton{text: "Reconnect CLI"}
                }
                View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                    coord_meta := Label{width: Fill max_lines: 3 text: ""
                        draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                    coord_refresh := mod.widgets.ux.ToolButton{text: "Refresh coordinator"}
                }
                View{width: Fill height: Fill flow: Right spacing: 8.0
                    topics_card := mod.widgets.ux.Card{width: 300 height: Fill flow: Down
                        padding: 10.0 spacing: 4.0
                        View{width: Fill height: Fit flow: Right spacing: 6.0 align: Align{y: 0.5}
                            Label{text: "Topics"
                                draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                            topic_refresh := mod.widgets.ux.ToolButton{text: "Refresh"}
                        }
                        ScrollYView{width: Fill height: Fill flow: Down spacing: 2.0
                            topic_0 := mod.widgets.ux.QuietButton{width: Fill height: 30 min_height: 30 text: ""}
                            topic_1 := mod.widgets.ux.QuietButton{width: Fill height: 30 min_height: 30 text: ""}
                            topic_2 := mod.widgets.ux.QuietButton{width: Fill height: 30 min_height: 30 text: ""}
                            topic_3 := mod.widgets.ux.QuietButton{width: Fill height: 30 min_height: 30 text: ""}
                            topic_4 := mod.widgets.ux.QuietButton{width: Fill height: 30 min_height: 30 text: ""}
                            topic_5 := mod.widgets.ux.QuietButton{width: Fill height: 30 min_height: 30 text: ""}
                            topic_6 := mod.widgets.ux.QuietButton{width: Fill height: 30 min_height: 30 text: ""}
                            topic_7 := mod.widgets.ux.QuietButton{width: Fill height: 30 min_height: 30 text: ""}
                        }
                        topic_status := Label{width: Fill max_lines: 4 text: ""
                            draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                    }
                    output_card := mod.widgets.ux.Card{width: Fill height: Fill flow: Down
                        padding: 10.0 spacing: 4.0
                        stream_head := Label{width: Fill max_lines: 1 text: "No stream"
                            draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                        ScrollYView{width: Fill height: Fill flow: Down
                            stream_view := Label{width: Fill max_lines: 0 text: ""}
                        }
                        stream_status := Label{width: Fill max_lines: 3 text: ""
                            draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                        View{width: Fill height: Fit flow: Right spacing: 6.0 align: Align{y: 0.5}
                            echo := mod.widgets.ux.ToolButton{text: "Echo"}
                            hz := mod.widgets.ux.ToolButton{text: "Hz (60 s)"}
                            stream_stop := mod.widgets.ux.ToolButton{text: "Stop"}
                        }
                    }
                    side := View{width: 340 height: Fill flow: Down spacing: 8.0
                        nodes_card := mod.widgets.ux.Card{width: Fill height: Fill flow: Down
                            padding: 10.0 spacing: 4.0
                            View{width: Fill height: Fit flow: Right spacing: 6.0 align: Align{y: 0.5}
                                Label{text: "Nodes"
                                    draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                                node_refresh := mod.widgets.ux.ToolButton{text: "Refresh"}
                            }
                            ScrollYView{width: Fill height: Fill flow: Down spacing: 2.0
                                node_0 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                node_1 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                node_2 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                node_3 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                node_4 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                node_5 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                node_6 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                node_7 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                            }
                            node_status := Label{width: Fill max_lines: 2 text: ""
                                draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                            node_info := Label{width: Fill max_lines: 8 text: ""
                                draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                        }
                        params_card := mod.widgets.ux.Card{width: Fill height: Fill flow: Down
                            padding: 10.0 spacing: 4.0
                            View{width: Fill height: Fit flow: Right spacing: 6.0 align: Align{y: 0.5}
                                Label{text: "Params"
                                    draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                                param_refresh := mod.widgets.ux.ToolButton{text: "Refresh"}
                            }
                            ScrollYView{width: Fill height: Fill flow: Down spacing: 2.0
                                param_0 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                param_1 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                param_2 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                param_3 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                param_4 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                param_5 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                param_6 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                                param_7 := mod.widgets.ux.QuietButton{width: Fill height: 28 min_height: 28 text: ""}
                            }
                            param_status := Label{width: Fill max_lines: 2 text: ""
                                draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                            param_value := Label{width: Fill max_lines: 6 text: ""
                                draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                        }
                    }
                }
                cmd_log := Label{width: Fill max_lines: 4 text: ""
                    draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
                notice := Label{width: Fill max_lines: 2 text: ""
                    draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}}
            }
        });
        Self {
            root: WidgetRef::script_from_value(vm, value),
            cli: None,
            cli_path: "CLI: resolving…".to_owned(),
            version: String::new(),
            coordinator: "Coordinator: checking…".to_owned(),
            topics: Vec::new(),
            topic_status: String::new(),
            selected_topic: None,
            nodes: Vec::new(),
            node_status: String::new(),
            node_info: String::new(),
            selected_node: None,
            params: Vec::new(),
            param_status: String::new(),
            param_value: String::new(),
            selected_param: None,
            stream: None,
            stream_running: false,
            stream_exit_reported: true,
            stream_revision: 0,
            stream_stats: StreamStats::default(),
            stream_tail: Vec::new(),
            stream_note: String::new(),
            history: VecDeque::new(),
            notice: String::new(),
            light: None,
            connected: false,
            dirty: true,
        }
    }

    fn connect(&mut self, cx: &mut Cx) {
        self.cli = None;
        self.version.clear();
        self.cli_path = "CLI: resolving…".to_owned();
        match DoraCli::discover() {
            Ok(cli) => {
                self.cli_path = format!("CLI: {}", cli.program().display());
                match cli.version() {
                    Ok(output) => {
                        self.record(&output);
                        let line = output
                            .stdout
                            .lines()
                            .map(str::trim)
                            .find(|line| !line.is_empty())
                            .unwrap_or("<no version output>");
                        self.version = if output.success() {
                            format!("version: {line}")
                        } else {
                            format!("version: {} · {line}", outcome_label(&output))
                        };
                    }
                    Err(error) => self.version = format!("version: failed: {error}"),
                }
                self.cli = Some(cli);
            }
            Err(error) => {
                self.cli_path = format!("CLI: not found ({error})");
                self.version = format!("Set DOROBOT_DORA_BIN or put dora-cli on PATH.");
                self.coordinator = "Coordinator: unknown — no dora CLI to query.".to_owned();
                self.topics.clear();
                self.selected_topic = None;
                self.topic_status = "No CLI, so no topics.".to_owned();
                self.nodes.clear();
                self.selected_node = None;
                self.node_status = "No CLI, so no nodes.".to_owned();
                self.params.clear();
                self.selected_param = None;
                self.param_status = "No CLI, so no params.".to_owned();
                self.dirty = true;
                return;
            }
        }
        self.refresh_coordinator(cx);
    }

    fn refresh_coordinator(&mut self, cx: &mut Cx) {
        let Some(cli) = self.cli.clone() else {
            self.coordinator = "Coordinator: unknown — no dora CLI to query.".to_owned();
            self.dirty = true;
            return;
        };
        match cli.list() {
            Ok(output) => {
                self.record(&output);
                if output.success() {
                    let body = output.stdout.trim();
                    self.coordinator = if body.is_empty() {
                        "Coordinator: running · no dataflows listed. Launch the graph with `dora start <dataflow>.yaml`."
                            .to_owned()
                    } else {
                        format!("Coordinator: running\n{}", clip(body, 600))
                    };
                    self.notice.clear();
                    self.refresh_topics(cx, false);
                    self.refresh_nodes(cx, false);
                } else {
                    let reason = output
                        .first_error_line()
                        .unwrap_or("no output from the CLI");
                    self.coordinator = format!(
                        "Coordinator: not reachable — start it with `dora up`, then `dora start <dataflow>.yaml`.\n{reason}"
                    );
                    self.topics.clear();
                    self.selected_topic = None;
                    self.topic_status =
                        "Coordinator not running; topic list is unavailable.".to_owned();
                    self.nodes.clear();
                    self.selected_node = None;
                    self.node_status =
                        "Coordinator not running; node list is unavailable.".to_owned();
                    self.params.clear();
                    self.selected_param = None;
                    self.param_status = "Select a node once the coordinator is running.".to_owned();
                    self.notice.clear();
                    self.dirty = true;
                }
            }
            Err(error) => {
                self.coordinator = format!("Coordinator: `dora list` could not start: {error}");
                self.dirty = true;
            }
        }
    }

    fn refresh_topics(&mut self, _cx: &mut Cx, announce: bool) {
        let Some(cli) = self.cli.clone() else {
            self.notice = "Resolve the dora CLI first.".to_owned();
            self.dirty = true;
            return;
        };
        match cli.topic_list() {
            Ok(output) => {
                self.record(&output);
                if output.success() {
                    self.topics = parse_ids(&output.stdout, IdKind::Topic);
                    self.selected_topic = self
                        .selected_topic
                        .filter(|index| *index < self.topics.len());
                    self.topic_status = if self.topics.is_empty() {
                        let raw = output.stdout.trim();
                        if raw.is_empty() {
                            "No topics. Start a dataflow on the coordinator with debug inspection enabled."
                                .to_owned()
                        } else {
                            format!("No topic ids parsed; raw output:\n{}", clip(raw, 300))
                        }
                    } else {
                        format!("{} topic(s) on the coordinator.", self.topics.len())
                    };
                    if announce {
                        self.notice = format!("Loaded {} topic(s).", self.topics.len());
                    }
                } else {
                    self.topics.clear();
                    self.selected_topic = None;
                    self.topic_status = format!("topic list failed: {}", error_detail(&output));
                    if announce {
                        self.notice = self.topic_status.clone();
                    }
                }
            }
            Err(error) => {
                self.topics.clear();
                self.selected_topic = None;
                self.topic_status = format!("topic list failed: {error}");
                if announce {
                    self.notice = self.topic_status.clone();
                }
            }
        }
        self.dirty = true;
    }

    fn refresh_nodes(&mut self, _cx: &mut Cx, announce: bool) {
        let Some(cli) = self.cli.clone() else {
            self.notice = "Resolve the dora CLI first.".to_owned();
            self.dirty = true;
            return;
        };
        match cli.node_list() {
            Ok(output) => {
                self.record(&output);
                if output.success() {
                    self.nodes = parse_ids(&output.stdout, IdKind::Node);
                    self.selected_node =
                        self.selected_node.filter(|index| *index < self.nodes.len());
                    self.node_status = if self.nodes.is_empty() {
                        let raw = output.stdout.trim();
                        if raw.is_empty() {
                            "No nodes. Start a dataflow with `dora start` on the coordinator."
                                .to_owned()
                        } else {
                            format!("No node ids parsed; raw output:\n{}", clip(raw, 300))
                        }
                    } else {
                        format!("{} node(s) running.", self.nodes.len())
                    };
                    if announce {
                        self.notice = format!("Loaded {} node(s).", self.nodes.len());
                    }
                } else {
                    self.nodes.clear();
                    self.selected_node = None;
                    self.node_status = format!("node list failed: {}", error_detail(&output));
                    if announce {
                        self.notice = self.node_status.clone();
                    }
                }
            }
            Err(error) => {
                self.nodes.clear();
                self.selected_node = None;
                self.node_status = format!("node list failed: {error}");
                if announce {
                    self.notice = self.node_status.clone();
                }
            }
        }
        self.dirty = true;
    }

    fn select_node(&mut self, cx: &mut Cx, index: usize) {
        self.selected_node = Some(index);
        self.selected_param = None;
        self.params.clear();
        self.param_value.clear();
        let Some(node) = self.nodes.get(index).cloned() else {
            self.dirty = true;
            return;
        };
        let Some(cli) = self.cli.clone() else {
            self.node_status = "Resolve the dora CLI first.".to_owned();
            self.dirty = true;
            return;
        };
        match cli.node_info(&node) {
            Ok(output) => {
                self.record(&output);
                self.node_status = format!("node info {node} · {}", outcome_label(&output));
                self.node_info = detail(&output);
            }
            Err(error) => {
                self.node_status = format!("node info {node} failed to start: {error}");
                self.node_info.clear();
            }
        }
        self.refresh_params(cx, false);
        self.dirty = true;
    }

    fn refresh_params(&mut self, _cx: &mut Cx, announce: bool) {
        let Some(node) = self
            .selected_node
            .and_then(|index| self.nodes.get(index))
            .cloned()
        else {
            self.params.clear();
            self.selected_param = None;
            self.param_status =
                "Select a node first; `dora param list <NODE>` takes the node positionally."
                    .to_owned();
            self.param_value.clear();
            if announce {
                self.notice = self.param_status.clone();
            }
            self.dirty = true;
            return;
        };
        let Some(cli) = self.cli.clone() else {
            self.param_status = "Resolve the dora CLI first.".to_owned();
            self.dirty = true;
            return;
        };
        match cli.param_list(&node) {
            Ok(output) => {
                self.record(&output);
                if output.success() {
                    self.params = parse_ids(&output.stdout, IdKind::Param);
                    self.selected_param = self
                        .selected_param
                        .filter(|index| *index < self.params.len());
                    self.param_status = if self.params.is_empty() {
                        format!(
                            "No params parsed for {node}; raw output:\n{}",
                            clip(output.stdout.trim(), 300)
                        )
                    } else {
                        format!("{} param(s) on {node}.", self.params.len())
                    };
                } else {
                    self.params.clear();
                    self.selected_param = None;
                    self.param_status =
                        format!("param list {node} failed: {}", error_detail(&output));
                }
                if announce {
                    self.notice = self.param_status.clone();
                }
            }
            Err(error) => {
                self.params.clear();
                self.selected_param = None;
                self.param_status = format!("param list {node} failed to start: {error}");
                if announce {
                    self.notice = self.param_status.clone();
                }
            }
        }
        self.dirty = true;
    }

    fn select_param(&mut self, index: usize) {
        self.selected_param = Some(index);
        let Some(node) = self
            .selected_node
            .and_then(|node_index| self.nodes.get(node_index))
            .cloned()
        else {
            self.param_value = "Select a node first.".to_owned();
            self.dirty = true;
            return;
        };
        let Some(key) = self.params.get(index).cloned() else {
            self.dirty = true;
            return;
        };
        let Some(cli) = self.cli.clone() else {
            self.param_value = "Resolve the dora CLI first.".to_owned();
            self.dirty = true;
            return;
        };
        match cli.param_get(&node, &key) {
            Ok(output) => {
                self.record(&output);
                self.param_status = format!("param get {node} {key} · {}", outcome_label(&output));
                self.param_value = detail(&output);
            }
            Err(error) => {
                self.param_status = format!("param get {node} {key} failed to start: {error}");
                self.param_value.clear();
            }
        }
        self.dirty = true;
    }

    fn start_stream(&mut self, kind: StreamKind) {
        let Some(cli) = self.cli.clone() else {
            self.notice = "Resolve the dora CLI first.".to_owned();
            self.dirty = true;
            return;
        };
        let Some(topic) = self
            .selected_topic
            .and_then(|index| self.topics.get(index))
            .cloned()
        else {
            self.notice = format!(
                "Select a topic before starting {}.\nIf the list is empty, start the graph on the coordinator: `dora up` then `dora start <dataflow>.yaml`. A `dora run` graph uses an isolated coordinator and cannot be watched here.",
                kind.label()
            );
            self.dirty = true;
            return;
        };
        let args = match kind {
            StreamKind::Echo => DoraCli::topic_echo_args(&topic),
            StreamKind::Hz => DoraCli::topic_hz_args(&topic),
        };
        // One stream at a time: drop the previous child before spawning.
        self.cancel_stream();
        match cli.start_stream(&args, DEFAULT_RING_LINES) {
            Ok(session) => {
                self.push_history(format!("$ dora {}  →  started", args.join(" ")));
                self.stream_note = session.command_line();
                self.stream = Some(session);
                self.stream_running = true;
                self.stream_exit_reported = false;
                self.stream_revision = 0;
                self.stream_stats = StreamStats::default();
                self.stream_tail.clear();
                self.notice = match kind {
                    StreamKind::Echo => {
                        "Echo streaming. Starting another stream cancels this one.".to_owned()
                    }
                    StreamKind::Hz => format!(
                        "Hz runs for {HZ_DURATION_SECONDS} s in non-TTY mode, then prints final stats. Use Stop to cancel earlier."
                    ),
                };
            }
            Err(error) => {
                self.notice = format!("Cannot start {}: {error}", kind.label());
            }
        }
        self.dirty = true;
    }

    fn cancel_stream(&mut self) {
        if let Some(mut session) = self.stream.take() {
            // `is_running` reaps an already-exited child; only a live child is
            // killed by the session's Drop.
            let was_running = session.is_running();
            drop(session);
            self.stream_running = false;
            if was_running {
                self.push_history("stream stopped (CLI child killed)".to_owned());
                self.notice = "Stream stopped; the CLI child was killed.".to_owned();
            } else {
                self.push_history("stream output cleared".to_owned());
                self.notice = "Stream output cleared.".to_owned();
            }
            self.dirty = true;
        }
    }

    fn poll_stream(&mut self) {
        let Some(session) = self.stream.as_mut() else {
            return;
        };
        let stats = session.stats();
        let mut reaped = None;
        if stats.revision != self.stream_revision {
            self.stream_revision = stats.revision;
            self.stream_stats = stats;
            self.stream_tail = session
                .tail(DISPLAY_LINES)
                .into_iter()
                .map(|line| clip(&line, MAX_LINE_CHARS))
                .collect();
            self.dirty = true;
        }
        let running = session.is_running();
        if self.stream_running != running {
            self.stream_running = running;
            self.dirty = true;
        }
        if !running && !self.stream_exit_reported {
            self.stream_exit_reported = true;
            reaped = Some(format!(
                "$ {}  →  {} · {:.1} s",
                session.command_line(),
                match session.exit_code() {
                    Some(0) => "exit 0".to_owned(),
                    Some(code) => format!("exit {code}"),
                    None => "ended without an exit code".to_owned(),
                },
                session.elapsed().as_secs_f64()
            ));
        }
        if let Some(line) = reaped {
            self.push_history(line);
            self.dirty = true;
        }
    }

    fn record(&mut self, output: &CliOutput) {
        self.push_history(output.summary());
    }

    fn push_history(&mut self, line: String) {
        self.history.push_back(line);
        while self.history.len() > HISTORY_LINES {
            self.history.pop_front();
        }
    }

    fn render(&mut self, cx: &mut Cx) {
        self.root
            .label(cx, ids!(cli_meta))
            .set_text(cx, &format!("{}\n{}", self.cli_path, self.version));
        self.root
            .label(cx, ids!(coord_meta))
            .set_text(cx, &self.coordinator);

        for (index, path) in topic_slot_ids().iter().enumerate() {
            let button = self.root.button(cx, *path);
            if let Some(topic) = self.topics.get(index) {
                button.set_visible(cx, true);
                button.set_text(
                    cx,
                    &format!(
                        "{}{}",
                        if self.selected_topic == Some(index) {
                            "> "
                        } else {
                            "  "
                        },
                        clip(topic, 80)
                    ),
                );
            } else {
                button.set_visible(cx, false);
            }
        }
        self.root
            .label(cx, ids!(topic_status))
            .set_text(cx, &self.topic_status);

        for (index, path) in node_slot_ids().iter().enumerate() {
            let button = self.root.button(cx, *path);
            if let Some(node) = self.nodes.get(index) {
                button.set_visible(cx, true);
                button.set_text(
                    cx,
                    &format!(
                        "{}{}",
                        if self.selected_node == Some(index) {
                            "> "
                        } else {
                            "  "
                        },
                        clip(node, 80)
                    ),
                );
            } else {
                button.set_visible(cx, false);
            }
        }
        self.root
            .label(cx, ids!(node_status))
            .set_text(cx, &self.node_status);
        self.root
            .label(cx, ids!(node_info))
            .set_text(cx, &self.node_info);

        for (index, path) in param_slot_ids().iter().enumerate() {
            let button = self.root.button(cx, *path);
            if let Some(param) = self.params.get(index) {
                button.set_visible(cx, true);
                button.set_text(
                    cx,
                    &format!(
                        "{}{}",
                        if self.selected_param == Some(index) {
                            "> "
                        } else {
                            "  "
                        },
                        clip(param, 80)
                    ),
                );
            } else {
                button.set_visible(cx, false);
            }
        }
        self.root
            .label(cx, ids!(param_status))
            .set_text(cx, &self.param_status);
        self.root
            .label(cx, ids!(param_value))
            .set_text(cx, &self.param_value);

        self.render_stream(cx);

        let history = self
            .history
            .iter()
            .rev()
            .take(HISTORY_SHOWN)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        self.root.label(cx, ids!(cmd_log)).set_text(cx, &history);
        self.root.label(cx, ids!(notice)).set_text(cx, &self.notice);

        let has_cli = self.cli.is_some();
        let can_stream = has_cli && self.selected_topic.is_some();
        self.root
            .button(cx, ids!(topic_refresh))
            .set_enabled(cx, has_cli);
        self.root
            .button(cx, ids!(node_refresh))
            .set_enabled(cx, has_cli);
        self.root
            .button(cx, ids!(param_refresh))
            .set_enabled(cx, has_cli);
        self.root.button(cx, ids!(echo)).set_enabled(cx, can_stream);
        self.root.button(cx, ids!(hz)).set_enabled(cx, can_stream);
        self.root
            .button(cx, ids!(stream_stop))
            .set_enabled(cx, self.stream.is_some());
    }

    fn render_stream(&mut self, cx: &mut Cx) {
        let head = match &self.stream {
            Some(_) => format!(
                "{} · {}",
                self.stream_note,
                if self.stream_running {
                    "streaming"
                } else {
                    "stopped"
                }
            ),
            None => "No stream".to_owned(),
        };
        self.root.label(cx, ids!(stream_head)).set_text(cx, &head);

        let body = if !self.stream_tail.is_empty() {
            self.stream_tail.join("\n")
        } else if self.stream.is_some() {
            "Waiting for output…".to_owned()
        } else {
            "No output yet.\nStart the graph on the coordinator: `dora up`, then `dora start <dataflow>.yaml`.\nA graph launched with `dora run` runs in an isolated coordinator and cannot be inspected here."
                .to_owned()
        };
        self.root.label(cx, ids!(stream_view)).set_text(cx, &body);

        let status = if let Some(_session) = &self.stream {
            format!(
                "{} · {} lines total · {} in buffer of {} · {} older dropped\nShowing the newest {}.",
                if self.stream_running {
                    "running"
                } else {
                    "exited"
                },
                self.stream_stats.total,
                self.stream_stats.len,
                self.stream_stats.capacity,
                self.stream_stats.dropped,
                self.stream_tail.len()
            )
        } else {
            "Read-only: Echo and Hz only. No `param set`, `topic pub` or `node restart`.".to_owned()
        };
        self.root
            .label(cx, ids!(stream_status))
            .set_text(cx, &status);
    }
}

impl PluginPanel for DoraCliPanel {
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
                    (ids!(topics_card), Themed::Bg),
                    (ids!(output_card), Themed::Bg),
                    (ids!(nodes_card), Themed::Bg),
                    (ids!(params_card), Themed::Bg),
                    (ids!(cli_refresh), Themed::Both),
                    (ids!(coord_refresh), Themed::Both),
                    (ids!(topic_refresh), Themed::Both),
                    (ids!(node_refresh), Themed::Both),
                    (ids!(param_refresh), Themed::Both),
                    (ids!(echo), Themed::Both),
                    (ids!(hz), Themed::Both),
                    (ids!(stream_stop), Themed::Both),
                ],
                light,
            );
            self.light = Some(light);
            self.dirty = true;
        }
        if !self.connected {
            self.connected = true;
            self.connect(cx);
        }
        self.poll_stream();
        if self.dirty {
            self.dirty = false;
            self.render(cx);
        }
    }

    fn handle_actions(
        &mut self,
        cx: &mut Cx,
        actions: &Actions,
        _backend: &mut dyn Backend,
    ) -> bool {
        if self.root.button(cx, ids!(cli_refresh)).clicked(actions) {
            self.connect(cx);
            self.dirty = true;
            return true;
        }
        if self.root.button(cx, ids!(coord_refresh)).clicked(actions) {
            self.refresh_coordinator(cx);
            return true;
        }
        if self.root.button(cx, ids!(topic_refresh)).clicked(actions) {
            self.refresh_topics(cx, true);
            return true;
        }
        if self.root.button(cx, ids!(node_refresh)).clicked(actions) {
            self.refresh_nodes(cx, true);
            return true;
        }
        if self.root.button(cx, ids!(param_refresh)).clicked(actions) {
            self.refresh_params(cx, true);
            return true;
        }
        if self.root.button(cx, ids!(echo)).clicked(actions) {
            self.start_stream(StreamKind::Echo);
            return true;
        }
        if self.root.button(cx, ids!(hz)).clicked(actions) {
            self.start_stream(StreamKind::Hz);
            return true;
        }
        if self.root.button(cx, ids!(stream_stop)).clicked(actions) {
            self.cancel_stream();
            return true;
        }
        for (index, path) in topic_slot_ids().iter().enumerate() {
            if self.root.button(cx, *path).clicked(actions) {
                self.selected_topic = Some(index);
                self.notice = self
                    .topics
                    .get(index)
                    .map(|topic| format!("Topic {topic} selected; choose Echo or Hz."))
                    .unwrap_or_default();
                self.dirty = true;
                return true;
            }
        }
        for (index, path) in node_slot_ids().iter().enumerate() {
            if self.root.button(cx, *path).clicked(actions) {
                self.select_node(cx, index);
                return true;
            }
        }
        for (index, path) in param_slot_ids().iter().enumerate() {
            if self.root.button(cx, *path).clicked(actions) {
                self.select_param(index);
                return true;
            }
        }
        false
    }

    fn on_disable(&mut self, _cx: &mut Cx) {
        // Dropping the session kills the CLI child.
        self.cancel_stream();
    }
}

fn outcome_label(output: &CliOutput) -> String {
    if output.timed_out {
        "timed out".to_owned()
    } else {
        match output.status {
            Some(0) => "exit 0".to_owned(),
            Some(code) => format!("exit {code}"),
            None => "no exit status".to_owned(),
        }
    }
}

fn error_detail(output: &CliOutput) -> String {
    let text = if output.stderr.trim().is_empty() {
        output.stdout.trim()
    } else {
        output.stderr.trim()
    };
    if text.is_empty() {
        outcome_label(output)
    } else {
        clip(text, 600)
    }
}

fn detail(output: &CliOutput) -> String {
    if output.timed_out {
        return format!(
            "Timed out. Partial output:\n{}",
            clip(output.body().trim(), DETAIL_CHARS)
        );
    }
    let body = output.body();
    let body = body.trim();
    if body.is_empty() {
        format!("{} · no output", outcome_label(output))
    } else if output.success() {
        clip(body, DETAIL_CHARS)
    } else {
        format!("{}\n{}", outcome_label(output), clip(body, DETAIL_CHARS))
    }
}

fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let mut shortened: String = text.chars().take(max_chars).collect();
        shortened.push('…');
        shortened
    }
}
