//! Shared data types for dorobot robotics visualization
//!
//! These types are used across the Dora dataflow, Makepad UI, and Rerun logging.

use serde::{Deserialize, Serialize};

/// 3D point with optional color and intensity
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub intensity: f32,
}

impl Point3D {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self {
            x,
            y,
            z,
            r: 255,
            g: 255,
            b: 255,
            intensity: 1.0,
        }
    }

    pub fn with_color(mut self, r: u8, g: u8, b: u8) -> Self {
        self.r = r;
        self.g = g;
        self.b = b;
        self
    }

    pub fn with_intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    pub fn position(&self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn color(&self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
}

/// Point cloud data from LiDAR or depth sensors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointCloud {
    pub points: Vec<Point3D>,
    pub timestamp_ns: u64,
    pub frame_id: String,
}

impl PointCloud {
    pub fn new(frame_id: impl Into<String>) -> Self {
        Self {
            points: Vec::new(),
            timestamp_ns: 0,
            frame_id: frame_id.into(),
        }
    }

    pub fn with_points(mut self, points: Vec<Point3D>) -> Self {
        self.points = points;
        self
    }

    pub fn with_timestamp(mut self, timestamp_ns: u64) -> Self {
        self.timestamp_ns = timestamp_ns;
        self
    }

    pub fn positions(&self) -> Vec<[f32; 3]> {
        self.points.iter().map(|p| p.position()).collect()
    }

    pub fn colors(&self) -> Vec<[u8; 3]> {
        self.points.iter().map(|p| p.color()).collect()
    }
}

/// 2D LiDAR scan data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LidarScan {
    pub ranges: Vec<f32>,
    pub intensities: Vec<f32>,
    pub angle_min: f32,
    pub angle_max: f32,
    pub angle_increment: f32,
    pub range_min: f32,
    pub range_max: f32,
    pub timestamp_ns: u64,
}

impl LidarScan {
    /// Convert 2D scan to 3D points (assuming z=0 plane)
    pub fn to_points(&self) -> Vec<Point3D> {
        let mut points = Vec::with_capacity(self.ranges.len());
        let mut angle = self.angle_min;

        for (i, &range) in self.ranges.iter().enumerate() {
            if range >= self.range_min && range <= self.range_max {
                let x = range * angle.cos();
                let y = range * angle.sin();
                let intensity = self.intensities.get(i).copied().unwrap_or(1.0);
                points.push(Point3D::new(x, y, 0.0).with_intensity(intensity));
            }
            angle += self.angle_increment;
        }
        points
    }
}

/// 3D pose (position + orientation)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Pose3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub qx: f32,
    pub qy: f32,
    pub qz: f32,
    pub qw: f32,
}

impl Pose3D {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self {
            x,
            y,
            z,
            qx: 0.0,
            qy: 0.0,
            qz: 0.0,
            qw: 1.0,
        }
    }

    pub fn with_quaternion(mut self, qx: f32, qy: f32, qz: f32, qw: f32) -> Self {
        self.qx = qx;
        self.qy = qy;
        self.qz = qz;
        self.qw = qw;
        self
    }

    pub fn translation(&self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn quaternion(&self) -> [f32; 4] {
        [self.qx, self.qy, self.qz, self.qw]
    }

    /// Convert quaternion to yaw angle (rotation around Z axis)
    pub fn yaw(&self) -> f32 {
        let siny_cosp = 2.0 * (self.qw * self.qz + self.qx * self.qy);
        let cosy_cosp = 1.0 - 2.0 * (self.qy * self.qy + self.qz * self.qz);
        siny_cosp.atan2(cosy_cosp)
    }
}

/// Robot state combining pose and velocity
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RobotState {
    pub pose: Pose3D,
    pub linear_velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub timestamp_ns: u64,
}

/// Camera image frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub encoding: ImageEncoding,
    pub timestamp_ns: u64,
    pub frame_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageEncoding {
    Rgb8,
    Rgba8,
    Bgr8,
    Bgra8,
    Mono8,
    Mono16,
    Depth16,
    Depth32F,
}

impl ImageFrame {
    pub fn rgb8(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            data,
            encoding: ImageEncoding::Rgb8,
            timestamp_ns: 0,
            frame_id: String::new(),
        }
    }

    pub fn depth16(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            data,
            encoding: ImageEncoding::Depth16,
            timestamp_ns: 0,
            frame_id: String::new(),
        }
    }
}

/// Depth image with metric scale
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<f32>,
    pub min_depth: f32,
    pub max_depth: f32,
    pub timestamp_ns: u64,
    pub frame_id: String,
}

/// Joint state for robot arm or mobile base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointState {
    pub names: Vec<String>,
    pub positions: Vec<f32>,
    pub velocities: Vec<f32>,
    pub efforts: Vec<f32>,
    pub timestamp_ns: u64,
}

/// Bounding box for object detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox3D {
    pub center: Pose3D,
    pub size: [f32; 3],
    pub label: String,
    pub confidence: f32,
    pub track_id: Option<u32>,
}

/// Collection of detected objects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedObjects {
    pub objects: Vec<BoundingBox3D>,
    pub timestamp_ns: u64,
    pub frame_id: String,
}

/// Sensor status for monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensorStatus {
    Ok,
    Warning,
    Error,
    Disconnected,
}

/// Overall robot system status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub lidar_status: SensorStatus,
    pub camera_status: SensorStatus,
    pub imu_status: SensorStatus,
    pub motor_status: SensorStatus,
    pub battery_percent: f32,
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub timestamp_ns: u64,
}

impl Default for SystemStatus {
    fn default() -> Self {
        Self {
            lidar_status: SensorStatus::Disconnected,
            camera_status: SensorStatus::Disconnected,
            imu_status: SensorStatus::Disconnected,
            motor_status: SensorStatus::Disconnected,
            battery_percent: 0.0,
            cpu_usage: 0.0,
            memory_usage: 0.0,
            timestamp_ns: 0,
        }
    }
}

/// Message types for Dora communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RobotMessage {
    PointCloud(PointCloud),
    LidarScan(LidarScan),
    Image(ImageFrame),
    Depth(DepthFrame),
    Pose(RobotState),
    Joints(JointState),
    Detections(DetectedObjects),
    Status(SystemStatus),
}
