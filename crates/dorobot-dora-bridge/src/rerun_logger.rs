//! Rerun visualization logger
//!
//! Provides integration with Rerun for 3D robotics visualization.
//! Can run alongside the Makepad UI or as the primary visualizer.

use anyhow::Result;
use dorobot_types::*;
use rerun::{RecordingStream, RecordingStreamBuilder};

/// Rerun visualization logger
///
/// # Usage
///
/// ```ignore
/// // Create logger with spawned viewer
/// let logger = RerunLogger::spawn("my_robot")?;
///
/// // Or connect to existing viewer
/// let logger = RerunLogger::connect("my_robot", "127.0.0.1:9876")?;
///
/// // Log sensor data
/// logger.log_point_cloud(&point_cloud)?;
/// logger.log_robot_pose(&robot_state)?;
/// logger.log_image("camera/rgb", &image)?;
/// ```
pub struct RerunLogger {
    rec: RecordingStream,
}

impl RerunLogger {
    /// Create a new logger that spawns a native Rerun viewer
    pub fn spawn(app_id: &str) -> Result<Self> {
        let rec = RecordingStreamBuilder::new(app_id).spawn()?;
        Ok(Self { rec })
    }

    /// Create a new logger that connects to an existing viewer via gRPC
    pub fn connect(app_id: &str, addr: &str) -> Result<Self> {
        let rec = RecordingStreamBuilder::new(app_id).connect_grpc_opts(addr)?;
        Ok(Self { rec })
    }

    /// Create a new logger that saves to a file
    pub fn save(app_id: &str, path: &str) -> Result<Self> {
        let rec = RecordingStreamBuilder::new(app_id).save(path)?;
        Ok(Self { rec })
    }

    /// Create a new logger using the default gRPC connection
    pub fn connect_default(app_id: &str) -> Result<Self> {
        let rec = RecordingStreamBuilder::new(app_id).connect_grpc()?;
        Ok(Self { rec })
    }

    /// Get the underlying recording stream for advanced usage
    pub fn stream(&self) -> &RecordingStream {
        &self.rec
    }

    /// Set the current time for subsequent logs (nanoseconds since epoch)
    pub fn set_time_nanos(&self, timeline: &str, nanos: i64) {
        self.rec.set_timestamp_nanos_since_epoch(timeline, nanos);
    }

    /// Set the current time using sequence number
    pub fn set_time_sequence(&self, timeline: &str, sequence: i64) {
        self.rec.set_time_sequence(timeline, sequence);
    }

    /// Log a point cloud
    pub fn log_point_cloud(&self, cloud: &PointCloud) -> Result<()> {
        let entity_path = format!("sensors/{}", cloud.frame_id);

        self.set_time_nanos("sensor_time", cloud.timestamp_ns as i64);

        let positions: Vec<[f32; 3]> = cloud.positions();
        let colors: Vec<[u8; 3]> = cloud.colors();

        self.rec.log(
            entity_path,
            &rerun::Points3D::new(positions).with_colors(colors),
        )?;

        Ok(())
    }

    /// Log a 2D lidar scan as 3D points
    pub fn log_lidar_scan(&self, scan: &LidarScan, entity_path: &str) -> Result<()> {
        self.set_time_nanos("sensor_time", scan.timestamp_ns as i64);

        let points = scan.to_points();
        let positions: Vec<[f32; 3]> = points.iter().map(|p| p.position()).collect();

        // Color by intensity
        let colors: Vec<[u8; 3]> = points
            .iter()
            .map(|p| {
                let v = (p.intensity * 255.0) as u8;
                [v, v, 255] // Blue-tinted intensity
            })
            .collect();

        self.rec.log(
            entity_path,
            &rerun::Points3D::new(positions)
                .with_colors(colors)
                .with_radii([0.02]),
        )?;

        Ok(())
    }

    /// Log robot pose as a transform
    pub fn log_robot_pose(&self, state: &RobotState, entity_path: &str) -> Result<()> {
        self.set_time_nanos("sensor_time", state.timestamp_ns as i64);

        let [qx, qy, qz, qw] = state.pose.quaternion();

        self.rec.log(
            entity_path,
            &rerun::Transform3D::from_translation_rotation(
                state.pose.translation(),
                rerun::Quaternion::from_xyzw([qx, qy, qz, qw]),
            ),
        )?;

        // Also log coordinate axes for visualization
        self.rec.log(
            format!("{}/axes", entity_path),
            &rerun::Arrows3D::from_vectors([[0.5, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 0.5]])
                .with_colors([[255, 0, 0], [0, 255, 0], [0, 0, 255]]),
        )?;

        Ok(())
    }

    /// Log robot trajectory as a line strip
    pub fn log_trajectory(&self, poses: &[Pose3D], entity_path: &str) -> Result<()> {
        if poses.is_empty() {
            return Ok(());
        }

        let positions: Vec<[f32; 3]> = poses.iter().map(|p| p.translation()).collect();

        self.rec.log(
            entity_path,
            &rerun::LineStrips3D::new([positions]).with_colors([[0, 255, 128]]),
        )?;

        Ok(())
    }

