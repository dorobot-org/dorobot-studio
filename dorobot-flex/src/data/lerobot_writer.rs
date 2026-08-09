//! LeRobot dataset writer — the keystone the recording console needs.
//!
//! Writes the v2.x on-disk layout that [`crate::data::LeRobotDataset`] reads,
//! so a recorded episode can be reopened by the player without a conversion
//! step. Video encoding is deliberately *not* here: frames are handed to an
//! encoder node and only their path is recorded, which keeps this module
//! testable without FFmpeg or hardware.
//!
//! ```text
//! <root>/
//!   meta/info.json          fps, robot_type, total_* counters, features
//!   meta/episodes.jsonl     one line per episode: index, length, tasks
//!   meta/tasks.jsonl        task_index -> description
//!   data/chunk-000/episode_000000.parquet
//!   videos/chunk-000/<camera key>/episode_000000.mp4
//! ```
//!
//! ## Staging
//!
//! An episode is written to a staging directory first and only moved into the
//! dataset on [`EpisodeWriter::commit`]. That is what makes the console's
//! review-before-save step cheap: discarding is a directory delete, and a
//! crashed session can never leave a half-episode in the dataset.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::builder::{FixedSizeListBuilder, Float32Builder};
use arrow_array::{FixedSizeListArray, Float64Array, Int64Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;

use super::DatasetError;

/// Chunk size used for `chunk-NNN` directory bucketing.
const CHUNK_SIZE: u64 = 1000;

fn chunk_dir(episode_index: u64) -> String {
    format!("chunk-{:03}", episode_index / CHUNK_SIZE)
}

fn episode_stem(episode_index: u64) -> String {
    format!("episode_{:06}", episode_index)
}

/// One recorded frame. Mirrors [`crate::data::EpisodeFrame`] on the read side.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameRecord {
    pub state: Vec<f32>,
    pub action: Vec<f32>,
}

/// Describes the rig being recorded, used to build `meta/info.json`.
#[derive(Clone, Debug)]
pub struct DatasetSpec {
    pub robot_type: String,
    pub fps: f64,
    /// Camera keys in the order they appear, e.g. `observation.images.cam_high`.
    pub camera_keys: Vec<String>,
    /// Per-joint names, written into `features` so plots can label series
    /// instead of falling back to `state[0..n]`.
    pub joint_names: Vec<String>,
}

impl DatasetSpec {
    fn state_dim(&self) -> usize {
        self.joint_names.len()
    }
}

/// Creates and appends to a LeRobot dataset on disk.
pub struct DatasetWriter {
    root: PathBuf,
    spec: DatasetSpec,
    episodes: Vec<EpisodeEntry>,
    tasks: Vec<String>,
}

#[derive(Clone, Debug)]
struct EpisodeEntry {
    index: u64,
    length: u64,
    task_index: usize,
}

