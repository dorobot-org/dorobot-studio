//! Run-directory discovery and `dora.log` parsing.
//!
//! This module deliberately depends on `std` and `serde_json` only. It has no
//! Makepad or Studio backend types, so every rule below can be exercised with
//! `cargo test -p dorobot-plugin-diagnostics` without a window, GPU or Dora.
//!
//! Directory shape (see `runtime/src/main.rs` and
//! `crates/dorobot-studio-core/src/api/workflow.rs`):
//!
//! ```text
//! runs/studio-<epoch-millis>/
//!   config.json             runtime Config (model, socket, run_id, …)
//!   dataflow.yaml           generated Dora graph
//!   dora.log                stdout + stderr of `dora run --log-format json`
//!   startup.json            once the scene is ready
//!   progress.json           live scene counters
//!   recording-status.json   phase/frames/saved/dataset
//!   controller.json         latest controller input
//!   command.json            latest safe command
//!   stop                    written by Studio when it asks the runtime to stop
//!   failure-<n>.txt         written by the runtime on a guarded failure
//! ```
//!
//! Nothing here assumes every file exists: partial runs are the normal case
//! after an interrupted session.

use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Directory prefix used by `SimulationRuntime::start`.
pub const RUN_PREFIX: &str = "studio-";
/// How many log lines the UI keeps by default. The tail reader never loads the
/// whole file when it is larger than this (plus one line of scan slack).
pub const DEFAULT_TAIL_LINES: usize = 5_000;
const TAIL_CHUNK: usize = 64 * 1024;
/// Hard byte ceiling for the tail scan, so a file without newlines cannot make
/// the UI read a multi-gigabyte log into memory.
const MAX_TAIL_BYTES: u64 = 8 * 1024 * 1024;

// ------------------------------------------------------------------- levels --

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    /// A line that carried no recognisable level. Kept visible: plain-text
    /// launcher errors often have none.
    Unknown,
}

impl LogLevel {
    /// Parse the spellings emitted by tracing, `log`, Dora and the launcher.
    pub fn parse(value: &str) -> Option<Self> {
        let token = value
            .trim()
            .trim_matches(|c: char| matches!(c, '[' | ']' | '"' | '\'' | ',' | ':' | '(' | ')'))
            .to_ascii_uppercase();
        Some(match token.as_str() {
            "TRACE" | "TRC" => Self::Trace,
            "DEBUG" | "DBG" => Self::Debug,
            "INFO" | "INF" | "NOTICE" => Self::Info,
            "WARN" | "WARNING" | "WRN" => Self::Warn,
            "ERROR" | "ERR" | "CRITICAL" | "FATAL" => Self::Error,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Unknown => "LOG",
        }
    }

    /// Unknown lines are treated as info for filtering so that launcher output
    /// does not silently disappear when the user picks "Info".
    fn rank(self) -> u8 {
        match self {
            Self::Trace => 0,
            Self::Debug => 1,
            Self::Unknown | Self::Info => 2,
            Self::Warn => 3,
            Self::Error => 4,
        }
    }

    pub fn passes(self, minimum: Self) -> bool {
        self.rank() >= minimum.rank()
    }
}

// --------------------------------------------------------------- log lines --

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub level: LogLevel,
    /// Dora/tracing target, module, node or logger name when one was present.
    pub source: Option<String>,
    pub message: String,
    /// Timestamp as emitted (ISO-8601 or epoch millis). Not normalised, so a
    /// mixed log remains readable and round-trips through export.
    pub ts: Option<String>,
    /// The original line, used as a lossless fallback for export and search.
    pub raw: String,
}

impl LogLine {
    /// Parse one line. Blank lines return `None`; anything else is always
    /// represented, even when it is not JSON or has no level.
    pub fn parse(line: &str) -> Option<Self> {
        let raw = line.trim_end_matches('\r').to_string();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let json = if trimmed.starts_with('{') {
            serde_json::from_str::<serde_json::Value>(trimmed)
                .ok()
                .and_then(|value| Self::from_json(&raw, &value))
        } else {
            None
        };
        Some(json.unwrap_or_else(|| Self::from_plain(&raw, trimmed)))
    }

