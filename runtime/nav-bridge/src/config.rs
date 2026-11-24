//! `config/tracer.json`: every field has a default.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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
    /// Nav2 u8 costs with a 32-byte header (the planner's `costmap`)
    #[serde(rename = "costmap")]
    CostmapU8,
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
    pub costmap: f32,
}

impl Default for Rates {
    fn default() -> Self {
        Rates { pose: 20.0, cloud: 5.0, scan: 10.0, trail: 2.0, status: 5.0, costmap: 1.0 }
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
            footprint: [220, 220, 220, 255],
            plan: [0, 180, 255, 255],
            trail: [0, 220, 0, 255],
            goal: [255, 170, 0, 255],
            scan: [255, 0, 0, 255],
        }
    }
}

/// Line and arrow radii in metres, for Rerun (the 2D canvas has its own pixel widths).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Radii {
    pub plan: f32,
    pub trail: f32,
    pub goal: f32,
    pub footprint: f32,
}

impl Default for Radii {
    fn default() -> Self {
        Radii { plan: 0.025, trail: 0.02, goal: 0.04, footprint: 0.01 }
    }
}

/// Command channel (Studio to robot). Goals and initial poses stay off unless `goal_enabled`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct CmdConfig {
    pub goal_enabled: bool,
    /// Robot-side token file; a leading `~/` is expanded.
    pub token_file: String,
    /// UDP address of the robot's nav_command.
    pub forward: String,
    pub reply_timeout_ms: u64,
    /// Token bucket refill rate (burst 2).
    pub max_goal_per_s: f32,
}

impl Default for CmdConfig {
    fn default() -> Self {
        CmdConfig {
            goal_enabled: false,
            token_file: "~/.config/dviz/cmd_token".into(),
            forward: "127.0.0.1:9998".into(),
            reply_timeout_ms: 1000,
            max_goal_per_s: 2.0,
        }
    }
}

impl CmdConfig {
    /// `forward` as an address. nav_command listens on 127.0.0.1 only, so any other
    /// address, a host name or port 0 is refused; the error names the value.
    pub fn forward_addr(&self) -> Result<SocketAddr, String> {
        let bad = |why: &str| format!("cmd.forward {:?}: {why}", self.forward);
        let addr: SocketAddr = self.forward.parse().map_err(|_| bad("not an <ip>:<port> address"))?;
        if addr.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
            return Err(bad("must be 127.0.0.1:<port> (nav_command listens on 127.0.0.1 only)"));
        }
        if addr.port() == 0 {
            return Err(bad("port 0"));
        }
        Ok(addr)
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
    pub radii: Radii,
    /// Body model Studio draws, sent in `nav/status`
    pub robot_model: Option<String>,
    pub log_forward_per_node_hz: f32,
    pub cmd: CmdConfig,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        let inputs = [
            ("pose", InputFormat::Odometry),
            ("path", InputFormat::Path),
            ("goal", InputFormat::Goal),
            ("goal_status", InputFormat::GoalStatus),
            ("scan", InputFormat::LaserScan),
            ("cloud", InputFormat::CloudXyzi),
            ("trust", InputFormat::Trust),
            ("refine_state", InputFormat::RefineState),
            ("gate_state", InputFormat::GateState),
            ("chassis", InputFormat::ChassisState),
            ("costmap", InputFormat::CostmapU8),
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
            radii: Radii::default(),
            robot_model: Some("tracer_mini".into()),
            log_forward_per_node_hz: 5.0,
            cmd: CmdConfig::default(),
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
        assert_eq!(c.colors.trail, [0, 220, 0, 255]);
        assert_eq!(c.robot_model.as_deref(), Some("tracer_mini"));
        assert_eq!(c.rates_hz.costmap, 1.0);
        assert_eq!(c.inputs.get("costmap"), Some(&InputFormat::CostmapU8));
        assert_eq!(c.inputs.get("pose"), Some(&InputFormat::Odometry));
        let shipped = load_config(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config/tracer.json"))).unwrap();
        assert_eq!(shipped.footprint, c.footprint);
        assert_eq!(shipped.inputs, c.inputs);
        assert_eq!(shipped.rates_hz, c.rates_hz);
        assert_eq!(shipped.radii, c.radii);
        assert_eq!(shipped.robot_model, c.robot_model);
        assert!(shipped.map_yaml.is_some());
    }

    /// RViz-style defaults (the plan blue, the trail green, the goal orange, a light footprint).
    #[test]
    fn default_colors_are_rviz() {
        let want = Colors {
            map_cloud: [180, 180, 180, 255],
            footprint: [220, 220, 220, 255],
            plan: [0, 180, 255, 255],
            trail: [0, 220, 0, 255],
            goal: [255, 170, 0, 255],
            scan: [255, 0, 0, 255],
        };
        assert_eq!(BridgeConfig::default().colors, want);
        let shipped = load_config(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config/tracer.json"))).unwrap();
        assert_eq!(shipped.colors, want);
        let r = BridgeConfig::default().radii;
        assert_eq!((r.plan, r.trail, r.goal, r.footprint), (0.025, 0.02, 0.04, 0.01));
    }

    #[test]
    fn cmd_config_defaults() {
        let d = CmdConfig::default();
        assert!(!d.goal_enabled);
        assert_eq!(d.token_file, "~/.config/dviz/cmd_token");
        assert_eq!(d.forward, "127.0.0.1:9998");
        assert_eq!(d.reply_timeout_ms, 1000);
        assert_eq!(d.max_goal_per_s, 2.0);
        let c: BridgeConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c.cmd, d);
        let shipped = load_config(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config/tracer.json"))).unwrap();
        assert_eq!(shipped.cmd, CmdConfig::default());
        let c: BridgeConfig = serde_json::from_str(r#"{"cmd":{"goal_enabled":true}}"#).unwrap();
        assert!(c.cmd.goal_enabled);
        assert_eq!(c.cmd.forward, "127.0.0.1:9998");
    }

    /// Ruling R7: nav_command listens on 127.0.0.1 only, so anything else is refused.
    #[test]
    fn forward_must_be_ipv4_loopback() {
        let f = |s: &str| CmdConfig { forward: s.into(), ..CmdConfig::default() }.forward_addr();
        assert_eq!(f("127.0.0.1:9998"), Ok("127.0.0.1:9998".parse().unwrap()));
        assert_eq!(f("127.0.0.1:19998").map(|a| a.port()), Ok(19998));
        for bad in [
            "0.0.0.0:9998",
            "192.168.1.5:9998",
            "127.0.0.2:9998",
            "[::1]:9998",
            "localhost:9998",
            "127.0.0.1",
            "127.0.0.1:0",
            " 127.0.0.1:9998",
            "",
        ] {
            let err = f(bad).expect_err(bad);
            assert!(err.starts_with("cmd.forward "), "{err}");
            assert!(err.contains(&format!("{bad:?}")), "names the value: {err}");
        }
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
