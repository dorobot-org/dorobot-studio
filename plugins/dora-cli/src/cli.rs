//! Read-only adapter around the `dora` CLI binary.
//!
//! Studio deliberately does not link `dora-node-api` and does not speak the
//! coordinator protocol. It runs the CLI as a child process and shows the
//! result. Only read commands are implemented here: there is no `param set`,
//! `topic pub`, `node restart` or `dora start`.
//!
//! # Verified argument shapes
//!
//! The argument shapes below were checked against the locally installed
//! `dora-cli 1.0.0-rc1` (`dora <cmd> --help`) on 2026-11-26. They are *not*
//! guaranteed across Dora forks, which is why the panel shows the resolved
//! binary path and its `--version` output instead of assuming a version.
//!
//! ```text
//! dora --version
//! dora list
//! dora topic list
//! dora node list
//! dora node info <NODE>
//! dora param list <NODE>
//! dora param get <NODE> <KEY>
//! dora topic echo <NODE>/<OUTPUT>          (streams until interrupted)
//! dora topic hz   <NODE>/<OUTPUT> --window <SECONDS> --duration <SECONDS>
//! ```
//!
//! Important deviations from the original task sketch, verified with `--help`:
//!
//! - `param list` and `param get` take the node **positionally**, not as
//!   `--node <node>`.
//! - `topic echo`/`topic hz` take data identifiers as `node_id/output_id`.
//! - `topic hz` has no TUI when stdout is a pipe and then **requires**
//!   `--duration`; without it the CLI exits with an argument error. The panel
//!   therefore passes a duration and explains that Hz shows final stats.
//! - `-d/--dataflow <UUID_OR_NAME>` is optional for all of these. The adapter
//!   omits it; a coordinator running more than one dataflow may need it, which
//!   is a documented limitation of this skeleton.
//!
//! Every command is passed as an argument array (`Command::args`); no command
//! is ever assembled into a shell string.
//!
//! # Streaming
//!
//! [`StreamSession`] starts the child, reads stdout/stderr line by line on
//! background threads and pushes lines into a bounded ring buffer. Dropping the
//! session kills and reaps the child. The panel polls [`StreamSession::stats`]
//! and [`StreamSession::tail`] from the UI thread, so a slow stream never blocks
//! the frame.

use std::{
    collections::VecDeque,
    env, fs, io,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

/// Environment variable that overrides executable discovery.
pub const BIN_ENV: &str = "DOROBOT_DORA_BIN";
/// Timeout for one-shot commands (`--version`, `list`, `topic list`, …).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
/// Ring-buffer capacity for a streaming command.
pub const DEFAULT_RING_LINES: usize = 5_000;
/// `topic hz` sliding window, seconds.
pub const HZ_WINDOW_SECONDS: u32 = 10;
/// `topic hz` duration when stdout is not a TTY, seconds.
pub const HZ_DURATION_SECONDS: u32 = 60;

// ------------------------------------------------------------------ errors --

#[derive(Debug)]
pub enum CliError {
    /// The executable could not be resolved (env override or PATH).
    BinaryNotFound(String),
    /// The executable exists but `Command::spawn` failed.
    Spawn {
        program: PathBuf,
        args: Vec<String>,
        source: io::Error,
    },
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BinaryNotFound(message) => formatter.write_str(message),
            Self::Spawn {
                program,
                args,
                source,
            } => write!(
                formatter,
                "cannot start {} {}: {source}",
                program.display(),
                args.join(" ")
            ),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn { source, .. } => Some(source),
            Self::BinaryNotFound(_) => None,
        }
    }
}

// -------------------------------------------------------------- executable --

/// Resolve the CLI from the process environment.
pub fn resolve_binary() -> Result<PathBuf, CliError> {
    resolve_binary_from(
        env::var_os(BIN_ENV).as_deref(),
        env::var_os("PATH").as_deref(),
    )
}

