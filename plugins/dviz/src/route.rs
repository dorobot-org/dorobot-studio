//! Routing of one dviz payload into the session state and the visual sink.

use crate::nav_scene::{nav_layer, NavLayer, NavScene};
use dviz_protocol::{
    parse_message, GraphEdge, GraphNode, GraphUpdate, LogData, LogEntry, LogLevel, MvizMessage,
    NodeDefinition,
};
use std::{
    collections::{BTreeMap, VecDeque},
    mem,
};

/// Most log entries kept; older ones are pushed out.
pub const LOG_CAP: usize = 500;
/// Longest log text kept, in bytes; longer text is cut when it is stored.
pub const LOG_TEXT_CAP: usize = 2048;

/// Where visual messages (everything except logs and graph updates) go.
pub trait Sink: Send {
    fn log(&mut self, entity: &str, msg: &MvizMessage, binary: Option<&[u8]>)
        -> Result<(), String>;

    /// Removes what is shown for `entity` and everything below it. Called
    /// when the entity is switched off, on the session's worker thread like
    /// `log` (never on the UI thread).
    fn clear(&mut self, _entity: &str) -> Result<(), String> {
        Ok(())
    }
}

/// Sink work that does not come with a received payload. `SessionState`
/// queues it; the session's worker hands it to the sink before the next
/// message, so the UI thread never calls the sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SinkAction {
    /// `Sink::clear` of this entity.
    Clear(String),
}

impl SinkAction {
    pub(crate) fn apply(&self, sink: &mut dyn Sink) -> Result<(), String> {
        match self {
            SinkAction::Clear(entity) => sink.clear(entity),
        }
    }
}

/// Records `(entity, msg_type)` for every `log` and the entity of every
/// `clear`; for tests. It keeps every call, so the session's default sink
/// is a discarding one instead.
#[derive(Debug, Default)]
pub struct CountingSink {
    pub calls: Vec<(String, String)>,
    pub clears: Vec<String>,
}

impl Sink for CountingSink {
    fn log(&mut self, entity: &str, msg: &MvizMessage, _: Option<&[u8]>) -> Result<(), String> {
        self.calls.push((entity.to_string(), msg.msg_type.clone()));
        Ok(())
    }

    fn clear(&mut self, entity: &str) -> Result<(), String> {
        self.clears.push(entity.to_string());
        Ok(())
    }
}

/// One visual entity (topic without the prefix) seen on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityInfo {
    pub enabled: bool,
    pub count: u64,
    pub msg_type: String,
}

