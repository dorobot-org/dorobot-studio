//! LeRobot Dataset Loader
//!
//! Parses LeRobot v2.0 and v3.0 dataset formats.

use std::collections::HashMap;

/// Batch size for Arrow parquet reader (rows per batch)
const ARROW_BATCH_SIZE: usize = 8192;

/// Maximum number of chunk directories to search
const MAX_CHUNK_SEARCH: u32 = 100;

/// Maximum number of files to search within a chunk
const MAX_FILE_SEARCH: u32 = 100;

/// Maximum number of video files to search within a camera directory
const MAX_VIDEO_FILE_SEARCH: u32 = 10;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::BufReader;

use serde::{Deserialize, Serialize};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::RowAccessor;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use arrow_array::{Array, Int64Array, Float64Array, Float32Array, FixedSizeListArray, ListArray, RecordBatchReader};

/// Read a float vector cell.
///
/// v2.0 files store `observation.state` / `action` as `FixedSizeList<float>`,
/// v3.0 files as `List<float>`. Accepting only one silently yields empty
/// states — the dataset loads, the frame count looks right, and every plot is
/// blank. Accept both.
fn float_vec_at(col: &dyn Array, row: usize) -> Vec<f32> {
    if let Some(a) = col.as_any().downcast_ref::<FixedSizeListArray>() {
        return a
            .value(row)
            .as_any()
            .downcast_ref::<Float32Array>()
            .map(|v| v.values().to_vec())
            .unwrap_or_default();
    }
    if let Some(a) = col.as_any().downcast_ref::<ListArray>() {
        return a
            .value(row)
            .as_any()
            .downcast_ref::<Float32Array>()
            .map(|v| v.values().to_vec())
            .unwrap_or_default();
    }
    Vec::new()
}

/// Timestamps are f64 in v2.0 files and f32 in v3.0 files.
fn f64_at(col: &dyn Array, row: usize) -> Option<f64> {
    if let Some(a) = col.as_any().downcast_ref::<Float64Array>() {
        return Some(a.value(row));
    }
    if let Some(a) = col.as_any().downcast_ref::<Float32Array>() {
        return Some(a.value(row) as f64);
    }
    None
}
use arrow_schema::ArrowError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatasetError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),
    #[error("Arrow error: {0}")]
    Arrow(#[from] ArrowError),
    #[error("Missing file: {0}")]
    MissingFile(String),
    #[error("Invalid dataset format: {0}")]
    InvalidFormat(String),
}

/// Dataset info from meta/info.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetInfo {
    #[serde(default)]
    pub codebase_version: String,
    #[serde(default)]
    pub robot_type: String,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub total_episodes: u64,
    #[serde(default)]
    pub total_frames: u64,
    #[serde(default)]
    pub features: HashMap<String, FeatureInfo>,
    #[serde(default)]
    pub video: bool,
    #[serde(default)]
    pub chunks_size: u64,
}

impl Default for DatasetInfo {
    fn default() -> Self {
        Self {
            codebase_version: "2.0".to_string(),
            robot_type: "unknown".to_string(),
            fps: 30.0,
            total_episodes: 0,
            total_frames: 0,
            features: HashMap::new(),
            video: false,
            chunks_size: 1000,
        }
    }
}

/// Feature info from info.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureInfo {
    #[serde(default)]
    pub dtype: String,
    #[serde(default)]
    pub shape: Vec<u64>,
    /// Names can be either an array of strings, an object with named arrays, or null
    #[serde(default, deserialize_with = "deserialize_names")]
    pub names: Option<Vec<String>>,
}

/// Deserialize names which can be:
/// - null -> None
/// - ["name1", "name2"] -> Some(vec!["name1", "name2"])
/// - { "motors": ["name1", "name2"] } -> Some(vec!["name1", "name2"])
fn deserialize_names<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor, MapAccess, SeqAccess};

    struct NamesVisitor;

    impl<'de> Visitor<'de> for NamesVisitor {
        type Value = Option<Vec<String>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("null, array of strings, or object containing array of strings")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut names = Vec::new();
            while let Some(name) = seq.next_element::<String>()? {
                names.push(name);
            }
            Ok(Some(names))
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            // Take the first array value found in the map
            while let Some((_, value)) = map.next_entry::<String, Vec<String>>()? {
                return Ok(Some(value));
            }
            Ok(None)
        }
    }

    deserializer.deserialize_any(NamesVisitor)
}

/// Task info from meta/tasks.jsonl
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub task_index: u64,
    pub task: String,
}

