//! The `dviz_view` panel: connection, the 3D window, the display list, logs
//! and nodes, as a list or as the dataflow graph with the clicked node's
//! detail.
//!
//! The panel only holds a handle to the process-wide [`DvizSession`]; every
//! `sync` reads a fresh snapshot of it. Nothing here waits on the session:
//! `connect` and `disconnect` return at once and report through the snapshot.

use crate::{
    open_viewer, DvizGraphWidgetRefExt, DvizSession, EntityInfo, SessionOptions, SessionSnapshot,
    ViewerState,
};
use dorobot_studio_core::{
    api::Backend,
    ui::frame::{self, Themed},
};
use dorobot_studio_plugin::*;
use dviz_protocol::{GraphEdge, GraphNode, GraphNodeStatus, LogEntry, LogLevel};
use std::{collections::BTreeMap, time::Duration};

/// Newest log lines shown; the `logs` label holds this many.
const LOG_LINES: usize = 12;
/// Longest log line shown, in characters; longer entries end in "…".
const LOG_LINE_CHARS: usize = 160;
/// How long the session may listen without a peer before the status hints
/// at entering the robot's endpoint.
const NO_PEER_HINT_AFTER: Duration = Duration::from_secs(2);
/// Nodes listed; the `nodes` label holds this many.
const NODE_LINES: usize = 12;

pub struct DvizViewPanel {
    root: WidgetRef,
    session: &'static DvizSession,
    /// One row per entity, in the snapshot's (sorted) order.
    rows: Vec<EntityRow>,
    warn_only: bool,
    light: Option<f64>,
    /// Nodes as the graph rather than the list.
    graph_view: bool,
    /// The node last clicked in the graph; its detail shows in graph view.
    selected: Option<String>,
    /// `(graph_view, light)` the List / Graph buttons were last styled for.
    view_buttons: Option<(bool, f64)>,
}

struct EntityRow {
    name: String,
    toggle: CheckBoxRef,
    count: LabelRef,
}

