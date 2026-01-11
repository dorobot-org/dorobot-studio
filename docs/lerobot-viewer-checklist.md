# LeRobot Makepad Viewer: Implementation Checklist

> Detailed implementation plan leveraging Rerun components where possible

**Date:** January 2026
**Target:** 8-week MVP

---

## Overview: Rerun Component Reuse Strategy

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    LEROBOT MAKEPAD VIEWER ARCHITECTURE                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                      MAKEPAD UI LAYER                           │   │
│  │  [Build from scratch - Makepad widgets & shaders]               │   │
│  │                                                                 │   │
│  │  • Video Player Widget        • Time-Series Plot               │   │
│  │  • Timeline Widget            • Robot 3D Viewer                │   │
│  │  • Episode Browser            • Multi-Camera View              │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                    │                                    │
│                                    ▼                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                      BRIDGE / ADAPTER LAYER                     │   │
│  │  [Thin conversion between Rerun types ↔ Makepad types]          │   │
│  │                                                                 │   │
│  │  • LeRobot Parquet → Rerun EntityDb                            │   │
│  │  • Rerun types → Makepad GPU data                              │   │
│  │  • Timeline sync between video/data/3D                         │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                    │                                    │
│                                    ▼                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                   RERUN REUSABLE COMPONENTS                     │   │
│  │  [Direct dependency - no modification needed]                   │   │
│  │                                                                 │   │
│  │  ✅ re_sdk_types      - Data types (Vec3D, Quaternion, etc.)   │   │
│  │  ✅ re_chunk          - Arrow-based data chunks                │   │
│  │  ✅ re_chunk_store    - Time-indexed storage                   │   │
│  │  ✅ re_entity_db      - Entity database with queries           │   │
│  │  ✅ re_query          - LatestAt / Range queries               │   │
│  │  ✅ re_log_encoding   - .rrd file read/write                   │   │
│  │  ✅ re_log_types      - Timeline, TimeInt, EntityPath          │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                    │                                    │
│                                    ▼                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    EXTERNAL DEPENDENCIES                        │   │
│  │                                                                 │   │
│  │  • arrow/parquet     - LeRobot data files                      │   │
│  │  • ffmpeg-next       - MP4 video decoding                      │   │
│  │  • urdf-rs           - Robot model parsing                     │   │
│  │  • gltf              - 3D model loading (from Rerun pattern)   │   │
│  │  • nalgebra          - Forward kinematics                      │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Project Setup & Data Layer (Week 1-2)

### 1.1 Project Structure Setup

- [ ] **Create crate structure**
  ```
  dorobot/
  ├── crates/
  │   ├── dorobot-app/           # Makepad UI application
  │   ├── dorobot-data/          # Data loading (Parquet, video)
  │   ├── dorobot-bridge/        # Rerun ↔ Makepad adapters
  │   └── dorobot-types/         # Shared types
  └── Cargo.toml                 # Workspace
  ```

- [ ] **Configure Cargo.toml with Rerun dependencies**
  ```toml
  [workspace.dependencies]
  # Rerun reusable crates (NO UI dependencies)
  re_sdk_types = { git = "https://github.com/rerun-io/rerun", default-features = false, features = ["glam"] }
  re_types_core = { git = "https://github.com/rerun-io/rerun" }
  re_chunk = { git = "https://github.com/rerun-io/rerun" }
  re_chunk_store = { git = "https://github.com/rerun-io/rerun" }
  re_entity_db = { git = "https://github.com/rerun-io/rerun" }
  re_query = { git = "https://github.com/rerun-io/rerun" }
  re_log_types = { git = "https://github.com/rerun-io/rerun" }
  re_log_encoding = { git = "https://github.com/rerun-io/rerun", features = ["decoder", "encoder"] }

  # Data loading
  arrow = "53"
  parquet = "53"

  # Video
  ffmpeg-next = "7"

  # Robot model
  urdf-rs = "0.8"
  nalgebra = "0.33"

  # Makepad
  makepad-widgets = { git = "https://github.com/makepad/makepad", branch = "main" }
  ```