/// Episode metadata from meta/episodes.parquet
#[derive(Debug, Clone)]
pub struct EpisodeMetadata {
    pub episode_index: u64,
    pub tasks: Vec<u64>,
    pub length: u64,
}

/// Episode data frame
#[derive(Debug, Clone)]
pub struct EpisodeFrame {
    pub frame_index: u64,
    pub timestamp: f64,
    pub state: Vec<f32>,
    pub action: Vec<f32>,
}

/// Loaded episode data
#[derive(Debug, Clone)]
pub struct EpisodeData {
    pub episode_index: u64,
    pub frames: Vec<EpisodeFrame>,
    pub video_paths: HashMap<String, PathBuf>,
    /// Starting frame index in the video file (for v3.0 concatenated videos)
    pub video_frame_offset: u64,
}

/// LeRobot Dataset
pub struct LeRobotDataset {
    pub root_path: PathBuf,
    pub info: DatasetInfo,
    pub tasks: Vec<TaskInfo>,
    pub episodes: Vec<EpisodeMetadata>,
}

impl LeRobotDataset {
    /// Open a LeRobot dataset from a directory
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DatasetError> {
        let root_path = path.as_ref().to_path_buf();

        // Load info.json first (needed for v3.0 episode loading)
        let info = Self::load_info(&root_path)?;

        // Load tasks (supports both v2.0 and v3.0)
        let tasks = Self::load_tasks(&root_path)?;

        // Load episodes (supports both v2.0 and v3.0)
        let episodes = Self::load_episode_metadata(&root_path, &info)?;

        Ok(Self {
            root_path,
            info,
            tasks,
            episodes,
        })
    }

    /// Load meta/info.json
    fn load_info(root: &Path) -> Result<DatasetInfo, DatasetError> {
        let info_path = root.join("meta").join("info.json");
        if !info_path.exists() {
            return Err(DatasetError::MissingFile("meta/info.json".to_string()));
        }

        let file = File::open(&info_path)?;
        let reader = BufReader::new(file);
        let info: DatasetInfo = serde_json::from_reader(reader)?;
        Ok(info)
    }

    /// Load tasks - supports both v2.0 (tasks.jsonl) and v3.0 (tasks.parquet)
    fn load_tasks(root: &Path) -> Result<Vec<TaskInfo>, DatasetError> {
        // Try v2.0 format first (tasks.jsonl)
        let jsonl_path = root.join("meta").join("tasks.jsonl");
        if jsonl_path.exists() {
            let file = File::open(&jsonl_path)?;
            let reader = BufReader::new(file);
            let mut tasks = Vec::new();

            for line in std::io::BufRead::lines(reader) {
                let line = line?;
                if !line.trim().is_empty() {
                    let task: TaskInfo = serde_json::from_str(&line)?;
                    tasks.push(task);
                }
            }
            return Ok(tasks);
        }

        // Try v3.0 format (tasks.parquet)
        let parquet_path = root.join("meta").join("tasks.parquet");
        if parquet_path.exists() {
            let file = File::open(&parquet_path)?;
            let reader = SerializedFileReader::new(file)?;
            let mut tasks = Vec::new();

            let mut iter = reader.get_row_iter(None)?;
            while let Some(record) = iter.next() {
                let record = record?;

                // task_index is typically first column
                let task_index = match record.get_long(0) {
                    Ok(v) => v as u64,
                    _ => match record.get_int(0) {
                        Ok(v) => v as u64,
                        _ => continue,
                    }
                };

                // task description is typically second column
                let task = match record.get_string(1) {
                    Ok(v) => v.to_string(),
                    _ => format!("Task {}", task_index),
                };

                tasks.push(TaskInfo { task_index, task });
            }
            return Ok(tasks);
        }

        // Tasks are optional
        Ok(vec![])
    }

    /// Load episode metadata - supports both v2.0 and v3.0 formats
    fn load_episode_metadata(root: &Path, info: &DatasetInfo) -> Result<Vec<EpisodeMetadata>, DatasetError> {
        // Try v2.0 format first (meta/episodes.parquet)
        let episodes_path = root.join("meta").join("episodes.parquet");
        if episodes_path.exists() {
            return Self::load_episodes_v2(&episodes_path);
        }

        // Try v3.0 format (meta/episodes/chunk-*/file-*.parquet)
        let episodes_dir = root.join("meta").join("episodes");
        if episodes_dir.exists() {
            return Self::load_episodes_v3(&episodes_dir, info);
        }

        // Fallback: generate episode metadata from info.json
        let total_episodes = info.total_episodes;
        let total_frames = info.total_frames;
        let frames_per_episode = if total_episodes > 0 {
            total_frames / total_episodes
        } else {
            0
        };

        let episodes: Vec<EpisodeMetadata> = (0..total_episodes)
            .map(|i| EpisodeMetadata {
                episode_index: i,
                tasks: vec![0],
                length: frames_per_episode,
            })
            .collect();

        Ok(episodes)
    }

