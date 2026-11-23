//! Dora node logs and the dataflow graph. Dora's `log_<node>.jsonl` lines become dviz
//! `log` entries (rate-limited per node); the dataflow YAML becomes `node_definition`
//! and `graph_update` messages. [`LogForward`] runs both from the worker's tick.

use crate::publish::Publish;
use crate::rate::RateLimiter;
use dviz_protocol::{
    serialize_message, GraphEdge, GraphNode, GraphNodeStatus, GraphUpdate, LogEntry, LogLevel,
    MvizMessage, NodeDefinition, NodeInputDef, NodeOutputDef,
};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Longest message text forwarded, in bytes (Studio keeps at most 2048).
pub const MAX_TEXT: usize = 2048;
/// Most bytes read from one log file per poll. A file that grew more since the last
/// poll is read from its last `MAX_READ` bytes; the lines before are skipped.
const MAX_READ: u64 = 64 * 1024;
/// Longest log line kept; a longer one is dropped.
const MAX_LINE: usize = 16 * 1024;
/// Most log files followed in the directory.
const MAX_FILES: usize = 64;
/// Most nodes the limiter tracks at once.
const MAX_LIMITED_NODES: usize = 256;

/// `YYYY-MM-DDTHH:MM:SS[.fff]Z` (UTC, as Dora writes `ts`) in Unix seconds.
fn parse_ts(s: &str) -> Option<f64> {
    let (date, time) = s.strip_suffix('Z')?.split_once('T')?;
    let mut d = date.splitn(3, '-');
    let y: i64 = d.next()?.parse().ok()?;
    let m: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    let mut t = time.splitn(3, ':');
    let h: i64 = t.next()?.parse().ok()?;
    let min: i64 = t.next()?.parse().ok()?;
    let sec: f64 = t.next()?.parse().ok()?;
    let valid = (1..=12).contains(&m)
        && (1..=31).contains(&day)
        && (0..24).contains(&h)
        && (0..60).contains(&min)
        && (0.0..61.0).contains(&sec);
    if !valid {
        return None;
    }
    // Days since 1970-01-01 (H. Hinnant's days_from_civil).
    let y = if m <= 2 { y - 1 } else { y };
    let (era, yoe) = (y.div_euclid(400), y.rem_euclid(400));
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some((days * 86_400 + h * 3600 + min * 60) as f64 + sec)
}

/// `s` cut to at most `max` bytes on a character boundary.
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn contains_error(msg: &str) -> bool {
    msg.as_bytes().windows(5).any(|w| w.eq_ignore_ascii_case(b"error"))
}

/// One line of Dora's `log_<node>.jsonl` (`{"ts","level","node","stream","msg",...}`) as
/// a [`LogEntry`]. Level: `msg` containing "error" (any case), or Dora level `error`,
/// is ERROR; otherwise stream `stderr`, or Dora level `warn`, is WARN; otherwise INFO.
/// The timestamp is `ts` in Unix seconds (0 without one); metadata keeps `stream` and
/// `ts`; the text is cut to [`MAX_TEXT`] bytes. `None` if the line has no `node` or
/// `msg` string.
pub fn dora_log_line(line: &str) -> Option<LogEntry> {
    let v: Value = serde_json::from_str(line).ok()?;
    let node = v.get("node")?.as_str()?;
    let msg = v.get("msg")?.as_str()?;
    let field = |key: &str| v.get(key).and_then(Value::as_str).unwrap_or("");
    let (stream, dora_level, ts) = (field("stream"), field("level"), field("ts"));
    let level = if contains_error(msg) || dora_level.eq_ignore_ascii_case("error") {
        LogLevel::Error
    } else if stream == "stderr"
        || dora_level.eq_ignore_ascii_case("warn")
        || dora_level.eq_ignore_ascii_case("warning")
    {
        LogLevel::Warn
    } else {
        LogLevel::Info
    };
    let mut entry =
        LogEntry::new(level, truncate(msg, MAX_TEXT), node).with_timestamp(parse_ts(ts).unwrap_or(0.0));
    entry.metadata.insert("stream".into(), stream.into());
    if !ts.is_empty() {
        entry.metadata.insert("ts".into(), ts.into());
    }
    Some(entry)
}

/// Per-node line cap: at most `n = max(1, round(hz))` lines in any window of `n / hz`
/// seconds, so 5 Hz is 5 lines per second and 0.5 Hz one line per 2 s. 0 Hz (or a
/// non-finite rate) lets nothing through.
pub struct LogLimiter {
    cap: usize,
    window: Duration,
    /// Per node, the times of the lines let through within the window.
    recent: HashMap<String, VecDeque<Instant>>,
}

