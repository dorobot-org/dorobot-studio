//! LeRobot Dataset Loader
//!
//! Parses LeRobot v2.0 dataset format.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::BufReader;

use serde::{Deserialize, Serialize};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::RowAccessor;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatasetError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),
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
        for chunk_idx in 0..100 {
            let chunk_dir = episodes_dir.join(format!("chunk-{:03}", chunk_idx));
            if !chunk_dir.exists() {
                break;
            }

            // Iterate through files in chunk
            for file_idx in 0..100 {
                let file_path = chunk_dir.join(format!("file-{:03}.parquet", file_idx));
                if !file_path.exists() {
                    break;
                }

                let file = File::open(&file_path)?;
                let reader = SerializedFileReader::new(file)?;

                // Get schema to find column indices by name
                let schema = reader.metadata().file_metadata().schema();
                let episode_idx_col = Self::find_column_index_static(schema, "episode_index");
                let length_col = Self::find_column_index_static(schema, "length");

                let mut iter = reader.get_row_iter(None)?;
                while let Some(record) = iter.next() {
                    let record = record?;

                    // Get episode_index
                    let episode_index = episode_idx_col
                        .and_then(|col| Self::extract_int_static(&record, col))
                        .unwrap_or(0) as u64;

                    // Get length
                    let length = length_col
                        .and_then(|col| Self::extract_int_static(&record, col))
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
    pub fn calculate_video_frame_offset(&self, episode_index: u64) -> u64 {
        self.episodes
            .iter()
            .filter(|ep| ep.episode_index < episode_index)
            .map(|ep| ep.length)
            .sum()
    }

    /// Load episode data from parquet files
    pub fn load_episode(&self, episode_index: u64) -> Result<EpisodeData, DatasetError> {
        // Calculate which chunk this episode is in
        let chunk_index = episode_index / self.info.chunks_size.max(1);
        let chunk_dir = format!("chunk-{:03}", chunk_index);

        // Find the data file - try v2.0 (episode_XXX.parquet) and v3.0 (file-XXX.parquet) formats
        let data_path = self.find_episode_file(&chunk_dir, episode_index)?;

        // Load parquet file
        let file = File::open(&data_path)?;
        let reader = SerializedFileReader::new(file)?;

        // Get schema to find column indices
        let schema = reader.metadata().file_metadata().schema();
        let episode_col = self.find_column_index(schema, "episode_index");
        let frame_col = self.find_column_index(schema, "frame_index");
        let timestamp_col = self.find_column_index(schema, "timestamp");
        let state_col = self.find_column_index(schema, "observation.state");
        let action_col = self.find_column_index(schema, "action");

        let mut frames = Vec::new();
        let mut iter = reader.get_row_iter(None)?;

        while let Some(record) = iter.next() {
            let record = record?;

            // For v3.0 format: filter by episode_index
            if let Some(ep_col) = episode_col {
                let row_episode = self.extract_int(&record, ep_col);
                if row_episode != episode_index as i64 {
                    continue;
                }
            }

            // Extract frame index
            let frame_index = if let Some(fc) = frame_col {
                self.extract_int(&record, fc) as u64
            } else {
                frames.len() as u64
            };

            // Extract timestamp
            let timestamp = if let Some(tc) = timestamp_col {
                self.extract_float(&record, tc)
            } else {
                frame_index as f64 / self.info.fps
            };

            // Try to extract state and action data from the record
            let state = self.extract_float_array(&record, state_col);
            let action = self.extract_float_array(&record, action_col);

            frames.push(EpisodeFrame {
                frame_index,
                timestamp,
                state,
                action,
            });
        }

        // Sort frames by frame_index
        frames.sort_by_key(|f| f.frame_index);

        // Calculate video frame offset (cumulative sum of previous episode lengths)
        // In LeRobot v3.0, all episodes are concatenated in a single MP4 file
        let video_frame_offset = self.calculate_video_frame_offset(episode_index);

        // Debug: write to file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[Dataset] episode {} - video_frame_offset={}, chunk_dir={}", episode_index, video_frame_offset, chunk_dir);
        }

        // Find video paths
        let video_paths = self.find_video_paths(&chunk_dir, episode_index);

        Ok(EpisodeData {
            episode_index,
            frames,
            video_paths,
            video_frame_offset,
        })
    }

    /// Extract integer value from a parquet row
    fn extract_int(&self, record: &parquet::record::Row, col_idx: usize) -> i64 {
        let fields: Vec<_> = record.get_column_iter().collect();
        if col_idx >= fields.len() {
            return 0;
        }
        let (_, field) = &fields[col_idx];
        use parquet::record::Field;
        match field {
            Field::Long(v) => *v,
            Field::Int(v) => *v as i64,
            _ => 0,
        }
    }

    /// Extract float value from a parquet row
    fn extract_float(&self, record: &parquet::record::Row, col_idx: usize) -> f64 {
        let fields: Vec<_> = record.get_column_iter().collect();
        if col_idx >= fields.len() {
            return 0.0;
        }
        let (_, field) = &fields[col_idx];
        use parquet::record::Field;
        match field {
            Field::Float(v) => *v as f64,
            Field::Double(v) => *v,
            _ => 0.0,
        }
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
        for file_idx in 0..100 {
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
    fn find_column_index(&self, schema: &parquet::schema::types::Type, name: &str) -> Option<usize> {
        Self::find_column_index_static(schema, name)
    }

    /// Find column index by name in schema (static version for use in static methods)
    fn find_column_index_static(schema: &parquet::schema::types::Type, name: &str) -> Option<usize> {
        if let parquet::schema::types::Type::GroupType { fields, .. } = schema {
            for (i, field) in fields.iter().enumerate() {
                if field.name() == name {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Extract integer value from a parquet row (static version)
    fn extract_int_static(record: &parquet::record::Row, col_idx: usize) -> Option<i64> {
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

    /// Extract float array from a parquet row field
    fn extract_float_array(&self, record: &parquet::record::Row, col_idx: Option<usize>) -> Vec<f32> {
        use parquet::record::Field;

        let Some(idx) = col_idx else {
            return Vec::new();
        };

        let fields: Vec<_> = record.get_column_iter().collect();
        if idx >= fields.len() {
            return Vec::new();
        }

        let (_, field) = &fields[idx];

        match field {
            Field::ListInternal(list) => {
                list.elements().iter().filter_map(|f| {
                    match f {
                        Field::Float(v) => Some(*v),
                        Field::Double(v) => Some(*v as f32),
                        _ => None,
                    }
                }).collect()
            }
            Field::Group(row) => {
                // Some datasets have nested structures
                row.get_column_iter().filter_map(|(_, f)| {
                    match f {
                        Field::Float(v) => Some(*v),
                        Field::Double(v) => Some(*v as f32),
                        _ => None,
                    }
                }).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Find video paths for an episode
    ///
    /// LeRobot v3.0 video structure:
    /// videos/{camera_name}/chunk-{chunk_index}/file-{file_index}.mp4
    ///
    /// LeRobot v2.0 video structure:
    /// videos/chunk-{chunk_index}/{camera_name}_{episode_index}.mp4
    fn find_video_paths(&self, chunk_dir: &str, episode_index: u64) -> HashMap<String, PathBuf> {
        let mut video_paths = HashMap::new();

        // Common camera names in LeRobot datasets
        let camera_patterns = [
            "observation.images.cam_high",
            "observation.images.cam_left_wrist",
            "observation.images.cam_right_wrist",
            "observation.images.top",
            "observation.images.wrist",
        ];

        // Try v3.0 format first: videos/{camera_name}/chunk-XXX/file-XXX.mp4
        for cam_name in camera_patterns {
            let cam_dir = self.root_path.join("videos").join(cam_name).join(chunk_dir);
            if cam_dir.exists() {
                // Find video file - try file-000.mp4, file-001.mp4, etc.
                for file_idx in 0..10 {
                    let video_path = cam_dir.join(format!("file-{:03}.mp4", file_idx));
                    if video_path.exists() {
                        video_paths.insert(cam_name.to_string(), video_path);
                        break;
                    }
                }
            }
        }

        // If no v3.0 videos found, try v2.0 format: videos/chunk-XXX/{camera_name}_{episode}.mp4
        if video_paths.is_empty() {
            let videos_dir = self.root_path.join("videos").join(chunk_dir);
            if videos_dir.exists() {
                for cam_name in camera_patterns {
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