    /// Load episodes from v2.0 format (single parquet file)
    fn load_episodes_v2(path: &Path) -> Result<Vec<EpisodeMetadata>, DatasetError> {
        let file = File::open(path)?;
        let reader = SerializedFileReader::new(file)?;
        let mut episodes = Vec::new();

        let mut iter = reader.get_row_iter(None)?;
        while let Some(record) = iter.next() {
            let record = record?;

            let episode_index = match record.get_long(0) {
                Ok(v) => v as u64,
                _ => match record.get_int(0) {
                    Ok(v) => v as u64,
                    _ => continue,
                }
            };

            let length = match record.get_long(2) {
                Ok(v) => v as u64,
                _ => match record.get_int(2) {
                    Ok(v) => v as u64,
                    _ => 0,
                }
            };

            episodes.push(EpisodeMetadata {
                episode_index,
                tasks: vec![],
                length,
            });
        }

        Ok(episodes)
    }

    /// Load episodes from v3.0 format (chunked parquet files)
    fn load_episodes_v3(episodes_dir: &Path, info: &DatasetInfo) -> Result<Vec<EpisodeMetadata>, DatasetError> {
        let mut episodes = Vec::new();

        // Iterate through chunk directories
        for chunk_idx in 0..MAX_CHUNK_SEARCH {
            let chunk_dir = episodes_dir.join(format!("chunk-{:03}", chunk_idx));
            if !chunk_dir.exists() {
                break;
            }

            // Iterate through files in chunk
            for file_idx in 0..MAX_FILE_SEARCH {
                let file_path = chunk_dir.join(format!("file-{:03}.parquet", file_idx));
                if !file_path.exists() {
                    break;
                }

                let file = File::open(&file_path)?;
                let reader = SerializedFileReader::new(file)?;

                // Get schema to find column indices by name
                let schema = reader.metadata().file_metadata().schema();
                let episode_idx_col = Self::find_column_index(schema, "episode_index");
                let length_col = Self::find_column_index(schema, "length");

                let mut iter = reader.get_row_iter(None)?;
                while let Some(record) = iter.next() {
                    let record = record?;

                    // Get episode_index
                    let episode_index = episode_idx_col
                        .and_then(|col| Self::extract_int(&record, col))
                        .unwrap_or(0) as u64;

                    // Get length
                    let length = length_col
                        .and_then(|col| Self::extract_int(&record, col))
                        .unwrap_or((info.total_frames / info.total_episodes.max(1)) as i64) as u64;

                    episodes.push(EpisodeMetadata {
                        episode_index,
                        tasks: vec![0],
                        length,
                    });
                }
            }
        }

        // Sort by episode index
        episodes.sort_by_key(|e| e.episode_index);
        Ok(episodes)
    }

    /// Get task description by index
    pub fn get_task(&self, task_index: u64) -> Option<&str> {
        self.tasks
            .iter()
            .find(|t| t.task_index == task_index)
            .map(|t| t.task.as_str())
    }

    /// Calculate the cumulative frame offset for an episode in the video file.
    ///
    /// In LeRobot v3.0, all episodes are concatenated in a single MP4 file.
    /// This returns the starting frame index of the given episode.
    /// Check if dataset uses per-episode video files (v2.0) vs concatenated (v3.0)
    pub fn has_per_episode_videos(&self) -> bool {
        // v2.0 uses data_path pattern with episode_index
        // v3.0 uses file-XXX.mp4 pattern
        self.info.codebase_version.starts_with("v2")
    }

    pub fn calculate_video_frame_offset(&self, episode_index: u64) -> u64 {
        // For v2.0 datasets with per-episode video files, offset is always 0
        if self.has_per_episode_videos() {
            return 0;
        }
        // For v3.0 concatenated videos, calculate cumulative offset
        self.episodes
            .iter()
            .filter(|ep| ep.episode_index < episode_index)
            .map(|ep| ep.length)
            .sum()
    }