- [ ] **Verify Rerun crates compile without egui**
  - Test: `cargo check -p dorobot-bridge`
  - Ensure no `egui`, `eframe`, `wgpu` dependencies pulled in

### 1.2 LeRobot Data Reader (dorobot-data)

- [ ] **Implement `LeRobotDataset` struct**
  ```rust
  pub struct LeRobotDataset {
      pub root_path: PathBuf,
      pub info: DatasetInfo,
      pub stats: DatasetStats,
      pub tasks: Vec<TaskInfo>,
      pub episodes: Vec<EpisodeMetadata>,
  }
  ```

- [ ] **Parse `meta/info.json`**
  - [ ] Extract schema (feature names, shapes, dtypes)
  - [ ] Extract fps
  - [ ] Extract path templates for data/video files

- [ ] **Parse `meta/stats.json`**
  - [ ] Load mean/std/min/max per feature
  - [ ] Convert to Rerun-compatible types

- [ ] **Parse `meta/tasks.jsonl`**
  - [ ] Load task descriptions
  - [ ] Build task_index → description map

- [ ] **Parse `meta/episodes/*.parquet`**
  - [ ] Load episode metadata (lengths, offsets)
  - [ ] Build episode index for fast lookup

- [ ] **Implement Parquet frame reader**
  - [ ] Read `data/chunk-XXX/file-XXX.parquet`
  - [ ] Extract columns: `observation.state`, `action`, `timestamp`, `frame_index`, `episode_index`
  - [ ] Support lazy loading / memory mapping

### 1.3 Rerun Bridge Layer (dorobot-bridge)

- [ ] **Define LeRobot → Rerun type mappings**
  ```rust
  // observation.state [f32; N] → re_sdk_types components
  pub fn observation_to_positions(state: &[f32]) -> Vec<Position3D>;

  // action [f32; N] → Rerun arrow batches
  pub fn actions_to_chunk(actions: &[Vec<f32>], timestamps: &[f64]) -> Chunk;
  ```

- [ ] **Implement `LeRobotEntityDb` wrapper**
  ```rust
  pub struct LeRobotEntityDb {
      inner: re_entity_db::EntityDb,
  }

  impl LeRobotEntityDb {
      pub fn from_lerobot_dataset(dataset: &LeRobotDataset) -> Self;
      pub fn load_episode(&mut self, episode_idx: u64) -> Result<()>;
      pub fn query_state_at(&self, time: f64) -> Option<Vec<f32>>;
      pub fn query_action_at(&self, time: f64) -> Option<Vec<f32>>;
  }
  ```

- [ ] **Store episode data in Rerun's timeline format**
  - [ ] Create `Timeline::new("frame")` for frame-based indexing
  - [ ] Create `Timeline::new("timestamp")` for time-based indexing
  - [ ] Log `observation.state` as custom component
  - [ ] Log `action` as custom component

### 1.4 Video Decoder (dorobot-data)

- [ ] **Implement `VideoDecoder` struct**
  ```rust
  pub struct VideoDecoder {
      decoder: ffmpeg::decoder::Video,
      scaler: ffmpeg::software::scaling::Context,
      frame_timestamps: Vec<f64>,
  }
  ```

- [ ] **Open MP4 files from LeRobot videos/ directory**
  - [ ] Support H.264 codec
  - [ ] Extract frame timestamps

- [ ] **Implement frame seeking**
  - [ ] `seek_to_time(time: f64) -> Result<Frame>`
  - [ ] `seek_to_frame(frame_idx: u64) -> Result<Frame>`

- [ ] **Convert frames to Makepad texture format**
  - [ ] FFmpeg AVFrame → RGB24 → Makepad `Vec<u32>` (BGRA)

- [ ] **Implement frame cache**
  - [ ] LRU cache for decoded frames
  - [ ] Prefetch adjacent frames during playback

---

## Phase 2: Core Visualization Widgets (Week 3-4)

