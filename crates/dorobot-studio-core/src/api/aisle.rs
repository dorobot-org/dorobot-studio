//! Local simulation jobs shared by the four AISLE Studio panels.
use serde_json::Value;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    fs, io,
    io::BufRead,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const REFERENCE: &str = "Inspect the left shoulder with a small positive joint movement.";
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Robot {
    Nano,
    G1,
}
impl Robot {
    pub fn name(self) -> &'static str {
        match self {
            Self::Nano => "Nano",
            Self::G1 => "G1",
        }
    }
    pub fn model(self) -> PathBuf {
        crate::runtime::root().join(match self {
            Self::Nano => "data/adora_nano/adora_nano.urdf",
            Self::G1 => "data/g1/g1.urdf",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Planner {
    Reference,
    Codex,
    OctosDeepSeek,
}
impl Planner {
    fn name(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Codex => "codex",
            Self::OctosDeepSeek => "octos-deepseek",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RgbTiming {
    Normal,
    Intermittent,
    Disconnected,
}
impl RgbTiming {
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Intermittent => "intermittent",
            Self::Disconnected => "disconnected",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetMotion {
    Stationary,
    PositiveY,
    NegativeY,
}
impl TargetMotion {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stationary => "stationary",
            Self::PositiveY => "slide-positive",
            Self::NegativeY => "slide-negative",
        }
    }
}
#[derive(Clone, Debug)]
pub enum Action {
    Select(Robot),
    Prepare {
        task: String,
        planner: Planner,
        revise: bool,
    },
    Inspect,
    Fixture(bool),
    Impact(bool),
    RgbTiming(RgbTiming),
    TargetMotion(TargetMotion),
    ToggleOnboardContext,
    EditRoom(bool),
    ImportRoom {
        source: String,
        edit: String,
    },
    DraftRoom {
        source: String,
        planner: Planner,
    },
    ApplyRoomDraft,
    DiscardRoomDraft,
    DefaultRoom,
    ShowEvaluation(bool),
    ShowDevice,
    DevicePreflight(Option<String>),
    OpenPreflight,
    Evaluate(String),
    SelectFailure(bool),
    Layout(u8),
    OlderPlan,
    NewerPlan,
    Run,
    Workers(bool),
    CycleBackgroundPeriod,
    ToggleObservationClock,
    ConnectWorker(String),
    Worker(WorkerAction),
    Stop,
    Previous,
    Next,
    Open,
    Export,
    ExportChange(RevisionSide),
    InspectDecision,
    TrainReach,
    TrainGrasp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationClock {
    Simulation,
    Wall,
}

impl ObservationClock {
    pub fn name(self) -> &'static str {
        match self {
            Self::Simulation => "simulation",
            Self::Wall => "wall",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum WorkerAction {
    Connect,
    Refresh,
    Stream,
    Observations,
    Previous,
    Next,
    Cancel,
    Fetch,
    Watch,
    Sample,
}
impl WorkerAction {
    fn name(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Refresh => "refresh",
            Self::Stream => "stream",
            Self::Observations => "observations",
            Self::Previous => "previous",
            Self::Next => "next",
            Self::Cancel => "cancel",
            Self::Fetch => "fetch",
            Self::Watch => "watch",
            Self::Sample => "sample",
        }
    }
}
fn read(path: &Path, name: &str) -> Option<Value> {
    serde_json::from_slice(&fs::read(path.join(name)).ok()?).ok()
}
#[derive(Clone, Copy, Debug)]
pub enum RevisionSide {
    Previous,
    Current,
}
impl RevisionSide {
    pub fn index(self) -> usize {
        match self {
            Self::Previous => 0,
            Self::Current => 1,
        }
    }
}

#[derive(serde::Serialize)]
pub struct ChangeFrame {
    pub trial: PathBuf,
    pub action_tick: u64,
    pub sample_tick: u64,
    pub index: usize,
    pub time_s: f64,
}

#[derive(serde::Serialize)]
pub struct TransferComparison {
    pub summary: String,
    pub frames: [Option<ChangeFrame>; 2],
    pub packets: [Option<(PathBuf, u64)>; 2],
}

impl TransferComparison {
    fn matches_observations(&self, receipt: &Value) -> bool {
        self.packets.iter().any(Option::is_some)
            && ["previous", "current"]
                .iter()
                .zip(&self.packets)
                .all(|(name, packet)| {
                    let view = &receipt["views"][*name];
                    match packet {
                        Some((path, tick)) => {
                            view["tick"].as_u64() == Some(*tick)
                                && view["trial"]
                                    .as_str()
                                    .is_some_and(|id| path.file_name().is_some_and(|n| n == id))
                        }
                        None => view.is_null(),
                    }
                })
    }
}
fn change_frame(trial: &Path, action_tick: u64, result: &Value) -> Option<ChangeFrame> {
    let recording = read(trial, "recording.json")?;
    let fps = recording["fps"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.)?;
    let samples = recording["samples"].as_array()?;
    let ticks = samples
        .iter()
        .map(|row| row["step"].as_u64())
        .collect::<Option<Vec<_>>>()?;
    if ticks.is_empty()
        || ticks.len() as u64 != result["frames"].as_u64()?
        || !ticks.windows(2).all(|w| w[0] < w[1])
    {
        return None;
    }
    // LeRobot timestamps are zero-based frame numbers divided by the recorded FPS.
    // For an action before the first exposure, show the earliest available frame.
    let index = ticks
        .partition_point(|tick| *tick <= action_tick)
        .saturating_sub(1);
    Some(ChangeFrame {
        trial: trial.to_owned(),
        action_tick,
        sample_tick: ticks[index],
        index,
        time_s: index as f64 / fps,
    })
}

struct ActionDifference {
    summary: String,
    ticks: Option<[u64; 2]>,
}

fn compare_actions(
    previous: &Path,
    current: &Path,
    end_ticks: [Option<u64>; 2],
) -> io::Result<ActionDifference> {
    let stream = |p: &Path| {
        Ok::<_, io::Error>(io::BufReader::new(fs::File::open(p.join("actions.jsonl"))?).lines())
    };
    let (mut left, mut right) = (stream(previous)?, stream(current)?);
    for index in 1usize.. {
        let (a, b) = (left.next().transpose()?, right.next().transpose()?);
        let parse = |line: Option<String>| -> io::Result<Option<Value>> {
            let row: Option<Value> = line.map(|text| serde_json::from_str(&text)).transpose()?;
            if row.as_ref().is_some_and(|v| !v.is_object()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid action row",
                ));
            }
            Ok(row)
        };
        let (a, b) = (parse(a)?, parse(b)?);
        if a.is_none() && b.is_none() {
            return Ok(ActionDifference {
                summary: format!("All {} recorded actions match.", index - 1),
                ticks: None,
            });
        }
        if a == b {
            continue;
        }
        let describe = |row: &Option<Value>| match row {
            Some(v) => format!(
                "{} at tick {}",
                v["op"].as_str().unwrap_or("unknown"),
                v["tick"]
            ),
            None => "trace ended".into(),
        };
        let tick = |row: &Option<Value>, end| match row {
            Some(v) => v["tick"].as_u64(),
            None => end,
        };
        return Ok(ActionDifference {
            summary: format!(
                "First change at action {index} · previous {}, current {}.",
                describe(&a),
                describe(&b)
            ),
            ticks: tick(&a, end_ticks[0])
                .zip(tick(&b, end_ticks[1]))
                .map(|(x, y)| [x, y]),
        });
    }
    unreachable!()
}

fn compare_transfer(
    history: &[PathBuf],
    path: &Path,
    current: &Value,
) -> Option<TransferComparison> {
    if current["kind"] != "table-transfer" || !current["task_success"].is_boolean() {
        return None;
    }
    let source = read(path, "source.json")?;
    let calibration = read(path, "calibration.json")?;
    let settled = |v: &Value| {
        v["cubes"]
            .as_object()
            .map(|c| c.values().filter(|r| r["passed"] == true).count())
    };
    for previous in history.iter().rev().filter(|p| p.as_path() < path) {
        let Some(prior) = read(previous, "result.json") else {
            continue;
        };
        if !prior["task_success"].is_boolean()
            || ![
                "kind",
                "robot",
                "model_hash",
                "fixture",
                "requested",
                "seed",
            ]
            .iter()
            .all(|k| !current[*k].is_null() && current[*k] == prior[*k])
        {
            continue;
        }
        let Some(old) = read(previous, "source.json") else {
            continue;
        };
        let Some(old_cal) = read(previous, "calibration.json") else {
            continue;
        };
        if source["room_revision_sha256"] != old["room_revision_sha256"]
            || source["wrist_rgb_timing"].as_str().unwrap_or("normal")
                != old["wrist_rgb_timing"].as_str().unwrap_or("normal")
            || source["support_code_hashes"]["nano_wrist_timing.py"]
                != old["support_code_hashes"]["nano_wrist_timing.py"]
            || source["grasp_motion"] != old["grasp_motion"]
            || (source["grasp_motion"].is_string()
                && (!source["support_code_hashes"]["nano_moving_grasp.py"].is_string()
                    || source["support_code_hashes"]["nano_moving_grasp.py"]
                        != old["support_code_hashes"]["nano_moving_grasp.py"]))
            || !["recipe", "nexus", "adapter_hash", "backend"]
                .iter()
                .all(|k| !source[*k].is_null() && source[*k] == old[*k])
            || ![
                "nano_living.py",
                "run_rgb_approach.py",
                "run_rgb_transfer.py",
                "nano_cameras.py",
                "nano_visuals.py",
            ]
            .iter()
            .all(|k| {
                source["support_code_hashes"][*k].is_string()
                    && source["support_code_hashes"][*k] == old["support_code_hashes"][*k]
            })
            || !["cameras", "joint_names", "rolling_radius_m"]
                .iter()
                .all(|k| calibration[*k] == old_cal[*k])
            || !calibration["cameras"].is_object()
        {
            continue;
        }
        let label = previous.file_name()?.to_string_lossy();
        let Some(previous_count) = settled(&prior) else {
            continue;
        };
        let code = if source["policy_sha256"] == old["policy_sha256"]
            && source["support_code_hashes"] == old["support_code_hashes"]
        {
            "Same local policy and support code."
        } else {
            "Local policy or support code changed."
        };
        let trace = compare_actions(
            previous,
            path,
            [
                prior["physics_ticks"].as_u64(),
                current["physics_ticks"].as_u64(),
            ],
        )
        .unwrap_or_else(|_| ActionDifference {
            summary: "Action trace unavailable.".into(),
            ticks: None,
        });
        let frames = trace
            .ticks
            .map(|[a, b]| {
                [
                    change_frame(previous, a, &prior),
                    change_frame(path, b, current),
                ]
            })
            .unwrap_or_default();
        let packet = |trial: &Path, result: &Value, cal: &Value, tick| {
            let stopped = &result["last_pickup_observation"];
            let stop_stamp = cal["physics_dt_s"]
                .as_f64()
                .filter(|dt| dt.is_finite() && *dt > 0.)
                .map(|dt| (tick as f64 * dt * 1e9).round() as u64);
            let path = if stopped["stopped_tick"].as_u64() == Some(tick)
                && stop_stamp.is_some()
                && stopped["stamp_ns"].as_u64() == stop_stamp
            {
                trial.join("last-pickup-observation.npz")
            } else {
                trial
                    .join("grasp-observations")
                    .join(format!("{tick:06}.npz"))
            };
            path.is_file().then(|| (trial.to_owned(), tick))
        };
        let packets = trace
            .ticks
            .map(|[a, b]| {
                [
                    packet(previous, &prior, &old_cal, a),
                    packet(path, current, &calibration, b),
                ]
            })
            .unwrap_or_default();
        let outcome = |v: &Value| {
            if v["task_success"] == true {
                "passed"
            } else {
                "failed"
            }
        };
        return Some(TransferComparison {frames,packets, summary:format!(
            "Matched layout {} · previous {label}\nPrevious {} ({} settled) · current {} ({} settled). {code} {}",
            current["seed"],
            outcome(&prior),
            previous_count,
            outcome(current),
            settled(current)?,trace.summary
        )});
    }
    None
}

struct Job {
    child: Child,
    dir: PathBuf,
    op: &'static str,
    last: Instant,
}
impl Job {
    fn artifact(&self) -> &'static str {
        match self.op {
            "fleet" => "fleet.json",
            "export" => "export.json",
            "room" | "room-draft" | "room-apply" => "room-ref.json",
            "evaluate" => "evaluation.json",
            "device-preflight" => "preflight.json",
            "compare-observations" => "observations.json",
            "run" => "result.json",
            "prepare" | "train-reach" | "train-grasp" => "controller.json",
            _ => "scene.json",
        }
    }
    fn stop(&mut self) {
        // The worker may own a Codex CLI child. Stop only the group created by launch().
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        #[cfg(not(unix))]
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub struct Workbench {
    pub robot: Robot,
    pub task: String,
    pub status: String,
    pub error: Option<String>,
    pub scene: Value,
    pub plan: Value,
    pub evidence: Value,
    pub progress: Value,
    pub selected: Option<PathBuf>,
    pub history: Vec<PathBuf>,
    pub prepared: Option<PathBuf>,
    pub trace: Vec<f32>,
    pub live: Option<PathBuf>,
    pub revisions: Vec<PathBuf>,
    pub compliant: bool,
    pub impact: bool,
    pub rgb_timing: RgbTiming,
    pub target_motion: TargetMotion,
    pub onboard_context: bool,
    pub room_editor: bool,
    pub show_evaluation: bool,
    pub evaluation: Value,
    pub show_device: bool,
    pub decision: Option<(PathBuf, Value)>,
    pub preflight: Value,
    device_endpoint: Option<(String, u8)>,
    preflights: std::collections::BTreeMap<String, PathBuf>,
    pub failure_index: usize,
    evaluation_archive: String,
    pub room_info: Value,
    pub room_path: Option<PathBuf>,
    pub room_draft_path: Option<PathBuf>,
    pub room_draft_info: Value,
    room_draft_receipt: Value,
    room_selection: Value,
    room_request: Option<(String, String)>,
    pub layout_seed: u8,
    pub comparison: Option<f64>,
    pub transfer_comparison: Option<TransferComparison>,
    pub dataset: Option<PathBuf>,
    exports: std::collections::BTreeMap<PathBuf, PathBuf>,
    pub background_period_s: f32,
    pub observation_clock: ObservationClock,
    pub worker_sample: Option<PathBuf>,
    pub workers_open: bool,
    pub fleet: Value,
    pub worker_observation: Value,
    pub worker_preview: Option<PathBuf>,
    worker_node: String,
    worker_action: WorkerAction,
    watcher: Option<Job>,
    job: Option<Job>,
    root: PathBuf,
}
impl Default for Workbench {
    fn default() -> Self {
        Self::with_root(crate::runtime::user_data().join("aisle"))
    }
}
impl Workbench {
    fn with_root(root: PathBuf) -> Self {
        let mut w = Self {
            robot: Robot::Nano,
            task: REFERENCE.into(),
            status: "Ready".into(),
            error: None,
            scene: Value::Null,
            plan: Value::Null,
            evidence: Value::Null,
            progress: Value::Null,
            selected: None,
            history: Vec::new(),
            prepared: None,
            trace: Vec::new(),
            live: None,
            revisions: Vec::new(),
            compliant: false,
            impact: false,
            rgb_timing: RgbTiming::Normal,
            target_motion: TargetMotion::Stationary,
            onboard_context: false,
            room_editor: false,
            show_evaluation: false,
            evaluation: Value::Null,
            show_device: false,
            decision: None,
            preflight: Value::Null,
            device_endpoint: None,
            preflights: Default::default(),
            failure_index: 0,
            evaluation_archive: String::new(),
            room_info: Value::Null,
            room_path: None,
            room_draft_path: None,
            room_draft_info: Value::Null,
            room_draft_receipt: Value::Null,
            room_selection: Value::Null,
            room_request: None,
            layout_seed: 0,
            comparison: None,
            transfer_comparison: None,
            dataset: None,
            exports: Default::default(),
            background_period_s: 1.0,
            observation_clock: ObservationClock::Simulation,
            worker_sample: None,
            workers_open: false,
            fleet: read(&root, "worker-selection.json").unwrap_or_default(),
            worker_observation: Value::Null,
            worker_preview: None,
            worker_node: String::from("local"),
            worker_action: WorkerAction::Connect,
            watcher: None,
            job: None,
            root,
        };
        if let Ok(entries) = fs::read_dir(&w.root) {
            let mut last_evaluation = None;
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.ends_with("-run") && path.is_dir() {
                    w.history.push(path);
                } else if name.ends_with("-evaluate")
                    && path.join("evaluation.json").is_file()
                    && last_evaluation.as_ref().is_none_or(|p| &path > p)
                {
                    last_evaluation = Some(path);
                } else if name.ends_with("-device-preflight") {
                    w.remember_preflight(&path);
                } else if name.ends_with("-export") {
                    let export = read(&path, "export.json");
                    if let Some(trial) = export.as_ref().and_then(|v| v["trial"].as_str()) {
                        w.exports.insert(w.root.join(trial), path.join("dataset"));
                    }
                }
            }
            if let Some(path) = last_evaluation {
                w.evaluation = read(&path, "evaluation.json").unwrap_or_default();
            }
            w.history.sort();
            let remembered = read(&w.root, "selected-trial.json");
            let selected = remembered
                .as_ref()
                .and_then(Value::as_str)
                .and_then(|name| {
                    w.history
                        .iter()
                        .find(|path| path.file_name().and_then(|v| v.to_str()) == Some(name))
                })
                .or_else(|| w.history.last())
                .cloned();
            if let Some(path) = selected {
                w.select_run(path);
                if w.evidence["robot"] == "G1" {
                    w.robot = Robot::G1;
                }
            }
        }
        if let Some(selection) = read(&w.root, "selected-room.json") {
            w.select_room(selection);
        }
        if let Some(alias) = w.fleet["node"].as_str() {
            w.worker_node = alias.to_owned();
        }
        w.restore_profile();
        w
    }
}
impl Workbench {
    pub fn worker_pid(&self) -> Option<u32> {
        self.job.as_ref().map(|j| j.child.id())
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn preflighting(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|job| job.op == "device-preflight")
    }
    fn select_room(&mut self, selection: Value) {
        self.room_path = selection["revision"]
            .as_str()
            .filter(|name| {
                (name.ends_with("-Nano-room") || name.ends_with("-Nano-room-draft"))
                    && Path::new(name).components().count() == 1
            })
            .map(|name| self.root.join(name).join("room.json"));
        self.room_info = self
            .room_path
            .as_deref()
            .and_then(|p| read(p.parent()?, "room.json"))
            .unwrap_or_default();
        self.room_selection = selection;
    }
    fn save_room(&mut self, selection: Value) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let path = self.root.join("selected-room.json");
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temp, serde_json::to_vec(&selection)?)?;
        fs::rename(temp, path)?;
        self.select_room(selection);
        Ok(())
    }
    fn restore_profile(&mut self) {
        let suffix = format!("-{}-prepare", self.robot.name());
        let trained_suffix = match self.robot {
            Robot::Nano => "-Nano-train-grasp",
            Robot::G1 => "-G1-train-reach",
        };
        self.revisions = fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().is_some_and(|n| {
                    let name = n.to_string_lossy();
                    name.ends_with(&suffix) || name.ends_with(trained_suffix)
                })
            })
            .map(|p| p.join("controller.json"))
            .filter(|p| p.is_file())
            .collect();
        self.revisions.sort();
        self.load_plan(self.revisions.last().cloned());
        let suffix = format!("-{}-run", self.robot.name());
        self.live = self
            .history
            .iter()
            .rev()
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(&suffix))
            })
            .cloned();
        self.progress = self
            .live
            .as_ref()
            .and_then(|p| read(p, "progress.json"))
            .unwrap_or_default();
    }
    fn load_plan(&mut self, path: Option<PathBuf>) {
        self.plan = path
            .as_ref()
            .and_then(|p| read(p.parent()?, "controller.json"))
            .unwrap_or_default();
        self.task = self.plan["task"].as_str().unwrap_or(REFERENCE).to_owned();
        self.prepared = path;
    }
    pub fn revision_index(&self) -> Option<usize> {
        self.revisions
            .iter()
            .position(|p| Some(p) == self.prepared.as_ref())
    }
    fn receipt(path: &Path) -> Value {
        if let Some(value) = read(path, "result.json") {
            return value;
        }
        let robot = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.rsplit('-').nth(1));
        let (outcome, reason) = if read(path, "stopped.json").is_some() {
            ("Stopped", "Stopped by operator".to_owned())
        } else if let Some(error) = read(path, "error.json") {
            (
                "Worker error",
                error["message"]
                    .as_str()
                    .unwrap_or("Worker failed")
                    .to_owned(),
            )
        } else {
            ("Unscored", "No final measurement was recorded".to_owned())
        };
        serde_json::json!({"robot":robot,"outcome":outcome,"reason":reason})
    }
    fn remember_preflight(&mut self, dir: &Path) {
        if let Some(receipt) = read(dir, "preflight.json") {
            if let Some(trial) = receipt["trial"].as_str() {
                let path = self.preflights.entry(trial.to_string()).or_default();
                if dir > path.as_path() {
                    *path = dir.to_owned();
                }
            }
        }
    }
    fn preflight_dir(&self) -> Option<&PathBuf> {
        self.preflights
            .get(self.selected.as_ref()?.file_name()?.to_str()?)
    }
    fn load_preflight(&mut self) {
        self.preflight = self
            .preflight_dir()
            .and_then(|dir| read(dir, "preflight.json"))
            .filter(|v| {
                v["controller_sha256"].is_string()
                    && v["controller_sha256"] == self.evidence["controller_hash"]
                    && v["model_sha256"] == self.evidence["model_hash"]
            })
            .unwrap_or_default();
    }
    pub fn dataset_for(&self, path: &Path) -> Option<PathBuf> {
        let dataset = self
            .exports
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.join("dataset"));
        dataset.join("meta/info.json").is_file().then_some(dataset)
    }
    pub fn change_frame(&self, side: RevisionSide) -> Option<&ChangeFrame> {
        self.transfer_comparison.as_ref()?.frames[side.index()].as_ref()
    }
    fn select_run(&mut self, path: PathBuf) {
        self.decision = None;
        self.dataset = self.dataset_for(&path);
        self.evidence = Self::receipt(&path);
        self.transfer_comparison = compare_transfer(&self.history, &path, &self.evidence);
        self.comparison = self
            .history
            .iter()
            .rev()
            .filter(|p| *p < &path)
            .filter_map(|p| read(p, "result.json"))
            .find(|v| {
                ["robot", "model_hash", "fixture", "joint", "requested_delta"]
                    .iter()
                    .all(|k| v[*k] == self.evidence[*k])
            })
            .and_then(|v| {
                Some(self.evidence["measured_delta"].as_f64()? - v["measured_delta"].as_f64()?)
            });
        self.trace = read(&path, "trajectory.json")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|r| {
                r["measured_rad"]
                    .as_f64()
                    .or_else(|| r["distance_m"].as_f64())
                    .or_else(|| r["table_contacts"].as_f64())
                    .map(|x| x as f32)
            })
            .collect();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let temporary = self
                .root
                .join(format!("selected-trial.{}.tmp", std::process::id()));
            if fs::write(&temporary, serde_json::json!(name).to_string()).is_ok() {
                let _ = fs::rename(temporary, self.root.join("selected-trial.json"));
            }
        }
        self.selected = Some(path);
        self.load_preflight();
    }
    fn start_preflight(&mut self, endpoint: Option<String>) -> io::Result<()> {
        self.device_endpoint = if let Some(endpoint) = endpoint {
            let (name, domain) = endpoint
                .trim()
                .split_once('@')
                .unwrap_or((endpoint.trim(), "0"));
            let domain = domain.parse::<u8>().ok().filter(|d| *d <= 232);
            if name.is_empty() || domain.is_none() {
                return Err(io::Error::other("Enter a network interface and a DDS domain between 0 and 232, for example en0@0."));
            }
            Some((name.to_owned(), domain.unwrap()))
        } else {
            None
        };
        self.launch("device-preflight", Planner::Reference, None)
    }
    fn launch(
        &mut self,
        op: &'static str,
        planner: Planner,
        feedback: Option<PathBuf>,
    ) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let robot_name = if op == "device-preflight" {
            self.evidence["robot"]
                .as_str()
                .ok_or_else(|| io::Error::other("Select a successful simulation trial"))?
        } else {
            self.robot.name()
        };
        let running_status = format!("Running · {robot_name} · {op}");
        let dir = self.root.join(format!("{now}-{robot_name}-{op}"));
        let studio = crate::runtime::root();
        let worker = studio.join("tools/aisle_workbench/worker.py");
        if !worker.is_file() {
            return Err(io::Error::other(
                "Install the AISLE workbench source checkout and its Python environment",
            ));
        }
        let python = std::env::var_os("DOROBOT_AISLE_PYTHON")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let local = studio.join(".venv-aisle/bin/python");
                if local.is_file() {
                    local
                } else {
                    PathBuf::from("python3")
                }
            });
        let mut cmd = Command::new(python);
        cmd.arg(worker)
            .arg(if op == "fleet-watch" { "fleet" } else { op })
            .arg("--out")
            .arg(&dir)
            .arg("--robot")
            .arg(robot_name);
        let repo = std::env::var_os("DOROBOT_AISLE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| studio.join("../AISLE"));
        if repo.join("src/aisle").is_dir() {
            let mut paths = vec![repo.join("src")];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PYTHONPATH").unwrap_or_default(),
            ));
            cmd.env(
                "PYTHONPATH",
                std::env::join_paths(paths).map_err(io::Error::other)?,
            );
        } else if std::env::var_os("DOROBOT_AISLE_ROOT").is_some() {
            return Err(io::Error::other(
                "DOROBOT_AISLE_ROOT must point to an AISLE checkout containing src/aisle",
            ));
        }
        if let Some(run) = feedback.as_ref().filter(|_| op != "export") {
            cmd.arg("--feedback").arg(run);
        }
        if op == "device-preflight" {
            let trial = self
                .selected
                .as_ref()
                .ok_or_else(|| io::Error::other("Select a simulation trial first"))?;
            cmd.arg("--trial").arg(trial);
            if let Some((interface, domain)) = &self.device_endpoint {
                cmd.args([
                    "--device-mode",
                    "subscribe",
                    "--device-interface",
                    interface,
                    "--dds-domain",
                    &domain.to_string(),
                ]);
            }
        }
        if op == "compare-observations" {
            let pair = self
                .transfer_comparison
                .as_ref()
                .ok_or_else(|| io::Error::other("Select a matched Nano trial"))?;
            for (flag, packet) in ["--previous-packet", "--current-packet"]
                .into_iter()
                .zip(&pair.packets)
            {
                if let Some((dir, tick)) = packet {
                    cmd.arg(flag).arg(dir).arg(tick.to_string());
                }
            }
        }
        if op == "evaluate" {
            cmd.arg("--archive").arg(&self.evaluation_archive);
        }
        if op == "room-apply" {
            cmd.arg("--room-revision")
                .arg(
                    self.room_draft_path
                        .as_deref()
                        .ok_or_else(|| io::Error::other("Review a draft first"))?,
                )
                .arg("--room-hash")
                .arg(
                    self.room_draft_receipt["sha256"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("Missing draft hash"))?,
                )
                .arg("--seed")
                .arg(self.layout_seed.to_string());
        }
        if op == "room-draft" {
            cmd.arg("--provider").arg(planner.name());
        }
        if matches!(op, "room" | "room-draft") {
            let (source, edit) = self
                .room_request
                .as_ref()
                .ok_or_else(|| io::Error::other("Choose a survey or edit the selected room"))?;
            if !source.trim().is_empty() {
                cmd.arg("--room-source").arg(source.trim());
            }
            if !edit.trim().is_empty() {
                cmd.arg("--room-edit").arg(edit.trim());
            }
            cmd.arg("--seed").arg(self.layout_seed.to_string());
        }
        let mobile =
            self.plan["kind"] == "table-transfer" || self.plan["kind"] == "visual-approach";
        let capture_onboard = op == "prepare"
            && self.onboard_context
            && self.robot == Robot::Nano
            && planner != Planner::Reference;
        if capture_onboard {
            cmd.arg("--onboard-context")
                .arg("--seed")
                .arg(self.layout_seed.to_string());
        }
        let submitting = op == "fleet"
            && matches!(
                self.worker_action,
                WorkerAction::Stream | WorkerAction::Observations
            );
        if matches!(op, "fleet" | "fleet-watch") {
            let action = if op == "fleet-watch" {
                WorkerAction::Watch
            } else {
                self.worker_action
            };
            cmd.args(["--fleet-action", action.name(), "--node", &self.worker_node]);
            if matches!(action, WorkerAction::Observations) {
                cmd.arg("--observation-period-s")
                    .arg(self.background_period_s.to_string())
                    .args(["--observation-clock", self.observation_clock.name()]);
            }
            if self.fleet["node"].as_str() == Some(&self.worker_node) {
                if let Some(id) = self.fleet["selected"]["id"].as_str() {
                    cmd.args(["--job-id", id]);
                }
            }
        }
        if op == "room" || ((op == "run" || submitting) && mobile) || capture_onboard {
            if let Some(path) = &self.room_path {
                cmd.arg("--room-revision").arg(path).arg("--room-hash").arg(
                    self.room_selection["sha256"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("Missing room revision hash"))?,
                );
            }
        }
        if matches!(op, "train-reach" | "train-grasp") {
            cmd.arg("--dataset").arg(
                self.dataset
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Export the selected episode first"))?,
            );
        }
        if matches!(op, "export" | "train-reach" | "train-grasp") {
            cmd.arg("--trial").arg(
                feedback
                    .as_ref()
                    .filter(|_| op == "export")
                    .or(self.selected.as_ref())
                    .ok_or_else(|| io::Error::other("Select a recorded trial"))?,
            );
        }
        if op == "prepare" {
            cmd.args(["--provider", planner.name(), "--task", &self.task]);
        }
        if op == "run" && self.plan["kind"] == "table-transfer" {
            cmd.arg("--live-preview");
        }
        if op == "run" || submitting {
            if mobile {
                cmd.arg("--rgb-timing").arg(self.rgb_timing.name());
                cmd.arg("--target-motion").arg(self.target_motion.name());
            }
            cmd.arg("--seed").arg(self.layout_seed.to_string());
            cmd.args([
                "--fixture",
                if self.plan["kind"] == "standing-inspection" && self.impact {
                    "impact"
                } else if self.compliant {
                    "compliant"
                } else {
                    "standard"
                },
            ]);
            cmd.arg("--controller").arg(
                self.prepared
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Prepare a controller for this robot first"))?,
            );
        }
        fs::create_dir(&dir)?;
        cmd.arg("--reserved");
        let log = fs::File::create(dir.join("worker.log"))?;
        #[cfg(unix)]
        cmd.process_group(0);
        let child = cmd
            .stdin(Stdio::piped())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()
            .inspect_err(|e| {
                let _ = fs::write(
                    dir.join("error.json"),
                    serde_json::json!({"message":e.to_string()}).to_string(),
                );
            });
        if op == "run" {
            self.live = Some(dir.clone());
            self.progress = Value::Null;
            self.history.push(dir.clone());
            self.select_run(dir.clone());
        }
        let job = Job {
            child: child?,
            dir,
            op,
            last: Instant::now(),
        };
        if op == "fleet-watch" {
            self.worker_preview = Some(job.dir.clone());
            self.watcher = Some(job);
        } else {
            self.status = running_status;
            self.job = Some(job);
        }
        Ok(())
    }
    pub fn dispatch(&mut self, action: Action) {
        if matches!(action, Action::Stop) {
            if let Some(mut j) = self.job.take() {
                j.stop();
                if j.dir.join(j.artifact()).is_file() {
                    self.finish(j, true);
                } else if j.dir.join("error.json").is_file() {
                    self.finish(j, false);
                } else {
                    self.status = "Stopped by operator".into();
                    let _ = fs::write(j.dir.join("stopped.json"), "{\"stopped\":true}");
                    if j.op == "run" {
                        self.select_run(j.dir);
                    } else if j.op == "device-preflight" {
                        let receipt = serde_json::json!({
                            "trial": self.selected.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()),
                            "robot": self.evidence["robot"], "kind": self.evidence["kind"],
                            "controller_sha256": self.evidence["controller_hash"],
                            "model_sha256": self.evidence["model_hash"],
                            "state": "Dry run stopped", "reason": self.status,
                            "motor_commands_sent": 0, "physical_qualification": false
                        });
                        let tmp = j.dir.join("cancelled-receipt.tmp");
                        if let Err(e) = fs::write(&tmp, receipt.to_string())
                            .and_then(|()| fs::rename(tmp, j.dir.join("preflight.json")))
                        {
                            self.error = Some(format!("Could not save the stopped preflight: {e}"));
                        } else {
                            self.remember_preflight(&j.dir);
                            self.load_preflight();
                        }
                    }
                }
            }
            return;
        }
        if self.busy() {
            return;
        }
        self.error = None;
        let result = match action {
            Action::Select(r) => {
                if self.robot != r {
                    self.robot = r;
                    self.scene = Value::Null;
                    self.restore_profile();
                    self.status = "Ready".into();
                }
                Ok(())
            }
            Action::Prepare {
                task,
                planner,
                revise,
            } => {
                self.task = task;
                self.launch(
                    "prepare",
                    planner,
                    if revise { self.selected.clone() } else { None },
                )
            }
            Action::InspectDecision => {
                self.launch("compare-observations", Planner::Reference, None)
            }
            Action::ShowDevice => {
                self.decision = None;
                self.show_device = true;
                self.show_evaluation = false;
                Ok(())
            }
            Action::DevicePreflight(endpoint) => self.start_preflight(endpoint),
            Action::ShowEvaluation(value) => {
                self.decision = None;
                self.show_device = false;
                self.show_evaluation = value;
                Ok(())
            }
            Action::SelectFailure(next) => {
                let count = self.evaluation["failures"].as_array().map_or(0, Vec::len);
                self.failure_index = if next {
                    (self.failure_index + 1).min(count.saturating_sub(1))
                } else {
                    self.failure_index.saturating_sub(1)
                };
                Ok(())
            }
            Action::Evaluate(path) => {
                self.evaluation_archive = path;
                self.launch("evaluate", Planner::Reference, None)
            }
            Action::EditRoom(value) => {
                self.room_editor = value;
                Ok(())
            }
            Action::DefaultRoom => self.save_room(Value::Null),
            Action::DraftRoom { source, planner } => {
                self.room_request = Some((source, String::new()));
                self.launch("room-draft", planner, None)
            }
            Action::ApplyRoomDraft => self.launch("room-apply", Planner::Reference, None),
            Action::DiscardRoomDraft => {
                self.room_draft_path = None;
                self.room_draft_info = Value::Null;
                self.room_draft_receipt = Value::Null;
                Ok(())
            }
            Action::ImportRoom { source, edit } => {
                self.room_request = Some((source, edit));
                self.launch("room", Planner::Reference, None)
            }
            Action::ToggleOnboardContext => {
                self.onboard_context = !self.onboard_context;
                Ok(())
            }
            Action::TargetMotion(value) => {
                self.target_motion = value;
                Ok(())
            }
            Action::RgbTiming(value) => {
                self.rgb_timing = value;
                Ok(())
            }
            Action::Impact(value) => {
                self.impact = value;
                Ok(())
            }
            Action::Fixture(value) => {
                self.compliant = value;
                Ok(())
            }
            Action::Layout(seed @ 0..=2) => {
                self.layout_seed = seed;
                Ok(())
            }
            Action::Layout(_) => Err(io::Error::other("Select a development layout from 0 to 2")),
            Action::OlderPlan | Action::NewerPlan => {
                if let Some(i) = self.revision_index() {
                    let index = if matches!(action, Action::OlderPlan) {
                        i.saturating_sub(1)
                    } else {
                        (i + 1).min(self.revisions.len() - 1)
                    };
                    self.load_plan(Some(self.revisions[index].clone()));
                }
                Ok(())
            }
            Action::TrainGrasp => self.launch("train-grasp", Planner::Reference, None),
            Action::TrainReach => self.launch("train-reach", Planner::Reference, None),
            Action::ExportChange(side) => {
                if let Some(frame) = self.change_frame(side) {
                    self.launch("export", Planner::Reference, Some(frame.trial.clone()))
                } else {
                    Err(io::Error::other("No recorded frame for this revision"))
                }
            }
            Action::Export => self.launch("export", Planner::Reference, None),
            Action::ToggleObservationClock => {
                self.observation_clock = match self.observation_clock {
                    ObservationClock::Simulation => ObservationClock::Wall,
                    ObservationClock::Wall => {
                        self.background_period_s = self.background_period_s.max(1.0);
                        ObservationClock::Simulation
                    }
                };
                Ok(())
            }
            Action::CycleBackgroundPeriod => {
                self.background_period_s = match self.background_period_s {
                    1.0 => 2.0,
                    2.0 => 5.0,
                    5.0 if self.observation_clock == ObservationClock::Wall => 0.5,
                    _ => 1.0,
                };
                Ok(())
            }
            Action::Workers(open) => {
                self.workers_open = open;
                self.stop_watch();
                if open && self.fleet.is_object() {
                    self.worker_action = WorkerAction::Refresh;
                    self.launch("fleet", Planner::Reference, None)
                } else {
                    Ok(())
                }
            }
            Action::ConnectWorker(node) => {
                self.worker_action = WorkerAction::Connect;
                self.worker_node = if node.trim().is_empty() {
                    "local".into()
                } else {
                    node.trim().into()
                };
                self.launch("fleet", Planner::Reference, None)
            }
            Action::Worker(action) => {
                self.worker_action = action;
                if let Some(alias) = self.fleet["node"].as_str() {
                    self.worker_node = alias.to_owned();
                }
                self.launch("fleet", Planner::Reference, None)
            }
            Action::Inspect => self.launch("scene", Planner::Reference, None),
            Action::Run => self.launch("run", Planner::Reference, None),
            Action::Previous | Action::Next => {
                if let Some(i) = self
                    .history
                    .iter()
                    .position(|p| Some(p) == self.selected.as_ref())
                {
                    let j = if matches!(action, Action::Previous) {
                        i.saturating_sub(1)
                    } else {
                        (i + 1).min(self.history.len() - 1)
                    };
                    self.select_run(self.history[j].clone());
                }
                Ok(())
            }
            Action::Open | Action::OpenPreflight => {
                let folder = if matches!(action, Action::OpenPreflight) {
                    self.preflight_dir()
                } else {
                    self.selected.as_ref()
                };
                if let Some(p) = folder {
                    Command::new(if cfg!(target_os = "macos") {
                        "open"
                    } else {
                        "xdg-open"
                    })
                    .arg(p)
                    .spawn()
                    .map(|mut opener| {
                        std::thread::spawn(move || {
                            let _ = opener.wait();
                        });
                    })
                } else {
                    Ok(())
                }
            }
            Action::Stop => unreachable!(),
        };
        if let Err(e) = result {
            self.status = e.to_string();
            self.error = Some(self.status.clone());
        }
    }
    fn stop_watch(&mut self) {
        if let Some(mut watch) = self.watcher.take() {
            watch.stop();
            let _ = fs::remove_dir_all(watch.dir);
        }
        if let Some(path) = self.worker_preview.take() {
            let _ = fs::remove_dir_all(path);
        }
        self.worker_observation = Value::Null;
    }
    pub fn poll(&mut self) {
        if let Some(watch) = self.watcher.as_mut() {
            if watch.last.elapsed() >= Duration::from_millis(100) {
                watch.last = Instant::now();
                if let Some(obs) = read(&watch.dir, "watch.json") {
                    self.worker_observation = obs;
                }
                if let Ok(Some(code)) = watch.child.try_wait() {
                    if !code.success() {
                        self.status = read(&watch.dir, "error.json")
                            .and_then(|e| e["message"].as_str().map(str::to_owned))
                            .unwrap_or_else(|| {
                                "Worker observation interrupted. Refresh to reconnect.".into()
                            });
                    }
                    self.watcher = None;
                }
            }
        }
        let Some(job) = &mut self.job else { return };
        if job.last.elapsed() < Duration::from_millis(70) {
            return;
        }
        job.last = Instant::now();
        if job.op == "run" {
            if let Some(p) = read(&job.dir, "progress.json") {
                self.progress = p;
            }
        }
        let status = match job.child.try_wait() {
            Ok(None) => return,
            r => r,
        };
        let job = self.job.take().unwrap();
        self.finish(job, status.is_ok_and(|s| s.is_some_and(|c| c.success())));
    }
    fn finish(&mut self, job: Job, success: bool) {
        if let Some(m) = read(&job.dir, "scene.json") {
            self.scene = m;
        }
        let artifact = job.artifact();
        if success && job.dir.join(artifact).is_file() {
            if job.op == "fleet" {
                self.stop_watch();
                self.fleet = read(&job.dir, artifact).unwrap_or_default();
                self.worker_sample = self.fleet["sample"].is_object().then(|| job.dir.clone());
                if let Err(error) = fs::copy(
                    job.dir.join(artifact),
                    self.root
                        .join(format!("worker-selection.{}.tmp", std::process::id())),
                )
                .and_then(|_| {
                    fs::rename(
                        self.root
                            .join(format!("worker-selection.{}.tmp", std::process::id())),
                        self.root.join("worker-selection.json"),
                    )
                }) {
                    self.status = error.to_string();
                    self.error = Some(self.status.clone());
                    return;
                }
                if let Some(name) = self.fleet["imported"]
                    .as_str()
                    .filter(|n| n.ends_with("-run") && Path::new(n).components().count() == 1)
                {
                    let path = self.root.join(name);
                    if !self.history.contains(&path) {
                        self.history.push(path.clone());
                        self.history.sort();
                    }
                    self.select_run(path);
                    self.status = "Worker trial available in Evidence".into();
                } else {
                    self.status = "Connected to simulation worker".into();
                }
                if self.workers_open && self.fleet["selected"]["id"].is_string() {
                    if let Err(e) = self.launch("fleet-watch", Planner::Reference, None) {
                        self.status = e.to_string();
                    }
                }
                return;
            }
            if job.op == "device-preflight" {
                self.remember_preflight(&job.dir);
                self.load_preflight();
            }
            if job.op == "evaluate" {
                self.evaluation = read(&job.dir, artifact).unwrap_or_default();
                self.failure_index = 0;
            }
            if job.op == "room-draft" {
                self.room_draft_path = Some(job.dir.join("room.json"));
                self.room_draft_info = read(&job.dir, "room.json").unwrap_or_default();
                self.room_draft_receipt = read(&job.dir, artifact).unwrap_or_default();
            }
            if matches!(job.op, "room" | "room-apply") {
                if let Err(error) = read(&job.dir, artifact)
                    .ok_or_else(|| io::Error::other("Missing room receipt"))
                    .and_then(|s| self.save_room(s))
                {
                    self.error = Some(error.to_string());
                    self.status = error.to_string();
                    return;
                }
                self.room_draft_path = None;
                self.room_draft_info = Value::Null;
                self.room_draft_receipt = Value::Null;
            }
            if job.op == "compare-observations" {
                self.decision = read(&job.dir, artifact)
                    .filter(|r| {
                        self.transfer_comparison
                            .as_ref()
                            .is_some_and(|pair| pair.matches_observations(r))
                    })
                    .map(|receipt| (job.dir.clone(), receipt));
            }
            if job.op == "export" {
                let dataset = job.dir.join("dataset");
                if let Some(name) =
                    read(&job.dir, artifact).and_then(|v| v["trial"].as_str().map(str::to_owned))
                {
                    if let Some(path) = self
                        .history
                        .iter()
                        .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(&name))
                    {
                        self.exports.insert(path.clone(), dataset.clone());
                        if self.selected.as_ref() == Some(path) {
                            self.dataset = Some(dataset);
                        }
                    }
                }
            }
            if matches!(job.op, "prepare" | "train-reach" | "train-grasp") {
                let p = job.dir.join(artifact);
                self.revisions.push(p.clone());
                self.load_plan(Some(p));
            }
            if job.op == "run" {
                self.select_run(job.dir);
            }
            self.status = if job.op == "evaluate" {
                let profile = if self.evaluation["profile"] == "g1-standing-reach-v1" {
                    "G1"
                } else {
                    "Nano"
                };
                format!("Evaluation verified · {profile}")
            } else if job.op == "device-preflight" {
                format!(
                    "{} · {}",
                    self.preflight["robot"].as_str().unwrap_or("Device"),
                    self.preflight["state"]
                        .as_str()
                        .unwrap_or("Preflight finished")
                )
            } else {
                format!("Completed · {} · {}", self.robot.name(), job.op)
            };
        } else {
            self.status = read(&job.dir, "error.json")
                .and_then(|v| v["message"].as_str().map(str::to_owned))
                .unwrap_or_else(|| {
                    format!(
                        "Worker failed. See {}",
                        job.dir.join("worker.log").display()
                    )
                });
            self.error = Some(self.status.clone());
            if job.op == "run" {
                if !job.dir.join("error.json").exists() {
                    let _ = fs::write(
                        job.dir.join("error.json"),
                        serde_json::json!({"message":self.status}).to_string(),
                    );
                }
                self.select_run(job.dir);
            }
        }
    }
}
impl Drop for Workbench {
    fn drop(&mut self) {
        self.stop_watch();
        if let Some(j) = &mut self.job {
            j.stop();
            if !j.dir.join(j.artifact()).is_file() && !j.dir.join("error.json").is_file() {
                let _ = fs::write(j.dir.join("stopped.json"), "{\"stopped\":true}");
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn fixture(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aisle-job-{case}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn pair_requires_matching_layout_and_reports_first_action_change() {
        let dir = fixture("matched-layout");
        let mut runs = Vec::new();
        let result = serde_json::json!({"kind":"table-transfer","robot":"Nano","model_hash":"model",
            "fixture":"living-room","requested":["red"],"seed":3,"task_success":true,
            "cubes":{"red":{"passed":true}}});
        let source = serde_json::json!({"recipe":"room","backend":"native","nexus":"engine",
            "adapter_hash":"adapter","support_code_hashes":{"nano_living.py":"physics",
            "run_rgb_approach.py":"recipe","run_rgb_transfer.py":"judge","nano_cameras.py":"rig",
            "nano_visuals.py":"mesh"}});
        for name in ["01-Nano-run", "02-Nano-run"] {
            let run = dir.join(name);
            fs::create_dir(&run).unwrap();
            fs::write(run.join("source.json"), source.to_string()).unwrap();
            fs::write(run.join("result.json"), result.to_string()).unwrap();
            fs::write(
                run.join("calibration.json"),
                r#"{"cameras":{"head":{}},"joint_names":["wheel"]}"#,
            )
            .unwrap();
            fs::write(run.join("actions.jsonl"), "{\"op\":\"sense\",\"tick\":0}\n").unwrap();
            runs.push(run);
        }
        let comparison = || compare_transfer(&runs, &runs[1], &result);
        assert!(comparison()
            .unwrap()
            .summary
            .contains("All 1 recorded actions match"));
        fs::write(runs[1].join("actions.jsonl"), r#"{"op":"wheel","tick":20}"#).unwrap();
        assert!(comparison()
            .unwrap()
            .summary
            .contains("First change at action 1"));
        let mut changed = result.clone();
        changed["seed"] = serde_json::json!(2);
        assert!(compare_transfer(&runs, &runs[1], &changed).is_none());
        let mut wrist = source.clone();
        wrist["wrist_rgb_timing"] = serde_json::json!("intermittent");
        wrist["support_code_hashes"]["nano_wrist_timing.py"] = serde_json::json!("guard-v1");
        fs::write(runs[1].join("source.json"), wrist.to_string()).unwrap();
        assert!(comparison().is_none());
        fs::write(runs[0].join("source.json"), wrist.to_string()).unwrap();
        assert!(comparison().is_some());
        wrist["wrist_rgb_timing"] = serde_json::json!("disconnected");
        fs::write(runs[0].join("source.json"), wrist.to_string()).unwrap();
        assert!(comparison().is_none());
        wrist["wrist_rgb_timing"] = serde_json::json!("intermittent");
        wrist["support_code_hashes"]["nano_wrist_timing.py"] = serde_json::json!("guard-v2");
        fs::write(runs[0].join("source.json"), wrist.to_string()).unwrap();
        assert!(comparison().is_none());
        fs::write(runs[0].join("source.json"), source.to_string()).unwrap();
        let mut sliding = source.clone();
        sliding["grasp_motion"] = serde_json::json!("slide-negative");
        sliding["support_code_hashes"]["nano_moving_grasp.py"] = serde_json::json!("fixture-v1");
        fs::write(runs[1].join("source.json"), sliding.to_string()).unwrap();
        assert!(comparison().is_none());
        fs::write(runs[0].join("source.json"), sliding.to_string()).unwrap();
        assert!(comparison().is_some());
        sliding["support_code_hashes"]["nano_moving_grasp.py"] = serde_json::json!("fixture-v2");
        fs::write(runs[0].join("source.json"), sliding.to_string()).unwrap();
        assert!(comparison().is_none());
        fs::write(runs[0].join("source.json"), source.to_string()).unwrap();
        fs::write(runs[1].join("source.json"), source.to_string()).unwrap();
        fs::remove_file(runs[0].join("calibration.json")).unwrap();
        assert!(comparison().is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn decision_seek_uses_recorded_steps_and_zero_based_video_time() {
        let run = fixture("recording-step");
        let result = serde_json::json!({"frames":3});
        fs::write(
            run.join("recording.json"),
            r#"{"samples":[{"step":20},{"step":40},{"step":60}],"fps":5}"#,
        )
        .unwrap();
        for (tick, index, step) in [
            (0, 0, 20),
            (39, 0, 20),
            (40, 1, 40),
            (55, 1, 40),
            (76, 2, 60),
        ] {
            let frame = change_frame(&run, tick, &result).unwrap();
            assert_eq!((frame.index, frame.sample_tick), (index, step));
            assert_eq!(frame.time_s, index as f64 / 5.);
        }
        assert!(change_frame(&run, 40, &serde_json::json!({"frames":4})).is_none());
        fs::write(
            run.join("recording.json"),
            r#"{"samples":[{"step":20},{"step":60},{"step":40}],"fps":5}"#,
        )
        .unwrap();
        assert!(change_frame(&run, 40, &result).is_none());
        fs::remove_dir_all(run).unwrap();
    }

    #[test]
    fn failed_prepare_keeps_runnable_revision() {
        let dir = fixture("failed-prepare");
        let plan = dir.join("controller.json");
        fs::write(&plan, r#"{"joint":"left_j2_pitch","task":"inspection"}"#).unwrap();
        let mut w = Workbench::with_root(dir.clone());
        w.root = plan.clone(); // A file at the job root makes launch fail deterministically.
        w.revisions = vec![plan.clone()];
        w.load_plan(Some(plan.clone()));
        w.dispatch(Action::Prepare {
            task: "new instruction".into(),
            planner: Planner::Codex,
            revise: false,
        });
        assert!(w.error.is_some());
        assert_eq!(w.prepared.as_ref(), Some(&plan));
        assert_eq!(w.revision_index(), Some(0));
        assert_eq!(w.plan["joint"], "left_j2_pitch");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stopping_after_result_publication_retains_only_the_committed_outcome() {
        let dir = fixture("stop");
        fs::write(
            dir.join("result.json"),
            r#"{"robot":"Nano","task_success":true}"#,
        )
        .unwrap();
        let child = Command::new("sleep")
            .arg("15")
            .process_group(0)
            .spawn()
            .unwrap();
        let mut w = Workbench::with_root(dir.clone());
        w.job = Some(Job {
            child,
            dir: dir.clone(),
            op: "run",
            last: Instant::now(),
        });
        w.dispatch(Action::Stop);
        assert!(w.status.starts_with("Completed"));
        assert_eq!(w.evidence["task_success"], true);
        assert!(!dir.join("stopped.json").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
