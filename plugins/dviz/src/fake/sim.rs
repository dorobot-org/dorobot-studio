// Moved from dviz (dorobot-org/dviz, commit 5276598),
// dviz-rerun-bridge/src/simulation.rs, Apache-2.0. Only this notice and the
// module docs below differ from the original; see the repository NOTICE.

//! Simulated sensor data generators for testing and demonstration
//!
//! Provides realistic sensor data streams for IMU, LiDAR, and vehicle pose.

use std::f32::consts::PI;

/// IMU sensor data
#[derive(Debug, Clone, Default)]
pub struct ImuData {
    pub timestamp: f64,
    pub linear_acceleration: [f32; 3],  // m/s^2
    pub angular_velocity: [f32; 3],      // rad/s
    pub orientation: [f32; 4],           // quaternion [x, y, z, w]
}

/// Single LiDAR point
#[derive(Debug, Clone, Copy, Default)]
pub struct LidarPoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub intensity: f32,
}

/// LiDAR scan data
#[derive(Debug, Clone, Default)]
pub struct LidarScan {
    pub timestamp: f64,
    pub points: Vec<LidarPoint>,
}

/// Vehicle pose in 3D space
#[derive(Debug, Clone, Default)]
pub struct VehiclePose {
    pub timestamp: f64,
    pub position: [f32; 3],      // [x, y, z] meters
    pub orientation: [f32; 4],   // quaternion [x, y, z, w]
    pub velocity: [f32; 3],      // [vx, vy, vz] m/s
}

/// Sensor data simulator
pub struct SensorSimulator {
    /// Current simulation time in seconds
    time: f64,
    /// Time step per update
    dt: f64,
    /// Vehicle position for trajectory
    vehicle_x: f32,
    vehicle_y: f32,
    vehicle_heading: f32,
    /// Path history
    path_history: Vec<[f32; 3]>,
}

impl SensorSimulator {
    pub fn new() -> Self {
        Self {
            time: 0.0,
            dt: 0.02, // 50 Hz update rate
            vehicle_x: 0.0,
            vehicle_y: 0.0,
            vehicle_heading: 0.0,
            path_history: Vec::new(),
        }
    }

    /// Advance simulation by one time step
    pub fn step(&mut self) {
        self.time += self.dt;

        // Update vehicle position along a figure-8 path
        let t = self.time as f32 * 0.3;
        let scale = 10.0;

        self.vehicle_x = scale * (t).sin();
        self.vehicle_y = scale * (2.0 * t).sin() * 0.5;
        self.vehicle_heading = (t).cos().atan2(-(t).sin());

        // Record path history
        if self.path_history.len() > 500 {
            self.path_history.remove(0);
        }
        self.path_history.push([self.vehicle_x, self.vehicle_y, 0.0]);
    }

    /// Get current simulation time
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Get path history for visualization
    pub fn path_history(&self) -> &[[f32; 3]] {
        &self.path_history
    }

    /// Generate IMU data with realistic noise
    pub fn generate_imu(&self) -> ImuData {
        let noise = || (rand_simple(self.time) - 0.5) * 0.1;

        // Simulate acceleration from vehicle motion
        let t = self.time as f32 * 0.3;
        let ax = -10.0 * 0.3 * 0.3 * t.sin() + noise() as f32;
        let ay = -10.0 * 0.5 * 4.0 * 0.3 * 0.3 * (2.0 * t).sin() + noise() as f32;

        // Angular velocity (yaw rate)
        let yaw_rate = 0.3 * t.cos() + noise() as f32;

        // Orientation quaternion from heading
        let half_heading = self.vehicle_heading / 2.0;
        let qw = half_heading.cos();
        let qz = half_heading.sin();

        ImuData {
            timestamp: self.time,
            linear_acceleration: [ax, ay, 9.81 + noise() as f32], // Include gravity
            angular_velocity: [noise() as f32, noise() as f32, yaw_rate],
            orientation: [0.0, 0.0, qz, qw],
        }
    }

