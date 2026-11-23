//! Local simulation jobs shared by the four AISLE Studio panels.
use serde_json::Value;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    fs, io,
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
#[derive(Clone, Debug)]
pub enum Action {
    Select(Robot),
    Prepare {
        task: String,
        codex: bool,
        revise: bool,
    },
    Inspect,
    Fixture(bool),
    OlderPlan,
    NewerPlan,
    Run,
    Stop,
    Previous,
    Next,
    Open,
}
fn read(path: &Path, name: &str) -> Option<Value> {
    serde_json::from_slice(&fs::read(path.join(name)).ok()?).ok()
}
struct Job {
    child: Child,
    dir: PathBuf,
    op: &'static str,
    last: Instant,
}
impl Job {
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
    pub comparison: Option<f64>,
    job: Option<Job>,
    root: PathBuf,
}
impl Default for Workbench {
    fn default() -> Self {
        let root = crate::runtime::user_data().join("aisle");
        let mut w = Self {
            robot: Robot::Nano,
            task: REFERENCE.into(),
            status: "Ready".into(),
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
            comparison: None,
            job: None,
            root,
        };
        if let Ok(entries) = fs::read_dir(&w.root) {
            w.history = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && p.file_name()
                            .is_some_and(|n| n.to_string_lossy().ends_with("-run"))
                })
                .collect();
            w.history.sort();
            if let Some(path) = w.history.last().cloned() {
                w.select_run(path);
            }
        }
        w.restore_profile();
        w
    }
}
impl Workbench {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    fn restore_profile(&mut self) {
        let suffix = format!("-{}-prepare", self.robot.name());
        self.revisions = fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(&suffix))
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
        self.evidence = Self::receipt(&path);
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
            .filter_map(|r| r["measured_rad"].as_f64().map(|x| x as f32))
            .collect();
        self.selected = Some(path);
    }
    fn launch(
        &mut self,
        op: &'static str,
        codex: bool,
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
        if op == "prepare" {
            cmd.args([
                "--provider",
                if codex { "codex" } else { "reference" },
                "--task",
                &self.task,
            ]);
        }
        if op == "run" {
            cmd.args([
                "--fixture",
                if self.compliant {
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
            .stdin(Stdio::null())
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
                self.status = "Stopped by operator".into();
                let _ = fs::write(j.dir.join("stopped.json"), "{\"stopped\":true}");
                if j.op == "run" {
                    self.select_run(j.dir);
                }
            }
            return;
        }
        if self.busy() {
            return;
        }
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
                codex,
                revise,
            } => {
                self.task = task;
                self.prepared = None;
                self.plan = Value::Null;
                self.launch(
                    "prepare",
                    codex,
                    if revise { self.selected.clone() } else { None },
                )
            }
            Action::Fixture(value) => {
                self.compliant = value;
                Ok(())
            }
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
            Action::Inspect => self.launch("scene", false, None),
            Action::Run => self.launch("run", false, None),
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
        if let Some(m) = read(&job.dir, "scene.json") {
            self.scene = m;
        }
        let artifact = match job.op {
            "run" => "result.json",
            "prepare" => "controller.json",
            _ => "scene.json",
        };
        if status.is_ok_and(|s| s.is_some_and(|c| c.success())) && job.dir.join(artifact).is_file()
        {
            if job.op == "prepare" {
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
            let _ = fs::write(j.dir.join("stopped.json"), "{\"stopped\":true}");
        }
    }
}