impl EntityInfo {
    /// A newly seen entity: on, except the PCD map (`map/cloud`), which is
    /// large and only sent once someone switches it on.
    fn new(entity: &str) -> Self {
        Self {
            enabled: nav_layer(entity) != Some(NavLayer::MapCloud),
            count: 0,
            msg_type: String::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct SessionState {
    /// The newest `LOG_CAP` log entries, oldest first.
    pub logs: VecDeque<LogEntry>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub entities: BTreeMap<String, EntityInfo>,
    /// Payloads evicted, oldest first, from the full receive queue.
    pub dropped: u64,
    /// Payloads that could not be parsed, or were refused as too large.
    pub malformed: u64,
    /// Visual messages and clears the sink rejected or panicked on.
    pub sink_errors: u64,
    /// Payloads routed.
    pub received: u64,
    /// The latest navigation layers.
    pub nav: NavScene,
    /// Sink work waiting for the next routing, oldest first; at most one
    /// clear per entity.
    pub(crate) sink_actions: VecDeque<SinkAction>,
}

impl SessionState {
    /// Turns sink delivery for `entity` on or off; counting goes on either
    /// way. Switching it off also queues a clear of what the sink shows for
    /// it, which the next routing (the session's worker) carries out.
    pub fn set_entity_enabled(&mut self, entity: &str, enabled: bool) {
        let info = self
            .entities
            .entry(entity.to_string())
            .or_insert_with(|| EntityInfo::new(entity));
        let was_enabled = mem::replace(&mut info.enabled, enabled);
        let clear = SinkAction::Clear(entity.to_string());
        if was_enabled && !enabled && !self.sink_actions.contains(&clear) {
            self.sink_actions.push_back(clear);
        }
    }

    /// The queued sink work, oldest first, leaving none queued.
    pub(crate) fn take_sink_actions(&mut self) -> VecDeque<SinkAction> {
        mem::take(&mut self.sink_actions)
    }
}

/// Routes one payload received on `topic`: first the queued sink work (such
/// as clears of entities switched off), then logs and graph messages update
/// `state`; any other type registers its entity and, if that entity is
/// enabled, goes to `sink`.
pub fn route(
    state: &mut SessionState,
    sink: &mut dyn Sink,
    prefix: &str,
    topic: &str,
    payload: &[u8],
) {
    for action in state.take_sink_actions() {
        if action.apply(sink).is_err() {
            state.sink_errors += 1;
        }
    }
    if let Some(visual) = accept(state, prefix, topic, payload) {
        if sink
            .log(&visual.entity, &visual.msg, visual.binary)
            .is_err()
        {
            state.sink_errors += 1;
        }
    }
}

/// A visual message `accept` hands on to the sink.
pub(crate) struct Visual<'a> {
    pub entity: String,
    pub msg: MvizMessage,
    pub binary: Option<&'a [u8]>,
}

/// The part of `route` that touches only `state`, so the session can drop the
/// state lock before the sink call. A sink error must then be counted in
/// `sink_errors` by the caller.
pub(crate) fn accept<'a>(
    state: &mut SessionState,
    prefix: &str,
    topic: &str,
    payload: &'a [u8],
) -> Option<Visual<'a>> {
    state.received += 1;
    let Some((msg, binary)) = parse_message(payload) else {
        state.malformed += 1;
        return None;
    };
    let understood = match msg.msg_type.as_str() {
        "log" => serde_json::from_value(msg.data)
            .map(|data| push_log(state, data, msg.timestamp))
            .is_ok(),
        "node_definition" => serde_json::from_value(msg.data)
            .map(|definition| add_node_definition(state, definition, msg.timestamp))
            .is_ok(),
        "graph_update" => serde_json::from_value::<GraphUpdate>(msg.data)
            .map(|update| {
                state.nodes = update.nodes;
                state.edges = update.edges;
            })
            .is_ok(),
        _ => {
            let entity = topic
                .strip_prefix(prefix)
                .and_then(|rest| rest.strip_prefix('/'))
                .unwrap_or(topic);
            let info = state
                .entities
                .entry(entity.to_string())
                .or_insert_with(|| EntityInfo::new(entity));
            info.count += 1;
            if info.msg_type != msg.msg_type {
                info.msg_type.clone_from(&msg.msg_type);
            }
            let enabled = info.enabled;
            if let Some(layer) = nav_layer(entity) {
                match state.nav.try_apply(layer, &msg, binary) {
                    Ok(true) => {}
                    Ok(false) => return None,
                    Err(()) => {
                        state.malformed += 1;
                        return None;
                    }
                }
            }
            return enabled.then(|| Visual {
                entity: entity.to_string(),
                msg,
                binary,
            });
        }
    };
    if !understood {
        state.malformed += 1;
    }
    None
}

fn push_log(state: &mut SessionState, data: LogData, timestamp: Option<f64>) {
    let mut metadata = data.metadata.unwrap_or_default();
    if let Some(port) = data.port {
        metadata.insert("port".to_string(), port);
    }
    if let Some(port_type) = data.port_type {
        metadata.insert("port_type".to_string(), port_type);
    }
    state.logs.push_back(LogEntry {
        level: LogLevel::from_str(&data.level),
        message: truncate(data.message, LOG_TEXT_CAP),
        node_id: data.node_id,
        timestamp: timestamp.unwrap_or(0.0),
        metadata,
    });
    while state.logs.len() > LOG_CAP {
        state.logs.pop_front();
    }
}

/// `text` cut to at most `max` bytes, on a character boundary, keeping no
/// more memory than it needs.
fn truncate(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.shrink_to_fit();
    }
    text
}