/// Resolve with explicit inputs so tests never mutate the process environment.
///
/// Priority: `DOROBOT_DORA_BIN` (must point at an executable file) → first
/// executable `dora` on `PATH`. A set-but-broken override is an error instead
/// of silently falling back, so a typo is visible in the panel.
pub fn resolve_binary_from(
    override_bin: Option<&std::ffi::OsStr>,
    path_var: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, CliError> {
    if let Some(value) = override_bin {
        if value.is_empty() {
            return Err(CliError::BinaryNotFound(format!(
                "{BIN_ENV} is set but empty; point it at the dora executable or unset it"
            )));
        }
        let path = PathBuf::from(value);
        if is_executable_file(&path) {
            return Ok(path);
        }
        return Err(CliError::BinaryNotFound(format!(
            "{BIN_ENV} points to {} but it is not an executable file",
            path.display()
        )));
    }

    let Some(path_var) = path_var else {
        return Err(CliError::BinaryNotFound(format!(
            "dora executable not found and PATH is unset; set {BIN_ENV} or add dora-cli to PATH"
        )));
    };
    let mut searched = Vec::new();
    for directory in env::split_paths(path_var) {
        // An empty PATH entry means "current directory" on Unix; skip it so a
        // repository file named `dora` cannot be executed implicitly.
        if directory.as_os_str().is_empty() {
            continue;
        }
        searched.push(directory.display().to_string());
        for name in executable_names() {
            let candidate = directory.join(name);
            if is_executable_file(&candidate) {
                return Ok(candidate);
            }
        }
    }
    Err(CliError::BinaryNotFound(format!(
        "dora executable not found; set {BIN_ENV} or add dora-cli to PATH (searched: {})",
        if searched.is_empty() {
            "<empty PATH>".to_owned()
        } else {
            searched.join(", ")
        }
    )))
}

#[cfg(windows)]
fn executable_names() -> &'static [&'static str] {
    &["dora.exe", "dora"]
}

#[cfg(not(windows))]
fn executable_names() -> &'static [&'static str] {
    &["dora"]
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

// ------------------------------------------------------------- one-shot API --

/// Result of one completed one-shot invocation.
#[derive(Clone, Debug)]
pub struct CliOutput {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// `None` when the process was killed by a signal or by the timeout.
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub elapsed: Duration,
}

impl CliOutput {
    pub fn success(&self) -> bool {
        !self.timed_out && self.status == Some(0)
    }

    /// Compact line for the panel's command log: exit code and duration.
    pub fn summary(&self) -> String {
        format!(
            "{} {}  →  {} · {} ms",
            program_label(&self.program),
            self.args.join(" "),
            match (self.timed_out, self.status) {
                (true, _) => "timed out".to_owned(),
                (false, Some(0)) => "exit 0".to_owned(),
                (false, Some(code)) => format!("exit {code}"),
                (false, None) => "no exit status".to_owned(),
            },
            self.elapsed.as_millis()
        )
    }

    pub fn first_error_line(&self) -> Option<&str> {
        self.stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
    }

    /// stdout when the command succeeded, otherwise stderr (falling back to
    /// stdout). Used by the panel's read-only detail panes.
    pub fn body(&self) -> String {
        if self.success() {
            self.stdout.clone()
        } else if !self.stderr.trim().is_empty() {
            self.stderr.clone()
        } else {
            self.stdout.clone()
        }
    }
}

#[derive(Clone, Debug)]
pub struct DoraCli {
    program: PathBuf,
    timeout: Duration,
}

impl DoraCli {
    /// Resolve the executable and use [`DEFAULT_TIMEOUT`] for one-shot commands.
    pub fn discover() -> Result<Self, CliError> {
        Ok(Self::with_program(resolve_binary()?))
    }

    pub fn with_program(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    pub fn run(&self, args: &[&str]) -> Result<CliOutput, CliError> {
        self.run_with_timeout(args, self.timeout)
    }

    /// Run a one-shot command, killing it after `timeout`.
    ///
    /// stdout and stderr are drained on separate threads so a noisy child
    /// cannot deadlock on a full pipe. On timeout the process is killed and
    /// reaped; the partial output is returned with `timed_out = true` rather
    /// than thrown away, because it is often the only diagnostic available.
    pub fn run_with_timeout(
        &self,
        args: &[&str],
        timeout: Duration,
    ) -> Result<CliOutput, CliError> {
        let started = Instant::now();
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|source| CliError::Spawn {
            program: self.program.clone(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            source,
        })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let stdout_reader = thread::spawn(move || read_all(stdout));
        let stderr_reader = thread::spawn(move || read_all(stderr));

        let deadline = started + timeout;
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CliError::Spawn {
                        program: self.program.clone(),
                        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
                        source: error,
                    });
                }
            }
            if Instant::now() >= deadline {
                timed_out = true;
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            thread::sleep(Duration::from_millis(5));
        };
        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();
        Ok(CliOutput {
            program: self.program.clone(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            status: if timed_out { None } else { status },
            stdout,
            stderr,
            timed_out,
            elapsed: started.elapsed(),
        })
    }

    pub fn version(&self) -> Result<CliOutput, CliError> {
        self.run(&["--version"])
    }

    pub fn list(&self) -> Result<CliOutput, CliError> {
        self.run(&["list"])
    }

    pub fn topic_list(&self) -> Result<CliOutput, CliError> {
        self.run(&["topic", "list"])
    }

    pub fn node_list(&self) -> Result<CliOutput, CliError> {
        self.run(&["node", "list"])
    }

    pub fn node_info(&self, node: &str) -> Result<CliOutput, CliError> {
        self.run(&["node", "info", node])
    }

    /// Verified argument order: node is positional (`dora param list <NODE>`).
    pub fn param_list(&self, node: &str) -> Result<CliOutput, CliError> {
        self.run(&["param", "list", node])
    }

    /// Verified argument order: node then key (`dora param get <NODE> <KEY>`).
    pub fn param_get(&self, node: &str, key: &str) -> Result<CliOutput, CliError> {
        self.run(&["param", "get", node, key])
    }

    /// `dora topic echo <topic>`; streams until the session is dropped.
    pub fn topic_echo_args(topic: &str) -> Vec<String> {
        vec!["topic".into(), "echo".into(), topic.into()]
    }

    /// `dora topic hz <topic> --window 10 --duration 60`.
    ///
    /// `--duration` is required because Studio always gives the child a pipe,
    /// and the CLI refuses to start the TUI when stdout is not a terminal.
    pub fn topic_hz_args(topic: &str) -> Vec<String> {
        vec![
            "topic".into(),
            "hz".into(),
            topic.into(),
            "--window".into(),
            HZ_WINDOW_SECONDS.to_string(),
            "--duration".into(),
            HZ_DURATION_SECONDS.to_string(),
        ]
    }

    pub fn start_stream(
        &self,
        args: &[String],
        capacity: usize,
    ) -> Result<StreamSession, CliError> {
        StreamSession::start(self, args, capacity)
    }
}

