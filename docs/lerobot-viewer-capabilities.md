# LeRobot Dataset Viewer: Makepad Capabilities Analysis

> A comprehensive analysis of the LeRobot dataset format and required Makepad capabilities for building a visualization tool.

**Date:** January 2026

---

## Table of Contents

1. [LeRobot Dataset v3 Format Overview](#lerobot-dataset-v3-format-overview)
2. [Data Types and Schema](#data-types-and-schema)
3. [Visualization Requirements](#visualization-requirements)
4. [Required Makepad Components](#required-makepad-components)
5. [Implementation Roadmap](#implementation-roadmap)
6. [Architecture Design](#architecture-design)

---

## LeRobot Dataset v3 Format Overview

LeRobot v3.0 is a standardized format for robot learning data from Hugging Face, designed for imitation learning and robot control research.

### Directory Structure

```
dataset_root/
├── data/
│   └── chunk-XXX/
│       └── file-XXX.parquet    # Frame-level tabular data
├── meta/
│   ├── info.json               # Schema, fps, features, path templates
│   ├── stats.json              # Normalization statistics (mean/std/min/max)
│   ├── tasks.jsonl             # Task descriptions → task_index mapping
│   └── episodes/
│       └── chunk-XXX.parquet   # Episode metadata (lengths, offsets)
└── videos/
    └── observation.images.<camera_key>/
        └── chunk-XXX/
            └── file-XXX.mp4    # Concatenated video frames
```

### Key Design Principles

| Principle | Description |
|-----------|-------------|
| **File-based storage** | Many episodes per Parquet/MP4 file (not one file per episode) |
| **Relational metadata** | Episode boundaries resolved via metadata, not filenames |
| **Hub-native streaming** | Stream directly from Hugging Face Hub without downloading |
| **Scalability** | Designed for millions of episodes |

---

## Data Types and Schema

### Core Parquet Fields

| Field | Type | Description |
|-------|------|-------------|
| `observation.state` | `list[float32]` | Robot proprioceptive state (joint angles, end-effector pos) |
| `action` | `list[float32]` | Target joint angles or commanded movements |
| `timestamp` | `float32` | Seconds elapsed from episode start |
| `episode_index` | `int64` | Episode identifier |
| `frame_index` | `int64` | Frame position within episode (0-indexed) |
| `index` | `int64` | Global unique identifier across dataset |
| `next.done` | `bool` | Episode termination flag |
| `task_index` | `int64` | Reference to task in tasks.jsonl |

### Video/Image Fields

| Field | Type | Description |
|-------|------|-------------|
| `observation.images.<camera>` | `VideoFrame` | Camera observation |

**VideoFrame Structure:**
```python
{
    'path': str,       # Path to MP4 file
    'timestamp': float # Timestamp within video (seconds)
}
```

### Robot-Specific Dimensions

**Example: SO-101 Follower Arm (6-DOF + gripper)**
```
observation.state: [7]  # 6 joint angles + gripper position
action: [7]             # 6 joint targets + gripper command
```

**Example: ALOHA Bimanual (2x 6-DOF arms)**
```
observation.state: [14]  # 7 per arm (6 joints + gripper)
action: [14]             # Commands for both arms
```

### Temporal Relationship

**Critical:** The action at frame `t` is the action that *caused* the observation at frame `t+1`.

```
Frame t:   observation.state[t], action[t]
Frame t+1: observation.state[t+1] = f(observation.state[t], action[t])
```

### Statistics Format (stats.json)

```json
{
  "observation.state": {
    "mean": [0.1, 0.2, ...],
    "std": [0.05, 0.03, ...],
    "min": [-1.0, -1.0, ...],
    "max": [1.0, 1.0, ...]
  },
  "action": {
    "mean": [...],
    "std": [...],
    "min": [...],
    "max": [...]
  }
}
```

---

## Visualization Requirements

Based on analysis of LeRobot data and Rerun's robotics visualization capabilities:

### 1. Video Playback

| Requirement | Priority | Description |
|-------------|----------|-------------|
| MP4 decoding | Critical | Decode H.264/H.265 video from MP4 containers |
| Multi-camera sync | Critical | Display multiple camera views synchronized |
| Frame-accurate seek | High | Seek to exact frame by timestamp |
| Playback controls | High | Play, pause, step forward/back, speed control |
| Timeline scrubbing | High | Interactive timeline with frame preview |

### 2. Time-Series Visualization

| Requirement | Priority | Description |
|-------------|----------|-------------|
| Joint angle plots | Critical | Plot observation.state over time |
| Action plots | Critical | Plot action commands over time |
| Multi-channel display | High | Display 6-14 channels simultaneously |
| Synchronized cursor | High | Vertical line showing current frame |
| Zoom/pan | High | Navigate large episodes (1000+ frames) |
| Value readout | Medium | Show exact values at cursor position |
| Statistics overlay | Medium | Show mean/std bands from stats.json |

### 3. Robot Arm Visualization (3D)

| Requirement | Priority | Description |
|-------------|----------|-------------|
| URDF loading | High | Load robot model from URDF file |
| Forward kinematics | High | Compute joint positions from angles |
| Joint rendering | High | Render arm segments and joints |
| End-effector tracking | High | Show gripper/tool position |
| Ghost/trajectory | Medium | Show predicted/past poses as ghosts |
| Coordinate frames | Medium | Display joint coordinate systems |
| Collision geometry | Low | Show collision meshes |

### 4. Episode Navigation

| Requirement | Priority | Description |
|-------------|----------|-------------|
| Episode list | Critical | Browse all episodes in dataset |
| Episode metadata | High | Show length, task, timestamps |
| Task filtering | High | Filter episodes by task_index |
| Thumbnail preview | Medium | Episode preview images |
| Search | Medium | Search by task description |

### 5. Data Inspection

| Requirement | Priority | Description |
|-------------|----------|-------------|
| Schema viewer | High | Display info.json schema |
| Statistics display | High | Show stats.json values |
| Raw data table | Medium | View parquet data as table |
| Export | Low | Export clips/data subsets |

### 6. Comparison & Analysis

| Requirement | Priority | Description |
|-------------|----------|-------------|
| Side-by-side episodes | Medium | Compare two episodes |
| Action prediction overlay | Medium | Overlay model predictions on ground truth |
| Anomaly highlighting | Low | Highlight unusual states/actions |

---

## Required Makepad Components

### Existing Makepad Components (Reuse)

| Component | Location | Use Case |
|-----------|----------|----------|
| `Button`, `Label` | makepad-widgets | UI controls |
| `ScrollView` | makepad-widgets | Episode list, data tables |
| `Slider` | makepad-widgets | Timeline scrubbing |
| `Splitter` | makepad-widgets | Panel layout |
| `DrawQuad` | makepad-draw | 2D backgrounds, image display |
| `DrawCube` | makepad-draw | 3D robot segment rendering |

### New Components to Build

#### 1. Video Player Widget

**Purpose:** Decode and display MP4 video frames synchronized with data

```rust
live_design!{
    VideoPlayer = {{VideoPlayer}} {
        width: Fill,
        height: Fill,

        draw_frame: {
            texture video_frame: texture2d
            fn pixel(self) -> vec4 {
                return sample2d(self.video_frame, self.pos);
            }
        }
    }
}

pub struct VideoPlayer {
    // Video decoding
    decoder: VideoDecoder,          // ffmpeg/gstreamer binding
    frame_buffer: Texture,          // Current frame texture

    // Playback state
    current_time: f64,
    playback_speed: f64,
    is_playing: bool,

    // Sync
    frame_timestamps: Vec<f64>,     // Frame timestamp index
}
```

**Dependencies:**
- Video decoding: `ffmpeg-next` or `gstreamer` crate
- Texture upload: Makepad `Texture::VecBGRAu8_32`

#### 2. Time-Series Plot Widget

**Purpose:** Display multi-channel time-series data with synchronized cursor

```rust
live_design!{
    TimeSeriesPlot = {{TimeSeriesPlot}} {
        width: Fill,
        height: 200,

        draw_bg: { color: #1a1a1a }
        draw_grid: { color: #333333 }
        draw_line: {
            fn pixel(self) -> vec4 {
                // Anti-aliased line rendering
                let dist = abs(self.local_y - self.value);
                let aa = fwidth(dist);
                let alpha = 1.0 - smoothstep(0.0, aa * 2.0, dist);
                return vec4(self.line_color.rgb, alpha);
            }
        }
        draw_cursor: { color: #ff0000 }
    }
}

pub struct TimeSeriesPlot {
    // Data
    channels: Vec<TimeSeriesChannel>,
    time_range: (f64, f64),
    value_range: (f64, f64),

    // Interaction
    cursor_time: f64,
    zoom_level: f64,
    pan_offset: f64,

    // Display
    channel_colors: Vec<Vec4>,
    show_statistics: bool,
}

pub struct TimeSeriesChannel {
    name: String,
    data: Vec<(f64, f64)>,  // (timestamp, value)
    stats: Option<ChannelStats>,
}
```

#### 3. Robot Arm 3D Viewer

**Purpose:** Render robot arm from joint angles with URDF model

```rust
live_design!{
    RobotArmViewer = {{RobotArmViewer}} {
        width: Fill,
        height: Fill,

        draw_joint: <DrawCube> {
            color: #4080ff
        }
        draw_link: <DrawCube> {
            color: #808080
        }
        draw_end_effector: <DrawCube> {
            color: #ff4040
        }
        draw_trajectory: <DrawLines> {
            color: #ffff00
        }
    }
}

pub struct RobotArmViewer {
    // Robot model
    urdf: Option<UrdfModel>,
    joint_angles: Vec<f32>,

    // Camera
    orbit_camera: OrbitCamera,

    // Rendering
    joint_transforms: Vec<Mat4>,
    link_meshes: Vec<Geometry>,

    // Trajectory
    trajectory_points: Vec<Vec3>,
    show_trajectory: bool,
}

pub struct UrdfModel {
    joints: Vec<UrdfJoint>,
    links: Vec<UrdfLink>,
    kinematic_chain: Vec<usize>,
}
```

#### 4. Dataset Browser Widget

**Purpose:** Navigate episodes and load data from LeRobot format

```rust
pub struct DatasetBrowser {
    // Dataset
    dataset_path: String,
    info: DatasetInfo,
    episodes: Vec<EpisodeMetadata>,

    // Current selection
    selected_episode: Option<usize>,

    // Data cache
    loaded_episode: Option<EpisodeData>,
    video_cache: LruCache<String, VideoDecoder>,
}

pub struct EpisodeMetadata {
    index: u64,
    length: u64,
    task_index: u64,
    task_description: String,
    data_file: String,
    video_files: HashMap<String, String>,
}

pub struct EpisodeData {
    timestamps: Vec<f64>,
    observation_state: Vec<Vec<f32>>,
    actions: Vec<Vec<f32>>,
    frame_indices: Vec<u64>,
}
```

#### 5. Parquet Reader

**Purpose:** Read LeRobot parquet files efficiently

```rust
pub struct ParquetReader {
    // File handles
    data_files: Vec<PathBuf>,
    episode_files: Vec<PathBuf>,

    // Schema
    schema: DatasetSchema,

    // Caching
    row_group_cache: LruCache<(usize, usize), RecordBatch>,
}

impl ParquetReader {
    pub fn load_episode(&self, episode_index: u64) -> Result<EpisodeData>;
    pub fn load_frame_range(&self, start: u64, end: u64) -> Result<Vec<Frame>>;
    pub fn get_episode_metadata(&self, index: u64) -> Result<EpisodeMetadata>;
}
```

**Dependencies:** `parquet` and `arrow` crates

#### 6. Timeline Widget

**Purpose:** Visual timeline with markers and scrubbing

```rust
live_design!{
    Timeline = {{Timeline}} {
        height: 60,

        draw_track: { color: #2a2a2a }
        draw_playhead: { color: #ff0000 }
        draw_episode_marker: { color: #4080ff }
        draw_frame_ticks: { color: #404040 }
    }
}

pub struct Timeline {
    // Time range
    total_duration: f64,
    visible_range: (f64, f64),

    // Playhead
    current_time: f64,

    // Markers
    episode_boundaries: Vec<f64>,
    keyframes: Vec<f64>,

    // Interaction
    is_scrubbing: bool,
    snap_to_frames: bool,
}
```

#### 7. Multi-Camera View

**Purpose:** Display multiple synchronized camera views

```rust
live_design!{
    MultiCameraView = {{MultiCameraView}} {
        layout: Grid { columns: 2 }

        camera_view_template: <VideoPlayer> {
            width: Fill,
            height: Fill,
        }
    }
}

pub struct MultiCameraView {
    cameras: Vec<CameraView>,
    layout: CameraLayout,  // Grid, Horizontal, Vertical, Single
    sync_time: f64,
}

pub struct CameraView {
    name: String,  // e.g., "observation.images.front"
    player: VideoPlayer,
    enabled: bool,
}
```

### Component Dependency Graph

```
┌─────────────────────────────────────────────────────────────────┐
│                      LeRobot Viewer App                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐ │
│  │  Dataset Browser │  │   Timeline       │  │ Playback Ctrl │ │
│  └────────┬─────────┘  └────────┬─────────┘  └───────┬───────┘ │
│           │                     │                    │          │
│           └─────────────────────┼────────────────────┘          │
│                                 │                               │
│                    ┌────────────▼────────────┐                  │
│                    │   Synchronization Hub   │                  │
│                    │  (Current Time Manager) │                  │
│                    └────────────┬────────────┘                  │
│                                 │                               │
│           ┌─────────────────────┼─────────────────────┐         │
│           │                     │                     │         │
│  ┌────────▼────────┐  ┌────────▼────────┐  ┌────────▼────────┐ │
│  │ Multi-Camera    │  │ Time-Series     │  │ Robot Arm 3D    │ │
│  │ View            │  │ Plot            │  │ Viewer          │ │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘ │
│           │                    │                    │          │
│  ┌────────▼────────┐  ┌────────▼────────┐  ┌────────▼────────┐ │
│  │ Video Player    │  │ DrawLines       │  │ URDF Loader     │ │
│  │ (per camera)    │  │ DrawGrid        │  │ Forward Kinem.  │ │
│  └────────┬────────┘  └─────────────────┘  └────────┬────────┘ │
│           │                                         │          │
│  ┌────────▼────────┐                       ┌────────▼────────┐ │
│  │ Video Decoder   │                       │ DrawCube        │ │
│  │ (ffmpeg)        │                       │ OrbitCamera     │ │
│  └─────────────────┘                       └─────────────────┘ │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                      Data Layer                                 │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐ │
│  │  Parquet Reader  │  │  Video Cache     │  │ Stats Cache   │ │
│  └──────────────────┘  └──────────────────┘  └───────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

---

## Implementation Roadmap

### Phase 1: Core Data Loading (Week 1-2)

| Task | Description | Effort |
|------|-------------|--------|
| Parquet reader | Read LeRobot parquet files with arrow crate | 3 days |
| Metadata parser | Parse info.json, stats.json, tasks.jsonl | 1 day |
| Episode indexer | Build episode index from meta/episodes/ | 2 days |
| Video decoder | Integrate ffmpeg for MP4 decoding | 3 days |

**Deliverable:** Load dataset and extract frames/data by episode

### Phase 2: Basic Visualization (Week 3-4)

| Task | Description | Effort |
|------|-------------|--------|
| Video player widget | Display decoded video frames | 3 days |
| Timeline widget | Scrubbing and playback controls | 2 days |
| Time-series plot | Basic multi-channel line plot | 4 days |
| Sync manager | Synchronize video and plots | 2 days |

**Deliverable:** Play episode with synchronized video and joint plots

### Phase 3: Robot 3D Viewer (Week 5-6)

| Task | Description | Effort |
|------|-------------|--------|
| URDF parser | Parse URDF XML for robot model | 2 days |
| Forward kinematics | Compute transforms from joint angles | 2 days |
| 3D arm renderer | Render robot links and joints | 3 days |
| Orbit camera | Interactive 3D camera controls | 2 days |

**Deliverable:** Animated 3D robot arm synchronized with data

### Phase 4: Dataset Browser (Week 7)

| Task | Description | Effort |
|------|-------------|--------|
| Episode list | Scrollable episode browser | 2 days |
| Task filtering | Filter by task description | 1 day |
| Search | Full-text search in tasks | 1 day |
| Thumbnails | Episode preview generation | 2 days |

**Deliverable:** Navigate and select episodes from dataset

### Phase 5: Polish & Features (Week 8)

| Task | Description | Effort |
|------|-------------|--------|
| Multi-camera layout | Grid/split views | 2 days |
| Statistics overlay | Mean/std bands on plots | 1 day |
| Export | Export clips/screenshots | 2 days |
| Hub streaming | Load from Hugging Face Hub directly | 2 days |

**Deliverable:** Production-ready LeRobot viewer

---

## Architecture Design

### Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    Hugging Face Hub / Local                     │
│                                                                 │
│   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────────────┐   │
│   │ Parquet │  │   MP4   │  │  JSON   │  │ Episode Parquet │   │
│   │  data/  │  │ videos/ │  │  meta/  │  │ meta/episodes/  │   │
│   └────┬────┘  └────┬────┘  └────┬────┘  └────────┬────────┘   │
└────────┼────────────┼───────────┼─────────────────┼────────────┘
         │            │           │                 │
         ▼            ▼           ▼                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Data Layer (Rust)                         │
│                                                                 │
│   ┌─────────────────┐  ┌─────────────────┐  ┌───────────────┐  │
│   │  ParquetReader  │  │  VideoDecoder   │  │ MetadataCache │  │
│   │  (arrow crate)  │  │  (ffmpeg-next)  │  │  (serde_json) │  │
│   └────────┬────────┘  └────────┬────────┘  └───────┬───────┘  │
│            │                    │                   │          │
│            └────────────────────┼───────────────────┘          │
│                                 │                              │
│                    ┌────────────▼────────────┐                 │
│                    │     DatasetManager      │                 │
│                    │  - Episode loading      │                 │
│                    │  - Frame synchronization│                 │
│                    │  - Cache management     │                 │
│                    └────────────┬────────────┘                 │
└─────────────────────────────────┼──────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Makepad UI Layer                             │
│                                                                 │
│   ┌─────────────────────────────────────────────────────────┐  │
│   │                    AppState                              │  │
│   │  - current_time: f64                                     │  │
│   │  - current_episode: Option<EpisodeData>                  │  │
│   │  - playback_state: PlaybackState                         │  │
│   └─────────────────────────────────────────────────────────┘  │
│                              │                                  │
│         ┌────────────────────┼────────────────────┐            │
│         │                    │                    │            │
│         ▼                    ▼                    ▼            │
│   ┌───────────┐       ┌───────────┐       ┌───────────┐       │
│   │  Camera   │       │  Plots    │       │  Robot    │       │
│   │  Views    │       │  Panel    │       │  3D View  │       │
│   └───────────┘       └───────────┘       └───────────┘       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### State Management

```rust
pub struct AppState {
    // Dataset
    pub dataset: Option<LeRobotDataset>,
    pub current_episode: Option<EpisodeData>,

    // Playback
    pub current_time: f64,
    pub playback_speed: f64,
    pub is_playing: bool,

    // View state
    pub selected_cameras: Vec<String>,
    pub selected_channels: Vec<String>,
    pub camera_layout: CameraLayout,

    // 3D view
    pub robot_model: Option<UrdfModel>,
    pub show_trajectory: bool,
    pub trajectory_length: usize,
}

pub enum AppAction {
    // Dataset
    LoadDataset(PathBuf),
    SelectEpisode(u64),

    // Playback
    Play,
    Pause,
    Seek(f64),
    SetSpeed(f64),
    StepForward,
    StepBackward,

    // View
    ToggleCamera(String),
    ToggleChannel(String),
    SetCameraLayout(CameraLayout),

    // 3D
    LoadUrdf(PathBuf),
    ToggleTrajectory,
}
```

---

## External Dependencies

### Required Crates

| Crate | Purpose | Notes |
|-------|---------|-------|
| `arrow` | Arrow data format | For parquet reading |
| `parquet` | Parquet file reading | LeRobot data files |
| `ffmpeg-next` | Video decoding | MP4/H.264 decoding |
| `serde_json` | JSON parsing | info.json, tasks.jsonl |
| `urdf-rs` | URDF parsing | Robot model loading |
| `nalgebra` | Linear algebra | Forward kinematics |
| `lru` | LRU cache | Frame/data caching |

### Optional Crates

| Crate | Purpose | Notes |
|-------|---------|-------|
| `reqwest` | HTTP client | Hub streaming |
| `hf-hub` | Hugging Face Hub API | Dataset downloading |
| `image` | Image processing | Thumbnail generation |

---

## Summary: Capabilities to Develop

### Critical (Must Have)

1. **Parquet Data Reader** - Load observation.state, action, timestamps
2. **Video Player** - Decode and display MP4 camera feeds
3. **Time-Series Plot** - Visualize joint angles and actions over time
4. **Timeline with Scrubbing** - Navigate through episode
5. **Playback Controls** - Play, pause, step, speed control
6. **Synchronization** - Keep video, plots, and 3D view in sync

### High Priority

7. **Episode Browser** - Navigate dataset episodes
8. **Multi-Camera View** - Display multiple camera angles
9. **Robot 3D Viewer** - Animated robot arm from URDF
10. **Task Filtering** - Filter episodes by task description

### Medium Priority

11. **Statistics Display** - Show mean/std from stats.json
12. **Zoom/Pan on Plots** - Navigate long episodes
13. **Value Readout** - Exact values at cursor position
14. **Trajectory Ghost** - Show past/future robot poses

### Low Priority

15. **Hub Streaming** - Load directly from Hugging Face
16. **Export** - Export clips or data subsets
17. **Side-by-Side Comparison** - Compare two episodes
18. **Model Prediction Overlay** - Show policy predictions

---

## References

- [LeRobotDataset v3.0 Documentation](https://huggingface.co/docs/lerobot/en/lerobot-dataset-v3)
- [LeRobot Datasets v3 Blog Post](https://huggingface.co/blog/lerobot-datasets-v3)
- [LeRobot Dataset Format - Phospho Docs](https://docs.phospho.ai/learn/lerobot-dataset)
- [Rerun Robotics Visualization](https://rerun.io/)
- [Comparing RViz, Foxglove, Rerun](https://www.reduct.store/blog/comparison-rviz-foxglove-rerun)