impl LogLimiter {
    pub fn new(per_node_hz: f32) -> Self {
        let (cap, window) = if per_node_hz.is_finite() && per_node_hz > 0.0 {
            let cap = per_node_hz.round().max(1.0);
            (cap as usize, Duration::from_secs_f32(cap / per_node_hz))
        } else {
            (0, Duration::ZERO)
        };
        LogLimiter { cap, window, recent: HashMap::new() }
    }

    /// Whether one more line of `node` may go out at `now` (counts it if so).
    pub fn allow(&mut self, node: &str, now: Instant) -> bool {
        if self.cap == 0 {
            return false;
        }
        let window = self.window;
        let expired = |t: &Instant| now.checked_duration_since(*t).is_some_and(|d| d >= window);
        if !self.recent.contains_key(node) {
            if self.recent.len() >= MAX_LIMITED_NODES {
                self.recent.retain(|_, q| q.back().is_some_and(|t| !expired(t)));
                if self.recent.len() >= MAX_LIMITED_NODES {
                    return false;
                }
            }
            self.recent.insert(node.to_owned(), VecDeque::with_capacity(self.cap));
        }
        let Some(q) = self.recent.get_mut(node) else { return false };
        while q.front().is_some_and(expired) {
            q.pop_front();
        }
        if q.len() < self.cap {
            q.push_back(now);
            true
        } else {
            false
        }
    }
}

/// Node definitions and edges from a Dora dataflow YAML. An input is `name: node/output`
/// or `name: {source: node/output, ...}`; every input is kept in its node's definition,
/// but Dora timers (`dora/...`) are not edges. Output destinations are the nodes that
/// read them. `operator` is the node's `path` (or a single operator's file).
pub fn graph_from_yaml(text: &str) -> Result<(Vec<NodeDefinition>, Vec<GraphEdge>), String> {
    use serde_yaml::Value as Y;
    let doc: Y = serde_yaml::from_str(text).map_err(|e| format!("dataflow yaml: {e}"))?;
    let nodes = doc.get("nodes").and_then(Y::as_sequence).ok_or("dataflow yaml: no `nodes` list")?;
    let mut defs: Vec<NodeDefinition> = Vec::with_capacity(nodes.len());
    let mut edges = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let id = node.get("id").and_then(Y::as_str).ok_or_else(|| format!("dataflow yaml: node {i} has no id"))?;
        let mut inputs = Vec::new();
        for (name, input) in node.get("inputs").and_then(Y::as_mapping).into_iter().flatten() {
            let source = input.as_str().or_else(|| input.get("source").and_then(Y::as_str));
            let (Some(name), Some(source)) = (name.as_str(), source) else { continue };
            if let Some((from, port)) = source.split_once('/').filter(|(from, _)| *from != "dora") {
                edges.push(GraphEdge {
                    from_node: from.into(),
                    from_port: port.into(),
                    to_node: id.into(),
                    to_port: name.into(),
                });
            }
            inputs.push(NodeInputDef { name: name.into(), source: source.into() });
        }
        let outputs = (node.get("outputs").and_then(Y::as_sequence).into_iter().flatten())
            .filter_map(Y::as_str)
            .map(|name| NodeOutputDef { name: name.into(), destinations: Vec::new() })
            .collect();
        let operator = node.get("path").and_then(Y::as_str).or_else(|| {
            let op = node.get("operator")?;
            ["python", "shared-library"].iter().find_map(|k| op.get(k)?.as_str())
        });
        defs.push(NodeDefinition { id: id.into(), inputs, outputs, operator: operator.map(str::to_owned) });
    }
    for def in &mut defs {
        for out in &mut def.outputs {
            for e in edges.iter().filter(|e| e.from_node == def.id && e.from_port == out.name) {
                if !out.destinations.contains(&e.to_node) {
                    out.destinations.push(e.to_node.clone());
                }
            }
        }
    }
    Ok((defs, edges))
}

/// Follows the `log_*.jsonl` files of one directory (Dora's `out/<dataflow_id>/`).
pub struct LogTail {
    dir: PathBuf,
    files: HashMap<PathBuf, TailState>,
}

struct TailState {
    /// (device, inode): another file under the same name is a rotation.
    id: (u64, u64),
    /// Bytes of the file consumed so far.
    pos: u64,
    /// The start of a line whose newline has not been written yet.
    partial: Vec<u8>,
    /// Discarding up to the next newline (after a jump ahead or an overlong line).
    skipping: bool,
}

