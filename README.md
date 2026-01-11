# Dorobot

Robotics dataset visualization with **Makepad** UI and **Rerun** 3D viewer integration.

## Overview

Dorobot provides a unified dashboard for robotics visualization:

- **Makepad UI**: Custom 2D dashboard with status panels, sensor gauges, minimap, and logs
- **Rerun Integration**: 3D point cloud, trajectory, and detection visualization
- **Dora Dataflow**: Real-time sensor data streaming from robotics pipelines

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Dora Dataflow                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐    │
│  │  LiDAR   │  │  Camera  │  │   Pose   │  │   Object     │    │
│  │  Driver  │  │  Driver  │  │ Estimator│  │  Detector    │    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬───────┘    │
│       │             │             │               │             │
│       └─────────────┴──────┬──────┴───────────────┘             │
│                            │                                    │
│                   ┌────────▼────────┐                           │
│                   │  Dorobot Bridge │                           │
│                   └────────┬────────┘                           │
└────────────────────────────┼────────────────────────────────────┘
                             │
              ┌──────────────┴──────────────┐
              │                             │
              ▼                             ▼
    ┌─────────────────┐           ┌─────────────────┐
    │ SharedRobotState│           │   RerunLogger   │
    │  (dirty tracking)│           │   (3D viewer)   │
    └────────┬────────┘           └─────────────────┘
             │
             ▼
    ┌─────────────────┐
    │   Makepad UI    │
    │  (50ms polling) │
    │                 │
    │ ┌─────────────┐ │
    │ │Status Panel │ │
    │ │Sensor Gauges│ │
    │ │  Minimap    │ │
    │ │  Log Panel  │ │
    │ └─────────────┘ │
    └─────────────────┘
```

## Project Structure

```
dorobot/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── dorobot-types/            # Shared data types
│   │   └── src/lib.rs            # Point3D, PointCloud, Pose3D, etc.
│   │
│   ├── dorobot-dora-bridge/      # Dora + Rerun integration
│   │   └── src/
│   │       ├── dirty.rs          # DirtyValue, DirtyVec
│   │       ├── shared_state.rs   # SharedRobotState
│   │       └── rerun_logger.rs   # RerunLogger wrapper
│   │
│   └── dorobot-app/              # Makepad UI application
│       └── src/
│           ├── app.rs            # Main application
│           └── widgets/          # Custom widgets
│               ├── status_panel.rs
│               ├── sensor_gauge.rs
│               ├── log_panel.rs
│               └── minimap.rs
│
└── dataflows/                    # Dora dataflow definitions
    ├── robot_viz.yaml            # Example dataflow
    └── nodes/                    # Python node implementations
        ├── lidar_driver.py
        └── pose_estimator.py
```

## Quick Start

### 1. Build the application

```bash
cd ~/home/dorobot
cargo build --release -p dorobot-app
```

### 2. Run the Makepad UI

```bash
cargo run --release -p dorobot-app
```

The UI will start with simulated demo data. Click "Launch Rerun" to open the 3D viewer.

**Note**: Requires the local Rerun repository at `~/home/rerun` (uses path dependency).

### 3. Run with Dora dataflow (optional)

```bash
# Install dora
pip install dora-rs

# Start the dataflow
dora start dataflows/robot_viz.yaml

# Run the UI connected to dataflow
cargo run --release -p dorobot-app -- --dora
```

## Integration Patterns

### Pattern 1: Rerun as External Viewer

```rust
use dorobot_dora_bridge::RerunLogger;
use dorobot_types::PointCloud;

// Launch Rerun viewer
let logger = RerunLogger::spawn("my_robot")?;

// Log sensor data
logger.log_point_cloud(&point_cloud)?;
logger.log_robot_pose(&robot_state, "robot/base")?;
```

### Pattern 2: SharedRobotState for UI Updates

```rust
use dorobot_dora_bridge::{SharedRobotState, StateReader, StateWriter};

// Create shared state
let state = SharedRobotState::new_shared();

// Dora node (producer)
let writer = StateWriter::new(state.clone());
writer.set_point_cloud(cloud);
writer.set_robot_state(pose);

// Makepad UI (consumer, on timer)
let reader = StateReader::new(state.clone());
if let Some(cloud) = reader.poll_point_cloud() {
    update_visualization(cloud);
}
```

### Pattern 3: Hybrid (Makepad + Rerun)

```rust
// Log to both Makepad UI and Rerun
fn handle_sensor_data(&mut self, data: SensorData) {
    // Update Makepad UI (2D gauges, status)
    self.shared_state.system_status.set(data.status);

    // Log to Rerun (3D visualization)
    self.rerun_logger.log_point_cloud(&data.cloud)?;
}
```

## Data Types

| Type | Description | Rerun Visualization |
|------|-------------|---------------------|
| `PointCloud` | 3D point cloud with colors | `Points3D` |
| `LidarScan` | 2D laser scan | `Points3D` (converted) |
| `ImageFrame` | RGB/Depth images | `Image`, `DepthImage` |
| `Pose3D` | Position + quaternion | `Transform3D` |
| `RobotState` | Pose + velocity | `Transform3D` + arrows |
| `BoundingBox3D` | Object detection | `Boxes3D` |
| `JointState` | Robot arm joints | `Scalar` (per joint) |

## Dependencies

- **makepad-widgets**: GPU-accelerated UI framework
- **rerun**: Time-aware multimodal visualization
- **dora-node-api**: Robotics dataflow framework
- **parking_lot**: Fast synchronization primitives

## License

MIT