    /// Generate LiDAR scan simulating environment
    pub fn generate_lidar(&self) -> LidarScan {
        let mut points = Vec::with_capacity(3600);

        let num_beams = 64;
        let num_angles = 360;

        for beam in 0..num_beams {
            let elevation = -15.0_f32.to_radians() +
                (beam as f32 / num_beams as f32) * 30.0_f32.to_radians();

            for angle_idx in 0..num_angles {
                let azimuth = (angle_idx as f32 / num_angles as f32) * 2.0 * PI;

                // Simulate range based on simple environment
                let range = self.simulate_lidar_range(azimuth, elevation);

                if range > 0.5 && range < 100.0 {
                    let x = range * elevation.cos() * azimuth.cos();
                    let y = range * elevation.cos() * azimuth.sin();
                    let z = range * elevation.sin();

                    // Transform to world frame
                    let cos_h = self.vehicle_heading.cos();
                    let sin_h = self.vehicle_heading.sin();

                    let world_x = self.vehicle_x + x * cos_h - y * sin_h;
                    let world_y = self.vehicle_y + x * sin_h + y * cos_h;
                    let world_z = z + 1.5; // Sensor height

                    // Intensity based on range
                    let intensity = (1.0 - range / 100.0).max(0.1);

                    points.push(LidarPoint {
                        x: world_x,
                        y: world_y,
                        z: world_z,
                        intensity,
                    });
                }
            }
        }

        LidarScan {
            timestamp: self.time,
            points,
        }
    }

    /// Simulate LiDAR range based on virtual environment
    fn simulate_lidar_range(&self, azimuth: f32, elevation: f32) -> f32 {
        // Ground plane
        if elevation < -5.0_f32.to_radians() {
            let ground_dist = 1.5 / (-elevation.sin()).max(0.01);
            if ground_dist < 50.0 {
                return ground_dist;
            }
        }

        // Simulate some obstacles (boxes around the path)
        let obstacles = [
            ([5.0_f32, 5.0, 0.0], 2.0_f32),   // Box at (5, 5)
            ([-5.0, -3.0, 0.0], 1.5),          // Box at (-5, -3)
            ([8.0, -2.0, 0.0], 3.0),           // Box at (8, -2)
            ([-8.0, 4.0, 0.0], 2.5),           // Box at (-8, 4)
        ];

        let ray_dir = [
            azimuth.cos() * elevation.cos(),
            azimuth.sin() * elevation.cos(),
            elevation.sin(),
        ];

        let cos_h = self.vehicle_heading.cos();
        let sin_h = self.vehicle_heading.sin();

        let world_dir = [
            ray_dir[0] * cos_h - ray_dir[1] * sin_h,
            ray_dir[0] * sin_h + ray_dir[1] * cos_h,
            ray_dir[2],
        ];

        let mut min_range = 100.0_f32;

        for (center, size) in &obstacles {
            let rel = [
                center[0] - self.vehicle_x,
                center[1] - self.vehicle_y,
                center[2] + size - 1.5,
            ];

            // Simple sphere intersection for obstacles
            let dist = (rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2]).sqrt();
            if dist < min_range + size {
                // Check if ray points towards obstacle
                let dot = rel[0] * world_dir[0] + rel[1] * world_dir[1] + rel[2] * world_dir[2];
                if dot > 0.0 {
                    let range = dot - size * 0.8;
                    if range > 0.0 && range < min_range {
                        min_range = range;
                    }
                }
            }
        }

        min_range
    }

    /// Generate vehicle pose
    pub fn generate_pose(&self) -> VehiclePose {
        let t = self.time as f32 * 0.3;

        // Velocity from derivative of position
        let vx = 10.0 * 0.3 * t.cos();
        let vy = 10.0 * 0.5 * 2.0 * 0.3 * (2.0 * t).cos();

        // Orientation quaternion
        let half_heading = self.vehicle_heading / 2.0;

        VehiclePose {
            timestamp: self.time,
            position: [self.vehicle_x, self.vehicle_y, 0.0],
            orientation: [0.0, 0.0, half_heading.sin(), half_heading.cos()],
            velocity: [vx, vy, 0.0],
        }
    }
}

impl Default for SensorSimulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple deterministic pseudo-random for reproducibility
fn rand_simple(seed: f64) -> f64 {
    let x = (seed * 12.9898 + 78.233).sin() * 43758.5453;
    x - x.floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulator_step() {
        let mut sim = SensorSimulator::new();
        assert_eq!(sim.time(), 0.0);

        sim.step();
        assert!(sim.time() > 0.0);
    }

    #[test]
    fn test_imu_generation() {
        let sim = SensorSimulator::new();
        let imu = sim.generate_imu();

        // Check gravity is approximately correct
        assert!((imu.linear_acceleration[2] - 9.81).abs() < 0.5);
    }

    #[test]
    fn test_lidar_generation() {
        let sim = SensorSimulator::new();
        let scan = sim.generate_lidar();

        // Should have points
        assert!(!scan.points.is_empty());
    }
}