fn program_label(program: &Path) -> String {
    program
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.display().to_string())
}

fn read_all(mut reader: impl Read) -> String {
    let mut bytes = Vec::new();
    let _ = reader.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------------------------------------------------------------- streaming --

/// Bounded line ring. The oldest line is dropped when the buffer is full.
#[derive(Debug)]
struct RingBuffer {
    lines: VecDeque<String>,
    capacity: usize,
    dropped: u64,
    total: u64,
    revision: u64,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            capacity: capacity.max(1),
            dropped: 0,
            total: 0,
            revision: 0,
        }
    }

    fn push(&mut self, line: String) {
        self.total += 1;
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
            self.dropped += 1;
        }
        self.lines.push_back(line);
        self.revision += 1;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamStats {
    pub len: usize,
    pub capacity: usize,
    /// Lines pushed out by the ring (older than the window).
    pub dropped: u64,
    pub total: u64,
    /// Monotonic counter; the UI only re-reads the buffer when it changes.
    pub revision: u64,
}

/// A running streaming command (`topic echo` / `topic hz`).
///
/// Dropping the session kills and reaps the child, so the panel's "Stop" and
/// switching to another stream cannot leave orphan CLI processes behind.
pub struct StreamSession {
    program: PathBuf,
    args: Vec<String>,
    child: Option<Child>,
    ring: Arc<Mutex<RingBuffer>>,
    started: Instant,
    exit_code: Option<i32>,
}

impl StreamSession {
    fn start(cli: &DoraCli, args: &[String], capacity: usize) -> Result<Self, CliError> {
        let mut command = Command::new(&cli.program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|source| CliError::Spawn {
            program: cli.program.clone(),
            args: args.to_vec(),
            source,
        })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let ring = Arc::new(Mutex::new(RingBuffer::new(capacity)));
        spawn_line_reader(stdout, Arc::clone(&ring), "");
        spawn_line_reader(stderr, Arc::clone(&ring), "[stderr] ");
        Ok(Self {
            program: cli.program.clone(),
            args: args.to_vec(),
            child: Some(child),
            ring,
            started: Instant::now(),
            exit_code: None,
        })
    }