impl DvizViewPanel {
    pub fn create(vm: &mut ScriptVm, session: &'static DvizSession) -> Self {
        let value = script_eval!(vm, {
            use mod.prelude.widgets.*
            use mod.widgets.*
            View{width: Fill height: Fill flow: Down padding: 16.0 spacing: 12.0
                Label{text: "Visualization"
                    draw_text +: {text_style: mod.widgets.ux.TEXT_TITLE{}}}
                connection := mod.widgets.ux.Card{width: Fill height: Fit flow: Down padding: 14.0 spacing: 8.0
                    View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                        Label{text: "Mode"}
                        mode := TextInput{width: 90 height: 36 min_height: 36 text: "peer"}
                        Label{text: "Endpoint"}
                        endpoint := TextInput{width: Fill height: 36 min_height: 36
                            empty_text: "tcp/192.168.1.186:7447 · empty for multicast discovery"}
                        Label{text: "Prefix"}
                        prefix := TextInput{width: 110 height: 36 min_height: 36 text: "dviz"}
                    }
                    View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                        connect := mod.widgets.ux.PrimaryButton{text: "Connect"}
                        disconnect := mod.widgets.ux.ToolButton{text: "Disconnect"}
                        open_viewer := mod.widgets.ux.ToolButton{text: "Open 3D window"}
                    }
                    // State, hint or error, 3D window; room for one to wrap.
                    status := Label{width: Fill max_lines: 6 text: ""}
                }
                displays := mod.widgets.ux.Card{width: Fill height: Fit flow: Down padding: 14.0 spacing: 6.0
                    Label{text: "Displays"}
                    no_entities := Label{text: "No displays yet"}
                    entities := View{width: Fill height: Fit flow: Down spacing: 4.0}
                }
                View{width: Fill height: Fill flow: Right spacing: 12.0
                    log_card := mod.widgets.ux.Card{width: Fill height: Fill flow: Down padding: 12.0 spacing: 6.0
                        View{width: Fill height: Fit flow: Right spacing: 12.0 align: Align{y: 0.5}
                            Label{text: "Logs"}
                            log_level := CheckBox{text: "WARN and above"}
                        }
                        logs := Label{width: Fill max_lines: 12 text: ""}
                    }
                    node_card := mod.widgets.ux.Card{width: Fill height: Fill flow: Down padding: 12.0 spacing: 6.0
                        View{width: Fill height: Fit flow: Right spacing: 6.0 align: Align{y: 0.5}
                            Label{text: "Nodes"}
                            node_list := mod.widgets.ux.SegmentButton{text: "List"}
                            node_graph := mod.widgets.ux.SegmentButton{text: "Graph"}
                        }
                        nodes := Label{width: Fill max_lines: 12 text: ""}
                        // Before the graph: the graph fills what is left.
                        node_detail := Label{width: Fill max_lines: 6 text: "" visible: false}
                        graph := mod.widgets.DvizGraph{visible: false}
                    }
                }
            }
        });
        Self {
            root: WidgetRef::script_from_value(vm, value),
            session,
            rows: Vec::new(),
            warn_only: false,
            light: None,
            graph_view: false,
            selected: None,
            view_buttons: None,
        }
    }

    /// Redraws everything from a fresh snapshot.
    fn refresh(&mut self, cx: &mut Cx) {
        let snapshot = self.session.snapshot();
        self.root
            .label(cx, ids!(status))
            .set_text(cx, &status_text(&snapshot));
        self.sync_entities(cx, &snapshot.entities);
        self.root
            .label(cx, ids!(logs))
            .set_text(cx, &log_text(&snapshot.logs, self.warn_only, LOG_LINES));
        self.root
            .label(cx, ids!(nodes))
            .set_text(cx, &node_text(&snapshot.nodes, NODE_LINES));
        self.sync_node_view(cx, &snapshot);
    }

    /// Shows the list or the graph, and in graph view the selected node's
    /// detail. The graph is only fed while it is shown.
    fn sync_node_view(&mut self, cx: &mut Cx, snapshot: &SessionSnapshot) {
        let light = self.light.unwrap_or_else(frame::light_mode);
        if self.view_buttons != Some((self.graph_view, light)) {
            for (path, on) in [
                (ids!(node_list), !self.graph_view),
                (ids!(node_graph), self.graph_view),
            ] {
                frame::sync_nav_button(cx, &self.root.widget(cx, path), on, light);
            }
            self.view_buttons = Some((self.graph_view, light));
        }
        self.root
            .widget(cx, ids!(nodes))
            .set_visible(cx, !self.graph_view);
        self.root
            .widget(cx, ids!(graph))
            .set_visible(cx, self.graph_view);
        let detail = match (&self.selected, self.graph_view) {
            (Some(id), true) => node_detail(&snapshot.nodes, &snapshot.edges, id),
            _ => None,
        };
        let label = self.root.widget(cx, ids!(node_detail));
        label.set_text(cx, detail.as_deref().unwrap_or(""));
        label.set_visible(cx, detail.is_some());
        if self.graph_view {
            self.root.dviz_graph(cx, ids!(graph)).set(
                cx,
                &snapshot.nodes,
                &snapshot.edges,
                self.selected.as_deref(),
                light as f32,
            );
        }
    }

    fn sync_entities(&mut self, cx: &mut Cx, entities: &BTreeMap<String, EntityInfo>) {
        if !self.rows.iter().map(|row| &row.name).eq(entities.keys()) {
            self.rebuild_rows(cx, entities.keys());
        }
        for (row, info) in self.rows.iter().zip(entities.values()) {
            row.count.set_text(cx, &entity_text(info));
            if row.toggle.active(cx) != info.enabled {
                row.toggle.set_active(cx, info.enabled, Animate::No);
            }
        }
        self.root
            .widget(cx, ids!(no_entities))
            .set_visible(cx, self.rows.is_empty());
    }

    /// Replaces the rows; only runs when the set of entities changes.
    fn rebuild_rows<'a>(&mut self, cx: &mut Cx, names: impl Iterator<Item = &'a String>) {
        let container = self.root.widget(cx, ids!(entities));
        let mut rows = Vec::new();
        let mut children = Vec::new();
        for name in names {
            let row = cx.with_vm(|vm| {
                let value = script_eval!(vm, {
                    use mod.prelude.widgets.*
                    use mod.widgets.*
                    View{width: Fill height: Fit flow: Right spacing: 12.0 align: Align{y: 0.5}
                        toggle := Toggle{text: ""}
                        count := Label{text: ""}
                    }
                });
                WidgetRef::script_from_value(vm, value)
            });
            let toggle = row.check_box(cx, ids!(toggle));
            toggle.set_text(name);
            rows.push(EntityRow {
                name: name.clone(),
                toggle,
                count: row.label(cx, ids!(count)),
            });
            children.push((LiveId::from_str(name), row));
        }
        if let Some(mut view) = container.borrow_mut::<View>() {
            view.children.clear();
            view.children.extend(children);
        }
        cx.widget_tree_mark_dirty(container.widget_uid());
        container.redraw(cx);
        self.rows = rows;
    }
}

