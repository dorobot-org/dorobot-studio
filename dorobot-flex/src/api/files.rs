//! Backend backed by LeRobot datasets on disk.
//!
//! Replaces [`super::mock::MockBackend`] for the Library and Play screens so
//! they can be validated against real data. Hardware, Record and Eval still
//! report empty state: those need a robot, not a file.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::data::LeRobotDataset;

use super::*;

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
    let assets = PathBuf::from("data").join(&dir);
    let urdf = assets.join(format!("{dir}.urdf"));
    urdf.exists().then_some((urdf, assets))
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
                let v = if FLIP.get(i).copied().unwrap_or(false) { -v } else { *v };
                // The gripper is the one channel LeRobot normalizes 0..100.
                let t = if i == 5 { v / 100.0 } else { (v + 100.0) / 200.0 };
                lo + t * (hi - lo)
            }
        })
        .collect()
}

/// Movable joint limits, in URDF order, read from the model the viewer loads.
fn urdf_limits(path: &Path) -> Vec<(f32, f32)> {
    let Ok(text) = fs::read_to_string(path) else { return Vec::new() };
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

/// Scan `root` one level deep for anything that looks like a LeRobot dataset.
fn discover(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root) else { return found };
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
        let Ok(entries) = fs::read_dir(&dir) else { continue };
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
    root: PathBuf,
    screen: Screen,
    library: LibraryState,
    playback: PlaybackState,
    hardware: HardwareState,
    record: RecordState,
    eval: EvalState,
    /// Currently opened dataset, kept so episode selection avoids a reparse.
    open: Option<(String, LeRobotDataset)>,
}

impl FileBackend {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self::with_open(root, None)
    }

    /// `prefer` names the dataset to open on start; falls back to the first
    /// one found.
    pub fn with_open(root: impl AsRef<Path>, prefer: Option<&str>) -> Self {
        let mut b = Self {
            root: root.as_ref().to_path_buf(),
            screen: Screen::Library,
            library: LibraryState::default(),
            playback: PlaybackState::default(),
            hardware: HardwareState::default(),
            record: RecordState::default(),
            eval: EvalState::default(),
            open: None,
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
            let Ok(ds) = LeRobotDataset::open(&path) else { continue };
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
                // Curation tags are not persisted yet, so quality is unknown
                // rather than invented.
                good: 0,
                bad: 0,
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
        let Ok(ds) = LeRobotDataset::open(&summary.path) else { return };

        let fps = ds.info.fps.max(1e-6);
        let episodes: Vec<EpisodeEntry> = ds
            .episodes
            .iter()
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

        self.playback = PlaybackState {
            dataset_name: summary.name.clone(),
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
            robot_urdf: urdf_for(&ds.info.robot_type),
            joint_frames: Vec::new(),
        };
        self.open = Some((id.to_string(), ds));
        if let Some(idx) = self.playback.selected {
            self.select_episode(idx);
        }
    }

    fn select_episode(&mut self, index: u64) {
        let Some((_, ds)) = &self.open else { return };
        let Ok(data) = ds.load_episode(index) else { return };
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

        // Transpose the frames into channel-major series for the plot. Capped
        // because a legible plot is the point: six traces already crowd the
        // pane, and SO-100 has exactly six joints.
        let series = |pick: fn(&crate::data::lerobot_dataset::EpisodeFrame) -> &Vec<f32>,
                      names: &[String]| {
            let width = data.frames.first().map(|f| pick(f).len()).unwrap_or(0).min(6);
            (0..width)
                .map(|c| PlotChannel {
                    name: names
                        .get(c)
                        .cloned()
                        .unwrap_or_else(|| format!("ch {c}")),
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

        self.playback.video_paths = data.video_paths.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        self.playback.video_frame_offset = data.video_frame_offset;
        let robot = ds.info.robot_type.to_lowercase();
        let limits = self
            .playback
            .robot_urdf
            .as_ref()
            .map(|(u, _)| urdf_limits(u))
            .unwrap_or_default();
        self.playback.joint_frames = data
            .frames
            .iter()
            .map(|f| to_urdf_pose(&robot, &f.state, &limits))
            .collect();
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
        &self.playback
    }
    fn eval(&self) -> &EvalState {
        &self.eval
    }

    fn dispatch(&mut self, intent: Intent) {
        match intent {
            Intent::Navigate(s) => self.screen = s,
            Intent::RescanDatasets => self.rescan(),
            Intent::OpenDataset(id) => {
                self.open_dataset(&id);
                self.screen = Screen::Play;
            }
            Intent::SelectEpisode(i) => self.select_episode(i),
            Intent::TogglePlay => {
                // Restart from the head when play is pressed at the end,
                // rather than sitting there doing nothing.
                if !self.playback.is_playing
                    && self.playback.current_time >= self.playback.stats.duration_s - 1e-6
                {
                    self.playback.current_time = 0.0;
                }
                self.playback.is_playing = !self.playback.is_playing;
            }
            Intent::Seek(t) => {
                self.playback.current_time = t.clamp(0.0, self.playback.stats.duration_s);
            }
            Intent::StepFrames(n) => {
                let fps = self.playback.stats.fps.max(1e-6);
                let t = self.playback.current_time + n as f64 / fps;
                self.playback.current_time = t.clamp(0.0, self.playback.stats.duration_s);
                self.playback.is_playing = false;
            }
            Intent::SetSpeed(s) => self.playback.speed = s.clamp(0.05, 8.0),
            Intent::TagEpisode { episode, tag } => {
                if let Some(e) = self.playback.episodes.iter_mut().find(|e| e.index == episode) {
                    e.tag = tag;
                }
            }
            // Recording, hardware and Hub actions need a backend that does not
            // exist yet; ignoring them is better than pretending they worked.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_root_yields_an_empty_library() {
        let b = FileBackend::new("does/not/exist");
        assert!(b.library().datasets.is_empty());
        assert!(b.playback().episodes.is_empty());
    }
}