    #[cfg(test)]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn command_line(&self) -> String {
        format!("{} {}", self.program.display(), self.args.join(" "))
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn stats(&self) -> StreamStats {
        let ring = lock_ring(&self.ring);
        StreamStats {
            len: ring.lines.len(),
            capacity: ring.capacity,
            dropped: ring.dropped,
            total: ring.total,
            revision: ring.revision,
        }
    }

    /// Newest `max` lines, oldest-first, for display.
    pub fn tail(&self, max: usize) -> Vec<String> {
        let ring = lock_ring(&self.ring);
        let skip = ring.lines.len().saturating_sub(max);
        ring.lines.iter().skip(skip).cloned().collect()
    }

    /// Non-blocking liveness/exit check. Reaps the child when it has exited.
    pub fn is_running(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return false;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.exit_code = status.code();
                self.child = None;
                false
            }
            Ok(None) => true,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                self.child = None;
                false
            }
        }
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
}

impl Drop for StreamSession {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn_line_reader(
    reader: impl Read + Send + 'static,
    ring: Arc<Mutex<RingBuffer>>,
    prefix: &str,
) {
    // `prefix` is promoted to a String so the thread can own it.
    let prefix = prefix.to_owned();
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&buffer);
                    let line =
                        text.trim_end_matches(|character| character == '\n' || character == '\r');
                    if !line.is_empty() {
                        lock_ring(&ring).push(format!("{prefix}{line}"));
                    }
                }
            }
        }
    });
}

fn lock_ring(ring: &Arc<Mutex<RingBuffer>>) -> MutexGuard<'_, RingBuffer> {
    ring.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ----------------------------------------------------------- output parsing --

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdKind {
    Node,
    Topic,
    Param,
}

/// Extract identifiers from `topic list` / `node list` / `param list` output.
///
/// The CLI's table layout is not a stable contract, so this accepts both the
/// default table and `--format json`:
///
/// - JSON: walk objects/arrays and pick values under common id keys. Topics are
///   additionally required to contain `/` (`node_id/output_id`).
/// - Text: first whitespace token of every non-header line. Box-drawing
///   characters from table borders are stripped first.
///
/// Unknown shapes yield an empty list; callers show the raw output instead of
/// pretending there are entries.
pub fn parse_ids(stdout: &str, kind: IdKind) -> Vec<String> {
    let mut ids = Vec::new();
    let trimmed = stdout.trim();
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            collect_json_ids(&value, kind, &mut ids);
        }
    }
    if ids.is_empty() {
        for line in trimmed.lines() {
            if let Some(id) = table_id(line, kind) {
                push_unique(&mut ids, id);
            }
        }
    }
    ids
}

fn collect_json_ids(value: &serde_json::Value, kind: IdKind, ids: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(object) => {
            for key in [
                "id",
                "node",
                "node_id",
                "node_name",
                "topic",
                "topic_id",
                "output",
                "output_id",
                "key",
                "name",
            ] {
                if let Some(text) = object.get(key).and_then(serde_json::Value::as_str) {
                    if let Some(id) = normalize_id(text, kind) {
                        push_unique(ids, id);
                    }
                }
            }
            for child in object.values() {
                collect_json_ids(child, kind, ids);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_json_ids(item, kind, ids);
            }
        }
        _ => {}
    }
}

fn table_id(line: &str, kind: IdKind) -> Option<String> {
    let cleaned = line.trim_matches(|character: char| {
        character.is_whitespace() || "│|┌┐└┘├┤┬┴┼─".contains(character)
    });
    if cleaned.is_empty() {
        return None;
    }
    let first = cleaned.split_whitespace().next()?;
    let upper = first.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "NODE"
            | "NODES"
            | "TOPIC"
            | "TOPICS"
            | "PARAM"
            | "PARAMS"
            | "KEY"
            | "VALUE"
            | "ID"
            | "NAME"
            | "UUID"
            | "DATAFLOW"
            | "STATUS"
            | "TYPE"
            | "NO"
            | "ERROR"
            | "HINT"
            | "WARNING"
    ) {
        return None;
    }
    if first.starts_with('-') || first.starts_with('[') {
        return None;
    }
    normalize_id(first, kind)
}