    /// Tracing's JSON subscriber nests the message under `fields`; Dora also
    /// accepts flatter shapes. Only JSON objects are treated as records.
    fn from_json(raw: &str, value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let level = ["level", "severity", "level_name", "lvl"]
            .iter()
            .find_map(|key| object.get(*key).and_then(json_level))
            .unwrap_or(LogLevel::Unknown);
        let message = ["message", "msg", "event"]
            .iter()
            .find_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
            .map(str::to_owned)
            .or_else(|| {
                object
                    .get("fields")
                    .and_then(serde_json::Value::as_object)
                    .and_then(|fields| fields.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| raw.trim().to_owned());
        let source = ["target", "module", "source", "node", "logger"]
            .iter()
            .find_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
            .map(str::to_owned);
        let ts = ["timestamp", "time", "ts", "@timestamp"]
            .iter()
            .find_map(|key| object.get(*key).and_then(json_ts));
        Some(Self {
            level,
            source,
            message,
            ts,
            raw: raw.to_owned(),
        })
    }

    fn from_plain(raw: &str, text: &str) -> Self {
        let (ts, level, source, body) = split_plain(text);
        Self {
            level,
            source,
            message: body.to_owned(),
            ts,
            raw: raw.to_owned(),
        }
    }

    /// One human-readable line, used by the panel and by export.
    pub fn display(&self) -> String {
        let mut out = String::new();
        if let Some(ts) = &self.ts {
            out.push_str(ts);
            out.push(' ');
        }
        out.push_str(self.level.label());
        if let Some(source) = &self.source {
            out.push(' ');
            out.push_str(source);
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&self.message);
        out
    }

    /// Export preserves the raw line when it was not recognised, so a consumer
    /// can still grep the original JSON.
    pub fn to_export(&self) -> String {
        if self.level == LogLevel::Unknown && !self.raw.trim().is_empty() {
            self.raw.clone()
        } else {
            self.display()
        }
    }
}

fn json_level(value: &serde_json::Value) -> Option<LogLevel> {
    if let Some(text) = value.as_str() {
        return LogLevel::parse(text);
    }
    // `log`/syslog-ish numeric severities, with tracing's 1..=5 as a fallback.
    let number = value.as_u64()?;
    Some(match number {
        0 | 10 => LogLevel::Trace,
        1 | 20 => LogLevel::Debug,
        2 | 30 => LogLevel::Info,
        3 | 40 => LogLevel::Warn,
        4 | 50 | 60 => LogLevel::Error,
        _ => return None,
    })
}

fn json_ts(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        (!text.is_empty()).then(|| text.to_owned())
    } else {
        value.as_u64().map(|number| number.to_string())
    }
}

/// Split `[ts] [level] [source:] message` without regex.
fn split_plain(text: &str) -> (Option<String>, LogLevel, Option<String>, &str) {
    if let Some(inner) = text.strip_prefix('[') {
        if let Some(end) = inner.find(']') {
            let head = &inner[..end];
            let (ts, level, source) = scan_head(head, true);
            let body = inner[end + 1..].trim_start();
            return (ts, level, source, if body.is_empty() { text } else { body });
        }
    }
    let (ts, level, source, consumed) = scan_head_len(text, false);
    (ts, level, source, text[consumed..].trim_start())
}

fn scan_head(head: &str, bracketed: bool) -> (Option<String>, LogLevel, Option<String>) {
    let (ts, level, source, _) = scan_head_len(head, bracketed);
    (ts, level, source)
}

fn scan_head_len(text: &str, bracketed: bool) -> (Option<String>, LogLevel, Option<String>, usize) {
    let mut ts = None;
    let mut level = LogLevel::Unknown;
    let mut source = None;
    let mut rest = text;

    if let Some((word, tail)) = take_word(rest) {
        if looks_like_timestamp(word) {
            ts = Some(word.to_owned());
            rest = tail;
        }
    }
    if let Some((word, tail)) = take_word(rest) {
        if let Some(parsed) = LogLevel::parse(word) {
            level = parsed;
            rest = tail;
        }
    }
    if level != LogLevel::Unknown {
        if let Some((word, tail)) = take_word(rest) {
            let candidate = word.trim_end_matches(':');
            let explicit_target = word.ends_with(':') && is_source_token(candidate);
            // Inside `[...]` everything after the level is metadata, so a bare
            // token may be the target. Outside, require the `target:` colon
            // that tracing emits; otherwise the token is probably the message.
            if explicit_target || (bracketed && is_source_token(candidate)) {
                source = Some(candidate.to_owned());
                rest = tail;
            }
        }
    }
    (ts, level, source, text.len() - rest.len())
}

fn take_word(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    Some((&text[..end], &text[end..]))
}

fn looks_like_timestamp(word: &str) -> bool {
    let word = word.trim_matches(|c: char| c == '[' || c == ']');
    let digit = word.chars().any(|c| c.is_ascii_digit());
    digit && ((word.contains('-') && word.len() >= 10) || (word.contains(':') && word.len() >= 8))
}

fn is_source_token(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/' | '@'))
}

// ------------------------------------------------------------------- filter --

#[derive(Clone, Debug)]
pub struct LogFilter {
    pub min_level: LogLevel,
    pub keyword: String,
    pub source: String,
}

impl Default for LogFilter {
    fn default() -> Self {
        Self {
            min_level: LogLevel::Trace,
            keyword: String::new(),
            source: String::new(),
        }
    }
}

impl LogFilter {
    pub fn matches(&self, line: &LogLine) -> bool {
        if !line.level.passes(self.min_level) {
            return false;
        }
        let keyword = self.keyword.trim().to_lowercase();
        if !keyword.is_empty() {
            let haystack = format!("{}\n{}", line.display(), line.raw).to_lowercase();
            if !haystack.contains(&keyword) {
                return false;
            }
        }
        let source = self.source.trim().to_lowercase();
        if !source.is_empty()
            && !line
                .source
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&source)
        {
            return false;
        }
        true
    }
}

