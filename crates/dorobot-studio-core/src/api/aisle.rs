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
    ToggleOnboardContext,
    EditRoom(bool),
    ImportRoom {
        source: String,
        edit: String,
    },
    DefaultRoom,
    ShowEvaluation(bool),
    Evaluate(String),
    SelectFailure(bool),
    Layout(u8),
    OlderPlan,
    NewerPlan,
    Run,
    Stop,
    Previous,
    Next,
    Open,
    Export,
    TrainReach,
}
fn read(path: &Path, name: &str) -> Option<Value> {
    serde_json::from_slice(&fs::read(path.join(name)).ok()?).ok()
}
fn compare_actions(previous: &Path, current: &Path) -> io::Result<String> {
    let stream = |p: &Path| {
        Ok::<_, io::Error>(io::BufReader::new(fs::File::open(p.join("actions.jsonl"))?).lines())
    };
    let (mut left, mut right) = (stream(previous)?, stream(current)?);
    for index in 1usize.. {
        let (a, b) = (left.next().transpose()?, right.next().transpose()?);
        match (a, b) {
            (None, None) => return Ok(format!("All {} recorded actions match.", index - 1)),
            (Some(a), Some(b)) => {
                let a: Value = serde_json::from_str(&a)?;
                let b: Value = serde_json::from_str(&b)?;
                if !a.is_object() || !b.is_object() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid action row",
                    ));
                }
                if a != b {
                    return Ok(format!(
                        "First change at action {index} · previous {} at step {}, current {} at step {}.",
                        a["op"].as_str().unwrap_or("unknown"),
                        a["tick"],
                        b["op"].as_str().unwrap_or("unknown"),
                        b["tick"]
                    ));
                }
            }
            (None, _) => return Ok(format!("Previous trace ends before action {index}.")),
            (_, None) => return Ok(format!("Current trace ends before action {index}.")),
        }
    }
    unreachable!()
}