impl LogTail {
    pub fn new(dir: PathBuf) -> Self {
        LogTail { dir, files: HashMap::new() }
    }

    /// The complete lines written since the last call, file by file (sorted by name),
    /// oldest first. A new file is read from its start; a file that is shorter than
    /// what was read (truncated) or is another file under the same name (rotated) is
    /// read again from its start. Memory is bounded: at most `MAX_READ` bytes per file
    /// per call (a file that grew more is read from its last `MAX_READ` bytes), lines
    /// longer than `MAX_LINE` are dropped, at most `MAX_FILES` files. Empty lines are
    /// skipped; a missing directory gives no lines.
    pub fn poll(&mut self) -> Vec<String> {
        let mut paths: Vec<PathBuf> = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| is_log_file(p)).collect(),
            Err(_) => Vec::new(),
        };
        paths.sort();
        paths.truncate(MAX_FILES);
        self.files.retain(|p, _| paths.contains(p));
        let mut lines = Vec::new();
        for path in paths {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if !meta.is_file() {
                continue;
            }
            let id = file_id(&meta);
            let state = self.files.entry(path.clone()).or_insert_with(|| TailState::new(id));
            if state.id != id || meta.len() < state.pos {
                *state = TailState::new(id);
            }
            // A read error (e.g. the file vanished) is retried on the next poll.
            let _ = state.read_new(&path, meta.len(), &mut lines);
        }
        lines
    }
}

fn is_log_file(p: &Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("log_") && n.ends_with(".jsonl"))
}

#[cfg(unix)]
fn file_id(m: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (m.dev(), m.ino())
}

#[cfg(not(unix))]
fn file_id(_: &std::fs::Metadata) -> (u64, u64) {
    (0, 0)
}

impl TailState {
    fn new(id: (u64, u64)) -> Self {
        TailState { id, pos: 0, partial: Vec::new(), skipping: false }
    }

    /// Reads from `pos` up to `len` and appends the complete lines to `lines`.
    fn read_new(&mut self, path: &Path, len: u64, lines: &mut Vec<String>) -> std::io::Result<()> {
        if len <= self.pos {
            return Ok(());
        }
        let mut start = self.pos;
        if len - start > MAX_READ {
            // Jump ahead. Read one byte before the new start so that a line starting
            // exactly there is kept: the skip then ends at that byte's newline.
            start = len - MAX_READ - 1;
            self.partial.clear();
            self.skipping = true;
        }
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(start))?;
        let mut buf = Vec::new();
        file.take(len - start).read_to_end(&mut buf)?;
        self.pos = start + buf.len() as u64;
        let mut rest = &buf[..];
        while let Some(i) = rest.iter().position(|&b| b == b'\n') {
            if self.skipping {
                self.skipping = false;
            } else if self.partial.len() + i <= MAX_LINE {
                self.partial.extend_from_slice(&rest[..i]);
                let line = String::from_utf8_lossy(&self.partial);
                let line = line.trim_end_matches('\r');
                if !line.is_empty() {
                    lines.push(line.to_owned());
                }
            }
            self.partial.clear();
            rest = &rest[i + 1..];
        }
        if !self.skipping {
            if self.partial.len() + rest.len() > MAX_LINE {
                self.partial.clear();
                self.skipping = true;
            } else {
                self.partial.extend_from_slice(rest);
            }
        }
        Ok(())
    }
}

/// Topic entities (`dviz/<robot_id>/<entity>`) of the bridge's log and graph messages.
/// Studio routes them by message type (`log`, `node_definition`, `graph_update`).
pub const LOGS: &str = "logs";
pub const NODES: &str = "nodes";
pub const GRAPH: &str = "graph";
pub const ENTITIES: [&str; 3] = [LOGS, NODES, GRAPH];

/// Node logs are read once a second.
const POLL_HZ: f32 = 1.0;
/// The graph is sent every 5 s.
const GRAPH_HZ: f32 = 0.2;
/// A node that logged within this time is `active`, or `error` if one of those
/// lines was an ERROR; otherwise it is `idle`.
const ACTIVE_FOR: Duration = Duration::from_secs(10);
/// Most bridge notices kept.
const MAX_NOTICES: usize = 32;
/// Node id of the bridge's own notices when Dora's is not known.
const DEFAULT_NODE: &str = "nav_bridge";

