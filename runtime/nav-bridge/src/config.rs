//! `config/tracer.json`: every field has a default.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Wire format of a Dora input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputFormat {
    Odometry,
    Path,
    Goal,
    GoalStatus,
    LaserScan,
    CloudXyzi,
    Trust,
    RefineState,
    GateState,
    ChassisState,
}

/// Publish rate caps in Hz.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Rates {
    pub pose: f32,
    pub cloud: f32,
    pub scan: f32,
    pub trail: f32,
    pub status: f32,
}

impl Default for Rates {
    fn default() -> Self {
        Rates { pose: 20.0, cloud: 5.0, scan: 10.0, trail: 2.0, status: 5.0 }
    }
}

/// Default layer colours (RGBA).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub map_cloud: [u8; 4],
    pub footprint: [u8; 4],
    pub plan: [u8; 4],
    pub trail: [u8; 4],
    pub goal: [u8; 4],
    pub scan: [u8; 4],
}

impl Default for Colors {
    fn default() -> Self {
        Colors {
            map_cloud: [180, 180, 180, 255],
            footprint: [0, 200, 0, 255],
            plan: [0, 255, 0, 255],
            trail: [255, 140, 0, 255],
            goal: [255, 0, 255, 255],
            scan: [255, 0, 0, 255],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct BridgeConfig {
    pub robot_id: String,
    pub listen: Vec<String>,
    pub map_yaml: Option<PathBuf>,
    pub map_pcd: Option<PathBuf>,
    /// Footprint polygon in `base_footprint`.
    pub footprint: Vec<[f32; 2]>,
    /// Dora input name to wire format.
    pub inputs: BTreeMap<String, InputFormat>,
    pub rates_hz: Rates,
    pub cloud_max_points: usize,
    pub map_cloud_max_points: usize,
    pub map_cloud_voxel_m: f32,
    pub trail_min_step_m: f32,
    pub trail_max_points: usize,
    pub colors: Colors,
    pub log_forward_per_node_hz: f32,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        let inputs = [
            ("map_base", InputFormat::Odometry),
            ("plan", InputFormat::Path),
            ("goal", InputFormat::Goal),
            ("goal_status", InputFormat::GoalStatus),
            ("scan", InputFormat::LaserScan),
            ("cloud_body", InputFormat::CloudXyzi),
            ("trust", InputFormat::Trust),
            ("refine_state", InputFormat::RefineState),
            ("gate_state", InputFormat::GateState),
            ("chassis_state", InputFormat::ChassisState),
        ];
        BridgeConfig {
            robot_id: "tracer".into(),
            listen: vec!["tcp/0.0.0.0:7450".into()],
            map_yaml: None,
            map_pcd: None,
            footprint: vec![[0.09, 0.20], [0.09, -0.20], [-0.31, -0.20], [-0.31, 0.20]],
            inputs: inputs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            rates_hz: Rates::default(),
            cloud_max_points: 5000,
            map_cloud_max_points: 50_000,
            map_cloud_voxel_m: 0.10,
            trail_min_step_m: 0.05,
            trail_max_points: 3000,
            colors: Colors::default(),
            log_forward_per_node_hz: 5.0,
        }
    }
}

/// Read and parse a config file. Errors name the path.
pub fn load_config(path: &Path) -> Result<BridgeConfig, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_shipped_config() {
        let c: BridgeConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c.robot_id, "tracer");
        assert_eq!(c.listen, vec!["tcp/0.0.0.0:7450"]);
        assert_eq!(c.footprint.len(), 4);
        assert_eq!(c.rates_hz.pose, 20.0);
        assert_eq!(c.cloud_max_points, 5000);
        assert_eq!(c.trail_max_points, 3000);
        assert_eq!(c.colors.trail, [255, 140, 0, 255]);
        assert_eq!(c.inputs.get("map_base"), Some(&InputFormat::Odometry));
        let shipped = load_config(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config/tracer.json"))).unwrap();
        assert_eq!(shipped.footprint, c.footprint);
        assert_eq!(shipped.inputs, c.inputs);
        assert_eq!(shipped.rates_hz, c.rates_hz);
        assert!(shipped.map_yaml.is_some());
    }

    #[test]
    fn partial_and_bad_config() {
        let d = std::env::temp_dir().join(format!("navbridge-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("c.json");
        std::fs::write(&p, r#"{"robot_id":"x","rates_hz":{"pose":1},"inputs":{"a":"laser_scan"}}"#).unwrap();
        let c = load_config(&p).unwrap();
        assert_eq!(c.robot_id, "x");
        assert_eq!(c.rates_hz.pose, 1.0);
        assert_eq!(c.rates_hz.cloud, 5.0);
        assert_eq!(c.inputs.get("a"), Some(&InputFormat::LaserScan));
        std::fs::write(&p, "{").unwrap();
        assert!(load_config(&p).is_err());
        assert!(load_config(&d.join("missing.json")).unwrap_err().contains("missing.json"));
    }
}
