# Dorobot

Robotics dataset visualization with **Makepad** UI framework.

## Overview

Dorobot provides tools for visualizing robotics datasets, with a focus on **LeRobot** format datasets:

- **dorobot-flex**: Main application with flexible panel layout for dataset visualization
- **dorobot-studio**: Legacy application with Rerun integration

## dorobot-flex

The primary application for LeRobot dataset visualization featuring:

- **Multi-camera video playback** with synchronized scrubbing
- **3D robot visualization** from URDF models
- **Time series plots** for joint states and sensor data
- **Flexible panel layout** with drag-and-drop rearrangement
- **Episode browser** with hierarchical task grouping

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     dorobot-flex Application                     │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌──────────────────────────────────────────┐  │
│  │   Sidebar   │  │              Panel Grid                   │  │
│  │             │  │  ┌────────────┬────────────┐              │  │
│  │ - Dataset   │  │  │ VideoPlayer│ RobotView  │  ← Drag &    │  │
│  │   Browser   │  │  │ (cam_high) │   (URDF)   │    Drop      │  │
│  │ - Episode   │  │  ├────────────┼────────────┤    Panels    │  │
│  │   List      │  │  │ VideoPlayer│ VideoPlayer│              │  │
│  │ - Episode   │  │  │ (cam_low)  │ (cam_wrist)│              │  │
│  │   Info      │  │  └────────────┴────────────┘              │  │
│  └─────────────┘  └──────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  Playback Controls  │  Timeline  │  Time Series Plot       ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

### Panel Drag-and-Drop

Panels can be rearranged by dragging their title bars. The content follows the panel:
- Video panels swap their video sources
- Robot view can be moved to any panel position
- Panel visibility can be toggled via the layout menu

## Project Structure

```
dorobot/
├── Cargo.toml                    # Workspace root
├── dorobot-flex/                 # Main application
│   ├── src/
│   │   ├── app.rs               # Application logic + drag-drop handling
│   │   ├── app_data.rs          # Panel registry + shared state
│   │   ├── data/
│   │   │   └── lerobot_dataset.rs  # LeRobot format parser
│   │   └── widgets/
│   │       ├── video_player.rs  # FFmpeg video decoder
│   │       ├── robot_viewer.rs  # URDF 3D renderer
│   │       └── episode_list.rs  # Hierarchical episode browser
│   └── *.md                     # Development documentation
│
├── dorobot-studio/              # Legacy app (Rerun integration)
├── crates/
│   ├── dorobot-types/           # Shared data types
│   └── dorobot-dora-bridge/     # Dora dataflow bridge (legacy)
└── examples/                    # URDF and Rerun examples
```

## Quick Start

### Prerequisites

- Rust toolchain (1.75+)
- FFmpeg development libraries (for video decoding)

On macOS:
```bash
brew install ffmpeg
```

On Ubuntu:
```bash
sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev
```

### Build and Run

```bash
cd ~/home/dorobot
cargo run --release -p dorobot-flex
```

### Loading a Dataset

1. Click **Open Dataset** in the sidebar
2. Navigate to a LeRobot dataset directory (containing `meta/info.json`)
3. Select an episode from the episode list
4. Use playback controls or drag the timeline to scrub

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Space | Play/Pause |
| Left/Right | Step frame |
| Home/End | Jump to start/end |

## LeRobot Dataset Format

dorobot-flex expects the standard LeRobot dataset structure:

```
dataset/
├── meta/
│   ├── info.json           # Dataset metadata (fps, robot_type, etc.)
│   ├── episodes.jsonl      # Episode index
│   └── tasks.jsonl         # Task descriptions (optional)
├── data/
│   └── chunk-000/
│       └── episode_000000.parquet  # Frame data
└── videos/
    └── chunk-000/
        ├── observation.images.cam_high/
        │   └── episode_000000.mp4
        └── observation.images.cam_low/
            └── episode_000000.mp4
```

## Dependencies

- **makepad-widgets**: GPU-accelerated UI framework
- **makepad-app-shell**: Flexible panel layout shell
- **makepad-urdf-player**: URDF model loader and renderer
- **ffmpeg-next**: Video decoding
- **parquet/arrow**: Data file parsing

## License

MIT