pub fn filter_lines<'a>(lines: &'a [LogLine], filter: &LogFilter) -> Vec<&'a LogLine> {
    lines.iter().filter(|line| filter.matches(line)).collect()
}

// --------------------------------------------------------------- run model --

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStatus {
    /// `failure-*.txt` exists.
    Failed,
    /// Studio wrote the `stop` marker.
    Stopped,
    /// No exit marker. This is not a liveness check; a crashed run also lands
    /// here. Live detection stays out of scope for D-03.3.
    Unmarked,
}

impl RunStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Unmarked => "no exit marker",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunInfo {
    pub run_id: String,
    pub dir: PathBuf,
    /// Epoch milliseconds parsed from `studio-<millis>`.
    pub started_ms: Option<u64>,
    /// Newest mtime of `dora.log` (fallback: the directory itself).
    pub modified: Option<SystemTime>,
    pub status: RunStatus,
    /// First line of `failure-*.txt`, if any.
    pub failure: Option<String>,
    /// Dataset recorded from this run, read from `recording-status.json`.
    pub dataset: Option<PathBuf>,
    pub log_path: PathBuf,
    pub log_bytes: u64,
}

impl RunInfo {
    pub fn started_utc(&self) -> Option<String> {
        self.started_ms.map(format_utc_ms)
    }

    /// The dataset path is shown in the UI only when it still resolves.
    pub fn dataset_ready(&self) -> bool {
        self.dataset.as_ref().is_some_and(|path| path.is_dir())
    }
}

