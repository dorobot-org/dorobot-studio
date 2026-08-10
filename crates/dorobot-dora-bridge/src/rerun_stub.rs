//! Stand-in for [`super::rerun_logger`] when the `rerun` feature is off.
//!
//! Rerun is an optional viewer, but it is also by far the heaviest dependency
//! in the tree — enabling it pulls and compiles the whole rerun workspace. The
//! player does not need it, so it is off by default and this keeps the callers
//! compiling unchanged.
//!
//! Every constructor fails rather than returning a logger that silently drops
//! what it is given: a build without rerun should say so, not look connected.

use anyhow::{bail, Result};

use dorobot_types::*;

/// Same surface as the real logger, minus the viewer.
pub struct RerunLogger {
    _private: (),
}

const DISABLED: &str = "this build has no rerun support; rebuild with --features rerun";

impl RerunLogger {
    pub fn spawn(_app_id: &str) -> Result<Self> {
        bail!(DISABLED)
    }
    pub fn connect(_app_id: &str, _addr: &str) -> Result<Self> {
        bail!(DISABLED)
    }
    pub fn save(_app_id: &str, _path: &str) -> Result<Self> {
        bail!(DISABLED)
    }
    pub fn connect_default(_app_id: &str) -> Result<Self> {
        bail!(DISABLED)
    }

    // Unreachable in practice — no constructor hands one out — but present so
    // callers type-check against the same API either way.
    pub fn set_time_nanos(&self, _timeline: &str, _nanos: i64) {}
    pub fn set_time_sequence(&self, _timeline: &str, _sequence: i64) {}
    pub fn log_point_cloud(&self, _cloud: &PointCloud) -> Result<()> {
        bail!(DISABLED)
    }
    pub fn log_lidar_scan(&self, _scan: &LidarScan, _entity_path: &str) -> Result<()> {
        bail!(DISABLED)
    }
    pub fn log_robot_pose(&self, _state: &RobotState, _entity_path: &str) -> Result<()> {
        bail!(DISABLED)
    }
    pub fn log_trajectory(&self, _poses: &[Pose3D], _entity_path: &str) -> Result<()> {
        bail!(DISABLED)
    }
    pub fn log_image(&self, _entity_path: &str, _image: &ImageFrame) -> Result<()> {
        bail!(DISABLED)
    }
    pub fn log_depth(&self, _entity_path: &str, _depth: &DepthFrame) -> Result<()> {
        bail!(DISABLED)
    }
    pub fn log_detections(&self, _detections: &DetectedObjects, _entity_path: &str) -> Result<()> {
        bail!(DISABLED)
    }
    pub fn log_joints(&self, _joints: &JointState, _entity_path: &str) -> Result<()> {
        bail!(DISABLED)
    }
    pub fn log_status(&self, _status: &SystemStatus, _entity_path: &str) -> Result<()> {
        bail!(DISABLED)
    }
    pub fn log_text(&self, _entity_path: &str, _message: &str) -> Result<()> {
        bail!(DISABLED)
    }
}
