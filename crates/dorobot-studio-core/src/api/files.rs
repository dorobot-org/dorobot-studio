//! Backend backed by LeRobot datasets on disk.
//!
//! Local datasets, curation, Hub transfers and managed LeRobot capture.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::data::{EpisodeFrame, LeRobotDataset, telemetry_model::{TelemetryModel, SO101_MODEL_ID}};

use super::*;

fn remove_take_staging(path: &Path) {
    if path.parent() == Some(std::env::temp_dir().as_path())
        && path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("dorobot-capture-"))
    {
        let _ = fs::remove_dir_all(path);
    }
}

/// URDF and asset dir for a dataset's `robot_type`, when one ships in `data/`.
/// Mirrors the shipping player's mapping so both resolve the same robot.
fn urdf_for(robot_type: &str) -> Option<(PathBuf, PathBuf)> {
    let robot = robot_type.to_lowercase();
    const KNOWN: &[(&str, &str)] = &[
        ("so101", "so100"),
        ("so100", "so100"),
        ("aimee", "so100"),
        ("lekiwi", "lekiwi"),
        ("moss", "moss"),
        ("koch", "koch"),
        ("vx300s", "vx300s"),
    ];
    let dir = KNOWN
        .iter()
        .find(|(pat, _)| robot.contains(pat))
        .map(|(_, d)| (*d).to_string())
        .unwrap_or(robot);
    let relative = PathBuf::from("data").join(&dir);
    let assets = if relative.is_dir() { relative } else {
        crate::runtime::root().join("data").join(&dir)
    };
    let urdf = assets.join(format!("{dir}.urdf"));
    urdf.exists().then_some((urdf, assets))
}

/// Only jobs exported by Nexus declare this convention. Older SO-101
/// datasets use the existing robot-specific replay mappings below.
fn nexus_replay_model(dataset: &LeRobotDataset) -> Option<TelemetryModel> {
    if dataset.info.robot_type != "so101_follower" { return None; }
    let bytes = fs::read(dataset.root_path.join("meta/dorobot_capture.json")).ok()?;
    let provenance: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if !matches!(provenance["source_mode"].as_str(), Some("synthetic" | "calibrated_leader"))
        || provenance["sample_hz_simulated"].as_u64() != Some(20) { return None; }
    TelemetryModel::resolve(SO101_MODEL_ID, &crate::runtime::root())
}

fn nexus_replay_poses(
    profile: &TelemetryModel, names: &[String], frames: &[EpisodeFrame],
) -> Result<Vec<Vec<f32>>, String> {
    use telemetry::{JointUnit, JointValue};
    use urdf_rs::JointType;
    // RobotView uses movable joints in the order they occur in the URDF.
    let urdf = urdf_rs::read_file(&profile.urdf).map_err(|e| e.to_string())?;
    let order: Vec<_> = urdf.joints.into_iter()
        .filter(|joint| matches!(joint.joint_type, JointType::Revolute | JointType::Continuous | JointType::Prismatic))
        .map(|joint| joint.name).collect();
    let mut joints: Vec<_> = names.iter().map(|key| {
        let name = key.strip_suffix(".pos").unwrap_or(key);
        JointValue {
            name: name.to_owned(), position: 0.0,
            unit: if name == "gripper" { JointUnit::PercentOpen } else { JointUnit::Deg },
        }
    }).collect();
    frames.iter().map(|frame| {
        if frame.state.len() != joints.len() { return Err("state dimensions differ from the named axes".into()); }
        for (joint, &value) in joints.iter_mut().zip(&frame.state) { joint.position = value.into(); }
        profile.viewer_angles(&joints, &order).map_err(|e| e.to_string())
    }).collect()
}

/// Turn a recorded pose into URDF joint values.
///
/// LeRobot does not record units in the dataset, so the convention has to be
/// established against the model. Raw degrees is ruled out: it puts 1183 of
/// 2724 joint samples outside this URDF's declared limits. Two candidates
/// survive, and `DOROBOT_JOINT_MAP` selects between them while the SO-100 case
/// is being confirmed against video:
///
/// - `signflip` — degrees, with shoulder_pan/lift, elbow and wrist_flex
///   inverted. Three of those are forced: the recorded range only fits the
///   joint limit when negated.
/// - `norm` (default) — LeRobot's normalized units, +/-100 across each joint's
///   range and 0..100 for the gripper. The recorded spans match that shape:
///   every joint lands inside +/-102 while the gripper alone stays in 0..29.
///
/// Direction is a separate question, and the data cannot answer it: shoulder
/// pan's limit is symmetric, so a mirrored sweep satisfies every bound the
/// model declares. It took the video to catch that pan runs the other way —
/// the arm swung right as the recording panned left. Pan sits at 0.35 (dead
/// centre) on frame 0, so inverting it moves the rest pose by under a degree
/// and reverses the whole sweep, which is exactly what was observed.
fn to_urdf_pose(robot: &str, state: &[f32], limits: &[(f32, f32)]) -> Vec<f32> {
    if !(robot.contains("so100") || robot.contains("so101") || robot.contains("aimee")) {
        return state.to_vec();
    }
    let signflip = std::env::var("DOROBOT_JOINT_MAP").as_deref() == Ok("signflip");
    const SIGN: [f32; 6] = [-1.0, -1.0, -1.0, -1.0, 1.0, 1.0];
    // Which normalized channels run opposite to the URDF's axis.
    const FLIP: [bool; 6] = [true, false, false, false, false, false];
    state
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if signflip {
                v.to_radians() * SIGN.get(i).copied().unwrap_or(1.0)
            } else {
                let (lo, hi) = limits.get(i).copied().unwrap_or((-3.15, 3.15));
                let v = if FLIP.get(i).copied().unwrap_or(false) {
                    -v
                } else {
                    *v
                };
                // The gripper is the one channel LeRobot normalizes 0..100.
                let t = if i == 5 {
                    v / 100.0
                } else {
                    (v + 100.0) / 200.0
                };
                lo + t * (hi - lo)
            }
        })
        .collect()
}

/// Movable joint limits, in URDF order, read from the model the viewer loads.
fn urdf_limits(path: &Path) -> Vec<(f32, f32)> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for block in text.split("<joint ").skip(1) {
        let end = block.find("</joint>").unwrap_or(block.len());
        let block = &block[..end];
        if block.contains("type=\"fixed\"") {
            continue;
        }
        let grab = |key: &str| -> Option<f32> {
            let i = block.find(key)? + key.len();
            let rest = &block[i..];
            let j = rest.find('"')?;
            rest[..j].parse().ok()
        };
        if let (Some(lo), Some(hi)) = (grab("lower=\""), grab("upper=\"")) {
            out.push((lo, hi));
        }
    }
    out
}

/// Serial ports that could be a robot arm.
///
/// Bluetooth and debug consoles are always present on macOS and are never an
/// arm, so they are listed separately rather than offered as candidates: the
/// SO-100 shows up through a USB serial bridge (`usbserial`/`usbmodem` here,
/// `ttyUSB`/`ttyACM` on Linux).
pub fn serial_ports() -> (Vec<String>, usize) {
    let Ok(entries) = fs::read_dir("/dev") else {
        return (Vec::new(), 0);
    };
    let mut candidates = Vec::new();
    let mut others = 0usize;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let is_serial =
            name.starts_with("cu.") || name.starts_with("ttyUSB") || name.starts_with("ttyACM");
        if !is_serial {
            continue;
        }
        if name.contains("usbserial")
            || name.contains("usbmodem")
            || name.starts_with("ttyUSB")
            || name.starts_with("ttyACM")
        {
            candidates.push(format!("/dev/{name}"));
        } else {
            others += 1;
        }
    }
    candidates.sort();
    (candidates, others)
}

/// Scan `root` one level deep for anything that looks like a LeRobot dataset.
fn discover(root: &Path) -> Vec<PathBuf> {
    if root.join("meta/info.json").is_file() {
        return vec![root.to_path_buf()];
    }
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return found;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && p.join("meta").join("info.json").exists() {
            found.push(p);
        }
    }
    found.sort();
    found
}

/// Total bytes under a directory. Metadata-only walk, so it stays fast even
/// for datasets with hundreds of megabytes of video.
fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => total += e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => {}
            }
        }
    }
    total
}