/// Discover `runs/studio-*` directories newest-first. A missing runs directory
/// is an empty result, not an error: first launch has nothing to show.
pub fn discover_runs(runs_dir: &Path) -> Vec<RunInfo> {
    let Ok(entries) = fs::read_dir(runs_dir) else {
        return Vec::new();
    };
    let mut runs: Vec<RunInfo> = entries
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            if !dir.is_dir() {
                return None;
            }
            let run_id = entry.file_name().to_string_lossy().into_owned();
            if !run_id.starts_with(RUN_PREFIX) {
                return None;
            }
            Some(read_run(dir, run_id))
        })
        .collect();
    runs.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| b.run_id.cmp(&a.run_id))
    });
    runs
}

fn read_run(dir: PathBuf, run_id: String) -> RunInfo {
    let started_ms = run_id
        .strip_prefix(RUN_PREFIX)
        .and_then(|stamp| stamp.parse::<u64>().ok());
    let log_path = dir.join("dora.log");
    let log_bytes = fs::metadata(&log_path).map(|meta| meta.len()).unwrap_or(0);
    let modified = fs::metadata(&log_path)
        .and_then(|meta| meta.modified())
        .or_else(|_| fs::metadata(&dir).and_then(|meta| meta.modified()))
        .ok();
    let failure_path = find_failure(&dir);
    let failure = failure_path.as_deref().and_then(read_failure).map(|text| {
        // Keep the panel row readable; the full file remains on disk.
        truncate_chars(&text, 240)
    });
    let status = if failure_path.is_some() {
        RunStatus::Failed
    } else if dir.join("stop").is_file() {
        RunStatus::Stopped
    } else {
        RunStatus::Unmarked
    };
    let dataset = dataset_from_status(&dir);
    RunInfo {
        run_id,
        dir,
        started_ms,
        modified,
        status,
        failure,
        dataset,
        log_path,
        log_bytes,
    }
}

fn find_failure(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.starts_with("failure-").then(|| entry.path())
    })
}

fn read_failure(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let first = text.lines().next().unwrap_or("").trim();
    Some(if first.is_empty() {
        "runtime reported a failure".to_owned()
    } else {
        first.to_owned()
    })
}

fn dataset_from_status(dir: &Path) -> Option<PathBuf> {
    let value = read_json(&dir.join("recording-status.json"))?;
    let dataset = value.get("dataset")?.as_str()?.trim();
    (!dataset.is_empty()).then(|| PathBuf::from(dataset))
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// ---------------------------------------------------------------- log tail --

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogTail {
    pub lines: Vec<String>,
    pub total_bytes: u64,
    /// True when older lines existed before the tail we read.
    pub truncated: bool,
}

/// Read the last `max_lines` lines without loading the whole file.
///
/// The file is scanned backwards in 64 KiB chunks. If the scan would exceed
/// `MAX_TAIL_BYTES`, the oldest read line is dropped and the result is marked
/// truncated. Missing files stay an `io::Error` so callers can tell "no log"
/// apart from "empty log".
pub fn read_log_tail(path: &Path, max_lines: usize) -> io::Result<LogTail> {
    let mut file = File::open(path)?;
    let total_bytes = file.metadata()?.len();
    if total_bytes == 0 {
        return Ok(LogTail {
            total_bytes,
            ..LogTail::default()
        });
    }
    let max_lines = max_lines.max(1);
    let mut position = total_bytes;
    let mut newlines = 0usize;
    let mut buffer: Vec<u8> = Vec::new();
    // One extra newline covers the common "file ends with '\n'" case.
    while position > 0 && newlines < max_lines + 1 && total_bytes - position < MAX_TAIL_BYTES {
        let size = (TAIL_CHUNK as u64).min(position) as usize;
        position -= size as u64;
        file.seek(SeekFrom::Start(position))?;
        let mut chunk = vec![0u8; size];
        file.read_exact(&mut chunk)?;
        newlines += chunk.iter().filter(|byte| **byte == b'\n').count();
        chunk.extend_from_slice(&buffer);
        buffer = chunk;
    }
    let mut truncated = position > 0;
    if truncated {
        // The first chunk boundary is almost always mid-line; drop it.
        if let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            buffer.drain(..=newline);
        } else {
            buffer.clear();
        }
    }
    let text = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_owned())
        .collect();
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.len() > max_lines {
        lines.drain(..lines.len() - max_lines);
        // Older lines were dropped even when the whole file fit in one chunk.
        truncated = true;
    }
    Ok(LogTail {
        lines,
        total_bytes,
        truncated,
    })
}

