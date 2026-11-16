# DoRobot Studio

Adora Nano simulation, SO-100/SO-101 LeRobot data collection and dataset review
with the **Makepad** UI framework.

**Start here: [Detailed user guide](docs/USER_GUIDE.md)** — installation, first
simulation, controller mappings, camera layouts, recording/replay, real hardware
calibration, Hub transfers and troubleshooting.

Current source builds include a compact **EN / CN** button at the bottom of the
sidebar. It shows the current language; click to switch **English / 简体中文**.
It remembers your choice without stopping
recording or playback. See [language support](docs/I18N.md).

The standalone RC uses Rust for Studio, native Dora nodes, Nexus simulation,
controller/serial control, three-camera recording and dataset transfer. It
packages no Python runtime or bindings. See the [native runtime](runtime/README.md)
and [macOS release guide](tools/release/README.md). FFmpeg remains a native codec
dependency; Python is used only by development/build and acceptance scripts.

For the current capture workflow, build requirements, verification results and remaining hardware checks, see [the functional review and setup guide](docs/FUNCTIONAL_REVIEW.md). Production mode is the default; use `--demo` for generated fixtures.

This is a standalone Git repository and Cargo workspace. Open this directory
as the project root. The application builds without a sibling Nexus checkout;
its shared UI crate is included in `crates/dorobot-ux`.

### iOS appearance and Makepad JIT

The desktop app uses OctoSense's current Makepad iOS light/dark style sheets,
with blue accents, grouped surfaces and rounded controls. Start it through the
Relax Rust JIT:

```sh
./tools/run_jit.sh
./tools/run_jit.sh --check
./tools/run_jit.sh --dark
```

The first launch builds a pinned runner in your user cache; later launches
reuse it. Rust, Cargo, Git, Python 3 and a platform C compiler are required.
The default JIT launch omits FFmpeg. With FFmpeg installed, use
`./tools/run_jit.sh --video` for video decoding. See [JIT setup and validation](docs/JIT.md)
for runtime versions, source editing and tested limits.

### Adora Nano preview

Nano's Feetech mobile base is also a supported robot in **Robot setup → Real robot → Adora Nano**. It has a simulated-bus test mode and an explicit USB connection
for PS4/PS5 control through Dora. See the [driver guide](robots/adora_nano_feetech/README.md)
for setup, supported capabilities and validation. This driver covers the base;
arm and camera hardware drivers are not included in the supplied package.

The Nano URDF and meshes are included in `data/adora_nano`. From this project:

```sh
./tools/run_nano_preview.sh
```

The JIT launcher creates three synthetic joint-motion clips if needed and opens
them in the Play screen. Space plays/pauses; Home returns to the start. These
clips are kinematic previews, not captured data or commands for hardware.
This launcher builds without FFmpeg/video decoding; the standard build below
includes video playback. Model provenance and limits are recorded alongside
the assets.

### Live streaming

For the built-in scene, choose **Robot setup → Simulation → Start simulation**.
Studio opens **Operate** when physics and all three cameras are ready. Use
**Record → Stop → Save → Replay** to capture and review a take.

For an existing external Dora source, choose **Robot setup → More → External
stream**, enter its local socket path, and click **Connect**. The three Nano cameras, robot
pose and joint signals share one sample clock. Studio displays the measured
preview rate, reports stale input, and reconnects automatically. Disconnecting
the viewer leaves the source recording running.

Operate and Replay retain separate panel layouts and the replay playhead. When a
source finishes, **Open recording** opens its finalized dataset in Replay.
The [native runtime guide](runtime/README.md) describes the built-in Dora producer.
The [Nano room integration guide](tools/aisle-validation/README.md#mobile-nano-in-a-living-room)
retains the original AISLE validation experiments. The recording workflow currently
captures at 5 Hz; the Live mode does not imply 30 fps throughput.

For a Codex-operated Nexus simulation that picks three floor cubes and places
them on a coffee table while streaming into Studio, see the
[three-cube live demo](tools/nano-three-cubes/README.md).

## Overview

Dorobot provides tools for visualizing robotics datasets, with a focus on **LeRobot** format datasets:

- **dorobot-flex**: DoRobot Studio application with capture, playback, and dataset review
- **crates/**: Shared UI, data types, and the optional Dora bridge

## dorobot-flex

The primary application for LeRobot dataset visualization featuring:

- **Multi-camera video playback** with synchronized scrubbing
- **Live Dora streaming** with connection controls, measured preview rate and automatic reconnection
- **3D robot visualization** from URDF models
- **Time series plots** for joint states and sensor data
- **Flexible panel layout** with drag rearrangement, resizing, maximize/restore and local layout recovery
- **Additional native Replay window** sharing the episode and transport, with its own layout
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

Drag a panel title to rearrange its content. Drag a divider to resize the grid;
use the panel's expand button to maximize it, and Escape to restore the grid.
**Replay → Panels** reopens closed views, changes the arrangement, resets the
layout, or opens a separate native **Replay window**. Both windows share the
selected episode and playback; their panel layouts are independent. The **Speed**
control cycles through 0.25×, 0.5×, 1× and 2× in both windows and survives restart. Closing
the extra window preserves the main task. Layouts and navigation width are
saved locally. Hardware uses the same panel behavior and has **Restore panels**.

Replay provides 3D, the available camera views and tracking error, including
all three cameras in a captured Nano dataset.
Camera panels explain missing media. Record and Hardware keep their controls
visible with setup guidance; recording stays disabled until a profile exists.
See the [functional UX judge loop](docs/ux/functional-review.md) for the workflow
checks, score, and unverified hardware limits.

## Project Structure

```
dorobot-studio/
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
├── data/                       # Local robot descriptions and meshes
│   └── adora_nano/             # Self-contained Nano model snapshot
├── crates/
│   ├── dorobot-ux/              # Shared visual language
│   ├── dorobot-types/           # Shared data types
│   └── dorobot-dora-bridge/     # Dora dataflow bridge (legacy)
└── examples/                    # URDF and Rerun examples
```

## Quick Start

### Prerequisites

- A current stable Rust toolchain
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
cd dorobot-studio
cargo run --release -p dorobot-flex
```

### Loading a Dataset

1. Open **Library → Open folder…**
2. Choose a LeRobot dataset directory (containing `meta/info.json`), or a parent library folder
3. Open a dataset and use **Episodes** to select an episode
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