fn normalize_id(text: &str, kind: IdKind) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 256 || text.chars().any(char::is_whitespace) {
        return None;
    }
    if kind == IdKind::Topic && !text.contains('/') {
        return None;
    }
    Some(text.to_owned())
}

fn push_unique(ids: &mut Vec<String>, id: String) {
    if !ids.contains(&id) {
        ids.push(id);
    }
}

// -------------------------------------------------------------------- tests --

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn fake_cli_named(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn fake_cli(dir: &Path, body: &str) -> PathBuf {
        fake_cli_named(dir, "fake-dora", body)
    }

    fn wait_until(mut condition: impl FnMut() -> bool, timeout: Duration, message: &str) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if condition() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {message}");
    }

    fn process_alive(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[test]
    fn resolution_prefers_the_env_override_and_falls_back_to_path() {
        let temp = tempfile::tempdir().unwrap();
        let path_dir = temp.path().join("bin");
        fs::create_dir_all(&path_dir).unwrap();
        let from_env = fake_cli(&temp.path(), "exit 0");
        let from_path = fake_cli_named(&path_dir, "dora", "exit 0");
        let path_var = std::ffi::OsString::from(path_dir.as_os_str());

        assert_eq!(
            resolve_binary_from(Some(from_env.as_os_str()), Some(&path_var)).unwrap(),
            from_env
        );
        assert_eq!(
            resolve_binary_from(None, Some(&path_var)).unwrap(),
            from_path
        );

        let missing = temp.path().join("nope").into_os_string();
        let error = resolve_binary_from(Some(&missing), Some(&path_var)).unwrap_err();
        assert!(format!("{error}").contains(BIN_ENV), "{error}");
        let error = resolve_binary_from(None, Some(std::ffi::OsStr::new(""))).unwrap_err();
        assert!(format!("{error}").contains("not found"), "{error}");
    }

    #[test]
    fn one_shot_commands_use_the_verified_argument_shapes() {
        let temp = tempfile::tempdir().unwrap();
        // Echo every argument, one per line, so repeated invocations can be
        // checked against the exact argv the adapter builds.
        let binary = fake_cli(
            temp.path(),
            "for arg in \"$@\"; do printf '%s\\n' \"$arg\"; done",
        );
        let cli = DoraCli::with_program(&binary);
        for (output, expected) in [
            (cli.version().unwrap(), vec!["--version"]),
            (cli.list().unwrap(), vec!["list"]),
            (cli.topic_list().unwrap(), vec!["topic", "list"]),
            (cli.node_list().unwrap(), vec!["node", "list"]),
            (cli.node_info("arm").unwrap(), vec!["node", "info", "arm"]),
            (cli.param_list("arm").unwrap(), vec!["param", "list", "arm"]),
            (
                cli.param_get("arm", "speed").unwrap(),
                vec!["param", "get", "arm", "speed"],
            ),
        ] {
            let actual: Vec<_> = output.stdout.lines().collect();
            assert_eq!(actual, expected, "argv mismatch for {}", output.summary());
        }
    }

    #[test]
    fn arguments_with_spaces_and_shell_characters_are_not_split() {
        let temp = tempfile::tempdir().unwrap();
        let binary = fake_cli(
            temp.path(),
            "for arg in \"$@\"; do printf '%s\\n' \"$arg\"; done",
        );
        let cli = DoraCli::with_program(&binary);
        let output = cli
            .param_get("node with space", "key; echo injected")
            .unwrap();
        let actual: Vec<_> = output.stdout.lines().collect();
        assert_eq!(
            actual,
            ["param", "get", "node with space", "key; echo injected"]
        );
        assert!(!output.stdout.contains("\ninjected"));
    }

    #[test]
    fn one_shot_captures_status_and_stderr() {
        let temp = tempfile::tempdir().unwrap();
        let binary = fake_cli(
            temp.path(),
            "echo working; echo 'coordinator refused' >&2; exit 3",
        );
        let cli = DoraCli::with_program(&binary);
        let output = cli.list().unwrap();
        assert_eq!(output.status, Some(3));
        assert!(!output.success());
        assert_eq!(output.stdout.trim(), "working");
        assert_eq!(output.first_error_line(), Some("coordinator refused"));
    }

    #[test]
    fn one_shot_honours_the_timeout_and_kills_the_child() {
        let temp = tempfile::tempdir().unwrap();
        let pid_file = temp.path().join("pid");
        // `exec` makes the shell replace itself, so the pid written here is the
        // process the adapter must kill.
        let binary = fake_cli(
            temp.path(),
            &format!("echo $$ > '{}'; exec sleep 30", pid_file.display()),
        );
        let cli = DoraCli::with_program(&binary);
        let started = Instant::now();
        let output = cli
            .run_with_timeout(&["list"], Duration::from_secs(3))
            .unwrap();
        assert!(output.timed_out);
        assert_eq!(output.status, None);
        assert!(started.elapsed() < Duration::from_secs(5));
        let pid = fs::read_to_string(&pid_file).unwrap_or_else(|error| {
            panic!(
                "fake CLI did not write its pid: {error}; stderr={:?}",
                output.stderr
            )
        });
        wait_until(
            || !process_alive(pid.trim()),
            Duration::from_secs(3),
            "timed-out child to be killed",
        );
    }

    #[test]
    fn streaming_reads_lines_into_a_bounded_ring() {
        let temp = tempfile::tempdir().unwrap();
        let binary = fake_cli(
            temp.path(),
            "i=1\nwhile [ $i -le 20 ]; do echo \"line $i\"; i=$((i+1)); done\nexec sleep 30",
        );
        let cli = DoraCli::with_program(&binary);
        let session = cli
            .start_stream(&["topic".into(), "echo".into(), "arm/out".into()], 5)
            .unwrap();
        wait_until(
            || session.stats().total >= 20,
            Duration::from_secs(5),
            "20 streamed lines",
        );
        let stats = session.stats();
        assert_eq!(stats.capacity, 5);
        assert_eq!(stats.len, 5);
        assert_eq!(stats.total, 20);
        assert_eq!(stats.dropped, 15);
        assert_eq!(
            session.tail(5),
            ["line 16", "line 17", "line 18", "line 19", "line 20"]
        );
        assert_eq!(session.tail(2), ["line 19", "line 20"]);
    }

    #[test]
    fn dropping_a_stream_kills_the_child() {
        let temp = tempfile::tempdir().unwrap();
        let pid_file = temp.path().join("stream-pid");
        let binary = fake_cli(
            temp.path(),
            &format!("echo $$ > '{}'; exec sleep 30", pid_file.display()),
        );
        let cli = DoraCli::with_program(&binary);
        let session = cli
            .start_stream(&DoraCli::topic_echo_args("arm/out"), DEFAULT_RING_LINES)
            .unwrap();
        assert_eq!(session.args(), ["topic", "echo", "arm/out"]);
        wait_until(
            || pid_file.exists(),
            Duration::from_secs(5),
            "stream child pid",
        );
        let pid = fs::read_to_string(&pid_file).unwrap();
        assert!(process_alive(pid.trim()));
        drop(session);
        wait_until(
            || !process_alive(pid.trim()),
            Duration::from_secs(3),
            "stream child to be killed on drop",
        );
    }

    #[test]
    fn hz_arguments_include_the_non_tty_duration() {
        assert_eq!(
            DoraCli::topic_hz_args("arm/out"),
            [
                "topic",
                "hz",
                "arm/out",
                "--window",
                "10",
                "--duration",
                "60"
            ]
        );
        assert_eq!(
            DoraCli::topic_echo_args("arm/out"),
            ["topic", "echo", "arm/out"]
        );
    }

    #[test]
    fn parses_table_and_json_ids_without_duplicates() {
        let table = "\
TOPIC                 TYPE
arm/out               Float32
camera/image          Image
arm/out               Float32
";
        assert_eq!(parse_ids(table, IdKind::Topic), ["arm/out", "camera/image"]);
        assert!(parse_ids(table, IdKind::Node).contains(&"arm/out".to_owned()));
        assert_eq!(
            parse_ids("arm\ncamera\narm\n", IdKind::Node),
            ["arm", "camera"]
        );

        let json = r#"[{"id":"arm","status":"running"},{"node_id":"camera"}]"#;
        assert_eq!(parse_ids(json, IdKind::Node), ["arm", "camera"]);
        let json = r#"{"topics":[{"output_id":"arm/out"},{"id":"cam/image"}]}"#;
        assert_eq!(parse_ids(json, IdKind::Topic), ["arm/out", "cam/image"]);
        assert!(parse_ids("No topics found", IdKind::Topic).is_empty());
    }
}