    /// Load episode data from parquet files using Arrow batch reader for performance
    pub fn load_episode(&self, episode_index: u64) -> Result<EpisodeData, DatasetError> {
        use std::time::Instant;

        let t_start = Instant::now();

        // Calculate which chunk this episode is in
        let chunk_index = episode_index / self.info.chunks_size.max(1);
        let chunk_dir = format!("chunk-{:03}", chunk_index);

        // Find the data file
        let data_path = self.find_episode_file(&chunk_dir, episode_index)?;
        let is_v3_format = data_path.file_name()
            .map(|n| n.to_string_lossy().starts_with("file-"))
            .unwrap_or(false);

        let t_find = t_start.elapsed();

        // Open file with Arrow reader (no projection for simplicity - columns are small)
        let file = File::open(&data_path)?;
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)?
            .with_batch_size(ARROW_BATCH_SIZE)
            .build()?;

        let t_open = t_start.elapsed();

        // Get the schema from the reader
        let schema = reader.schema();

        // Find column indices by name in the batch schema
        let ep_col_idx = schema.fields().iter().position(|f| f.name() == "episode_index");
        let frame_col_idx = schema.fields().iter().position(|f| f.name() == "frame_index");
        let ts_col_idx = schema.fields().iter().position(|f| f.name() == "timestamp");
        let state_col_idx = schema.fields().iter().position(|f| f.name() == "observation.state");
        let action_col_idx = schema.fields().iter().position(|f| f.name() == "action");

        let t_schema = t_start.elapsed();

        let mut frames = Vec::new();
        let mut rows_processed = 0u64;

        for batch_result in reader {
            let batch = batch_result?;
            let num_rows = batch.num_rows();
            rows_processed += num_rows as u64;

            // Get episode column for filtering (v3.0 format)
            let episode_filter: Option<Vec<bool>> = if is_v3_format {
                ep_col_idx.and_then(|idx| {
                    batch.column(idx).as_any()
                        .downcast_ref::<Int64Array>()
                        .map(|arr| {
                            (0..num_rows).map(|i| arr.value(i) as u64 == episode_index).collect()
                        })
                })
            } else {
                None
            };

            // Extract columns
            let frame_arr = frame_col_idx.and_then(|idx| {
                batch.column(idx).as_any().downcast_ref::<Int64Array>()
            });

            // Keep the columns as `&dyn Array`: their concrete Arrow type
            // differs between v2.0 and v3.0 files.
            let ts_arr = ts_col_idx.map(|idx| batch.column(idx).as_ref());
            let state_arr = state_col_idx.map(|idx| batch.column(idx).as_ref());
            let action_arr = action_col_idx.map(|idx| batch.column(idx).as_ref());

            for row in 0..num_rows {
                // Filter by episode for v3.0 format
                if let Some(ref filter) = episode_filter {
                    if !filter[row] {
                        continue;
                    }
                }

                let frame_index = frame_arr.map(|a| a.value(row) as u64).unwrap_or(frames.len() as u64);
                let timestamp = ts_arr
                    .and_then(|a| f64_at(a, row))
                    .unwrap_or(frame_index as f64 / self.info.fps);

                let state = state_arr.map(|a| float_vec_at(a, row)).unwrap_or_default();
                let action = action_arr.map(|a| float_vec_at(a, row)).unwrap_or_default();

                frames.push(EpisodeFrame {
                    frame_index,
                    timestamp,
                    state,
                    action,
                });
            }
        }

        let t_iterate = t_start.elapsed();

        // Sort frames by frame_index
        frames.sort_by_key(|f| f.frame_index);

        // Calculate video frame offset
        let video_frame_offset = self.calculate_video_frame_offset(episode_index);

        // Find video paths
        let video_paths = self.find_video_paths(&chunk_dir, episode_index);

        let t_total = t_start.elapsed();

        // Timing log
        ::log::debug!("[Dataset] episode {} timing: find={:?}, open={:?}, schema={:?}, iterate={:?} (rows={}), total={:?}",
            episode_index, t_find, t_open, t_schema, t_iterate, rows_processed, t_total);