    /// Log an RGB image
    pub fn log_image(&self, entity_path: &str, image: &ImageFrame) -> Result<()> {
        self.set_time_nanos("sensor_time", image.timestamp_ns as i64);

        match image.encoding {
            ImageEncoding::Rgb8 => {
                self.rec.log(
                    entity_path,
                    &rerun::Image::from_rgb24(
                        image.data.as_slice(),
                        [image.width, image.height],
                    ),
                )?;
            }
            ImageEncoding::Rgba8 => {
                self.rec.log(
                    entity_path,
                    &rerun::Image::from_rgba32(
                        image.data.as_slice(),
                        [image.width, image.height],
                    ),
                )?;
            }
            ImageEncoding::Mono8 => {
                self.rec.log(
                    entity_path,
                    &rerun::Image::from_l8(
                        image.data.as_slice(),
                        [image.width, image.height],
                    ),
                )?;
            }
            _ => {
                log::warn!("Unsupported image encoding: {:?}", image.encoding);
            }
        }

        Ok(())
    }

    /// Log a depth image
    pub fn log_depth(&self, entity_path: &str, depth: &DepthFrame) -> Result<()> {
        self.set_time_nanos("sensor_time", depth.timestamp_ns as i64);

        // Convert f32 depth values to bytes for U16 format
        // Scale from meters to millimeters and clamp to u16 range
        let depth_u16: Vec<u8> = depth
            .data
            .iter()
            .flat_map(|&d| {
                let mm = (d * 1000.0).clamp(0.0, 65535.0) as u16;
                mm.to_le_bytes()
            })
            .collect();

        self.rec.log(
            entity_path,
            &rerun::DepthImage::from_gray16(
                depth_u16,
                [depth.width, depth.height],
            ),
        )?;

        Ok(())
    }

    /// Log 3D bounding boxes for detected objects
    pub fn log_detections(&self, detections: &DetectedObjects, entity_path: &str) -> Result<()> {
        self.set_time_nanos("sensor_time", detections.timestamp_ns as i64);

        if detections.objects.is_empty() {
            return Ok(());
        }

        let centers: Vec<[f32; 3]> = detections
            .objects
            .iter()
            .map(|o| o.center.translation())
            .collect();

        let sizes: Vec<[f32; 3]> = detections.objects.iter().map(|o| o.size).collect();

        let labels: Vec<&str> = detections.objects.iter().map(|o| o.label.as_str()).collect();

        // Color by confidence
        let colors: Vec<[u8; 3]> = detections
            .objects
            .iter()
            .map(|o| {
                let g = (o.confidence * 255.0) as u8;
                [255 - g, g, 0]
            })
            .collect();

        self.rec.log(
            entity_path,
            &rerun::Boxes3D::from_centers_and_sizes(centers, sizes)
                .with_colors(colors)
                .with_labels(labels),
        )?;

        Ok(())
    }

    /// Log joint states as scalars
    pub fn log_joints(&self, joints: &JointState, entity_path: &str) -> Result<()> {
        self.set_time_nanos("sensor_time", joints.timestamp_ns as i64);

        // Log positions as scalars
        for (name, &pos) in joints.names.iter().zip(&joints.positions) {
            self.rec.log(
                format!("{}/{}", entity_path, name),
                &rerun::Scalars::new([pos as f64]),
            )?;
        }

        Ok(())
    }

    /// Log system status as text
    pub fn log_status(&self, status: &SystemStatus, entity_path: &str) -> Result<()> {
        self.set_time_nanos("sensor_time", status.timestamp_ns as i64);

        let status_text = format!(
            "LiDAR: {:?} | Camera: {:?} | IMU: {:?} | Motor: {:?}\nBattery: {:.0}% | CPU: {:.1}% | Memory: {:.1}%",
            status.lidar_status,
            status.camera_status,
            status.imu_status,
            status.motor_status,
            status.battery_percent,
            status.cpu_usage,
            status.memory_usage
        );

        self.rec.log(entity_path, &rerun::TextLog::new(status_text))?;

        Ok(())
    }

    /// Log a text message
    pub fn log_text(&self, entity_path: &str, message: &str) -> Result<()> {
        self.rec.log(entity_path, &rerun::TextLog::new(message))?;
        Ok(())
    }
}

/// Builder for creating a RerunLogger with custom configuration
pub struct RerunLoggerBuilder {
    app_id: String,
    spawn: bool,
    connect_addr: Option<String>,
    save_path: Option<String>,
    connect_default: bool,
}

impl RerunLoggerBuilder {
    pub fn new(app_id: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            spawn: false,
            connect_addr: None,
            save_path: None,
            connect_default: false,
        }
    }

    /// Spawn a native viewer window
    pub fn spawn(mut self) -> Self {
        self.spawn = true;
        self
    }

    /// Connect to an existing viewer at a specific address
    pub fn connect(mut self, addr: impl Into<String>) -> Self {
        self.connect_addr = Some(addr.into());
        self
    }

    /// Connect to default gRPC endpoint
    pub fn connect_default(mut self) -> Self {
        self.connect_default = true;
        self
    }

    /// Save to a file
    pub fn save(mut self, path: impl Into<String>) -> Self {
        self.save_path = Some(path.into());
        self
    }

    /// Build the logger
    pub fn build(self) -> Result<RerunLogger> {
        if let Some(addr) = self.connect_addr {
            RerunLogger::connect(&self.app_id, &addr)
        } else if let Some(path) = self.save_path {
            RerunLogger::save(&self.app_id, &path)
        } else if self.connect_default {
            RerunLogger::connect_default(&self.app_id)
        } else if self.spawn {
            RerunLogger::spawn(&self.app_id)
        } else {
            // Default to spawn
            RerunLogger::spawn(&self.app_id)
        }
    }
}