fn compare_transfer(history: &[PathBuf], path: &Path, current: &Value) -> Option<String> {
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
        let trace =
            compare_actions(previous, path).unwrap_or_else(|_| "Action trace unavailable.".into());
        let outcome = |v: &Value| {
            if v["task_success"] == true {
                "passed"
            } else {
                "failed"
            }
        };
        return Some(format!(
            "Matched layout {} · previous {label}\nPrevious {} ({} settled) · current {} ({} settled). {code} {trace}",
            current["seed"],
            outcome(&prior),
            previous_count,
            outcome(current),
            settled(current)?
        ));
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
            "export" => "export.json",
            "room" => "room-ref.json",
            "evaluate" => "evaluation.json",
            "run" => "result.json",
            "prepare" | "train-reach" => "controller.json",
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
    pub onboard_context: bool,
    pub room_editor: bool,
    pub show_evaluation: bool,
    pub evaluation: Value,
    pub failure_index: usize,
    evaluation_archive: String,
    pub room_info: Value,
    pub room_path: Option<PathBuf>,
    room_selection: Value,
    room_request: Option<(String, String)>,
    pub layout_seed: u8,
    pub comparison: Option<f64>,
    pub transfer_comparison: Option<String>,
    pub dataset: Option<PathBuf>,
    exports: std::collections::BTreeMap<PathBuf, PathBuf>,
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
            onboard_context: false,
            room_editor: false,
            show_evaluation: false,
            evaluation: Value::Null,
            failure_index: 0,
            evaluation_archive: String::new(),
            room_info: Value::Null,
            room_path: None,
            room_selection: Value::Null,
            room_request: None,
            layout_seed: 0,
            comparison: None,
            transfer_comparison: None,
            dataset: None,
            exports: Default::default(),
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
            if let Some(path) = w.history.last().cloned() {
                w.select_run(path);
                if w.evidence["robot"] == "G1" {
                    w.robot = Robot::G1;
                }
            }
        }
        if let Some(selection) = read(&w.root, "selected-room.json") {
            w.select_room(selection);
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
    fn select_room(&mut self, selection: Value) {
        self.room_path = selection["revision"]
            .as_str()
            .filter(|name| {
                name.ends_with("-Nano-room") && Path::new(name).components().count() == 1
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
        let trained_suffix = format!("-{}-train-reach", self.robot.name());
        self.revisions = fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().is_some_and(|n| {
                    let name = n.to_string_lossy();
                    name.ends_with(&suffix) || name.ends_with(&trained_suffix)
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
    fn select_run(&mut self, path: PathBuf) {
        self.dataset = self.exports.get(&path).cloned().or_else(|| {
            let legacy = path.join("dataset");
            legacy.join("meta/info.json").is_file().then_some(legacy)
        });
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
        self.selected = Some(path);
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
        let dir = self.root.join(format!("{now}-{}-{op}", self.robot.name()));
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
            .arg(op)
            .arg("--out")
            .arg(&dir)
            .arg("--robot")
            .arg(self.robot.name());
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
        if let Some(run) = feedback {
            cmd.arg("--feedback").arg(run);
        }
        if op == "evaluate" {
            cmd.arg("--archive").arg(&self.evaluation_archive);
        }
        if op == "room" {
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
        if op == "room" || (op == "run" && mobile) || capture_onboard {
            if let Some(path) = &self.room_path {
                cmd.arg("--room-revision").arg(path).arg("--room-hash").arg(
                    self.room_selection["sha256"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("Missing room revision hash"))?,
                );
            }
        }
        if op == "train-reach" {
            cmd.arg("--dataset").arg(
                self.dataset
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Export the selected G1 episode first"))?,
            );
        }
        if op == "export" || op == "train-reach" {
            cmd.arg("--trial").arg(
                self.selected
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Select a recorded trial"))?,
            );
        }
        if op == "prepare" {
            cmd.args(["--provider", planner.name(), "--task", &self.task]);
        }
        if op == "run" {
            if self.plan["kind"] == "visual-approach" {
                cmd.arg("--rgb-timing").arg(self.rgb_timing.name());
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
        self.status = format!("Running · {} · {op}", self.robot.name());
        self.job = Some(Job {
            child: child?,
            dir,
            op,
            last: Instant::now(),
        });
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
            Action::ShowEvaluation(value) => {
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
            Action::ImportRoom { source, edit } => {
                self.room_request = Some((source, edit));
                self.launch("room", Planner::Reference, None)
            }
            Action::ToggleOnboardContext => {
                self.onboard_context = !self.onboard_context;
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
            Action::TrainReach => self.launch("train-reach", Planner::Reference, None),
            Action::Export => self.launch("export", Planner::Reference, None),
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
            Action::Open => {
                if let Some(p) = &self.selected {
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
    pub fn poll(&mut self) {
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
            if job.op == "evaluate" {
                self.evaluation = read(&job.dir, artifact).unwrap_or_default();
                self.failure_index = 0;
            }
            if job.op == "room" {
                if let Err(error) = read(&job.dir, artifact)
                    .ok_or_else(|| io::Error::other("Missing room receipt"))
                    .and_then(|s| self.save_room(s))
                {
                    self.error = Some(error.to_string());
                    self.status = error.to_string();
                    return;
                }
            }
            if job.op == "export" {
                let dataset = job.dir.join("dataset");
                if let Some(path) = &self.selected {
                    self.exports.insert(path.clone(), dataset.clone());
                }
                self.dataset = Some(dataset);
            }
            if job.op == "prepare" || job.op == "train-reach" {
                let p = job.dir.join(artifact);
                self.revisions.push(p.clone());
                self.load_plan(Some(p));
            }
            if job.op == "run" {
                self.select_run(job.dir);
            }
            self.status = format!("Completed · {} · {}", self.robot.name(), job.op);
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
            .contains("All 1 recorded actions match"));
        fs::write(runs[1].join("actions.jsonl"), r#"{"op":"wheel","tick":20}"#).unwrap();
        assert!(comparison().unwrap().contains("First change at action 1"));
        let mut changed = result.clone();
        changed["seed"] = serde_json::json!(2);
        assert!(compare_transfer(&runs, &runs[1], &changed).is_none());
        fs::remove_file(runs[0].join("calibration.json")).unwrap();
        assert!(comparison().is_none());
        fs::remove_dir_all(dir).unwrap();
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