impl PluginPanel for DvizViewPanel {
    fn root(&self) -> WidgetRef {
        self.root.clone()
    }

    fn sync(&mut self, cx: &mut Cx, _: &PanelContext<'_>) {
        let light = frame::light_mode();
        if self.light != Some(light) {
            frame::apply_light(
                cx,
                &self.root,
                &[
                    (ids!(connection), Themed::Bg),
                    (ids!(displays), Themed::Bg),
                    (ids!(log_card), Themed::Bg),
                    (ids!(node_card), Themed::Bg),
                    (ids!(connect), Themed::Both),
                    (ids!(disconnect), Themed::Both),
                    (ids!(open_viewer), Themed::Both),
                ],
                light,
            );
            self.light = Some(light);
        }
        self.refresh(cx);
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions, _: &mut dyn Backend) -> bool {
        if self.root.button(cx, ids!(connect)).clicked(actions) {
            let options = session_options(
                &self.root.text_input(cx, ids!(mode)).text(),
                &self.root.text_input(cx, ids!(endpoint)).text(),
                &self.root.text_input(cx, ids!(prefix)).text(),
            );
            // Returns at once; this error is also in the snapshot's
            // `last_error`, which the status line shows.
            let _ = self.session.connect(options);
        } else if self.root.button(cx, ids!(disconnect)).clicked(actions) {
            self.session.disconnect();
        } else if self.root.button(cx, ids!(open_viewer)).clicked(actions) {
            // Returns at once; the launch reports through the snapshot's
            // `viewer`, which the status line shows. An error here means a
            // launch is already running, and the status says "starting".
            let _ = open_viewer(self.session);
        } else if let Some(warn_only) = self.root.check_box(cx, ids!(log_level)).changed(actions) {
            self.warn_only = warn_only;
        } else if self.root.button(cx, ids!(node_list)).clicked(actions) {
            self.graph_view = false;
        } else if self.root.button(cx, ids!(node_graph)).clicked(actions) {
            self.graph_view = true;
        } else if let Some(id) = self.root.dviz_graph(cx, ids!(graph)).clicked_node(actions) {
            self.selected = Some(id);
        } else if let Some((name, enabled)) = self
            .rows
            .iter()
            .find_map(|row| Some((&row.name, row.toggle.changed(actions)?)))
        {
            self.session.set_entity_enabled(name, enabled);
        } else {
            return false;
        }
        self.refresh(cx);
        true
    }

    /// Studio calls this whenever the panel is hidden, including a switch to
    /// another panel, so it must not disconnect: the session keeps feeding
    /// the 3D window. The panel's only refresh is `sync`, which the host stops
    /// calling while the panel is hidden, so there is nothing else to stop.
    fn on_disable(&mut self, _: &mut Cx) {}
}

