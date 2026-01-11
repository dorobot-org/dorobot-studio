//! Shared robot state for Dora + Makepad communication
//!
//! This is the central data structure that Dora nodes write to
//! and the Makepad UI reads from on a timer.

use crate::dirty::{DirtyValue, DirtyVec};
use dorobot_types::*;
use std::sync::Arc;

/// Shared state between Dora dataflow and Makepad UI
///
/// # Usage Pattern
///
/// ```ignore
/// // Create shared state
/// let state = Arc::new(SharedRobotState::new());
///
/// // Dora node (producer thread)
/// let state_clone = state.clone();
/// std::thread::spawn(move || {
///     state_clone.point_cloud.set(new_cloud);
///     state_clone.robot_pose.set(new_pose);
/// });
///
/// // Makepad UI (consumer, on timer)
/// fn poll_state(&mut self, state: &SharedRobotState) {
///     if let Some(cloud) = state.point_cloud.read_if_dirty() {
///         self.update_point_cloud_view(cloud);
///     }
///     if let Some(pose) = state.robot_pose.read_if_dirty() {
///         self.update_robot_pose(pose);
///     }
/// }
/// ```
#[derive(Default)]
pub struct SharedRobotState {
    // Sensor data
    pub point_cloud: DirtyValue<Option<PointCloud>>,
    pub lidar_scan: DirtyValue<Option<LidarScan>>,
    pub camera_image: DirtyValue<Option<ImageFrame>>,
    pub depth_image: DirtyValue<Option<DepthFrame>>,

    // Robot state
    pub robot_state: DirtyValue<RobotState>,
    pub joint_state: DirtyValue<Option<JointState>>,

    // Perception results
    pub detected_objects: DirtyVec<BoundingBox3D>,

    // System status
    pub system_status: DirtyValue<SystemStatus>,

    // Logs and events
    pub logs: DirtyVec<LogEntry>,

    // Path/trajectory for visualization
    pub trajectory: DirtyVec<Pose3D>,
}

/// Log entry for system logging
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp_ns: u64,
    pub level: LogLevel,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl SharedRobotState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an Arc-wrapped instance for sharing between threads
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Log a message
    pub fn log(&self, level: LogLevel, source: impl Into<String>, message: impl Into<String>) {
        self.logs.push(LogEntry {
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            level,
            source: source.into(),
            message: message.into(),
        });
    }

    /// Add a pose to the trajectory history
    pub fn add_trajectory_point(&self, pose: Pose3D) {
        self.trajectory.push(pose);
    }

    /// Check if any data has been updated
    pub fn has_updates(&self) -> bool {
        self.point_cloud.is_dirty()
            || self.lidar_scan.is_dirty()
            || self.camera_image.is_dirty()
            || self.depth_image.is_dirty()
            || self.robot_state.is_dirty()
            || self.joint_state.is_dirty()
            || self.detected_objects.is_dirty()
            || self.system_status.is_dirty()
            || self.logs.is_dirty()
            || self.trajectory.is_dirty()
    }

    /// Clear all trajectory points
    pub fn clear_trajectory(&self) {
        self.trajectory.clear();
    }
}

/// Handle for UI to receive updates
pub struct StateReader {
    state: Arc<SharedRobotState>,
}

impl StateReader {
    pub fn new(state: Arc<SharedRobotState>) -> Self {
        Self { state }
    }

    /// Poll for point cloud updates
    pub fn poll_point_cloud(&self) -> Option<PointCloud> {
        self.state.point_cloud.read_if_dirty().flatten()
    }

    /// Poll for lidar scan updates
    pub fn poll_lidar_scan(&self) -> Option<LidarScan> {
        self.state.lidar_scan.read_if_dirty().flatten()
    }

    /// Poll for camera image updates
    pub fn poll_camera_image(&self) -> Option<ImageFrame> {
        self.state.camera_image.read_if_dirty().flatten()
    }

    /// Poll for depth image updates
    pub fn poll_depth_image(&self) -> Option<DepthFrame> {
        self.state.depth_image.read_if_dirty().flatten()
    }

    /// Poll for robot state updates
    pub fn poll_robot_state(&self) -> Option<RobotState> {
        self.state.robot_state.read_if_dirty()
    }

    /// Poll for joint state updates
    pub fn poll_joint_state(&self) -> Option<JointState> {
        self.state.joint_state.read_if_dirty().flatten()
    }

    /// Poll for detected objects
    pub fn poll_detected_objects(&self) -> Option<Vec<BoundingBox3D>> {
        self.state.detected_objects.read_if_dirty()
    }

    /// Poll for system status updates
    pub fn poll_system_status(&self) -> Option<SystemStatus> {
        self.state.system_status.read_if_dirty()
    }

    /// Poll for new log entries
    pub fn poll_logs(&self) -> Option<Vec<LogEntry>> {
        self.state.logs.drain_if_dirty()
    }

    /// Poll for trajectory updates
    pub fn poll_trajectory(&self) -> Option<Vec<Pose3D>> {
        self.state.trajectory.read_if_dirty()
    }

    /// Check if any updates are pending
    pub fn has_updates(&self) -> bool {
        self.state.has_updates()
    }
}

/// Handle for Dora nodes to write updates
pub struct StateWriter {
    state: Arc<SharedRobotState>,
}

impl StateWriter {
    pub fn new(state: Arc<SharedRobotState>) -> Self {
        Self { state }
    }

    pub fn set_point_cloud(&self, cloud: PointCloud) {
        self.state.point_cloud.set(Some(cloud));
    }

    pub fn set_lidar_scan(&self, scan: LidarScan) {
        self.state.lidar_scan.set(Some(scan));
    }

    pub fn set_camera_image(&self, image: ImageFrame) {
        self.state.camera_image.set(Some(image));
    }

    pub fn set_depth_image(&self, depth: DepthFrame) {
        self.state.depth_image.set(Some(depth));
    }

    pub fn set_robot_state(&self, state: RobotState) {
        // Also add to trajectory
        self.state.add_trajectory_point(state.pose);
        self.state.robot_state.set(state);
    }

    pub fn set_joint_state(&self, joints: JointState) {
        self.state.joint_state.set(Some(joints));
    }

    pub fn add_detected_object(&self, obj: BoundingBox3D) {
        self.state.detected_objects.push(obj);
    }

    pub fn set_detected_objects(&self, objects: Vec<BoundingBox3D>) {
        self.state.detected_objects.extend(objects);
    }

    pub fn set_system_status(&self, status: SystemStatus) {
        self.state.system_status.set(status);
    }

    pub fn log(&self, level: LogLevel, source: &str, message: &str) {
        self.state.log(level, source, message);
    }

    pub fn log_info(&self, source: &str, message: &str) {
        self.log(LogLevel::Info, source, message);
    }

    pub fn log_warning(&self, source: &str, message: &str) {
        self.log(LogLevel::Warning, source, message);
    }

    pub fn log_error(&self, source: &str, message: &str) {
        self.log(LogLevel::Error, source, message);
    }
}
