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