/// Session options from the panel's inputs. Endpoints may be separated by
/// commas or spaces; none means multicast discovery in peer mode. An empty
/// mode or prefix falls back to the default (`peer`, `dviz`).
fn session_options(mode: &str, endpoints: &str, prefix: &str) -> SessionOptions {
    let defaults = SessionOptions::default();
    let or_default = |value: &str, default: String| match value.trim() {
        "" => default,
        value => value.to_string(),
    };
    SessionOptions {
        mode: or_default(mode, defaults.mode),
        connect_endpoints: endpoints
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|endpoint| !endpoint.is_empty())
            .map(str::to_string)
            .collect(),
        listen_endpoints: Vec::new(),
        prefix: or_default(prefix, defaults.prefix),
    }
}

/// Connection state and counters, the no-peer hint once the session has
/// listened that long, any session error, then the 3D window's launch.
fn status_text(snapshot: &SessionSnapshot) -> String {
    let state = if snapshot.connected {
        "Connected"
    } else if snapshot.listening_for.is_some() {
        "Listening, no peer found yet"
    } else {
        "Not connected"
    };
    let mut lines = vec![format!(
        "{state} · received {} · dropped {} · malformed {}",
        snapshot.received, snapshot.dropped, snapshot.malformed
    )];
    if snapshot
        .listening_for
        .is_some_and(|waited| waited >= NO_PEER_HINT_AFTER)
    {
        lines.push("Hint: no dviz peer found by multicast; enter the robot's endpoint".into());
    }
    if let Some(error) = &snapshot.last_error {
        lines.push(format!("Error: {error}"));
    }
    match &snapshot.viewer {
        ViewerState::Closed => {}
        ViewerState::Starting => lines.push("3D window: starting…".into()),
        ViewerState::Open(viewer) => lines.push(format!("3D window: open ({viewer})")),
        ViewerState::Failed(error) => lines.push(format!("3D window: {error}")),
    }
    lines.join("\n")
}

/// The newest `limit` entries (only WARN and above if `warn_only`), newest
/// first, each cut to one line of at most `LOG_LINE_CHARS` characters. Newest
/// first so that when the label runs out of lines the oldest are cut.
fn log_text(logs: &[LogEntry], warn_only: bool, limit: usize) -> String {
    let shown: Vec<_> = logs
        .iter()
        .rev()
        .filter(|entry| !warn_only || entry.level >= LogLevel::Warn)
        .take(limit)
        .map(|entry| {
            one_line(
                &format!("{} {}: {}", entry.level, entry.node_id, entry.message),
                LOG_LINE_CHARS,
            )
        })
        .collect();
    if shown.is_empty() {
        return "No log messages".to_string();
    }
    shown.join("\n")
}