/// Adds the node if it is new and replaces its incoming edges with the ones
/// its inputs name (`source` = `node/output`).
fn add_node_definition(state: &mut SessionState, definition: NodeDefinition, at: Option<f64>) {
    if !state.nodes.iter().any(|node| node.id == definition.id) {
        state.nodes.push(GraphNode {
            id: definition.id.clone(),
            status: Default::default(),
            last_seen: at.unwrap_or(0.0),
        });
    }
    state.edges.retain(|edge| edge.to_node != definition.id);
    for input in definition.inputs {
        if let Some((from_node, from_port)) = input.source.split_once('/') {
            state.edges.push(GraphEdge {
                from_node: from_node.to_string(),
                from_port: from_port.to_string(),
                to_node: definition.id.clone(),
                to_port: input.name,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dviz_protocol::{binary_formats, serialize_message};
    use serde_json::{json, Value};

    fn message(msg_type: &str, data: Value, binary: Option<&[u8]>) -> Vec<u8> {
        let header = MvizMessage {
            msg_type: msg_type.to_string(),
            timestamp: Some(1.5),
            data,
            format: binary.map(|_| binary_formats::POINTS_XYZ_F32.to_string()),
            count: binary.map(|b| (b.len() / 12) as u32),
        };
        serialize_message(&header, binary)
    }

    fn log_line(n: usize) -> Vec<u8> {
        let data = json!({"level": "WARN", "message": format!("line {n}"), "node_id": "planner"});
        message("log", data, None)
    }

    fn points() -> Vec<u8> {
        let xyz: Vec<u8> = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        message("points3d", Value::Null, Some(&xyz))
    }

    fn graph(nodes: &[&str], edges: &[(&str, &str, &str, &str)]) -> Vec<u8> {
        let nodes: Vec<Value> = nodes
            .iter()
            .map(|id| json!({"id": id, "status": "active", "last_seen": 2.0}))
            .collect();
        let edges: Vec<Value> = edges
            .iter()
            .map(|(from_node, from_port, to_node, to_port)| {
                json!({"from_node": from_node, "from_port": from_port,
                       "to_node": to_node, "to_port": to_port})
            })
            .collect();
        message(
            "graph_update",
            json!({"nodes": nodes, "edges": edges, "timestamp": 2.0}),
            None,
        )
    }

    fn calls(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(entity, kind)| (entity.to_string(), kind.to_string()))
            .collect()
    }

    struct FailingSink;

    impl Sink for FailingSink {
        fn log(&mut self, _: &str, _: &MvizMessage, _: Option<&[u8]>) -> Result<(), String> {
            Err("bad points".into())
        }

        fn clear(&mut self, _: &str) -> Result<(), String> {
            Err("cannot clear".into())
        }
    }

    #[test]
    fn log_goes_to_ring_and_is_capped() {
        let mut state = SessionState::default();
        let mut sink = CountingSink::default();
        for n in 0..=LOG_CAP {
            route(&mut state, &mut sink, "dviz", "dviz/logs", &log_line(n));
        }
        assert_eq!(state.logs.len(), LOG_CAP);
        let oldest = state.logs.front().unwrap();
        assert_eq!(oldest.message, "line 1", "line 0 must have been pushed out");
        assert_eq!(oldest.level, LogLevel::Warn);
        assert_eq!(oldest.node_id, "planner");
        assert_eq!(oldest.timestamp, 1.5);
        assert_eq!(
            state.logs.back().unwrap().message,
            format!("line {LOG_CAP}")
        );
        assert!(sink.calls.is_empty());
        assert!(state.entities.is_empty());
        assert_eq!(state.received, LOG_CAP as u64 + 1);
        assert_eq!(state.malformed, 0);
    }

    #[test]
    fn points_reach_sink_and_register_entity() {
        let mut state = SessionState::default();
        let mut sink = CountingSink::default();
        route(&mut state, &mut sink, "dviz", "dviz/world/lidar", &points());
        assert_eq!(sink.calls, calls(&[("world/lidar", "points3d")]));
        let lidar = &state.entities["world/lidar"];
        assert!(lidar.enabled);
        assert_eq!(lidar.count, 1);
        assert_eq!(lidar.msg_type, "points3d");
        assert_eq!(state.received, 1);
    }

    #[test]
    fn disabled_entity_skips_sink() {
        let mut state = SessionState::default();
        let mut sink = CountingSink::default();
        route(&mut state, &mut sink, "dviz", "dviz/world/lidar", &points());
        state.set_entity_enabled("world/lidar", false);
        route(&mut state, &mut sink, "dviz", "dviz/world/lidar", &points());
        assert_eq!(sink.calls, calls(&[("world/lidar", "points3d")]));
        assert!(!state.entities["world/lidar"].enabled);
        assert_eq!(state.entities["world/lidar"].count, 2);
        state.set_entity_enabled("world/lidar", true);
        route(&mut state, &mut sink, "dviz", "dviz/world/lidar", &points());
        assert_eq!(sink.calls.len(), 2);
    }

    #[test]
    fn map_cloud_disabled_by_default() {
        for (topic, entity) in [
            ("dviz/tracer/map/cloud", "tracer/map/cloud"),
            ("dviz/map/cloud", "map/cloud"),
        ] {
            let mut state = SessionState::default();
            let mut sink = CountingSink::default();
            route(&mut state, &mut sink, "dviz", topic, &points());
            let info = &state.entities[entity];
            assert!(!info.enabled, "{entity} is on by default");
            assert_eq!(info.count, 1);
            assert!(sink.calls.is_empty(), "{entity} reached the sink");

            // Switched on, it is sent like any other entity.
            state.set_entity_enabled(entity, true);
            route(&mut state, &mut sink, "dviz", topic, &points());
            assert_eq!(sink.calls, calls(&[(entity, "points3d")]));
        }
        // The live cloud stays on.
        let mut state = SessionState::default();
        let mut sink = CountingSink::default();
        route(
            &mut state,
            &mut sink,
            "dviz",
            "dviz/tracer/sensors/cloud",
            &points(),
        );
        assert!(state.entities["tracer/sensors/cloud"].enabled);
    }

    #[test]
    fn switching_off_clears_once_before_the_next_message() {
        let mut state = SessionState::default();
        let mut sink = CountingSink::default();
        route(&mut state, &mut sink, "dviz", "dviz/world/lidar", &points());
        // Twice off is one switch-off; on and off again is another.
        state.set_entity_enabled("world/lidar", false);
        state.set_entity_enabled("world/lidar", false);
        assert!(sink.clears.is_empty(), "cleared before the next routing");
        route(&mut state, &mut sink, "dviz", "dviz/logs", &log_line(0));
        assert_eq!(sink.clears, ["world/lidar"]);
        route(&mut state, &mut sink, "dviz", "dviz/logs", &log_line(1));
        assert_eq!(sink.clears, ["world/lidar"], "cleared again");

        // Switching on clears nothing; the next message is sent again.
        state.set_entity_enabled("world/lidar", true);
        route(&mut state, &mut sink, "dviz", "dviz/world/lidar", &points());
        assert_eq!(sink.clears.len(), 1);
        assert_eq!(sink.calls.len(), 2);

        // A sink that fails to clear is counted like any other sink error.
        state.set_entity_enabled("world/lidar", false);
        route(
            &mut state,
            &mut FailingSink,
            "dviz",
            "dviz/logs",
            &log_line(2),
        );
        assert_eq!(state.sink_errors, 1);
    }

    #[test]
    fn malformed_and_unknown_messages_are_counted_not_fatal() {
        let mut state = SessionState::default();
        let mut sink = CountingSink::default();
        route(
            &mut state,
            &mut sink,
            "dviz",
            "dviz/world/lidar",
            b"{not json",
        );
        assert_eq!(state.malformed, 1);
        assert!(state.entities.is_empty());
        assert!(sink.calls.is_empty());

        let unknown = message("foo", json!({"anything": 1}), None);
        route(&mut state, &mut sink, "dviz", "dviz/world/thing", &unknown);
        assert_eq!(state.malformed, 1);
        assert_eq!(state.entities["world/thing"].msg_type, "foo");
        assert_eq!(state.entities["world/thing"].count, 1);
        assert_eq!(sink.calls, calls(&[("world/thing", "foo")]));

        let log_without_fields = message("log", json!({"level": "INFO"}), None);
        route(
            &mut state,
            &mut sink,
            "dviz",
            "dviz/logs",
            &log_without_fields,
        );
        assert_eq!(state.malformed, 2);
        assert!(state.logs.is_empty());
        assert_eq!(state.received, 3);
    }

    #[test]
    fn sink_error_counts_separately() {
        let mut state = SessionState::default();
        route(
            &mut state,
            &mut FailingSink,
            "dviz",
            "dviz/world/lidar",
            &points(),
        );
        assert_eq!(state.sink_errors, 1);
        assert_eq!(state.malformed, 0, "the payload itself was fine");
        assert_eq!(state.entities["world/lidar"].count, 1);
    }

    #[test]
    fn long_log_text_is_truncated() {
        let mut state = SessionState::default();
        let mut sink = CountingSink::default();
        let mut store = |text: &str| {
            let data = json!({"level": "INFO", "message": text, "node_id": "planner"});
            route(
                &mut state,
                &mut sink,
                "dviz",
                "dviz/logs",
                &message("log", data, None),
            );
            state.logs.back().unwrap().message.clone()
        };

        let long = "x".repeat(5000);
        let kept = store(&long);
        assert!(kept.len() <= LOG_TEXT_CAP, "{} bytes kept", kept.len());
        assert_eq!(kept, long[..LOG_TEXT_CAP]);

        // Cut on a character boundary: multi-byte text does not panic.
        let wide = "点".repeat(5000);
        let kept = store(&wide);
        assert!(kept.len() <= LOG_TEXT_CAP, "{} bytes kept", kept.len());
        assert!(kept.len() > LOG_TEXT_CAP - '点'.len_utf8());
        assert!(wide.starts_with(&kept));

        assert_eq!(store("short"), "short");
    }

    #[test]
    fn graph_update_replaces_nodes_and_edges() {
        let mut state = SessionState::default();
        let mut sink = CountingSink::default();
        let first = graph(
            &["camera", "planner"],
            &[("camera", "image", "planner", "frame")],
        );
        route(&mut state, &mut sink, "dviz", "dviz/graph", &first);
        let ids: Vec<&str> = state.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["camera", "planner"]);
        assert_eq!(state.edges.len(), 1);
        assert_eq!(state.edges[0].from_node, "camera");
        assert_eq!(state.edges[0].to_port, "frame");

        route(
            &mut state,
            &mut sink,
            "dviz",
            "dviz/graph",
            &graph(&["lidar"], &[]),
        );
        let ids: Vec<&str> = state.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["lidar"]);
        assert!(state.edges.is_empty());
        assert!(sink.calls.is_empty());
        assert!(state.entities.is_empty());
    }

    #[test]
    fn node_definition_adds_node_and_its_input_edges() {
        let mut state = SessionState::default();
        let mut sink = CountingSink::default();
        let definition = |inputs: Value| {
            message(
                "node_definition",
                json!({"id": "planner", "inputs": inputs, "outputs": []}),
                None,
            )
        };
        let first = definition(json!([
            {"name": "frame", "source": "camera/image"},
            {"name": "tick", "source": "timer"}
        ]));
        route(&mut state, &mut sink, "dviz", "dviz/nodes/planner", &first);
        assert_eq!(state.nodes.len(), 1);
        assert_eq!(state.nodes[0].id, "planner");
        assert_eq!(
            state.edges.len(),
            1,
            "a source without an output port has no edge"
        );
        let edge = &state.edges[0];
        assert_eq!(
            (
                &*edge.from_node,
                &*edge.from_port,
                &*edge.to_node,
                &*edge.to_port
            ),
            ("camera", "image", "planner", "frame")
        );

        let second = definition(json!([{"name": "scan", "source": "lidar/points"}]));
        route(&mut state, &mut sink, "dviz", "dviz/nodes/planner", &second);
        assert_eq!(state.nodes.len(), 1, "the node is not added twice");
        assert_eq!(
            state.edges.len(),
            1,
            "the node's old input edges are replaced"
        );
        assert_eq!(state.edges[0].from_node, "lidar");
        assert!(sink.calls.is_empty());
    }
}