/// Where the bridge finds node logs and the dataflow, and who it is.
#[derive(Debug, Clone, Default)]
pub struct LogSources {
    /// Dora's log directory for this dataflow (`log_<node>.jsonl` files).
    pub log_dir: Option<PathBuf>,
    /// Dataflow YAML for the graph.
    pub dataflow: Option<PathBuf>,
    /// The bridge's own node id. Its log lines are not forwarded (its notices are
    /// sent directly instead) and it is always `active` in the graph.
    pub own_node: Option<String>,
}

/// Forwards node logs and the dataflow graph; driven by the worker's tick.
pub struct LogForward {
    tail: Option<LogTail>,
    limiter: LogLimiter,
    graph: Option<(Vec<NodeDefinition>, Vec<GraphEdge>)>,
    /// Per graph node: when it last logged, and when it last logged an ERROR.
    seen: HashMap<String, (Instant, Option<Instant>)>,
    own_node: String,
    /// The bridge's own WARN entries (e.g. a map it could not load).
    notices: Vec<LogEntry>,
    /// How many notices the current log subscriber has been sent.
    notices_sent: usize,
    /// Whether someone subscribed to `logs` at the last poll.
    logs_wanted: bool,
    start: Instant,
    poll_rate: RateLimiter,
    graph_rate: RateLimiter,
}

impl LogForward {
    /// Reads the dataflow YAML now (on the worker thread); a missing or bad one
    /// becomes a WARN notice and no graph is sent.
    pub fn new(sources: LogSources, per_node_hz: f32) -> LogForward {
        let mut fwd = LogForward {
            tail: sources.log_dir.map(LogTail::new),
            limiter: LogLimiter::new(per_node_hz),
            graph: None,
            seen: HashMap::new(),
            own_node: sources.own_node.unwrap_or_else(|| DEFAULT_NODE.into()),
            notices: Vec::new(),
            notices_sent: 0,
            logs_wanted: false,
            start: Instant::now(),
            poll_rate: RateLimiter::new(POLL_HZ),
            graph_rate: RateLimiter::new(GRAPH_HZ),
        };
        if let Some(path) = sources.dataflow {
            let graph = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| graph_from_yaml(&t));
            match graph {
                Ok(graph) => fwd.graph = Some(graph),
                Err(e) => fwd.warn(format!("dataflow graph not published: {}: {e}", path.display())),
            }
        }
        fwd
    }

    /// A WARN from the bridge itself. It is sent to the current `logs` subscriber,
    /// and again to the next one whenever a subscriber appears after none.
    pub fn warn(&mut self, message: String) {
        if self.notices.len() < MAX_NOTICES {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
            self.notices.push(LogEntry::new(LogLevel::Warn, message, self.own_node.clone()).with_timestamp(now));
        }
    }

    /// Once a second: reads new log lines and, while someone subscribes to `logs`,
    /// sends them (at most `per_node_hz` lines per node per second) and the pending
    /// notices. Every 5 s while someone subscribes to `nodes` or `graph`: one
    /// `node_definition` per node, then a `graph_update`.
    pub fn tick(&mut self, now: Instant, out: &mut dyn Publish) {
        if self.poll_rate.ready(now) {
            self.poll(now, out);
        }
        if self.graph.is_some() && (out.wants(NODES) || out.wants(GRAPH)) && self.graph_rate.ready(now) {
            self.send_graph(now, out);
        }
    }

    fn poll(&mut self, now: Instant, out: &mut dyn Publish) {
        let wanted = out.wants(LOGS);
        if wanted {
            let from = if self.logs_wanted { self.notices_sent } else { 0 };
            for notice in &self.notices[from..] {
                out.put(LOGS, enc_log(notice));
            }
            self.notices_sent = self.notices.len();
        }
        self.logs_wanted = wanted;
        let lines = self.tail.as_mut().map(LogTail::poll).unwrap_or_default();
        for entry in lines.iter().filter_map(|line| dora_log_line(line)) {
            if entry.node_id == self.own_node {
                continue;
            }
            if let Some(seen) = self.seen.get_mut(&entry.node_id) {
                seen.0 = now;
                if entry.level == LogLevel::Error {
                    seen.1 = Some(now);
                }
            } else if self.graph.as_ref().is_some_and(|(defs, _)| defs.iter().any(|d| d.id == entry.node_id)) {
                let error = (entry.level == LogLevel::Error).then_some(now);
                self.seen.insert(entry.node_id.clone(), (now, error));
            }
            if wanted && self.limiter.allow(&entry.node_id, now) {
                out.put(LOGS, enc_log(&entry));
            }
        }
    }

    fn send_graph(&self, now: Instant, out: &mut dyn Publish) {
        let Some((defs, edges)) = &self.graph else { return };
        let t = now.saturating_duration_since(self.start).as_secs_f64();
        if out.wants(NODES) {
            for def in defs {
                out.put(NODES, enc("node_definition", t, serde_json::to_value(def)));
            }
        }
        if out.wants(GRAPH) {
            let nodes = defs.iter().map(|d| self.graph_node(&d.id, now)).collect();
            let update = GraphUpdate { nodes, edges: edges.clone(), timestamp: t };
            out.put(GRAPH, enc("graph_update", t, serde_json::to_value(&update)));
        }
    }

    /// Status from log activity; `last_seen` is seconds since the bridge started.
    fn graph_node(&self, id: &str, now: Instant) -> GraphNode {
        let recent = |t: Instant| now.saturating_duration_since(t) < ACTIVE_FOR;
        let since_start = |t: Instant| t.saturating_duration_since(self.start).as_secs_f64();
        let (status, last_seen) = if id == self.own_node {
            (GraphNodeStatus::Active, since_start(now))
        } else {
            match self.seen.get(id) {
                Some(&(last, error)) => {
                    let status = if error.is_some_and(recent) {
                        GraphNodeStatus::Error
                    } else if recent(last) {
                        GraphNodeStatus::Active
                    } else {
                        GraphNodeStatus::Idle
                    };
                    (status, since_start(last))
                }
                None => (GraphNodeStatus::Idle, 0.0),
            }
        };
        GraphNode { id: id.into(), status, last_seen }
    }
}