pub struct FileBackend {
    nexus: nexus::NexusRuntime,
    aisle: aisle::Workbench,
    simulation: super::workflow::SimulationRuntime,
    nano_driver: super::supported::NanoRuntime,
    live_client: Option<super::live::LiveClient>,
    telemetry_receiver: Option<telemetry::ZenohReceiver>,
    telemetry_state: telemetry::TelemetryState,
    live_preview: PlaybackState,
    connection: super::live::ConnectionState,
    live_status: String,
    root: PathBuf,
    status: String,
    hub_rx: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    rig: Option<(PathBuf, super::capture::Rig)>,
    worker: Option<super::capture::Worker>,
    take: Option<serde_json::Value>,
    save_rx: Option<std::sync::mpsc::Receiver<Result<u64, String>>>,
    excluded: BTreeSet<u64>,
    undo_exclude: Option<u64>,
    screen: Screen,
    library: LibraryState,
    playback: PlaybackState,
    hardware: HardwareState,
    record: RecordState,
    eval: EvalState,
    /// Currently opened dataset, kept so episode selection avoids a reparse.
    open: Option<(String, LeRobotDataset)>,
    replay_model: Option<TelemetryModel>,
}

impl FileBackend {
    pub fn configure_telemetry(&mut self, options: telemetry::ZenohOptions) {
        match telemetry::ZenohReceiver::start(options) {
            Ok(receiver) => self.telemetry_receiver = Some(receiver),
            Err(error) => {
                self.telemetry_state.error = Some(error.to_string());
                self.telemetry_state.phase = telemetry::MirrorPhase::Error;
            }
        }
    }

    pub fn connect_live(&mut self, socket: PathBuf) {
        self.nano_driver.stop();
        // Returning from the base-driver test must also release its page
        // selection, otherwise Hardware keeps hiding the simulation feed.
        if self.hardware.nano.take().is_some() { self.rescan_hardware(); }
        use super::live::{ConnectionPhase, ConnectionState};
        self.live_client = None;
        if let Some(live) = &mut self.live_preview.live { live.connected = false; }
        self.connection = ConnectionState { source: socket.to_string_lossy().trim().into(), ..Default::default() };
        self.screen = Screen::Live;
        self.playback.is_playing = false;
        let path = PathBuf::from(&self.connection.source);
        if !cfg!(unix) || !path.is_absolute() || self.connection.source.len() > 100 {
            self.connection.phase = ConnectionPhase::Error;
            self.connection.detail = "Enter an absolute local socket path (up to 100 bytes).".into();
        } else {
            self.live_preview = super::live::empty_preview();
            self.live_client = Some(super::live::LiveClient::connect(path));
            self.connection.phase = ConnectionPhase::Connecting;
            self.connection.detail = "Waiting for the Dora source; reconnects automatically.".into();
        }
        self.live_status = self.connection.summary();
        self.status = self.live_status.clone();
    }

    pub fn new(root: impl AsRef<Path>) -> Self {
        Self::with_open(root, None)
    }

    /// `prefer` names the dataset to open on start; falls back to the first
    /// one found.
    pub fn with_open(root: impl AsRef<Path>, prefer: Option<&str>) -> Self {
        let mut b = Self {
            nexus: nexus::NexusRuntime::default(),
            aisle: aisle::Workbench::default(),
            simulation: super::workflow::SimulationRuntime::default(),
            nano_driver: super::supported::NanoRuntime::default(),
            live_client: None,
            telemetry_receiver: None,
            telemetry_state: telemetry::TelemetryState::default(),
            live_preview: super::live::empty_preview(),
            connection: super::live::ConnectionState::default(),
            live_status: super::live::ConnectionState::default().summary(),
            root: root.as_ref().to_path_buf(),
            status: "Choose Simulation or Real robot to begin. Existing recordings are in Library.".into(),
            hub_rx: None,
            rig: None,
            worker: None,
            take: None,
            save_rx: None,
            excluded: BTreeSet::new(),
            undo_exclude: None,
            screen: Screen::Hardware,
            library: LibraryState::default(),
            playback: PlaybackState::default(),
            hardware: HardwareState::default(),
            record: RecordState::default(),
            eval: EvalState::default(),
            open: None,
            replay_model: None,
        };
        b.rescan();
        // Open something immediately so Play is never blank.
        let pick = prefer
            .and_then(|p| b.library.datasets.iter().find(|d| d.id.contains(p)))
            .or_else(|| b.library.datasets.first())
            .map(|d| d.id.clone());
        if let Some(id) = pick {
            b.open_dataset(&id);
        }
        b
    }