        Ok(EpisodeData {
            episode_index,
            frames,
            video_paths,
            video_frame_offset,
        })
    }

    /// Find episode data file with various naming conventions
    fn find_episode_file(&self, chunk_dir: &str, episode_index: u64) -> Result<PathBuf, DatasetError> {
        // v2.0 format: episode_XXX.parquet
        let v2_patterns = [
            format!("episode_{:06}.parquet", episode_index),
            format!("episode_{}.parquet", episode_index),
        ];

        for pattern in &v2_patterns {
            let path = self.root_path.join("data").join(chunk_dir).join(pattern);
            if path.exists() {
                return Ok(path);
            }
        }

        // v3.0 format: file-XXX.parquet (all episodes in one file)
        // Try file-000.parquet first, then iterate
        for file_idx in 0..MAX_FILE_SEARCH {
            let path = self.root_path.join("data").join(chunk_dir).join(format!("file-{:03}.parquet", file_idx));
            if path.exists() {
                return Ok(path);
            }
            if file_idx == 0 {
                // If file-000 doesn't exist, chunk doesn't exist
                break;
            }
        }

        Err(DatasetError::MissingFile(format!(
            "data/{}/episode or file parquet", chunk_dir
        )))
    }

    /// Find column index by name in schema
    fn find_column_index(schema: &parquet::schema::types::Type, name: &str) -> Option<usize> {
        if let parquet::schema::types::Type::GroupType { fields, .. } = schema {
            for (i, field) in fields.iter().enumerate() {
                if field.name() == name {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Extract integer value from a parquet row
    fn extract_int(record: &parquet::record::Row, col_idx: usize) -> Option<i64> {
        use parquet::record::Field;
        let fields: Vec<_> = record.get_column_iter().collect();
        if col_idx >= fields.len() {
            return None;
        }
        let (_, field) = &fields[col_idx];
        match field {
            Field::Long(v) => Some(*v),
            Field::Int(v) => Some(*v as i64),
            _ => None,
        }
    }

    /// Find video paths for an episode
    ///
    /// Supports multiple LeRobot formats:
    /// - v3.0: videos/{camera_name}/chunk-{chunk_index}/file-{file_index}.mp4
    /// - v2.1: videos/chunk-{chunk_index}/{video_key}/episode_{episode_index}.mp4
    /// - v2.0: videos/chunk-{chunk_index}/{camera_name}_{episode_index}.mp4
    fn find_video_paths(&self, chunk_dir: &str, episode_index: u64) -> HashMap<String, PathBuf> {
        let mut video_paths = HashMap::new();

        // Get video feature names from dataset info
        let video_features: Vec<&String> = self.info.features.iter()
            .filter(|(_, feature)| feature.dtype == "video" || feature.dtype.contains("video"))
            .map(|(name, _)| name)
            .collect();

        // Try v3.0 format: videos/{camera_name}/chunk-XXX/file-XXX.mp4
        for cam_name in &video_features {
            let cam_dir = self.root_path.join("videos").join(cam_name).join(chunk_dir);
            if cam_dir.exists() {
                for file_idx in 0..MAX_VIDEO_FILE_SEARCH {
                    let video_path = cam_dir.join(format!("file-{:03}.mp4", file_idx));
                    if video_path.exists() {
                        video_paths.insert(cam_name.to_string(), video_path);
                        break;
                    }
                }
            }
        }

        // Try v2.1 format: videos/chunk-XXX/{video_key}/episode_{episode_index}.mp4
        if video_paths.is_empty() {
            for cam_name in &video_features {
                let cam_dir = self.root_path.join("videos").join(chunk_dir).join(cam_name);
                if cam_dir.exists() {
                    for pattern in [
                        format!("episode_{:06}.mp4", episode_index),
                        format!("episode_{}.mp4", episode_index),
                    ] {
                        let video_path = cam_dir.join(&pattern);
                        if video_path.exists() {
                            video_paths.insert(cam_name.to_string(), video_path);
                            break;
                        }
                    }
                }
            }
        }

        // Try v2.0 format: videos/chunk-XXX/{camera_name}_{episode_index}.mp4
        if video_paths.is_empty() {
            let videos_dir = self.root_path.join("videos").join(chunk_dir);
            if videos_dir.exists() {
                for cam_name in &video_features {
                    for pattern in [
                        format!("{}_{:06}.mp4", cam_name, episode_index),
                        format!("{}_{}.mp4", cam_name, episode_index),
                    ] {
                        let video_path = videos_dir.join(&pattern);
                        if video_path.exists() {
                            video_paths.insert(cam_name.to_string(), video_path);
                            break;
                        }
                    }
                }
            }
        }

        video_paths
    }

    /// Get the number of episodes
    pub fn num_episodes(&self) -> usize {
        self.episodes.len()
    }

    /// Get episode by index
    pub fn get_episode_metadata(&self, index: u64) -> Option<&EpisodeMetadata> {
        self.episodes.iter().find(|e| e.episode_index == index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dataset_info_default() {
        let info = DatasetInfo::default();
        assert_eq!(info.fps, 30.0);
        assert_eq!(info.codebase_version, "2.0");
    }
}