fn enc(msg_type: &str, timestamp: f64, data: Result<Value, serde_json::Error>) -> Vec<u8> {
    let m = MvizMessage {
        msg_type: msg_type.to_owned(),
        timestamp: Some(timestamp),
        data: data.unwrap_or(Value::Null),
        format: None,
        count: None,
    };
    serialize_message(&m, None)
}

/// A `log` message; its data is the entry (Studio reads it as `LogData`).
pub fn enc_log(entry: &LogEntry) -> Vec<u8> {
    enc("log", entry.timestamp, serde_json::to_value(entry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dviz_protocol::{parse_message, LogData, LogLevel};
    use std::io::Write;
    use std::path::Path;
    use std::time::Duration;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn lines(name: &str) -> Vec<String> {
        fixture(name).lines().map(str::to_owned).collect()
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("navbridge-logs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn append(path: &Path, text: &str) {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn dora_log_levels() {
        let gate = lines("dora_logs/log_cmd_gate.jsonl");
        let astar = lines("dora_logs/log_astar_planner.jsonl");
        let stderr = lines("dora_logs/log_pose_fuser.jsonl");
        let real = [&gate[0], &astar[0], &stderr[0]];
        let levels: Vec<LogLevel> = real.iter().map(|l| dora_log_line(l).expect(l).level).collect();
        assert_eq!(levels, [LogLevel::Info, LogLevel::Info, LogLevel::Warn]);

        let e = dora_log_line(&astar[0]).unwrap();
        assert_eq!(e.node_id, "astar_planner");
        assert!(e.message.starts_with("[astar_planner] ../../modules/planner"), "{}", e.message);
        // 2026-11-23T17:15:51.801Z
        assert!((e.timestamp - 1_795_454_151.801).abs() < 1e-3, "{}", e.timestamp);
        assert_eq!(e.metadata.get("stream").map(String::as_str), Some("stdout"));

        // "error" anywhere in the text, any case, is an ERROR (this one is also stderr).
        let trust = lines("dora_logs/log_loc_trust.jsonl");
        assert_eq!(dora_log_line(&trust[0]).unwrap().level, LogLevel::Error);
        let stdout_error = gate[0].replace("starts DISARMED", "Error: CAN timeout");
        assert_eq!(dora_log_line(&stdout_error).unwrap().level, LogLevel::Error);
        // Dora's own `warn` records (stream stdout) are warnings too.
        let navcmd = lines("dora_logs/log_navcmd.jsonl");
        assert_eq!(dora_log_line(&navcmd[0]).unwrap().level, LogLevel::Warn);

        for bad in ["", "not json", "{}", r#"{"node":"x"}"#, r#"{"msg":"m"}"#, r#"{"node":"x","msg":null}"#] {
            assert!(dora_log_line(bad).is_none(), "{bad}");
        }
        let long = format!(r#"{{"node":"x","stream":"stdout","msg":"{}"}}"#, "é".repeat(5000));
        let e = dora_log_line(&long).unwrap();
        assert!(e.message.len() <= MAX_TEXT && e.message.chars().all(|c| c == 'é'));
        assert_eq!(e.timestamp, 0.0, "no ts");
    }

    #[test]
    fn ts_parses_to_unix_seconds() {
        assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(parse_ts("2000-03-01T00:00:00Z"), Some(951_868_800.0));
        assert!((parse_ts("2026-11-23T17:15:51.801Z").unwrap() - 1_795_454_151.801).abs() < 1e-3);
        for bad in ["", "2026-11-23", "2026-13-01T00:00:00Z", "2026-11-23T25:00:00Z", "x-y-zT1:2:3Z"] {
            assert_eq!(parse_ts(bad), None, "{bad}");
        }
    }

    #[test]
    fn log_limiter_per_node() {
        let mut lim = LogLimiter::new(5.0);
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let a = (0..10).filter(|i| lim.allow("a", at(i * 90))).count();
        assert_eq!(a, 5, "10 lines of one node within 1 s");
        let b = (0..3).filter(|i| lim.allow("b", at(i * 90))).count();
        assert_eq!(b, 3, "another node is not affected");
        assert!(lim.allow("a", at(1000)), "a new second");

        let mut off = LogLimiter::new(0.0);
        assert!(!off.allow("a", t0), "0 Hz forwards nothing");
        let mut slow = LogLimiter::new(0.5);
        assert!(slow.allow("a", t0));
        assert!(!slow.allow("a", at(1500)));
        assert!(slow.allow("a", at(2000)), "one line per 2 s");
    }

    #[test]
    fn graph_from_tracer_nav_yaml() {
        let (nodes, edges) = graph_from_yaml(&fixture("tracer_nav.yml")).unwrap();
        assert_eq!(nodes.len(), 14);
        let has = |from: &str, port: &str, to: &str, to_port: &str| {
            edges
                .iter()
                .any(|e| e.from_node == from && e.from_port == port && e.to_node == to && e.to_port == to_port)
        };
        assert!(has("astar_planner", "path", "pure_pursuit", "path"));
        // `{source: ..., queue_size: ...}` inputs are edges too.
        assert!(has("livox", "pointcloud", "super_lio", "pointcloud"));
        // Timers are inputs but not edges.
        assert!(edges.iter().all(|e| e.from_node != "dora"));
        let pp = nodes.iter().find(|n| n.id == "pure_pursuit").unwrap();
        assert!(pp.inputs.iter().any(|i| i.name == "tick" && i.source == "dora/timer/millis/50"));
        let astar = nodes.iter().find(|n| n.id == "astar_planner").unwrap();
        assert_eq!(astar.outputs.len(), 1);
        assert_eq!(astar.outputs[0].name, "path");
        assert_eq!(astar.outputs[0].destinations, ["pure_pursuit"]);
        assert!(astar.operator.as_deref().unwrap().ends_with("astar_planner_dora_node"));
        let livox = nodes.iter().find(|n| n.id == "livox").unwrap();
        assert!(livox.inputs.is_empty());
        let fan_out = &livox.outputs.iter().find(|o| o.name == "pointcloud").unwrap().destinations;
        assert_eq!(fan_out, &["super_lio", "hdl"]);
    }

    #[test]
    fn graph_yaml_errors() {
        for bad in ["nodes: [", "nodes: 3", "other: []", "nodes:\n  - path: x\n"] {
            assert!(graph_from_yaml(bad).is_err(), "{bad}");
        }
        let (nodes, edges) = graph_from_yaml("nodes: []").unwrap();
        assert!(nodes.is_empty() && edges.is_empty());
    }

    #[test]
    fn log_tail_reads_only_new_lines() {
        let d = tmpdir("tail");
        let f = d.join("log_a.jsonl");
        let mut tail = LogTail::new(d.clone());
        assert!(tail.poll().is_empty());
        append(&f, "one\ntwo\n");
        std::fs::write(d.join("other.txt"), "not a log\n").unwrap();
        assert_eq!(tail.poll(), ["one", "two"]);
        assert!(tail.poll().is_empty());
        append(&f, "three\npart");
        assert_eq!(tail.poll(), ["three"], "an unfinished line waits");
        append(&f, "ial\n");
        assert_eq!(tail.poll(), ["partial"]);
        std::fs::write(&f, "").unwrap();
        append(&f, "after truncate\n");
        assert_eq!(tail.poll(), ["after truncate"]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn log_tail_rotation_huge_files_and_long_lines() {
        let d = tmpdir("tail-big");
        let f = d.join("log_b.jsonl");
        append(&f, "old 1\nold 2\n");
        let mut tail = LogTail::new(d.clone());
        assert_eq!(tail.poll().len(), 2);
        // Rotation: the file is renamed away and a new one (another inode) takes its
        // name, even if the new one is longer than what was read.
        std::fs::rename(&f, d.join("b.old")).unwrap();
        append(&f, "new 1\nnew 2\nnew 3\nnew 4\n");
        assert_eq!(tail.poll(), ["new 1", "new 2", "new 3", "new 4"]);

        // A file far larger than one read: only its last whole lines are read.
        let g = d.join("log_c.jsonl");
        let line = "x".repeat(99);
        append(&g, &format!("{line}\n").repeat(10_000)); // 1 MB
        let got = tail.poll();
        assert!(!got.is_empty() && got.len() as u64 <= MAX_READ / 100 + 1, "{}", got.len());
        assert!(got.iter().all(|l| *l == line), "only whole lines");
        assert!(tail.poll().is_empty());

        // A line longer than MAX_LINE is dropped, the next one is kept.
        append(&g, &"y".repeat(MAX_LINE + 10));
        assert!(tail.poll().is_empty());
        append(&g, &format!("{}\nshort\n", "y".repeat(10)));
        assert_eq!(tail.poll(), ["short"]);

        // A deleted file is forgotten; a missing directory is not an error.
        std::fs::remove_file(&g).unwrap();
        assert!(tail.poll().is_empty());
        let mut missing = LogTail::new(d.join("nope"));
        assert!(missing.poll().is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    /// Records every put; `wanted` answers every `wants`.
    struct Out {
        wanted: bool,
        sent: Vec<(String, Vec<u8>)>,
    }

    impl Publish for Out {
        fn wants(&self, _: &str) -> bool {
            self.wanted
        }
        fn put(&mut self, entity: &str, bytes: Vec<u8>) {
            self.sent.push((entity.to_owned(), bytes));
        }
    }

    impl Out {
        fn new() -> Out {
            Out { wanted: true, sent: Vec::new() }
        }

        /// (type, data) of the messages put to `entity` since the last call.
        fn take(&mut self, entity: &str) -> Vec<(String, serde_json::Value)> {
            let (hit, rest): (Vec<_>, Vec<_>) = self.sent.drain(..).partition(|(e, _)| e == entity);
            self.sent = rest;
            hit.iter()
                .map(|(_, b)| parse_message(b).map(|(m, _)| (m.msg_type, m.data)).expect("dviz message"))
                .collect()
        }

        fn logs(&mut self) -> Vec<LogData> {
            let logs = self.take(LOGS);
            assert!(logs.iter().all(|(t, _)| t == "log"));
            logs.into_iter().map(|(_, d)| serde_json::from_value(d).expect("LogData")).collect()
        }

        fn graph(&mut self) -> Option<GraphUpdate> {
            let mut g = self.take(GRAPH);
            assert!(g.len() <= 1);
            let (t, data) = g.pop()?;
            assert_eq!(t, "graph_update");
            Some(serde_json::from_value(data).expect("GraphUpdate"))
        }
    }

    fn dora_line(node: &str, stream: &str, msg: &str) -> String {
        let line = serde_json::json!({
            "ts": "2026-11-23T17:15:51.801Z", "level": "stdout", "node": node, "stream": stream,
            "msg": msg, "target": null, "fields": null
        });
        format!("{line}\n")
    }

    #[test]
    fn forward_limits_lines_skips_own_and_resends_notices() {
        let d = tmpdir("fwd");
        let sources = LogSources { log_dir: Some(d.clone()), dataflow: None, own_node: Some("viz".into()) };
        let mut fwd = LogForward::new(sources, 5.0);
        fwd.warn("map/grid not published: /x/map.yaml: no such file".into());
        let mut text: String = (0..10).map(|i| dora_line("a", "stdout", &format!("a {i}"))).collect();
        text += &dora_line("b", "stderr", "b 0");
        text += &dora_line("viz", "stderr", "nav-bridge: no map/grid");
        append(&d.join("log_a.jsonl"), &text);
        let mut out = Out::new();
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);

        fwd.tick(at(0), &mut out);
        let logs = out.logs();
        let got: Vec<(&str, &str, &str)> =
            logs.iter().map(|l| (l.level.as_str(), l.node_id.as_str(), l.message.as_str())).collect();
        assert_eq!(got.len(), 7, "{got:?}");
        assert_eq!(got[0], ("WARN", "viz", "map/grid not published: /x/map.yaml: no such file"));
        assert_eq!(&got[1..6], [0, 1, 2, 3, 4].map(|i| ("INFO", "a", ["a 0", "a 1", "a 2", "a 3", "a 4"][i])));
        assert_eq!(got[6], ("WARN", "b", "b 0"));

        append(&d.join("log_a.jsonl"), &(0..3).map(|i| dora_line("a", "stdout", &format!("again {i}"))).collect::<String>());
        fwd.tick(at(500), &mut out);
        assert!(out.logs().is_empty(), "logs are read once a second");
        fwd.tick(at(1000), &mut out);
        assert_eq!(out.logs().len(), 3);

        out.wanted = false;
        append(&d.join("log_a.jsonl"), &dora_line("a", "stdout", "nobody listens"));
        fwd.tick(at(2000), &mut out);
        assert!(out.sent.is_empty(), "nothing is sent without a subscriber");
        out.wanted = true;
        fwd.tick(at(3000), &mut out);
        let logs = out.logs();
        assert_eq!(logs.len(), 1, "a new subscriber gets the notice again, not old lines");
        assert!(logs[0].message.contains("/x/map.yaml"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn forward_graph_every_5_s_with_status_from_logs() {
        let d = tmpdir("graph");
        let yml = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tracer_nav.yml");
        let sources = LogSources { log_dir: Some(d.clone()), dataflow: Some(yml), own_node: Some("nav_logger".into()) };
        let mut fwd = LogForward::new(sources, 5.0);
        let lines = dora_line("astar_planner", "stdout", "planning") + &dora_line("loc_trust", "stderr", "type_error");
        append(&d.join("log_x.jsonl"), &lines);
        let mut out = Out::new();
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let status = |g: &GraphUpdate, id: &str| g.nodes.iter().find(|n| n.id == id).unwrap().status;

        fwd.tick(at(0), &mut out);
        let defs = out.take(NODES);
        assert_eq!(defs.len(), 14);
        assert!(defs.iter().all(|(t, _)| t == "node_definition"));
        let defs: Vec<NodeDefinition> = defs.into_iter().map(|(_, d)| serde_json::from_value(d).unwrap()).collect();
        let pp = defs.iter().find(|d| d.id == "pure_pursuit").unwrap();
        assert!(pp.inputs.iter().any(|i| i.name == "path" && i.source == "astar_planner/path"));
        let g = out.graph().expect("graph_update");
        assert_eq!(g.nodes.len(), 14);
        assert!(g.edges.iter().any(|e| e.from_node == "astar_planner" && e.to_node == "pure_pursuit"));
        assert_eq!(status(&g, "astar_planner"), GraphNodeStatus::Active);
        assert_eq!(status(&g, "loc_trust"), GraphNodeStatus::Error);
        assert_eq!(status(&g, "livox"), GraphNodeStatus::Idle);
        assert_eq!(status(&g, "nav_logger"), GraphNodeStatus::Active, "the bridge itself");

        fwd.tick(at(4900), &mut out);
        assert!(out.graph().is_none() && out.take(NODES).is_empty());
        fwd.tick(at(5000), &mut out);
        assert_eq!(out.take(NODES).len(), 14);
        let g = out.graph().expect("resent after 5 s");
        assert_eq!(status(&g, "astar_planner"), GraphNodeStatus::Active);
        assert!((g.timestamp - 5.0).abs() < 0.1, "{}", g.timestamp);

        fwd.tick(at(15_000), &mut out);
        let g = out.graph().unwrap();
        assert_eq!(status(&g, "astar_planner"), GraphNodeStatus::Idle, "nothing logged for 10 s");
        assert_eq!(status(&g, "loc_trust"), GraphNodeStatus::Idle);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn missing_dataflow_is_a_warning() {
        let sources = LogSources { dataflow: Some("/nonexistent/flow.yml".into()), ..LogSources::default() };
        let mut fwd = LogForward::new(sources, 5.0);
        let mut out = Out::new();
        fwd.tick(Instant::now(), &mut out);
        let logs = out.logs();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, "WARN");
        assert_eq!(logs[0].node_id, "nav_bridge");
        assert!(logs[0].message.contains("/nonexistent/flow.yml"), "{}", logs[0].message);
        assert!(out.graph().is_none());
    }
}