pub fn parse_lines(lines: &[String]) -> Vec<LogLine> {
    lines
        .iter()
        .filter_map(|line| LogLine::parse(line))
        .collect()
}

#[derive(Clone, Debug)]
pub struct LoadedLog {
    pub lines: Vec<LogLine>,
    pub truncated: bool,
    pub total_bytes: u64,
}

pub fn load_run_log(run: &RunInfo, max_lines: usize) -> io::Result<LoadedLog> {
    let tail = read_log_tail(&run.log_path, max_lines)?;
    Ok(LoadedLog {
        lines: parse_lines(&tail.lines),
        truncated: tail.truncated,
        total_bytes: tail.total_bytes,
    })
}

// ------------------------------------------------------------------ export --

/// Write the current filtered view as `run-<id>-<utc>.log` inside `dir`.
pub fn export_filtered(
    lines: &[&LogLine],
    run_id: &str,
    dir: &Path,
    now_ms: u64,
) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let name = format!(
        "run-{}-{}.log",
        sanitize_run_id(run_id),
        compact_utc_ms(now_ms)
    );
    let path = dir.join(name);
    let mut output = String::new();
    for line in lines {
        output.push_str(&line.to_export());
        output.push('\n');
    }
    fs::write(&path, output)?;
    Ok(path)
}

fn sanitize_run_id(run_id: &str) -> String {
    let cleaned: String = run_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "run".to_owned()
    } else {
        cleaned
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// UTC `YYYY-MM-DD HH:MM:SS`. The panel labels this as UTC; local-time display
/// needs a timezone decision that D-03.3 deliberately leaves open.
pub fn format_utc_ms(milliseconds: u64) -> String {
    let seconds = (milliseconds / 1000) as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        day_seconds / 3600,
        (day_seconds % 3600) / 60,
        day_seconds % 60
    )
}

fn compact_utc_ms(milliseconds: u64) -> String {
    let seconds = (milliseconds / 1000) as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        day_seconds / 3600,
        (day_seconds % 3600) / 60,
        day_seconds % 60
    )
}

/// Howard Hinnant's civil-from-days, valid for the practical epoch range.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let mut shortened: String = text.chars().take(max).collect();
        shortened.push_str("...");
        shortened
    }
}

// ------------------------------------------------------------------- tests --

#[cfg(test)]
mod tests {
    use super::*;