impl DatasetWriter {
    /// Create a new dataset. Fails if `root` already contains one, so a
    /// mistyped path can never silently merge into an existing dataset.
    pub fn create(root: impl AsRef<Path>, spec: DatasetSpec) -> Result<Self, DatasetError> {
        let root = root.as_ref().to_path_buf();
        if root.join("meta").join("info.json").exists() {
            return Err(DatasetError::InvalidFormat(format!(
                "a dataset already exists at {}",
                root.display()
            )));
        }
        fs::create_dir_all(root.join("meta"))?;
        let w = Self { root, spec, episodes: Vec::new(), tasks: Vec::new() };
        w.write_meta()?;
        Ok(w)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Register a task description, returning its index. Repeated descriptions
    /// map to the same index, matching LeRobot's task table semantics.
    pub fn task_index(&mut self, description: &str) -> usize {
        if let Some(i) = self.tasks.iter().position(|t| t == description) {
            return i;
        }
        self.tasks.push(description.to_string());
        self.tasks.len() - 1
    }

    /// Begin an episode. Frames are buffered in a staging directory until
    /// [`EpisodeWriter::commit`].
    pub fn begin_episode(&mut self, task: &str) -> Result<EpisodeWriter, DatasetError> {
        let index = self.next_episode_index();
        let task_index = self.task_index(task);
        let staging = self.root.join(".staging").join(episode_stem(index));
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;
        Ok(EpisodeWriter {
            index,
            task_index,
            staging,
            frames: Vec::new(),
            videos: BTreeMap::new(),
            state_dim: self.spec.state_dim(),
        })
    }

    fn next_episode_index(&self) -> u64 {
        self.episodes.iter().map(|e| e.index + 1).max().unwrap_or(0)
    }

    /// Move a staged episode into the dataset and refresh the metadata.
    pub fn commit(&mut self, ep: EpisodeWriter) -> Result<u64, DatasetError> {
        if ep.frames.is_empty() {
            return Err(DatasetError::InvalidFormat(
                "refusing to commit an episode with no frames".into(),
            ));
        }
        let index = ep.index;

        // 1. frames -> data/chunk-NNN/episode_NNNNNN.parquet
        let data_dir = self.root.join("data").join(chunk_dir(index));
        fs::create_dir_all(&data_dir)?;
        let parquet_path = data_dir.join(format!("{}.parquet", episode_stem(index)));
        write_episode_parquet(&parquet_path, &ep, self.spec.fps)?;

        // 2. staged video files -> videos/chunk-NNN/<key>/episode_NNNNNN.mp4
        for (key, staged) in &ep.videos {
            let dst_dir = self.root.join("videos").join(chunk_dir(index)).join(key);
            fs::create_dir_all(&dst_dir)?;
            let dst = dst_dir.join(format!("{}.mp4", episode_stem(index)));
            // Rename first; fall back to copy when staging is on another device.
            if fs::rename(staged, &dst).is_err() {
                fs::copy(staged, &dst)?;
                let _ = fs::remove_file(staged);
            }
        }

        self.episodes.push(EpisodeEntry {
            index,
            length: ep.frames.len() as u64,
            task_index: ep.task_index,
        });
        self.write_meta()?;

        let _ = fs::remove_dir_all(&ep.staging);
        Ok(index)
    }

    /// Throw away a staged episode.
    pub fn discard(&self, ep: EpisodeWriter) -> Result<(), DatasetError> {
        if ep.staging.exists() {
            fs::remove_dir_all(&ep.staging)?;
        }
        Ok(())
    }

    /// Rewrite `meta/`. Called after every commit so an interrupted session
    /// still leaves a readable dataset.
    fn write_meta(&self) -> Result<(), DatasetError> {
        let meta = self.root.join("meta");
        fs::create_dir_all(&meta)?;

        let total_frames: u64 = self.episodes.iter().map(|e| e.length).sum();
        let dim = self.spec.state_dim();
        let names = serde_json::to_string(&self.spec.joint_names).unwrap_or_else(|_| "[]".into());

        let mut features = String::new();
        features.push_str(&format!(
            r#""observation.state":{{"dtype":"float32","shape":[{dim}],"names":{names}}},"#
        ));
        features.push_str(&format!(
            r#""action":{{"dtype":"float32","shape":[{dim}],"names":{names}}}"#
        ));
        for key in &self.spec.camera_keys {
            features.push_str(&format!(
                r#","{key}":{{"dtype":"video","shape":[480,640,3],"names":["height","width","channel"]}}"#
            ));
        }

        let info = format!(
            r#"{{
  "codebase_version": "v2.1",
  "robot_type": "{robot}",
  "fps": {fps},
  "total_episodes": {eps},
  "total_frames": {frames},
  "total_tasks": {tasks},
  "total_videos": {videos},
  "chunks_size": {chunks},
  "features": {{{features}}}
}}
"#,
            robot = self.spec.robot_type,
            fps = self.spec.fps,
            eps = self.episodes.len(),
            frames = total_frames,
            tasks = self.tasks.len(),
            videos = self.episodes.len() * self.spec.camera_keys.len(),
            chunks = CHUNK_SIZE,
            features = features,
        );
        fs::write(meta.join("info.json"), info)?;

        let mut episodes_jsonl = File::create(meta.join("episodes.jsonl"))?;
        for e in &self.episodes {
            let task = self.tasks.get(e.task_index).map(String::as_str).unwrap_or("");
            writeln!(
                episodes_jsonl,
                r#"{{"episode_index": {}, "tasks": [{}], "length": {}, "task": {}}}"#,
                e.index,
                e.task_index,
                e.length,
                serde_json::to_string(task).unwrap_or_else(|_| "\"\"".into()),
            )?;
        }