### 2.1 Video Player Widget

- [ ] **Create `VideoPlayer` Makepad widget**
  ```rust
  live_design!{
      VideoPlayer = {{VideoPlayer}} {
          width: Fill, height: Fill,

          draw_frame: <DrawQuad> {
              texture video_tex: texture2d
              fn pixel(self) -> vec4 {
                  return sample2d(self.video_tex, self.pos);
              }
          }
      }
  }
  ```

- [ ] **Implement texture upload from decoded frame**
  - [ ] Create `Texture::VecBGRAu8_32` from frame data
  - [ ] Handle partial updates for efficiency

- [ ] **Handle resize and aspect ratio**
  - [ ] Maintain video aspect ratio
  - [ ] Letterbox/pillarbox as needed

- [ ] **Emit frame change events**
  - [ ] `VideoFrameChanged { frame_idx, timestamp }`

### 2.2 Timeline Widget

- [ ] **Create `Timeline` Makepad widget**
  ```rust
  live_design!{
      Timeline = {{Timeline}} {
          height: 48,

          draw_track: <DrawQuad> { color: #2a2a2a }
          draw_playhead: <DrawQuad> { color: #ff4444 }
          draw_ticks: <DrawQuad> { color: #444444 }
      }
  }
  ```

- [ ] **Implement playhead rendering**
  - [ ] Show current position as vertical line
  - [ ] Display timestamp/frame number

- [ ] **Implement scrubbing**
  - [ ] Mouse drag to seek
  - [ ] Click to jump
  - [ ] Emit `TimelineSeek { time }` event

- [ ] **Add episode markers**
  - [ ] Visual indicators for episode boundaries
  - [ ] Optional keyframe markers

- [ ] **Add zoom/pan**
  - [ ] Mouse wheel to zoom
  - [ ] Drag to pan when zoomed

### 2.3 Playback Controller

- [ ] **Create `PlaybackController` component**
  ```rust
  pub struct PlaybackController {
      is_playing: bool,
      playback_speed: f64,  // 0.25, 0.5, 1.0, 2.0, etc.
      current_time: f64,
      duration: f64,
      loop_mode: LoopMode,  // None, Episode, Selection
  }
  ```

- [ ] **Implement playback state machine**
  - [ ] Play: advance time by delta * speed
  - [ ] Pause: hold current time
  - [ ] Step forward/back: advance by 1 frame

- [ ] **Create playback control buttons**
  - [ ] Play/Pause toggle
  - [ ] Step forward/back buttons
  - [ ] Speed selector dropdown

- [ ] **Handle frame timing**
  - [ ] Use dataset fps for frame duration
  - [ ] Ensure video/data sync at boundaries

### 2.4 Time-Series Plot Widget (Using Rerun Algorithms)

- [ ] **Create `TimeSeriesPlot` Makepad widget**
  ```rust
  live_design!{
      TimeSeriesPlot = {{TimeSeriesPlot}} {
          height: 150,

          draw_bg: <DrawQuad> { color: #1a1a1a }
          draw_grid: <DrawLines> { color: #333333 }
          draw_series: <DrawLines> {
              // Anti-aliased line rendering
              fn vertex(self) -> vec4 { ... }
              fn pixel(self) -> vec4 { ... }
          }
          draw_cursor: <DrawQuad> { color: #ff0000 }
      }
  }
  ```

- [ ] **Reference Rerun's line rendering algorithm**
  - [ ] Study `re_renderer/src/renderer/lines.rs` for anti-aliasing
  - [ ] Port line width and joint handling to Makepad shader

- [ ] **Implement data binding**
  - [ ] `set_data(channel: &str, data: &[(f64, f64)])`
  - [ ] Support multiple channels with colors

- [ ] **Query data from Rerun EntityDb**
  ```rust
  // Use re_query::RangeQuery for time range
  let query = RangeQuery::new(timeline, visible_time_range);
  let data = entity_db.storage_engine().range(&query, path, components);
  ```

