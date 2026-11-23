//! The `dviz_view` panel: connection, the 3D window, the display list, logs
//! and nodes.
//!
//! The panel only holds a handle to the process-wide [`DvizSession`]; every
//! `sync` reads a fresh snapshot of it. Nothing here waits on the session:
//! `connect` and `disconnect` return at once and report through the snapshot.

use crate::{open_viewer, DvizSession, EntityInfo, SessionOptions, SessionSnapshot};
use dorobot_studio_core::{
    api::Backend,
    ui::frame::{self, Themed},
};
use dorobot_studio_plugin::*;
use dviz_protocol::{GraphNode, GraphNodeStatus, LogEntry, LogLevel};
use std::collections::BTreeMap;

/// Newest log lines shown; the `logs` label holds this many.
const LOG_LINES: usize = 12;
/// Nodes listed; the `nodes` label holds this many.
const NODE_LINES: usize = 12;

pub struct DvizViewPanel {
    root: WidgetRef,
    session: &'static DvizSession,
    /// One row per entity, in the snapshot's (sorted) order.
    rows: Vec<EntityRow>,
    /// Why the last "Open 3D window" failed, until it succeeds.
    viewer_error: Option<String>,
    warn_only: bool,
    light: Option<f64>,
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
                    status := Label{width: Fill max_lines: 4 text: ""}
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
                        Label{text: "Nodes"}
                        nodes := Label{width: Fill max_lines: 12 text: ""}
                    }
                }
            }
        });
        Self {
            root: WidgetRef::script_from_value(vm, value),
            session,
            rows: Vec::new(),
            viewer_error: None,
            warn_only: false,
            light: None,
        }
    }

    /// Redraws everything from a fresh snapshot.
    fn refresh(&mut self, cx: &mut Cx) {
        let snapshot = self.session.snapshot();
        self.root
            .label(cx, ids!(status))
            .set_text(cx, &status_text(&snapshot, self.viewer_error.as_deref()));
        self.sync_entities(cx, &snapshot.entities);
        self.root
            .label(cx, ids!(logs))
            .set_text(cx, &log_text(&snapshot.logs, self.warn_only, LOG_LINES));
        self.root
            .label(cx, ids!(nodes))
            .set_text(cx, &node_text(&snapshot.nodes, NODE_LINES));
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
            self.viewer_error = open_viewer(self.session).err();
        } else if let Some(warn_only) = self.root.check_box(cx, ids!(log_level)).changed(actions) {
            self.warn_only = warn_only;
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

/// Connection state and counters, then any session or 3D window error.
fn status_text(snapshot: &SessionSnapshot, viewer_error: Option<&str>) -> String {
    let state = if snapshot.connected {
        "Connected"
    } else {
        "Not connected"
    };
    let mut lines = vec![format!(
        "{state} · received {} · dropped {} · malformed {}",
        snapshot.received, snapshot.dropped, snapshot.malformed
    )];
    if let Some(error) = &snapshot.last_error {
        lines.push(format!("Error: {error}"));
    }
    if let Some(error) = viewer_error {
        lines.push(format!("3D window: {error}"));
    }
    lines.join("\n")
}

/// The newest `limit` entries (only WARN and above if `warn_only`), oldest
/// first, one line each.
fn log_text(logs: &[LogEntry], warn_only: bool, limit: usize) -> String {
    let shown: Vec<_> = logs
        .iter()
        .rev()
        .filter(|entry| !warn_only || entry.level >= LogLevel::Warn)
        .take(limit)
        .collect();
    if shown.is_empty() {
        return "No log messages".to_string();
    }
    shown
        .iter()
        .rev()
        .map(|entry| {
            format!(
                "{} {}: {}",
                entry.level,
                entry.node_id,
                entry.message.replace('\n', " ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Name, status and last activity (seconds since the dataflow started).
fn node_text(nodes: &[GraphNode], limit: usize) -> String {
    if nodes.is_empty() {
        return "No nodes".to_string();
    }
    nodes
        .iter()
        .take(limit)
        .map(|node| {
            let status = match node.status {
                GraphNodeStatus::Active => "active",
                GraphNodeStatus::Idle => "idle",
                GraphNodeStatus::Error => "error",
            };
            format!("{} · {status} · last seen {:.1} s", node.id, node.last_seen)
        })
        .collect::<Vec<_>>()
        .join("\n")
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
            status_text(&snapshot, None),
            "Connected · received 7 · dropped 2 · malformed 1"
        );
        snapshot.connected = false;
        snapshot.last_error = Some("no route".into());
        assert_eq!(
            status_text(&snapshot, Some("no viewer")),
            "Not connected · received 7 · dropped 2 · malformed 1\n\
             Error: no route\n\
             3D window: no viewer"
        );
    }

    #[test]
    fn log_filter_keeps_the_newest_lines_in_order() {
        let logs = [
            entry(LogLevel::Error, "first"),
            entry(LogLevel::Info, "second"),
            entry(LogLevel::Warn, "third"),
            entry(LogLevel::Debug, "fourth\nmore"),
        ];
        assert_eq!(
            log_text(&logs, false, 2),
            "WARN planner: third\nDEBUG planner: fourth more"
        );
        assert_eq!(
            log_text(&logs, true, 10),
            "ERROR planner: first\nWARN planner: third"
        );
        assert_eq!(log_text(&logs[1..2], true, 10), "No log messages");
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
        assert_eq!(
            node_text(&nodes, 10),
            "planner · active · last seen 12.3 s\nlidar · error · last seen 3.0 s"
        );
        assert_eq!(node_text(&nodes, 1), "planner · active · last seen 12.3 s");
        assert_eq!(node_text(&[], 10), "No nodes");
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