    fn write_run(root: &Path, name: &str) -> PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("dora.log"), "").unwrap();
        dir
    }

    #[test]
    fn missing_runs_directory_is_empty() {
        let temp = tempfile::tempdir().unwrap();
        assert!(discover_runs(&temp.path().join("does-not-exist")).is_empty());
    }

    #[test]
    fn discovers_studio_dirs_newest_first() {
        let temp = tempfile::tempdir().unwrap();
        // Creation order matches the timestamp order so mtime and run id agree.
        for name in ["studio-100", "studio-200", "studio-300", "not-a-run"] {
            write_run(temp.path(), name);
        }
        fs::write(temp.path().join("studio-400"), "file, not a directory").unwrap();
        let runs = discover_runs(temp.path());
        let ids: Vec<_> = runs.iter().map(|run| run.run_id.as_str()).collect();
        assert_eq!(ids, ["studio-300", "studio-200", "studio-100"]);
        assert_eq!(runs[0].started_ms, Some(300));
    }

    #[test]
    fn metadata_reads_dataset_stop_and_failure_flags() {
        let temp = tempfile::tempdir().unwrap();
        let dir = write_run(temp.path(), "studio-1234");
        fs::write(
            dir.join("recording-status.json"),
            r#"{"phase":"saved","dataset":"/tmp/demo/nano-1"}"#,
        )
        .unwrap();
        let run = &discover_runs(temp.path())[0];
        assert_eq!(run.started_ms, Some(1234));
        assert_eq!(run.dataset, Some(PathBuf::from("/tmp/demo/nano-1")));
        assert!(!run.dataset_ready());
        assert_eq!(run.status, RunStatus::Unmarked);

        fs::write(dir.join("stop"), "stop").unwrap();
        assert_eq!(discover_runs(temp.path())[0].status, RunStatus::Stopped);

        fs::write(dir.join("failure-1.txt"), "guard rejected command").unwrap();
        let run = &discover_runs(temp.path())[0];
        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.failure.as_deref(), Some("guard rejected command"));
    }

    #[test]
    fn malformed_run_json_does_not_fail_discovery() {
        let temp = tempfile::tempdir().unwrap();
        let dir = write_run(temp.path(), "studio-9");
        fs::write(dir.join("recording-status.json"), "{oops").unwrap();
        fs::write(dir.join("config.json"), "[]").unwrap();
        let runs = discover_runs(temp.path());
        assert_eq!(runs.len(), 1);
        assert!(runs[0].dataset.is_none());
    }

    #[test]
    fn tail_keeps_the_last_lines_and_marks_truncation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("dora.log");
        let body: String = (0..6000).map(|i| format!("line {i}\n")).collect();
        fs::write(&path, &body).unwrap();
        let tail = read_log_tail(&path, 5_000).unwrap();
        assert_eq!(tail.lines.len(), 5_000);
        assert_eq!(tail.lines.first().unwrap(), "line 1000");
        assert_eq!(tail.lines.last().unwrap(), "line 5999");
        assert!(tail.truncated);
        assert_eq!(tail.total_bytes, body.len() as u64);
    }

    #[test]
    fn tail_handles_small_empty_and_unterminated_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("dora.log");

        fs::write(&path, "a\nb\nc\n").unwrap();
        let tail = read_log_tail(&path, 100).unwrap();
        assert_eq!(tail.lines, ["a", "b", "c"]);
        assert!(!tail.truncated);

        fs::write(&path, "").unwrap();
        let tail = read_log_tail(&path, 100).unwrap();
        assert!(tail.lines.is_empty());
        assert!(!tail.truncated);
        assert_eq!(tail.total_bytes, 0);

        fs::write(&path, "a\nb").unwrap();
        assert_eq!(read_log_tail(&path, 100).unwrap().lines, ["a", "b"]);
    }

    #[test]
    fn missing_log_is_an_error_but_empty_log_is_not() {
        let temp = tempfile::tempdir().unwrap();
        assert!(read_log_tail(&temp.path().join("absent.log"), 10).is_err());
        let path = temp.path().join("empty.log");
        fs::write(&path, "").unwrap();
        assert!(read_log_tail(&path, 10).unwrap().lines.is_empty());
    }

    #[test]
    fn parses_tracing_json_lines() {
        let line = LogLine::parse(
            r#"{"timestamp":"2026-01-02T03:04:05Z","level":"WARN","fields":{"message":"late frame"},"target":"dora_core::node"}"#,
        )
        .unwrap();
        assert_eq!(line.level, LogLevel::Warn);
        assert_eq!(line.source.as_deref(), Some("dora_core::node"));
        assert_eq!(line.message, "late frame");
        assert_eq!(line.ts.as_deref(), Some("2026-01-02T03:04:05Z"));
        assert!(line.display().contains("WARN dora_core::node late frame"));
    }

    #[test]
    fn parses_flat_json_and_numeric_levels() {
        // The 10/20/30/40/50 severity scale (pino/bunyan style).
        let line =
            LogLine::parse(r#"{"level":40,"msg":"motor timeout","module":"guard","ts":123}"#)
                .unwrap();
        assert_eq!(line.level, LogLevel::Warn);
        assert_eq!(line.source.as_deref(), Some("guard"));
        assert_eq!(line.message, "motor timeout");
        assert_eq!(line.ts.as_deref(), Some("123"));

        let line = LogLine::parse(r#"{"level":50,"msg":"motor stalled"}"#).unwrap();
        assert_eq!(line.level, LogLevel::Error);
    }

    #[test]
    fn parses_plain_tracing_and_bracket_lines() {
        let line = LogLine::parse("2026-01-02T03:04:05Z  INFO dora_core::node: ready").unwrap();
        assert_eq!(line.level, LogLevel::Info);
        assert_eq!(line.source.as_deref(), Some("dora_core::node"));
        assert_eq!(line.message, "ready");
        assert_eq!(line.ts.as_deref(), Some("2026-01-02T03:04:05Z"));

        let line = LogLine::parse("[2026-01-02T03:04:05Z INFO sim] started").unwrap();
        assert_eq!(line.level, LogLevel::Info);
        assert_eq!(line.source.as_deref(), Some("sim"));
        assert_eq!(line.message, "started");

        let line = LogLine::parse("[ERROR] driver: boom").unwrap();
        assert_eq!(line.level, LogLevel::Error);
        assert_eq!(line.message, "driver: boom");
    }

    #[test]
    fn keeps_non_json_and_blank_lines_honest() {
        let line = LogLine::parse("plain launcher message without a level").unwrap();
        assert_eq!(line.level, LogLevel::Unknown);
        assert_eq!(line.message, "plain launcher message without a level");
        assert!(LogLine::parse("   \n").is_none());
    }

    #[test]
    fn filter_combines_level_keyword_and_source() {
        let lines = parse_lines(&[
            "2026-01-02T03:04:05Z INFO sim: frame 1".to_owned(),
            "2026-01-02T03:04:06Z WARN sim: frame late".to_owned(),
            "2026-01-02T03:04:07Z ERROR guard: rejected".to_owned(),
            "raw message".to_owned(),
        ]);
        let filter = LogFilter {
            min_level: LogLevel::Warn,
            keyword: "frame".to_owned(),
            source: "sim".to_owned(),
        };
        let matched = filter_lines(&lines, &filter);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].level, LogLevel::Warn);
    }

    #[test]
    fn unknown_level_lines_stay_visible_at_info() {
        let lines = parse_lines(&["raw message".to_owned()]);
        let filter = LogFilter {
            min_level: LogLevel::Info,
            ..LogFilter::default()
        };
        assert_eq!(filter_lines(&lines, &filter).len(), 1);
    }

    #[test]
    fn export_writes_the_requested_name_and_content() {
        let temp = tempfile::tempdir().unwrap();
        let lines = parse_lines(&[
            "2026-01-02T03:04:05Z INFO sim: frame 1".to_owned(),
            r#"{"level":"ERROR","message":"boom"}"#.to_owned(),
        ]);
        let refs: Vec<_> = lines.iter().collect();
        let path = export_filtered(&refs, "studio-42", temp.path(), 0).unwrap();
        assert_eq!(
            path.file_name().unwrap(),
            "run-studio-42-19700101-000000.log"
        );
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("INFO sim frame 1"), "{body}");
        assert!(body.contains("ERROR boom"), "{body}");
    }

    #[test]
    fn utc_formatting_is_stable() {
        assert_eq!(format_utc_ms(0), "1970-01-01 00:00:00");
        assert_eq!(format_utc_ms(1_700_000_000_000), "2023-11-14 22:13:20");
        assert_eq!(compact_utc_ms(1_700_000_000_000), "20231114-221320");
    }

    #[test]
    fn run_id_is_sanitized_for_export() {
        assert_eq!(sanitize_run_id("studio-1/../x"), "studio-1____x");
        assert_eq!(sanitize_run_id(""), "run");
    }
}