- [ ] **Implement cursor sync**
  - [ ] Vertical line at current time
  - [ ] Value tooltip at cursor position

- [ ] **Implement zoom/pan**
  - [ ] Y-axis auto-scale or manual range
  - [ ] X-axis linked to timeline widget

- [ ] **Add statistics overlay**
  - [ ] Optional mean line
  - [ ] Optional ±std shading

### 2.5 Synchronization Manager

- [ ] **Create `SyncManager` component**
  ```rust
  pub struct SyncManager {
      current_time: f64,
      subscribers: Vec<Box<dyn TimeSyncSubscriber>>,
  }

  pub trait TimeSyncSubscriber {
      fn on_time_changed(&mut self, time: f64);
  }
  ```

- [ ] **Implement time broadcast**
  - [ ] When timeline scrubs → update all subscribers
  - [ ] When video frame changes → update timeline
  - [ ] When plot cursor moves → update all

- [ ] **Handle frame/time conversion**
  - [ ] `time_to_frame(time: f64, fps: f64) -> u64`
  - [ ] `frame_to_time(frame: u64, fps: f64) -> f64`

---

## Phase 3: Robot 3D Viewer (Week 5-6)

### 3.1 URDF Loader (Leverage Rerun Data Types)

- [ ] **Use `urdf-rs` crate for parsing**
  ```rust
  use urdf_rs::Robot;

  pub struct RobotModel {
      urdf: Robot,
      joints: Vec<JointInfo>,
      links: Vec<LinkInfo>,
      kinematic_chain: Vec<usize>,  // Root to end-effector
  }
  ```

- [ ] **Convert URDF transforms to Rerun types**
  ```rust
  use re_sdk_types::datatypes::{Vec3D, Quaternion, Mat4x4};

  pub fn urdf_origin_to_transform(origin: &urdf_rs::Pose) -> Transform3D {
      let translation = Vec3D::new(origin.xyz[0], origin.xyz[1], origin.xyz[2]);
      let rotation = Quaternion::from_euler_angles(origin.rpy[0], origin.rpy[1], origin.rpy[2]);
      Transform3D::from_translation_rotation(translation, rotation)
  }
  ```

- [ ] **Implement forward kinematics**
  - [ ] Build transform chain from joint angles
  - [ ] Support revolute and prismatic joints
  - [ ] Cache computed transforms

- [ ] **Load mesh geometries**
  - [ ] Support STL files (common in URDF)
  - [ ] Reference Rerun's `re_renderer/src/importer/stl.rs` pattern
  - [ ] Convert to Makepad `Geometry`

### 3.2 Robot Arm 3D Renderer

- [ ] **Create `RobotArmViewer` Makepad widget**
  ```rust
  live_design!{
      RobotArmViewer = {{RobotArmViewer}} {
          width: Fill, height: Fill,

          draw_link: <DrawMesh> {
              // Mesh rendering with lighting
          }
          draw_joint: <DrawCube> {
              color: #4080ff
          }
          draw_end_effector: <DrawCube> {
              color: #ff4040
          }
          draw_axes: <DrawLines> {
              // Coordinate frame axes
          }
      }
  }
  ```

- [ ] **Port Rerun's mesh rendering approach**
  - [ ] Reference `re_renderer/src/renderer/mesh_renderer.rs`
  - [ ] Implement instanced rendering for multiple links
  - [ ] Add Phong lighting

- [ ] **Implement joint visualization**
  - [ ] Revolute: cylinder/torus
  - [ ] Prismatic: sliding box
  - [ ] Show joint axes

- [ ] **Add coordinate frames**
  - [ ] XYZ axes at each joint
  - [ ] Optional world frame grid

### 3.3 Orbit Camera (From Rerun Reference)

- [ ] **Create `OrbitCamera` component**
  ```rust
  pub struct OrbitCamera {
      target: Vec3,      // Look-at point
      distance: f32,     // Distance from target
      yaw: f32,          // Horizontal angle
      pitch: f32,        // Vertical angle
      fov: f32,          // Field of view
  }
  ```