    pub fn rescan(&mut self) {
        let mut datasets = Vec::new();
        for path in discover(&self.root) {
            let ds = match LeRobotDataset::open(&path) {
                Ok(ds) => ds,
                Err(e) => {
                    self.status = format!("{}: {e}", path.display());
                    continue;
                }
            };
            let tags = Self::load_tags(&path.join("meta/dorobot_tags.json"));
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "dataset".into());
            datasets.push(DatasetSummary {
                id: name.clone(),
                name,
                episodes: ds.info.total_episodes as u32,
                fps: ds.info.fps,
                robot: if ds.info.robot_type.is_empty() {
                    "unknown".into()
                } else {
                    ds.info.robot_type.clone()
                },
                size_gb: dir_size(&path) as f64 / 1e9,
                // Curation tallies come from the persisted sidecar.
                good: tags.iter().filter(|(_, t)| *t == Tag::Good).count() as u32,
                bad: tags.iter().filter(|(_, t)| *t == Tag::Bad).count() as u32,
                sync: SyncState::LocalOnly,
                thumbnail: None,
                path,
            });
        }
        self.library.datasets = datasets;
        self.library.devices.clear();
        self.library.sessions.clear();
    }

    fn open_dataset(&mut self, id: &str) {
        let Some(summary) = self.library.datasets.iter().find(|d| d.id == id).cloned() else {
            return;
        };
        let ds = match LeRobotDataset::open(&summary.path) {
            Ok(ds) => ds,
            Err(e) => {
                self.status = e.to_string();
                return;
            }
        };
        self.excluded = fs::read(summary.path.join("meta/dorobot_excluded.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();

        let fps = ds.info.fps.max(1e-6);
        let episodes: Vec<EpisodeEntry> = ds
            .episodes
            .iter()
            .filter(|e| !self.excluded.contains(&e.episode_index))
            .map(|e| EpisodeEntry {
                index: e.episode_index,
                task_group: e
                    .tasks
                    .first()
                    .and_then(|t| ds.get_task(*t))
                    .unwrap_or("untitled")
                    .to_string(),
                duration_s: e.length as f64 / fps,
                tag: None,
            })
            .collect();

        // Channel names come from `features`, so plots can label series
        // instead of falling back to state[0..n].
        let names_of = |key: &str| -> Vec<String> {
            ds.info
                .features
                .get(key)
                .and_then(|f| f.names.clone())
                .unwrap_or_default()
        };

        self.replay_model = nexus_replay_model(&ds);
        self.playback = PlaybackState {
            dataset_name: summary.name.clone(),
            dataset_root: Some(
                summary.path.canonicalize().unwrap_or_else(|_| summary.path.clone()),
            ),
            selected: episodes.first().map(|e| e.index),
            episodes,
            stats: EpisodeStats::default(),
            drift_series: Vec::new(),
            state_names: names_of("observation.state"),
            action_names: names_of("action"),
            state_series: Vec::new(),
            action_series: Vec::new(),
            current_time: 0.0,
            is_playing: false,
            speed: 1.0,
            video_paths: BTreeMap::new(),
            video_frame_offset: 0,
            video_frame_offsets: BTreeMap::new(),
            robot_urdf: self.replay_model.as_ref().map(|model| (model.urdf.clone(), model.assets.clone()))
                .or_else(|| urdf_for(&ds.info.robot_type)),
            joint_frames: Vec::new(),
            scene: None,
            live: None,
        };
        // Restore any curation recorded for this dataset before.
        let saved = Self::load_tags(&summary.path.join("meta").join("dorobot_tags.json"));
        for (idx, tag) in saved {
            if let Some(e) = self.playback.episodes.iter_mut().find(|e| e.index == idx) {
                e.tag = Some(tag);
            }
        }
        self.open = Some((id.to_string(), ds));
        if let Some(idx) = self.playback.selected {
            self.select_episode(idx);
        }
    }

    fn select_episode(&mut self, index: u64) {
        let Some((_, ds)) = &self.open else { return };
        let data = match ds.load_episode(index) {
            Ok(data) => data,
            Err(e) => {
                self.status = format!("Episode {index}: {e}");
                self.playback.is_playing = false;
                return;
            }
        };
        let fps = ds.info.fps.max(1e-6);

        // Real drift: how far each frame's recorded timestamp is from where a
        // constant frame rate would put it, in frames.
        let drift: Vec<f64> = data
            .frames
            .iter()
            .map(|f| (f.timestamp - f.frame_index as f64 / fps) * fps)
            .collect();
        let mean_drift = if drift.is_empty() {
            0.0
        } else {
            drift.iter().sum::<f64>() / drift.len() as f64
        };

        let task = ds
            .episodes
            .iter()
            .find(|e| e.episode_index == index)
            .and_then(|e| e.tasks.first().copied())
            .and_then(|t| ds.get_task(t))
            .unwrap_or("")
            .to_string();

        // Keep every channel; the signal view pages through a small number of
        // lanes. A six-channel cap hides most joints on bimanual robots.
        let series = |pick: fn(&crate::data::lerobot_dataset::EpisodeFrame) -> &Vec<f32>,
                      names: &[String]| {
            let width = data
                .frames
                .first()
                .map(|f| pick(f).len())
                .unwrap_or(0);
            (0..width)
                .map(|c| PlotChannel {
                    name: names.get(c).cloned().unwrap_or_else(|| format!("ch {c}")),
                    points: data
                        .frames
                        .iter()
                        .filter_map(|f| pick(f).get(c).map(|v| (f.timestamp, *v as f64)))
                        .collect(),
                })
                .collect::<Vec<_>>()
        };
        self.playback.state_series = series(|f| &f.state, &self.playback.state_names);
        self.playback.action_series = series(|f| &f.action, &self.playback.action_names);

        self.playback.video_paths = data
            .video_paths
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        self.playback.video_frame_offset = data.video_frame_offset;
        self.playback.video_frame_offsets = data.video_frame_offsets.into_iter().collect();
        let mut pose_error = None;
        self.playback.joint_frames = if let Some(profile) = &self.replay_model {
            match nexus_replay_poses(profile, &self.playback.state_names, &data.frames) {
                Ok(poses) => poses,
                Err(error) => { pose_error = Some(error); Vec::new() }
            }
        } else {
            let robot = ds.info.robot_type.to_lowercase();
            let limits = self.playback.robot_urdf.as_ref()
                .map(|(urdf, _)| urdf_limits(urdf)).unwrap_or_default();
            data.frames.iter().map(|frame| to_urdf_pose(&robot, &frame.state, &limits)).collect()
        };
        self.playback.scene = match scene::SceneReplay::load(
            &ds.root_path,
            index,
            &data.frames.iter().map(|f| f.timestamp).collect::<Vec<_>>(),
        ) {
            Ok(scene) => scene,
            Err(e) => {
                ::log::error!("Replay scene: {e}");
                None
            }
        };
        // A new episode starts at its head, stopped.
        self.playback.current_time = 0.0;
        self.playback.is_playing = false;

        self.playback.selected = Some(index);
        self.playback.stats = EpisodeStats {
            frames: data.frames.len() as u64,
            duration_s: data.frames.len() as f64 / fps,
            fps,
            drift_frames: mean_drift,
            task,
            state_channels: data.frames.first().map(|f| f.state.len()).unwrap_or(0),
            action_channels: data.frames.first().map(|f| f.action.len()).unwrap_or(0),
        };
        self.playback.drift_series = drift;
        self.status = format!(
            "{} · episode {index} · {} frames",
            self.playback.dataset_name, self.playback.stats.frames
        );
        if let Some(error) = pose_error {
            self.status.push_str(&format!(" · Could not replay the SO-101 pose: {error}"));
        }
    }

    /// Rebuild the setup wizard from what is actually attached.
    ///
    /// Only the first step can be answered without the arm: finding the port is
    /// a filesystem question, while talking to the motors and sweeping the
    /// joints are not. The later steps stay pending and say so rather than
    /// showing a wizard that looks ready to run.
    pub fn rescan_hardware(&mut self) {
        if self.hardware.nano.is_some() {
            self.hardware.robot_label = super::supported::NANO_LABEL.into();
            self.hardware.robot_urdf = urdf_for("adora_nano");
            self.hardware.steps.clear();
            self.hardware.joints.clear();
            self.hardware.live_angles.clear();
            self.hardware.active_joint = None;
            self.hardware.can_advance = false;
            self.hardware.instruction = "Mobile base: forward, strafe and turn.\nArms, head and cameras are not included in this driver.\n\nL1: hold to drive · release to stop\nLeft stick: forward / strafe · right stick: turn\nOptions: pause / resume\n\nMotor IDs: 13 left front · 14 rear · 15 right front\nCommand units: vendor ticks. No measured odometry.".into();
            let ports = serial_ports().0;
            self.hardware.instruction.push_str(&if ports.is_empty() { "\n\nNo USB serial device found. Bus test is available without a robot.".into() } else { format!("\n\nUSB candidates: {}", ports.join(", ")) });
            return;
        }
        let (ports, others) = serial_ports();
        let found = !ports.is_empty();
        let step = |title: &str, state: StepState| WizardStep {
            title: title.into(),
            state,
        };
        self.hardware.steps = vec![
            step(
                "Find port",
                if found {
                    StepState::Done
                } else {
                    StepState::Active
                },
            ),
            step("Motors", StepState::Pending),
            step("Calibrate", StepState::Pending),
            step("Cameras", StepState::Pending),
            step("Save profile", StepState::Pending),
        ];
        self.hardware.robot_label = if found {
            "unidentified arm".into()
        } else {
            String::new()
        };
        // The mirror shows the model for whatever robot the open dataset names,
        // falling back to the SO-100 so the stage is not empty before one is
        // chosen.
        let robot = self.rig.as_ref().map(|(_,rig)|rig.robot_type.as_str()).unwrap_or("so101");
        self.hardware.robot_urdf = urdf_for(robot);
        self.hardware.instruction = if found {
            format!(
                "Found {}: {}",
                if ports.len() == 1 { "a port" } else { "ports" },
                ports.join(", ")
            )
        } else if others > 0 {
            format!("No arm found. {others} serial ports are present but all are Bluetooth or debug consoles — connect the arm over USB and rescan.")
        } else {
            "No serial ports at all. Connect the arm over USB and rescan.".into()
        };
        // Joints stay empty until a motor answers; a calibration list here
        // would be an invention.
        self.hardware.joints.clear();
        self.hardware.live_angles.clear();
        self.hardware.active_joint = None;
        self.hardware.can_advance = self.rig.is_some();
        if let Some((_, rig)) = &self.rig {
            self.hardware.robot_label = rig.robot_type.clone();
            self.hardware.instruction.push_str("\nRig loaded. Continue starts LeRobot calibration; follow the prompts and press Continue when asked.");
        }
    }

    /// Where curation tags live for a dataset. A sidecar, not a rewrite of the
    /// LeRobot metadata: tagging must never risk the recording itself.
    fn tag_file(&self) -> Option<PathBuf> {
        let id = self.open.as_ref().map(|(id, _)| id.clone())?;
        let ds = self.library.datasets.iter().find(|d| d.id == id)?;
        Some(ds.path.join("meta").join("dorobot_tags.json"))
    }

    /// Persist tags. Curation that does not survive a restart is not curation.
    fn save_tags(&mut self) -> Result<(), String> {
        let path = self.tag_file().ok_or("No dataset selected")?;
        // Keep tags belonging to excluded episodes, which are absent from the list.
        let mut tags: BTreeMap<u64, Tag> = Self::load_tags(&path).into_iter().collect();
        for e in &self.playback.episodes {
            tags.remove(&e.index);
            if let Some(t) = e.tag {
                tags.insert(e.index, t);
            }
        }
        let mut out = String::from("{\n");
        let mut first = true;
        for (index, t) in tags {
            if !first {
                out.push_str(",\n");
            }
            first = false;
            out.push_str(&format!("  \"{}\": \"{}\"", index, t.label()));
        }
        out.push_str("\n}\n");
        let temp = path.with_extension("json.tmp");
        if let Err(e) = fs::write(&temp, out).and_then(|_| fs::rename(temp, &path)) {
            return Err(format!("Could not save tags: {e}"));
        }
        // Library tallies are derived, so they follow the same edit.
        let (good, bad) = self
            .playback
            .episodes
            .iter()
            .fold((0, 0), |(g, b), e| match e.tag {
                Some(Tag::Good) => (g + 1, b),
                Some(Tag::Bad) => (g, b + 1),
                None => (g, b),
            });
        let id = self.open.as_ref().map(|(id, _)| id.clone());
        if let Some(d) = self
            .library
            .datasets
            .iter_mut()
            .find(|d| Some(&d.id) == id.as_ref())
        {
            d.good = good;
            d.bad = bad;
        }
        Ok(())
    }

    /// Read the sidecar written by [`Self::save_tags`].
    fn load_tags(path: &Path) -> Vec<(u64, Tag)> {
        let Ok(bytes) = fs::read(path) else {
            return Vec::new();
        };
        let Ok(map) = serde_json::from_slice::<BTreeMap<u64, String>>(&bytes) else {
            return Vec::new();
        };
        map.into_iter()
            .filter_map(|(i, tag)| match tag.as_str() {
                "good" => Some((i, Tag::Good)),
                "bad" => Some((i, Tag::Bad)),
                _ => None,
            })
            .collect()
    }

    fn configure(&mut self, path: PathBuf) {
        if self.worker.is_some() || self.save_rx.is_some() || self.take.is_some() {
            self.status =
                "Stop and save or discard the current take before changing profiles.".into();
            return;
        }
        match super::capture::Rig::load(&path) {
            Ok(rig) => {
                self.simulation.state.simulation = false;
                self.simulation.state.calibrated = false;
                self.record = RecordState {
                    robot_urdf: urdf_for(&rig.robot_type),
                    profile_label: rig.robot_type.clone(),
                    task: rig.task.clone(),
                    episode_target: rig.episode_target,
                    episode_index: 1,
                    cameras: rig.cameras.keys().cloned().collect(),
                    ..Default::default()
                };
                self.hardware.robot_label = rig.robot_type.clone();
                self.hardware.can_advance = true;
                self.hardware.instruction = "1. Check leader, follower and cameras. 2. Press Continue to calibrate; follow each LeRobot prompt. 3. Open Record after calibration completes.".into();
                let pending = fs::read(path.with_extension("pending-take.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
                self.rig = Some((path, rig));
                self.status =
                    "Profile ready. Calibrate in Robot setup, then open Record.".into();
                if let Some(take) =
                    pending.filter(|t| t["frames"].as_str().is_some_and(|p| Path::new(p).is_file()))
                {
                    self.record.last_take = Some(TakeReview {
                        thumbnails: Vec::new(),
                        verdict: TakeVerdict::ReadyToSave,
                        warnings: Vec::new(),
                    });
                    self.take = Some(take);
                    self.status =
                        "Recovered an unsaved take. Save or discard it before recording.".into();
                }
            }
            Err(e) => self.status = e,
        }
    }
    fn start_worker(&mut self, calibration: bool) {
        if self.worker.is_some() || self.save_rx.is_some() || (!calibration && self.take.is_some())
        {
            self.status = "Save or discard the pending take before recording again.".into();
            return;
        }
        let Some((path, _)) = &self.rig else {
            self.status = "Open Robot setup to create or load a rig profile.".into();
            return;
        };
        let profile = fs::read(path).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        for role in ["follower", "leader"] {
            let key = format!("{role}_port");
            let port = profile.as_ref().and_then(|v|v[&key].as_str()).unwrap_or("");
            if port.is_empty() || !Path::new(port).exists() {
                self.status = format!("{role} device not found: {port}. Connect it over USB, then create or load a profile with the correct serial port.");
                self.hardware.instruction = self.status.clone();
                return;
            }
        }
        match super::capture::Worker::start(path, calibration) {
            Ok(w) => {
                self.worker = Some(w);
                self.record.busy = true;
                if calibration { self.hardware.can_advance = false; self.simulation.state.calibrated = false; self.record.ready_to_record = false; }
                self.status = if calibration {
                    "Calibration starting…"
                } else {
                    "Connecting to the rig…"
                }
                .into();
            }
            Err(e) => self.status = e,
        }
    }
    fn exclude(&mut self, index: u64, undo: bool) {
        let Some((id, ds)) = &self.open else { return };
        let id = id.clone();
        let mut excluded = self.excluded.clone();
        if undo {
            excluded.remove(&index);
        } else {
            excluded.insert(index);
        }
        let path = ds.root_path.join("meta/dorobot_excluded.json");
        let temp = path.with_extension("json.tmp");
        let result = fs::write(&temp, serde_json::to_vec(&excluded).unwrap())
            .and_then(|_| fs::rename(temp, path));
        match result {
            Ok(()) => {
                self.undo_exclude = if undo { None } else { Some(index) };
                self.open_dataset(&id);
                self.status = if undo { "Episode restored." } else { "Episode excluded from review; original data preserved. Press Ctrl/Cmd+Z to undo." }.into();
            }
            Err(e) => self.status = format!("Could not save exclusion: {e}"),
        }
    }

    /// Distinct task descriptions, used by the Play tree's group headers.
    pub fn task_groups(&self) -> Vec<String> {
        self.playback
            .episodes
            .iter()
            .map(|e| e.task_group.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

impl Backend for FileBackend {
    fn rig_profile(&self) -> Option<PathBuf> { self.rig.as_ref().map(|(p,_)|p.clone()) }
    fn workflow(&self) -> super::workflow::WorkflowState { self.simulation.state.clone() }
    fn dataset_id(&self) -> Option<&str> {
        self.open.as_ref().map(|(id, _)| id.as_str())
    }
    fn status(&self) -> Option<&str> {
        if self.screen == Screen::Hardware { if let Some(nano) = &self.hardware.nano { return Some(&nano.detail); } }
        Some(if (self.screen == Screen::Live && self.hardware.nano.is_none()) || (self.screen == Screen::Record && self.simulation.state.simulation && self.simulation.state.active) { &self.live_status } else { &self.status })
    }
    fn screen(&self) -> Screen {
        self.screen
    }
    fn library(&self) -> &LibraryState {
        &self.library
    }
    fn hardware(&self) -> &HardwareState {
        &self.hardware
    }
    fn record(&self) -> &RecordState {
        &self.record
    }
    fn playback(&self) -> &PlaybackState {
        if (self.screen == Screen::Live && self.hardware.nano.is_none()) || (self.screen == Screen::Record && self.simulation.state.simulation && self.simulation.state.active) { &self.live_preview } else { &self.playback }
    }
    fn replay(&self) -> &PlaybackState { &self.playback }
    fn aisle(&self) -> Option<&aisle::Workbench> { Some(&self.aisle) }
    fn nexus(&self) -> Option<&nexus::NexusState> { Some(&self.nexus.state) }
    fn telemetry(&self) -> Option<&telemetry::TelemetryState> {
        Some(&self.telemetry_state)
    }

    fn live_connection(&self) -> Option<&super::live::ConnectionState> { Some(&self.connection) }
    fn eval(&self) -> &EvalState {
        &self.eval
    }

    fn dispatch(&mut self, intent: Intent) {
        if self.nexus.state.busy && matches!(intent, Intent::StartSimulation | Intent::WizardAdvance
            | Intent::RecordStart | Intent::ReRecord | Intent::ConnectNanoBase { .. } | Intent::SimulationCapture { .. }) {
            self.status = "Finish the local Nexus job before starting another device or simulator.".into();
            return;
        }
        if self.playback().live.is_some() {
            match &intent {
                Intent::Seek(_) | Intent::TogglePlay | Intent::StepFrames(_) | Intent::SelectEpisode(_)
                | Intent::SetSpeed(_) | Intent::TagEpisode {..} | Intent::DeleteEpisode(_)
                | Intent::UndoExclude => return,
                _ => {},
            }
        }

        match intent {
            Intent::Aisle(action) => self.aisle.dispatch(action),
            Intent::Nexus(intent) => {
                use nexus::NexusIntent::*;
                if !self.nexus.state.allows(&intent) { return; }
                match intent {
                    UseOpenDataset => {
                        if let Some((_, dataset)) = &self.open {
                            match dataset.root_path.canonicalize() {
                                Ok(path) => self.nexus.state.settings.dataset = path.display().to_string(),
                                Err(error) => self.nexus.state.snapshot.message = error.to_string(),
                            }
                        } else {
                            self.nexus.state.snapshot.message = "Open a dataset in Data Workspace first.".into();
                        }
                    }
                    OpenSavedDataset => {
                        if let Some(path) = self.nexus.state.snapshot.dataset.clone() {
                            self.dispatch(Intent::OpenFolder(path));
                        }
                    }
                    StartCapture | Train | Run if self.worker.is_some() || self.simulation.state.active
                        || self.nano_driver.state.active || self.take.is_some() || self.save_rx.is_some() => {
                        self.nexus.state.snapshot.message = "Finish the active robot or simulation session and review its take first.".into();
                    }
                    StartCapture => {
                        self.nexus.dispatch(StartCapture);
                        if self.nexus.state.busy { self.screen = Screen::Record; }
                    }
                    other => self.nexus.dispatch(other),
                }
            }
            Intent::SelectTelemetrySource(source) => self.telemetry_state.select(source),
            Intent::SetCapturePreferences { task, destination } => {
                if !self.simulation.state.pending() || self.simulation.state.phase == "review" {
                    self.simulation.state.task = task; self.simulation.state.destination = destination;
                }
            }
            Intent::SelectSimulation(selected) => {
                if self.simulation.state.simulation == selected { self.screen = Screen::Hardware; return; }
                if self.simulation.state.active || self.worker.is_some() || self.take.is_some() || self.save_rx.is_some() {
                    self.status = "Finish the current session before changing source. Stop simulation or save/discard the current take.".into();
                } else {
                    self.nano_driver.stop(); self.simulation.state.simulation = selected;
                    self.hardware.nano = None; self.screen = Screen::Hardware; self.rescan_hardware();
                    self.status = if selected { "Simulation setup · Start a scene when ready." } else { "Real robot setup · Choose a driver and configure its devices." }.into();
                }
            }
            Intent::StartSimulation => {
                self.nano_driver.stop();
                if self.worker.is_some() || self.take.is_some() { self.status = "Finish the real robot session first.".into(); return; }
                self.simulation.state.simulation = true;
                if let Err(e) = self.simulation.start() { self.status = e.clone(); self.simulation.state.detail = e; }
            }
            Intent::StopSimulation => {
                if let Err(e) = self.simulation.stop() { self.status = e; }
                else { self.dispatch(Intent::DisconnectLive); }
            }
            Intent::SimulationCapture { action, task, destination } => {
                if let Err(e) = self.simulation.command(&action, task, destination) { self.status = e; }
            }
            Intent::OpenSavedRecording => {
                let path = if self.simulation.state.simulation { self.simulation.state.dataset.clone() } else { self.rig.as_ref().map(|(_,r)|r.dataset.clone()) };
                if let Some(path) = path { self.dispatch(Intent::OpenFolder(path)); }
            }
            Intent::CreateRig { robot_type, follower, leader, cameras, task, destination } => {
                if self.worker.is_some() || self.take.is_some() || self.save_rx.is_some() || self.simulation.state.active { self.status = "Finish the current session before replacing the rig.".into(); return; }
                match super::capture::create_profile(&robot_type, &follower, &leader, &cameras, &task, &destination) {
                    Ok(path) => self.dispatch(Intent::ConfigureRig(path)), Err(e) => self.status = e,
                }
            }
            Intent::SetLiveSource(source) => {
                if self.live_client.is_none() { self.connection.source = source; }
            }
            Intent::ConnectLive(path) => self.connect_live(path),
            Intent::DisconnectLive => {
                self.live_client = None;
                self.connection.phase = super::live::ConnectionPhase::Disconnected;
                self.connection.preview_fps = 0.;
                self.connection.frame_age_ms = None;
                self.connection.detail = "Preview disconnected. Source simulation and recording continue.".into();
                if let Some(live) = &mut self.live_preview.live { live.connected = false; }
                self.live_status = self.connection.summary();
                if self.screen == Screen::Hardware {
                    self.status = "Robot setup · choose Nano simulation to return.".into();
                }
            }
            Intent::OpenLiveRecording => {
                if let Some(path) = self.connection.dataset.clone().filter(|p| p.join("meta/info.json").is_file()) {
                    self.dispatch(Intent::OpenFolder(path));
                } else {
                    self.connection.detail = "The source has not finished saving a recording yet.".into();
                    self.live_status = self.connection.summary();
                }
            }
            Intent::ConfigureRig(path) => { if self.simulation.state.active { self.status = "Stop simulation before switching to a real robot.".into(); return; } self.nano_driver.stop(); self.configure(path); self.screen = Screen::Hardware; },
            Intent::SelectNanoBase(selected) => {
                if self.simulation.state.active { self.status = "Stop simulation before switching to a real robot.".into(); return; }
                self.simulation.state.simulation = false;
                if selected { self.simulation.state.calibrated = false; self.record.ready_to_record = false; }
                if !selected { self.nano_driver.stop(); }
                self.hardware.nano = selected.then(|| self.nano_driver.state.clone());
                self.rescan_hardware();
            }
            Intent::ConnectNanoBase { mock, port } => {
                if self.worker.is_some() || self.simulation.state.active {
                    self.status = "Stop the capture worker and simulation before connecting the Nano base.".into();
                } else {
                    // A physical driver must not share gamepad control with an
                    // apparently live simulation in the Hardware page.
                    self.live_client = None;
                    if let Some(live) = &mut self.live_preview.live { live.connected = false; }
                    self.connection.phase = super::live::ConnectionPhase::Disconnected;
                    if let Err(error) = self.nano_driver.start(mock, &port) {
                        self.nano_driver.state.detail = error;
                    }
                    self.hardware.nano = Some(self.nano_driver.state.clone());
                    self.rescan_hardware();
                }
            }
            Intent::DisconnectNanoBase => self.nano_driver.stop(),
            Intent::HubTransfer { repo, destination } => {
                if self.hub_rx.is_some() || self.record.recording || self.save_rx.is_some() { self.status = "Finish the active operation before transferring a dataset.".into(); return; }
                let upload = destination.is_none();
                let path = destination.or_else(|| self.open.as_ref().map(|(_,ds)| ds.root_path.clone()));
                let Some(path) = path else { self.status = "Open a dataset first.".into(); return; };
                let (tx, rx) = std::sync::mpsc::channel(); self.hub_rx = Some(rx);
                self.status = format!("{} {repo}…", if upload { "Uploading" } else { "Downloading" });
                self.library.busy = Some(self.status.clone());
                std::thread::spawn(move || {
                    let result = std::process::Command::new(crate::runtime::native()).arg("hub").arg(if upload { "push" } else { "pull" }).arg(repo).arg(&path).output()
                        .map_err(|e| e.to_string()).and_then(|o| if o.status.success() { Ok(path) } else { Err(String::from_utf8_lossy(&o.stderr).chars().rev().take(1500).collect::<String>().chars().rev().collect()) });
                    let _ = tx.send(result);
                });
            },
            Intent::OpenFolder(path) => { self.root = path; self.rescan(); if let Some(id) = self.library.datasets.first().map(|d| d.id.clone()) { self.open_dataset(&id); self.screen = Screen::Play; } },
            Intent::NewRecordingSession => { self.screen = Screen::Hardware; },
            Intent::WizardAdvance => {
                if self.hardware.nano.is_some() { return; }
                if let Some(w) = &mut self.worker { if w.calibration { if let Err(e) = w.send("") { self.status = e; } } }
                else { self.start_worker(true); }
            }
            Intent::RecordStart => { if !self.simulation.state.calibrated { self.status = "Calibrate the leader and follower in Robot setup before recording.".into(); self.screen = Screen::Hardware; return; } self.nano_driver.stop(); self.start_worker(false); },
            Intent::RecordStop => { if let Some(w) = &mut self.worker { if let Err(e) = w.send("stop") { self.status = e; } self.status = "Stopping capture and closing cameras…".into(); self.record.busy = true; } },
            Intent::SaveEpisode => {
                if self.hub_rx.is_none() && self.save_rx.is_none() && self.worker.is_none() {
                    if let (Some((_, rig)), Some(take)) = (&self.rig, &self.take) {
                        let (rig, take) = (rig.clone(), take.clone());
                        let (tx, rx) = std::sync::mpsc::channel(); self.save_rx = Some(rx); self.record.busy = true;
                        std::thread::spawn(move || { let _ = tx.send(super::capture::save_take(&rig, &take)); });
                        self.status = "Saving episode…".into();
                    }
                }
            }
            Intent::DiscardLast | Intent::ReRecord => {
                if self.worker.is_none() && self.save_rx.is_none() {
                    if let Some(take) = self.take.take() { if let Some(path) = take["frames"].as_str().and_then(|p| Path::new(p).parent()) { remove_take_staging(path); } self.record.discarded += 1; }
                    if let Some((profile,_)) = &self.rig { let _ = fs::remove_file(profile.with_extension("pending-take.json")); }
                    self.record.last_take = None; self.status = "Take discarded. Ready to record.".into();
                    if matches!(intent, Intent::ReRecord) { self.start_worker(false); }
                }
            }
            Intent::SetSoundCues(on) => self.record.sound_cues = on,
            Intent::DeleteEpisode(index) => self.exclude(index, false),
            Intent::UndoExclude => { if let Some(i) = self.undo_exclude { self.exclude(i, true); } },
            Intent::Navigate(s) => {
                if !matches!(s, Screen::Hardware | Screen::Live) { self.nano_driver.stop(); }
                if s == Screen::Live { self.playback.is_playing = false; }
                if s == Screen::Hardware {
                    self.rescan_hardware();
                }
                self.screen = s;
                if s == Screen::Hardware {
                    self.status = if self.simulation.state.simulation { "Simulation setup · Start a scene, then open controls.".into() } else { self.hardware.instruction.clone() };
                } else if s == Screen::Record && !self.simulation.state.simulation {
                    self.status = if self.simulation.state.calibrated { format!("{} · {}",self.record.profile_label,self.record.task) } else { "Complete calibration in Robot setup before recording.".into() };
                }
            }
            Intent::RescanDatasets => {
                self.rescan();
                self.rescan_hardware();
            }
            Intent::OpenDataset(id) => {
                self.open_dataset(&id);
                self.screen = Screen::Play;
            }
            Intent::SelectEpisode(i) => self.select_episode(i),
            // Re-enumerate ports: the only wizard step that can be answered
            // without the arm attached.
            Intent::WizardRestartStep => { if self.hardware.nano.is_some() { self.rescan_hardware(); return; } if self.worker.as_ref().is_some_and(|w| !w.calibration) { self.status = "Stop and review the recording before restarting hardware setup.".into(); return; } self.worker = None; self.record.busy = false; self.rescan_hardware(); self.hardware.can_advance = self.rig.is_some(); },
            Intent::TogglePlay => {
                // Restart from the head when play is pressed at the end,
                // rather than sitting there doing nothing.
                if !self.playback.is_playing
                    && self.playback.current_time >= self.playback.stats.duration_s - 1e-6
                {
                    self.playback.current_time = 0.0;
                }
                self.playback.is_playing = !self.playback.is_playing && self.playback.stats.frames > 0;
            }
            Intent::Seek(t) => {
                if t.is_finite() { self.playback.current_time = t.clamp(0.0, self.playback.stats.duration_s); }
            }
            Intent::StepFrames(n) => {
                let fps = self.playback.stats.fps.max(1e-6);
                let t = self.playback.current_time + n as f64 / fps;
                if t.is_finite() { self.playback.current_time = t.clamp(0.0, self.playback.stats.duration_s); }
                self.playback.is_playing = false;
            }
            Intent::SetSpeed(s) => { if s.is_finite() { self.playback.speed = s.clamp(0.05, 8.0); } },
            Intent::TagEpisode { episode, tag } => {
                let previous = self.playback.episodes.iter().find(|e| e.index == episode).and_then(|e| e.tag);
                if let Some(e) = self.playback.episodes.iter_mut().find(|e| e.index == episode) {
                    // Clicking the tag an episode already carries clears it,
                    // so the same button both sets and undoes.
                    e.tag = if e.tag == tag { None } else { tag };
                }
                if let Err(e) = self.save_tags() {
                    if let Some(row) = self.playback.episodes.iter_mut().find(|e| e.index == episode) { row.tag = previous; }
                    self.status = e;
                }
            }
            // Transfers require a repository and destination in the native form.
            Intent::PullFromHub | Intent::PushToHub => self.status = "Use hf download/upload for Hub transfers; then Open folder or Rescan. No transfer has been started.".into(),
            Intent::StopRollout => { self.eval.driving = false; self.status = "No policy runner is connected.".into(); },
        }
    }

    fn poll(&mut self) {
        self.aisle.poll();
        self.nexus.poll();
        if let Some(receiver) = &mut self.telemetry_receiver {
            let now = std::time::Instant::now();
            let catalog = receiver.poll(now);
            self.telemetry_state.update(catalog, now, |id| {
                id == crate::data::telemetry_model::SO101_MODEL_ID
            });
        }
        let was_ready = self.simulation.state.ready;
        self.simulation.poll();
        if !was_ready && self.simulation.state.ready {
            if let Some(socket) = self.simulation.socket.clone() {
                let previous = self.screen;
                self.connect_live(socket);
                if previous != Screen::Hardware && previous != Screen::Live { self.screen = previous; }
            }
        }
        self.nano_driver.poll();
        if self.hardware.nano.is_some() { self.hardware.nano = Some(self.nano_driver.state.clone()); }
        use super::live::ConnectionPhase;
        if let Some(client) = &mut self.live_client {
            if let Some(mut packet) = client.take() {
                let finished = packet.header.finished;
                self.connection.dataset = packet.header.dataset.clone().filter(|p| p.join("meta/info.json").is_file());
                self.connection.phase = if finished { ConnectionPhase::Completed } else { ConnectionPhase::Streaming };
                self.connection.detail = if finished {
                    if self.connection.dataset.is_some() { "Recording ready. Open it in Replay." } else { "Last frame retained; waiting for the next session." }.into()
                } else { packet.header.phase.clone() };
                // Completion repeats the last image with a notification sequence;
                // it is not another captured frame.
                if !finished || self.live_preview.stats.frames == 0 {
                    if finished { packet.header.sequence = packet.header.sequence.saturating_sub(1); }
                    let assets = crate::runtime::root().join("data/adora_nano");
                    let model = (assets.join("adora_nano.urdf"), assets);
                    super::live::apply_packet(&mut self.live_preview, packet, model);
                }
                if finished {
                    if let Some(live) = &mut self.live_preview.live { live.finished = true; live.connected = false; }
                }
            }
            if client.stale() && self.connection.phase != ConnectionPhase::Completed {
                self.connection.phase = ConnectionPhase::Waiting;
                self.connection.detail = format!("Reconnecting automatically. {}", client.error().unwrap_or_else(|| "No new frames.".into()));
                if let Some(live) = &mut self.live_preview.live { live.connected = false; }
            }
            self.connection.preview_fps = if self.connection.phase == ConnectionPhase::Streaming { client.preview_fps() } else { 0. };
            self.connection.frame_age_ms = client.sequence.map(|_| client.last_frame.elapsed().as_millis() as u64);
            self.connection.received_frames = client.received_frames;
            self.live_status = self.connection.summary();
        }

        if let Some(result) = self.hub_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.hub_rx = None;
            self.library.busy = None;
            match result {
                Ok(path) => {
                    self.root = path;
                    self.rescan();
                    self.status = "Hub transfer completed.".into();
                }
                Err(e) => self.status = format!("Hub transfer failed: {e}"),
            }
        }
        let events: Vec<_> = self
            .worker
            .as_ref()
            .map(|w| w.events.try_iter().collect())
            .unwrap_or_default();
        for v in events {
            match v["event"].as_str().unwrap_or("") {
                "ready" => {
                    self.record.recording = true;
                    self.record.busy = false;
                    self.record.elapsed_s = 0.0;
                    self.record.joints.clear();
                    self.status = "Recording from the rig.".into();
                }
                "frame" => {
                    self.record.elapsed_s = v["timestamp"].as_f64().unwrap_or(0.0);
                    if let Some(values) = v["state"].as_array() {
                        for (i, value) in values.iter().enumerate() {
                            let value = value.as_f64().unwrap_or(0.0) as f32;
                            if self.record.joints.len() <= i {
                                self.record.joints.push(LiveJoint {
                                    name: format!("J{}", i + 1),
                                    value,
                                    min: value,
                                    max: value,
                                    history: Vec::new(),
                                });
                            }
                            let j = &mut self.record.joints[i];
                            j.value = value;
                            j.min = j.min.min(value);
                            j.max = j.max.max(value);
                            j.history.push(value);
                            if j.history.len() > 120 {
                                j.history.remove(0);
                            }
                        }
                    }
                    if let (Some((_, rig)), Some((urdf, _))) = (&self.rig, &self.record.robot_urdf)
                    {
                        self.record.pose = to_urdf_pose(
                            &rig.robot_type,
                            &self
                                .record
                                .joints
                                .iter()
                                .map(|j| j.value)
                                .collect::<Vec<_>>(),
                            &urdf_limits(urdf),
                        );
                    }
                    self.record.previews =
                        serde_json::from_value(v["previews"].clone()).unwrap_or_default();
                }
                "take" => {
                    self.record.recording = false;
                    self.record.busy = false;
                    self.record.last_take = Some(TakeReview {
                        thumbnails: self.record.previews.values().cloned().collect(),
                        verdict: TakeVerdict::ReadyToSave,
                        warnings: Vec::new(),
                    });
                    self.status =
                        "Take ready. Save it or discard it before recording again.".into();
                    if let Some((profile, _)) = &self.rig {
                        let temp = profile.with_extension("pending-take.tmp");
                        let result =
                            serde_json::to_vec(&v)
                                .map_err(|e| e.to_string())
                                .and_then(|bytes| {
                                    fs::write(&temp, bytes)
                                        .and_then(|_| {
                                            fs::rename(
                                                temp,
                                                profile.with_extension("pending-take.json"),
                                            )
                                        })
                                        .map_err(|e| e.to_string())
                                });
                        if let Err(e) = result {
                            self.status =
                                format!("Take ready, but restart recovery could not be saved: {e}");
                        }
                    }
                    self.take = Some(v);
                }
                "prompt" => {
                    self.hardware.instruction = v["message"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .rev()
                        .take(1200)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    self.hardware.can_advance = true;
                }
                "calibrated" => {
                    self.hardware.can_advance = true;
                    self.simulation.state.calibrated = true;
                    self.record.ready_to_record = true;
                    self.hardware.instruction =
                        "Leader and follower calibrated. Open Record to capture.".into();
                    self.status = self.hardware.instruction.clone();
                    self.record.busy = false;
                }
                "worker_exit" => {
                    self.worker = None;
                    if self.record.recording {
                        self.record.recording = false;
                        self.status = "Capture worker exited; no take was saved.".into();
                    }
                    self.record.busy = false;
                }
                "error" => {
                    self.status = v["message"].as_str().unwrap_or("Capture failed").into();
                    self.hardware.instruction = self.status.clone();
                    self.record.recording = false;
                    self.record.busy = false;

                }
                _ => {}
            }
        }
        let result = self.save_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = result {
            self.save_rx = None;
            self.record.busy = false;
            match result {
                Ok(index) => {
                    if let Some((profile, _)) = &self.rig {
                        let _ = fs::remove_file(profile.with_extension("pending-take.json"));
                    }
                    self.record.saved += 1;
                    self.record.episode_index = index as u32 + 2;
                    self.record.last_take = None;
                    if let Some(take) = self.take.take() {
                        if let Some(dir) =
                            take["frames"].as_str().and_then(|p| Path::new(p).parent())
                        {
                            remove_take_staging(dir);
                        }
                    }
                    if let Some((_, rig)) = &self.rig {
                        self.root = rig.dataset.clone();
                    }
                    self.rescan();
                    self.status = format!("Episode {index} saved. Open it in Library to review.");
                }
                Err(e) => self.status = format!("Save failed; take retained: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_source_and_pending_recording_survive_navigation() {
        let mut b = FileBackend::new("does/not/exist");
        b.simulation.state.active = true;
        b.simulation.state.ready = true;
        b.simulation.state.phase = "recording".into();
        b.simulation.state.frames = 42;
        b.live_preview = super::super::live::empty_preview();
        for screen in [Screen::Library, Screen::Hardware, Screen::Play, Screen::Live, Screen::Record] {
            b.dispatch(Intent::Navigate(screen));
            assert_eq!(b.simulation.state.phase, "recording");
            assert_eq!(b.simulation.state.frames,42);
        }
        assert!(b.playback().live.is_some());
        b.dispatch(Intent::SelectNanoBase(true));
        assert!(b.hardware.nano.is_none());
        assert!(b.status.contains("Stop simulation"));
        b.dispatch(Intent::SelectSimulation(false));
        assert!(b.simulation.state.simulation);
    }

    #[test]
    fn real_recording_routes_to_calibration_without_starting_worker() {
        let mut b = FileBackend::new("does/not/exist");
        b.dispatch(Intent::SelectSimulation(false));
        b.dispatch(Intent::RecordStart);
        assert_eq!(b.screen(), Screen::Hardware);
        assert!(b.worker.is_none());
        assert!(b.status.contains("Calibrate"));
    }

    #[test]
    fn returning_to_simulation_clears_the_base_driver_page() {
        let mut backend = FileBackend::new("does/not/exist");
        backend.dispatch(Intent::Navigate(Screen::Hardware));
        backend.dispatch(Intent::SelectNanoBase(true));
        backend.dispatch(Intent::ConnectLive("/tmp/nano-gamepad-regression.sock".into()));
        assert!(backend.hardware.nano.is_none());
        backend.live_preview.dataset_name = "Nano gamepad · regression".into();
        backend.connection.phase = super::super::live::ConnectionPhase::Streaming;
        backend.dispatch(Intent::Navigate(Screen::Hardware));
        assert!(backend.playback().live.is_none());
        backend.dispatch(Intent::Navigate(Screen::Live));
        assert!(backend.playback().live.is_some());
        assert_eq!(backend.playback().dataset_name, "Nano gamepad · regression");
    }

    #[test]
    fn nano_base_selection_is_distinct_from_simulation_and_calibration() {
        let mut backend = FileBackend::new("does/not/exist");
        backend.live_preview = super::super::live::empty_preview();
        backend.live_preview.dataset_name = "Nano gamepad · test".into();
        backend.connection.phase = super::super::live::ConnectionPhase::Streaming;
        backend.dispatch(Intent::Navigate(Screen::Hardware));
        assert!(backend.playback().live.is_none());
        backend.dispatch(Intent::Navigate(Screen::Live));
        assert!(backend.playback().live.is_some());
        backend.dispatch(Intent::Navigate(Screen::Hardware));
        backend.dispatch(Intent::SelectNanoBase(true));
        assert!(backend.hardware.nano.is_some());
        assert!(!backend.hardware.can_continue());
        assert!(backend.hardware.joints.is_empty());
        assert!(backend.hardware.live_angles.is_empty());
        assert!(backend.playback().live.is_none());
        assert!(backend.hardware.robot_urdf.is_some());
        backend.dispatch(Intent::ConnectNanoBase { mock: false, port: String::new() });
        assert!(!backend.hardware.nano.as_ref().unwrap().active);
        assert!(backend.status().unwrap().contains("serial device"));
        backend.dispatch(Intent::SelectNanoBase(false));
        assert!(backend.hardware.nano.is_none());
    }

    #[test]
    fn hardware_gamepad_view_keeps_replay_and_setup_separate() {
        let mut backend = FileBackend::new(std::env::temp_dir().join("dorobot-gamepad-no-dataset"));
        backend.playback.current_time = 12.5;
        backend.live_preview = super::super::live::empty_preview();
        backend.live_preview.dataset_name = "Nano gamepad · test".into();
        backend.live_preview.current_time = 3.;
        backend.connection.phase = super::super::live::ConnectionPhase::Streaming;
        backend.dispatch(Intent::Navigate(Screen::Hardware));
        assert!(backend.playback().live.is_none());
        backend.dispatch(Intent::Navigate(Screen::Live));
        assert!(backend.playback().live.is_some());
        assert_eq!(backend.playback().current_time, 3.);
        backend.dispatch(Intent::Seek(5.));
        assert_eq!(backend.replay().current_time, 12.5);
        backend.dispatch(Intent::Navigate(Screen::Play));
        assert_eq!(backend.playback().current_time, 12.5);
        backend.dispatch(Intent::Navigate(Screen::Hardware));
        backend.dispatch(Intent::DisconnectLive);
        assert!(backend.playback().live.is_none());
        assert_eq!(backend.replay().current_time, 12.5);
    }

    #[test]
    fn live_mode_preserves_replay_position_and_never_applies_live_transport() {
        let mut backend = FileBackend::new("does/not/exist");
        backend.playback.dataset_name = "saved dataset".into();
        backend.playback.selected = Some(7);
        backend.playback.current_time = 2.5;
        backend.playback.stats.duration_s = 10.;
        backend.playback.stats.fps = 5.;
        backend.playback.is_playing = true;
        backend.dispatch(Intent::ConnectLive(format!("/tmp/dorobot-missing-{}.sock", std::process::id()).into()));
        assert_eq!(backend.screen(), Screen::Live);
        backend.dispatch(Intent::Seek(9.));
        backend.dispatch(Intent::TogglePlay);
        let saved = crate::preferences::Session::snapshot(&backend, true);
        assert_eq!(saved.time, 2.5);
        assert_eq!(saved.episode, Some(7));
        assert!(!backend.replay().is_playing);
        backend.dispatch(Intent::DisconnectLive);
        backend.dispatch(Intent::Navigate(Screen::Play));
        assert_eq!(backend.playback().dataset_name, "saved dataset");
        assert_eq!(backend.playback().current_time, 2.5);
        backend.dispatch(Intent::ConnectLive("https://not-a-local-stream".into()));
        assert_eq!(backend.connection.phase, super::super::live::ConnectionPhase::Error);
        assert!(backend.live_client.is_none());
    }

    #[test]
    fn missing_root_yields_an_empty_library() {
        let b = FileBackend::new("does/not/exist");
        assert!(b.library().datasets.is_empty());
        assert!(b.playback().episodes.is_empty());
    }
    #[test]
    fn bimanual_signals_keep_all_channels_and_follow_episode_selection() {
        use crate::data::lerobot_writer::{DatasetSpec, DatasetWriter, FrameRecord};
        let root = std::env::temp_dir().join(format!("dorobot-signals-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let names: Vec<_> = (0..17).map(|i| format!("joint_{i}")).collect();
        let mut writer = DatasetWriter::create(&root, DatasetSpec {
            robot_type: "adora_nano".into(), fps: 10.0,
            camera_keys: vec![], joint_names: names.clone(),
        }).unwrap();
        for episode in 0..2 {
            let mut ep = writer.begin_episode("both arms").unwrap();
            for frame in 0..3 {
                let mut state = vec![0.0; 17];
                let mut action = vec![0.0; 17];
                state[16] = (episode * 3 + frame) as f32 * 0.1;
                action[16] = state[16] + 0.01;
                ep.push(FrameRecord { state, action }).unwrap();
            }
            writer.commit(ep).unwrap();
        }
        drop(writer);
        let mut backend = FileBackend::new(&root);
        for episode in 0..2 {
            backend.dispatch(Intent::SelectEpisode(episode));
            let state = backend.playback();
            assert_eq!(state.state_series.len(), 17);
            assert_eq!(state.action_series.len(), 17);
            assert_eq!(state.state_series.iter().map(|c| c.name.clone()).collect::<Vec<_>>(), names);
            for frame in 0..3 {
                let expected = (episode * 3 + frame) as f32 * 0.1;
                assert_eq!(state.state_series[16].points[frame as usize].1, expected as f64);
                assert_eq!(state.action_series[16].points[frame as usize].1, (expected + 0.01) as f64);
            }
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn nexus_saved_dataset_replays_named_degrees_on_the_official_model() {
        use crate::data::lerobot_writer::{DatasetSpec, DatasetWriter, FrameRecord};
        let root = std::env::temp_dir().join(format!("nexus-saved-replay-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dataset = root.join("take");
        let names = ["elbow_flex.pos", "gripper.pos", "wrist_roll.pos", "shoulder_lift.pos", "shoulder_pan.pos", "wrist_flex.pos"];
        let mut writer = DatasetWriter::create(&dataset, DatasetSpec {
            robot_type: "so101_follower".into(), fps: 20.0, camera_keys: vec![],
            joint_names: names.map(String::from).to_vec(),
        }).unwrap();
        let mut episode = writer.begin_episode("place cube").unwrap();
        episode.push(FrameRecord {state: vec![5., 100., 12., -2., 30., -8.], action: vec![0.; 6]}).unwrap();
        writer.commit(episode).unwrap();
        drop(writer);
        fs::write(dataset.join("meta/dorobot_capture.json"), r#"{"source_mode":"synthetic","sample_hz_simulated":20}"#).unwrap();
        let mut backend = FileBackend::new(root.join("empty"));
        backend.nexus.state.snapshot.dataset = Some(dataset);
        backend.dispatch(Intent::Nexus(nexus::NexusIntent::OpenSavedDataset));
        assert_eq!(backend.screen(), Screen::Play);
        let (urdf, _) = backend.replay().robot_urdf.as_ref().unwrap();
        assert_eq!(urdf.file_name().unwrap(), "so101_new_calib.urdf");
        let model = urdf_rs::read_file(urdf).unwrap();
        let viewer_order: Vec<_> = model.joints.iter().filter(|j| j.joint_type == urdf_rs::JointType::Revolute).map(|j|j.name.as_str()).collect();
        let angles = &backend.replay().joint_frames[0];
        assert_eq!(angles.len(), viewer_order.len());
        for (name, angle) in [("shoulder_pan", 30.0_f32.to_radians()), ("gripper", 1.74533),
                              ("elbow_flex", 5.0_f32.to_radians()), ("wrist_flex", -8.0_f32.to_radians())] {
            let index = viewer_order.iter().position(|&joint| joint==name).unwrap();
            assert!((angles[index] - angle).abs() < 1e-5, "{name} angle");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dataset_root_tags_exclusion_undo_and_restart() {
        use crate::data::lerobot_writer::{DatasetSpec, DatasetWriter, FrameRecord};
        let root = std::env::temp_dir().join(format!("dorobot-curation-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let spec = DatasetSpec {
            robot_type: "so100".into(),
            fps: 30.0,
            camera_keys: vec![],
            joint_names: vec!["j1".into()],
        };
        let mut writer = DatasetWriter::create(&root, spec).unwrap();
        for _ in 0..2 {
            let mut ep = writer.begin_episode("pick").unwrap();
            ep.push(FrameRecord {
                state: vec![0.0],
                action: vec![1.0],
            })
            .unwrap();
            writer.commit(ep).unwrap();
        }
        drop(writer);
        let mut b = FileBackend::new(&root);
        assert_eq!(b.library.datasets.len(), 1);
        assert_eq!(b.playback.stats.frames, 1);
        b.dispatch(Intent::Nexus(nexus::NexusIntent::UseOpenDataset));
        assert_eq!(Path::new(&b.nexus.state.settings.dataset), root.canonicalize().unwrap());
        b.nexus.state.snapshot.dataset = Some(root.clone());
        b.dispatch(Intent::Nexus(nexus::NexusIntent::OpenSavedDataset));
        assert_eq!(b.screen(), Screen::Play);
        b.dispatch(Intent::TagEpisode {
            episode: 0,
            tag: Some(Tag::Good),
        });
        b.dispatch(Intent::DeleteEpisode(0));
        assert_eq!(b.playback.episodes.len(), 1);
        b.dispatch(Intent::TagEpisode {
            episode: 1,
            tag: Some(Tag::Bad),
        });
        b.dispatch(Intent::UndoExclude);
        assert_eq!(b.playback.episodes.len(), 2);
        let mut b = FileBackend::new(&root);
        assert_eq!(b.playback.episodes[0].tag, Some(Tag::Good));
        assert_eq!(b.playback.episodes[1].tag, Some(Tag::Bad));
        // A UI relaunch without the original --dataset argument reopens the
        // actual folder, even when it lives outside the default library.
        b.dispatch(Intent::SelectEpisode(1));
        b.dispatch(Intent::Navigate(Screen::Play));
        let saved = crate::preferences::Session::snapshot(&b, true);
        let mut reopened = FileBackend::new(root.join("missing-library"));
        saved.restore(&mut reopened, true);
        assert_eq!(reopened.playback.selected, Some(1));
        assert_eq!(reopened.playback.episodes[1].tag, Some(Tag::Bad));
        assert_eq!(reopened.screen, Screen::Play);
        assert!(!reopened.playback.is_playing);
        // An explicit empty/different dataset takes precedence over recovery.
        let mut explicit = FileBackend::new(root.join("different-library"));
        saved.restore(&mut explicit, false);
        assert!(explicit.playback.episodes.is_empty());
        // A failed sidecar publication must not show an unsaved tag.
        fs::create_dir(root.join("meta/dorobot_tags.json.tmp")).unwrap();
        b.dispatch(Intent::TagEpisode {
            episode: 0,
            tag: Some(Tag::Bad),
        });
        assert_eq!(b.playback.episodes[0].tag, Some(Tag::Good));
        assert!(b.status.contains("Could not save tags"));
        fs::remove_dir_all(root).unwrap();
    }
}