/// `text` with every control character (newlines, tabs) as a space, cut to
/// `max_chars` characters with a trailing "…" when it is longer.
fn one_line(text: &str, max_chars: usize) -> String {
    let flat = text.chars().map(|c| if c.is_control() { ' ' } else { c });
    if text.chars().count() <= max_chars {
        return flat.collect();
    }
    let mut cut: String = flat.take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// One line per node, up to `limit`.
fn node_text(nodes: &[GraphNode], limit: usize) -> String {
    if nodes.is_empty() {
        return "No nodes".to_string();
    }
    nodes
        .iter()
        .take(limit)
        .map(node_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Name, status and last activity. `last_seen` is the sender's clock
/// (seconds since its dataflow started), not an age, so it is shown as a time.
fn node_line(node: &GraphNode) -> String {
    let status = match node.status {
        GraphNodeStatus::Active => "active",
        GraphNodeStatus::Idle => "idle",
        GraphNodeStatus::Error => "error",
    };
    format!("{} · {status} · seen at t={:.1} s", node.id, node.last_seen)
}

/// The node's line, then its inputs (`port ← node/output`) and outputs
/// (`output → node/port`); `None` when `id` is not among `nodes`.
fn node_detail(nodes: &[GraphNode], edges: &[GraphEdge], id: &str) -> Option<String> {
    let node = nodes.iter().find(|node| node.id == id)?;
    let list = |ports: Vec<String>| {
        if ports.is_empty() {
            "none".to_string()
        } else {
            ports.join(", ")
        }
    };
    let inputs = edges
        .iter()
        .filter(|edge| edge.to_node == id)
        .map(|edge| format!("{} ← {}/{}", edge.to_port, edge.from_node, edge.from_port))
        .collect();
    let outputs = edges
        .iter()
        .filter(|edge| edge.from_node == id)
        .map(|edge| format!("{} → {}/{}", edge.from_port, edge.to_node, edge.to_port))
        .collect();
    Some(format!(
        "{}\nInputs: {}\nOutputs: {}",
        node_line(node),
        list(inputs),
        list(outputs)
    ))
}

fn entity_text(info: &EntityInfo) -> String {
    if info.msg_type.is_empty() {
        info.count.to_string()
    } else {
        format!("{} · {}", info.msg_type, info.count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: LogLevel, message: &str) -> LogEntry {
        LogEntry::new(level, message, "planner")
    }

    #[test]
    fn options_come_from_the_inputs() {
        let options = session_options(" client ", "tcp/10.0.0.2:7447", " robot1 ");
        assert_eq!(
            options,
            SessionOptions {
                mode: "client".into(),
                connect_endpoints: vec!["tcp/10.0.0.2:7447".into()],
                listen_endpoints: Vec::new(),
                prefix: "robot1".into(),
            }
        );
    }

    #[test]
    fn empty_inputs_mean_peer_multicast_and_the_default_prefix() {
        assert_eq!(session_options("", "  ", ""), SessionOptions::default());
    }

    #[test]
    fn several_endpoints_split_on_commas_and_spaces() {
        let options = session_options("peer", "tcp/a:1, tcp/b:2  tcp/c:3,", "dviz");
        assert_eq!(options.connect_endpoints, ["tcp/a:1", "tcp/b:2", "tcp/c:3"]);
    }

    #[test]
    fn status_shows_state_counters_and_errors() {
        let mut snapshot = SessionSnapshot {
            connected: true,
            received: 7,
            dropped: 2,
            malformed: 1,
            ..Default::default()
        };
        assert_eq!(
            status_text(&snapshot),
            "Connected · received 7 · dropped 2 · malformed 1"
        );
        snapshot.connected = false;
        snapshot.last_error = Some("no route".into());
        snapshot.viewer = ViewerState::Failed("no viewer".into());
        assert_eq!(
            status_text(&snapshot),
            "Not connected · received 7 · dropped 2 · malformed 1\n\
             Error: no route\n\
             3D window: no viewer"
        );
    }

    #[test]
    fn status_shows_the_3d_window_launch() {
        let mut snapshot = SessionSnapshot::default();
        let first = "Not connected · received 0 · dropped 0 · malformed 0";
        assert_eq!(status_text(&snapshot), first);
        snapshot.viewer = ViewerState::Starting;
        assert_eq!(
            status_text(&snapshot),
            format!("{first}\n3D window: starting…")
        );
        snapshot.viewer = ViewerState::Open("Rerun 0.28.2".into());
        assert_eq!(
            status_text(&snapshot),
            format!("{first}\n3D window: open (Rerun 0.28.2)")
        );
    }

    #[test]
    fn status_shows_listening_and_the_no_peer_hint_after_two_seconds() {
        let mut snapshot = SessionSnapshot {
            listening_for: Some(Duration::from_millis(500)),
            ..Default::default()
        };
        assert_eq!(
            status_text(&snapshot),
            "Listening, no peer found yet · received 0 · dropped 0 · malformed 0"
        );
        snapshot.listening_for = Some(Duration::from_secs(2));
        assert_eq!(
            status_text(&snapshot),
            "Listening, no peer found yet · received 0 · dropped 0 · malformed 0\n\
             Hint: no dviz peer found by multicast; enter the robot's endpoint"
        );
        // A link ends the waiting.
        snapshot.listening_for = None;
        snapshot.connected = true;
        assert_eq!(
            status_text(&snapshot),
            "Connected · received 0 · dropped 0 · malformed 0"
        );
    }

    #[test]
    fn log_filter_keeps_the_newest_lines_newest_first() {
        let logs = [
            entry(LogLevel::Error, "first"),
            entry(LogLevel::Info, "second"),
            entry(LogLevel::Warn, "third"),
            entry(LogLevel::Debug, "fourth\nmore\r\tend"),
        ];
        // Newest on top: if the label cuts lines, the oldest go first.
        assert_eq!(
            log_text(&logs, false, 2),
            "DEBUG planner: fourth more  end\nWARN planner: third"
        );
        assert_eq!(
            log_text(&logs, true, 10),
            "WARN planner: third\nERROR planner: first"
        );
        assert_eq!(log_text(&logs[1..2], true, 10), "No log messages");
    }

    #[test]
    fn long_log_entries_are_cut_to_one_line() {
        let long = "x".repeat(300);
        let text = log_text(&[entry(LogLevel::Info, &long)], false, 10);
        assert_eq!(text.chars().count(), LOG_LINE_CHARS, "{text}");
        assert!(text.starts_with("INFO planner: xxx"), "{text}");
        assert!(text.ends_with("x…"), "{text}");

        // Characters, not bytes: multi-byte text is cut without a panic.
        let wide = "点云".repeat(200);
        let text = log_text(&[entry(LogLevel::Warn, &wide)], false, 10);
        assert_eq!(text.chars().count(), LOG_LINE_CHARS, "{text}");
        assert!(text.ends_with('…'), "{text}");

        // Exactly at the limit: kept whole.
        let prefix = "INFO planner: ";
        let fits = "y".repeat(LOG_LINE_CHARS - prefix.chars().count());
        let text = log_text(&[entry(LogLevel::Info, &fits)], false, 10);
        assert_eq!(text, format!("{prefix}{fits}"));
    }

    #[test]
    fn nodes_show_name_status_and_last_activity() {
        let nodes = [
            GraphNode {
                id: "planner".into(),
                status: GraphNodeStatus::Active,
                last_seen: 12.34,
            },
            GraphNode {
                id: "lidar".into(),
                status: GraphNodeStatus::Error,
                last_seen: 3.0,
            },
        ];
        // `last_seen` is the sender's clock (seconds since its start), not an age.
        assert_eq!(
            node_text(&nodes, 10),
            "planner · active · seen at t=12.3 s\nlidar · error · seen at t=3.0 s"
        );
        assert_eq!(node_text(&nodes, 1), "planner · active · seen at t=12.3 s");
        assert_eq!(node_text(&[], 10), "No nodes");
    }

    #[test]
    fn node_detail_lists_status_inputs_and_outputs() {
        let node = |id: &str, status| GraphNode {
            id: id.into(),
            status,
            last_seen: 2.0,
        };
        let edge = |from_node: &str, from_port: &str, to_node: &str, to_port: &str| GraphEdge {
            from_node: from_node.into(),
            from_port: from_port.into(),
            to_node: to_node.into(),
            to_port: to_port.into(),
        };
        let nodes = [
            node("camera", GraphNodeStatus::Idle),
            node("detector", GraphNodeStatus::Error),
        ];
        let edges = [
            edge("camera", "image", "detector", "frame"),
            edge("camera", "image", "recorder", "video"),
            edge("dora", "timer/millis/50", "camera", "tick"),
        ];
        assert_eq!(
            node_detail(&nodes, &edges, "camera").as_deref(),
            Some(
                "camera · idle · seen at t=2.0 s\n\
                 Inputs: tick ← dora/timer/millis/50\n\
                 Outputs: image → detector/frame, image → recorder/video"
            )
        );
        assert_eq!(
            node_detail(&nodes, &edges, "detector").as_deref(),
            Some(
                "detector · error · seen at t=2.0 s\n\
                 Inputs: frame ← camera/image\n\
                 Outputs: none"
            )
        );
        // `recorder` only appears in an edge.
        assert_eq!(node_detail(&nodes, &edges, "recorder"), None);
    }

    #[test]
    fn entity_text_shows_type_and_count() {
        let mut info = EntityInfo {
            enabled: true,
            count: 3,
            msg_type: String::new(),
        };
        assert_eq!(entity_text(&info), "3");
        info.msg_type = "points3d".into();
        assert_eq!(entity_text(&info), "points3d · 3");
    }
}