- [ ] **Reference Rerun's camera implementation**
  - [ ] Study `re_view_spatial/src/visualizers/camera.rs`
  - [ ] Port orbit math to Makepad

- [ ] **Implement mouse controls**
  - [ ] Left drag: rotate (yaw/pitch)
  - [ ] Right drag: pan (move target)
  - [ ] Scroll: zoom (change distance)

- [ ] **Generate view/projection matrices**
  - [ ] `view_matrix() -> Mat4`
  - [ ] `projection_matrix(aspect: f32) -> Mat4`

### 3.4 Trajectory Visualization

- [ ] **Create `TrajectoryRenderer` component**
  ```rust
  pub struct TrajectoryRenderer {
      points: Vec<Vec3>,
      max_points: usize,  // Rolling window
      line_color: Vec4,
  }
  ```

- [ ] **Port Rerun's line strip rendering**
  - [ ] Reference `re_renderer/src/renderer/lines.rs`
  - [ ] Implement in Makepad shader DSL
  - [ ] Support line width and caps

- [ ] **Add ghost poses**
  - [ ] Render past robot poses with transparency
  - [ ] Configurable number of ghosts

---

## Phase 4: Episode Browser & Navigation (Week 7)

### 4.1 Episode List Widget

- [ ] **Create `EpisodeList` Makepad widget**
  ```rust
  live_design!{
      EpisodeList = {{EpisodeList}} {
          list: <FlatList> {
              item: <EpisodeListItem> {
                  // Episode row template
              }
          }
      }
  }
  ```

- [ ] **Display episode metadata**
  - [ ] Episode index
  - [ ] Duration/frame count
  - [ ] Task description (truncated)

- [ ] **Implement virtual scrolling**
  - [ ] Only render visible items
  - [ ] Handle datasets with 10K+ episodes

- [ ] **Selection handling**
  - [ ] Single-click to preview
  - [ ] Double-click to load
  - [ ] Emit `EpisodeSelected { index }` event

### 4.2 Task Filter

- [ ] **Create task filter dropdown**
  - [ ] List all unique tasks from `tasks.jsonl`
  - [ ] Multi-select support

- [ ] **Implement filtering logic**
  - [ ] Filter `episodes` list by selected `task_index`
  - [ ] Show filtered count

### 4.3 Search

- [ ] **Add search input**
  - [ ] Full-text search in task descriptions
  - [ ] Fuzzy matching optional

- [ ] **Highlight matching episodes**
  - [ ] Filter list to matches
  - [ ] Highlight search terms

### 4.4 Thumbnails

- [ ] **Generate episode thumbnails**
  - [ ] Extract first frame from video
  - [ ] Scale to thumbnail size (e.g., 120x90)
  - [ ] Cache on disk

- [ ] **Display thumbnails in list**
  - [ ] Lazy load as items scroll into view
  - [ ] Placeholder while loading

---

## Phase 5: Multi-Camera & Polish (Week 8)

### 5.1 Multi-Camera View

- [ ] **Create `MultiCameraView` container**
  ```rust
  live_design!{
      MultiCameraView = {{MultiCameraView}} {
          // Dynamic layout based on camera count
          camera_grid: <View> {
              flow: Down
              // Populated dynamically
          }
      }
  }
  ```

- [ ] **Support layout modes**
  - [ ] Grid (2x2, 3x3)
  - [ ] Horizontal strip
  - [ ] Single + thumbnails
  - [ ] Picture-in-picture

- [ ] **Synchronize all camera views**
  - [ ] All cameras show same timestamp
  - [ ] Handle different fps across cameras

### 5.2 Statistics Display

- [ ] **Add statistics panel**
  - [ ] Show `stats.json` values
  - [ ] Per-channel mean/std/min/max

- [ ] **Overlay on plots**
  - [ ] Mean line
  - [ ] ±1σ, ±2σ shading

### 5.3 Export Features