        let mut tasks_jsonl = File::create(meta.join("tasks.jsonl"))?;
        for (i, t) in self.tasks.iter().enumerate() {
            writeln!(
                tasks_jsonl,
                r#"{{"task_index": {}, "task": {}}}"#,
                i,
                serde_json::to_string(t).unwrap_or_else(|_| "\"\"".into())
            )?;
        }
        Ok(())
    }
}

/// A single episode being recorded into staging.
pub struct EpisodeWriter {
    index: u64,
    task_index: usize,
    staging: PathBuf,
    frames: Vec<FrameRecord>,
    videos: BTreeMap<String, PathBuf>,
    state_dim: usize,
}

impl EpisodeWriter {
    pub fn index(&self) -> u64 {
        self.index
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Where an encoder node should write this episode's video for `key`.
    pub fn video_staging_path(&mut self, key: &str) -> PathBuf {
        let p = self.staging.join(format!("{}.mp4", key.replace('/', "_")));
        self.videos.insert(key.to_string(), p.clone());
        p
    }

    /// Append one frame. Dimension mismatches are rejected at push time rather
    /// than producing a dataset that fails to load later.
    pub fn push(&mut self, frame: FrameRecord) -> Result<(), DatasetError> {
        if frame.state.len() != self.state_dim || frame.action.len() != self.state_dim {
            return Err(DatasetError::InvalidFormat(format!(
                "frame {} has state {} / action {}, expected {}",
                self.frames.len(),
                frame.state.len(),
                frame.action.len(),
                self.state_dim
            )));
        }
        self.frames.push(frame);
        Ok(())
    }

    /// Integrity findings surfaced to the operator before saving.
    pub fn warnings(&self, expected_cameras: usize) -> Vec<String> {
        let mut out = Vec::new();
        if self.frames.is_empty() {
            out.push("no frames captured".into());
        }
        if self.videos.len() < expected_cameras {
            out.push(format!(
                "{} of {} cameras produced video",
                self.videos.len(),
                expected_cameras
            ));
        }
        for (key, path) in &self.videos {
            if !path.exists() {
                out.push(format!("{key}: encoder produced no file"));
            }
        }
        out
    }
}

/// Write the frame table.
///
/// Types are dictated by the reader, not by taste: it downcasts `frame_index`
/// to `Int64` and the vector columns to `FixedSizeList<Float32>`. Writing
/// `ListArray`/`UInt64` here produces a file that opens but yields empty
/// states — which is precisely what the round-trip test caught.
fn write_episode_parquet(
    path: &Path,
    ep: &EpisodeWriter,
    fps: f64,
) -> Result<(), DatasetError> {
    let n = ep.frames.len();
    let dim = ep.state_dim as i32;

    let vec_field = |name: &str| {
        Field::new(
            name,
            DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), dim),
            false,
        )
    };
    let schema = Arc::new(Schema::new(vec![
        Field::new("frame_index", DataType::Int64, false),
        Field::new("timestamp", DataType::Float64, false),
        Field::new("episode_index", DataType::Int64, false),
        vec_field("observation.state"),
        vec_field("action"),
    ]));

    let frame_index = Int64Array::from((0..n as i64).collect::<Vec<_>>());
    let timestamp =
        Float64Array::from((0..n).map(|i| i as f64 / fps.max(1e-6)).collect::<Vec<_>>());
    let episode_index = Int64Array::from(vec![ep.index as i64; n]);

    let mut state_b = FixedSizeListBuilder::new(Float32Builder::with_capacity(n * dim as usize), dim);
    let mut action_b = FixedSizeListBuilder::new(Float32Builder::with_capacity(n * dim as usize), dim);
    for f in &ep.frames {
        state_b.values().append_slice(&f.state);
        state_b.append(true);
        action_b.values().append_slice(&f.action);
        action_b.append(true);
    }
    let state: FixedSizeListArray = state_b.finish();
    let action: FixedSizeListArray = action_b.finish();

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(frame_index),
            Arc::new(timestamp),
            Arc::new(episode_index),
            Arc::new(state),
            Arc::new(action),
        ],
    )
    .map_err(|e| DatasetError::InvalidFormat(format!("building record batch: {e}")))?;

    let file = File::create(path)?;
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))
        .map_err(|e| DatasetError::InvalidFormat(format!("opening parquet writer: {e}")))?;
    writer
        .write(&batch)
        .map_err(|e| DatasetError::InvalidFormat(format!("writing parquet: {e}")))?;
    writer
        .close()
        .map_err(|e| DatasetError::InvalidFormat(format!("closing parquet: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::LeRobotDataset;

    fn spec() -> DatasetSpec {
        DatasetSpec {
            robot_type: "so101".into(),
            fps: 30.0,
            camera_keys: vec!["observation.images.cam_high".into()],
            joint_names: vec![
                "shoulder_pan".into(),
                "shoulder_lift".into(),
                "elbow_flex".into(),
            ],
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("dorobot-writer-{name}"));
        let _ = fs::remove_dir_all(&p);
        p
    }

    /// The whole point of the writer: what it writes, the shipping loader reads.
    #[test]
    fn round_trips_through_the_reader() {
        let root = tmp("roundtrip");
        let mut w = DatasetWriter::create(&root, spec()).unwrap();

        let mut ep = w.begin_episode("Pick the block").unwrap();
        for i in 0..48 {
            let t = i as f32 * 0.1;
            ep.push(FrameRecord {
                state: vec![t.sin(), t.cos(), t * 0.01],
                action: vec![t.sin() + 0.05, t.cos() - 0.02, t * 0.011],
            })
            .unwrap();
        }
        let idx = w.commit(ep).unwrap();
        assert_eq!(idx, 0);

        let ds = LeRobotDataset::open(&root).expect("writer output must be loadable");
        assert_eq!(ds.num_episodes(), 1);
        assert_eq!(ds.info.robot_type, "so101");
        assert_eq!(ds.info.fps, 30.0);
        assert_eq!(ds.get_task(0), Some("Pick the block"));

        let data = ds.load_episode(0).expect("episode must load");
        assert_eq!(data.frames.len(), 48);
        let f0 = &data.frames[0];
        assert_eq!(f0.frame_index, 0);
        assert_eq!(f0.state.len(), 3);
        assert_eq!(f0.action.len(), 3);
        // timestamps derive from fps, so drift is zero for a clean write
        let last = data.frames.last().unwrap();
        assert!((last.timestamp - 47.0 / 30.0).abs() < 1e-6, "ts {}", last.timestamp);
    }

    #[test]
    fn appends_multiple_episodes_and_reuses_task_indices() {
        let root = tmp("append");
        let mut w = DatasetWriter::create(&root, spec()).unwrap();
        for _ in 0..3 {
            let mut ep = w.begin_episode("same task").unwrap();
            for _ in 0..5 {
                ep.push(FrameRecord { state: vec![0.0; 3], action: vec![0.0; 3] }).unwrap();
            }
            w.commit(ep).unwrap();
        }
        let ds = LeRobotDataset::open(&root).unwrap();
        assert_eq!(ds.num_episodes(), 3);
        assert_eq!(ds.info.total_frames, 15);
        // one description reused across episodes -> a single task entry
        assert_eq!(ds.get_task(0), Some("same task"));
        assert_eq!(ds.get_task(1), None);
        assert_eq!(ds.load_episode(2).unwrap().frames.len(), 5);
    }

    #[test]
    fn discard_leaves_no_trace() {
        let root = tmp("discard");
        let mut w = DatasetWriter::create(&root, spec()).unwrap();
        let mut ep = w.begin_episode("throwaway").unwrap();
        ep.push(FrameRecord { state: vec![1.0; 3], action: vec![1.0; 3] }).unwrap();
        let staging = ep.staging.clone();
        w.discard(ep).unwrap();
        assert!(!staging.exists());
        assert!(!root.join("data").exists(), "discard must not touch the dataset");
    }

    #[test]
    fn rejects_bad_frames_and_empty_episodes() {
        let root = tmp("reject");
        let mut w = DatasetWriter::create(&root, spec()).unwrap();
        let mut ep = w.begin_episode("t").unwrap();
        assert!(ep.push(FrameRecord { state: vec![0.0; 2], action: vec![0.0; 3] }).is_err());
        assert!(w.commit(ep).is_err(), "empty episode must not commit");
    }

    #[test]
    fn reports_missing_camera_video() {
        let root = tmp("warn");
        let mut w = DatasetWriter::create(&root, spec()).unwrap();
        let mut ep = w.begin_episode("t").unwrap();
        ep.push(FrameRecord { state: vec![0.0; 3], action: vec![0.0; 3] }).unwrap();
        let warnings = ep.warnings(1);
        assert!(warnings.iter().any(|w| w.contains("0 of 1 cameras")), "{warnings:?}");
    }
}