- [ ] **Export video clip**
  - [ ] Select time range
  - [ ] Re-encode with ffmpeg

- [ ] **Export data slice**
  - [ ] Export selected episode as Parquet
  - [ ] Export as CSV

- [ ] **Export screenshot**
  - [ ] Capture current view
  - [ ] Save as PNG

### 5.4 Keyboard Shortcuts

- [ ] **Implement keyboard handling**
  - [ ] `Space`: Play/Pause
  - [ ] `Left/Right Arrow`: Step frame
  - [ ] `Home/End`: Jump to start/end
  - [ ] `+/-`: Zoom timeline
  - [ ] `1-9`: Switch camera views

### 5.5 Hugging Face Hub Integration (Optional)

- [ ] **Implement Hub streaming**
  ```rust
  use hf_hub::api::sync::Api;

  pub async fn load_from_hub(repo_id: &str) -> LeRobotDataset {
      let api = Api::new()?;
      let repo = api.dataset(repo_id);
      // Stream files on demand
  }
  ```

- [ ] **Progress indicator for downloads**
  - [ ] Show download progress bar
  - [ ] Cache downloaded files locally

---

## Summary: Rerun Components Used

### Directly Reused (No Modification)

| Component | Crate | Usage |
|-----------|-------|-------|
| Data types | `re_sdk_types` | `Vec3D`, `Quaternion`, `Color`, `Position3D` |
| Storage | `re_chunk`, `re_chunk_store` | Arrow-based time-series storage |
| Database | `re_entity_db` | Query interface for timeline data |
| Queries | `re_query` | `LatestAtQuery`, `RangeQuery` |
| Timelines | `re_log_types` | `Timeline`, `TimeInt`, `EntityPath` |
| File I/O | `re_log_encoding` | `.rrd` file read/write |

### Algorithm References (Ported to Makepad)

| Component | Source | Target |
|-----------|--------|--------|
| Line rendering | `re_renderer/src/renderer/lines.rs` | `DrawLines` shader |
| Point rendering | `re_renderer/src/renderer/point_cloud.rs` | `DrawPointCloud` shader |
| Mesh rendering | `re_renderer/src/renderer/mesh_renderer.rs` | `DrawMesh` shader |
| Orbit camera | `re_view_spatial/src/visualizers/camera.rs` | `OrbitCamera` component |
| GLTF import | `re_renderer/src/importer/gltf.rs` | Mesh loading |

### Built Fresh for Makepad

| Component | Reason |
|-----------|--------|
| Video Player | Makepad texture integration |
| Timeline Widget | Makepad event handling |
| Episode Browser | Makepad list virtualization |
| All UI widgets | Makepad `live_design!` |

---

## Effort Summary

| Phase | Tasks | Estimated Effort |
|-------|-------|------------------|
| Phase 1: Data Layer | 15 tasks | 2 weeks |
| Phase 2: Core Viz | 18 tasks | 2 weeks |
| Phase 3: Robot 3D | 13 tasks | 2 weeks |
| Phase 4: Browser | 10 tasks | 1 week |
| Phase 5: Polish | 14 tasks | 1 week |
| **Total** | **70 tasks** | **8 weeks** |

### Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Rerun crate compatibility | Pin to specific commit, test early |
| FFmpeg integration complexity | Use `ffmpeg-next` (well-maintained binding) |
| URDF mesh loading | Fall back to primitives if mesh fails |
| Performance with large datasets | Virtual scrolling, lazy loading |

---

## Quick Start Checklist

```bash
# 1. Create project structure
cargo new --lib dorobot-data
cargo new --lib dorobot-bridge
cargo new --lib dorobot-app

# 2. Add dependencies to workspace Cargo.toml
# (see Section 1.1)

# 3. Verify Rerun crates compile
cargo check -p dorobot-bridge

# 4. Run test with sample LeRobot dataset
# Download: https://huggingface.co/datasets/lerobot/aloha_sim_insertion_human
cargo run -p dorobot-app -- --dataset ./aloha_sim_insertion_human
```
