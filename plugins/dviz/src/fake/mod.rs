//! A synthetic dviz publisher: a simulated vehicle driving a figure-8 with a
//! lidar, IMU, log lines and a three-node dataflow graph. It stands in for a
//! real robot when trying the plugin. The `fake_publisher` example is a thin
//! command line over [`run_for`].

pub mod sim;

use dviz_protocol::{
    binary_formats, serialize_message, GraphEdge, GraphNode, GraphNodeStatus, GraphUpdate,
    MvizMessage, NodeDefinition, NodeInputDef, NodeOutputDef,
};
use serde_json::{json, Value};
use sim::SensorSimulator;
use std::time::{Duration, Instant};
use zenoh::{qos::CongestionControl, Wait};

/// How the publisher joins the network and what it publishes.
#[derive(Debug, Clone, PartialEq)]
pub struct FakeOptions {
    /// Endpoints to listen on, for example `tcp/0.0.0.0:7447`.
    pub listen_endpoints: Vec<String>,
    /// Endpoints to connect to. With neither list, multicast discovery is used.
    pub connect_endpoints: Vec<String>,
    /// Topic prefix; messages go to `<prefix>/<entity>`.
    pub prefix: String,
    /// Frames per second of the sensor data.
    pub hz: f64,
}

impl Default for FakeOptions {
    fn default() -> Self {
        Self {
            listen_endpoints: Vec::new(),
            connect_endpoints: Vec::new(),
            prefix: "dviz".into(),
            hz: 10.0,
        }
    }
}

const NODES: [&str; 3] = ["sim", "planner", "bridge"];
/// The simulator advances 20 ms per step.
const SIM_STEP_HZ: f64 = 50.0;
const GRAPH_EVERY: Duration = Duration::from_secs(2);
const LOG_EVERY: Duration = Duration::from_secs(1);

fn zenoh_config(options: &FakeOptions) -> Result<zenoh::Config, String> {
    let list = |endpoints: &[String]| {
        serde_json::to_string(endpoints).map_err(|e| format!("Bad endpoint list: {e}"))
    };
    let mut config = zenoh::Config::default();
    let mut set = |key: &str, value: &str| {
        config
            .insert_json5(key, value)
            .map_err(|e| format!("Zenoh config {key}: {e}"))
    };
    set("mode", r#""peer""#)?;
    if !options.connect_endpoints.is_empty() {
        set("connect/endpoints", &list(&options.connect_endpoints)?)?;
    }
    if !options.listen_endpoints.is_empty() {
        set("listen/endpoints", &list(&options.listen_endpoints)?)?;
    }
    let explicit = !options.connect_endpoints.is_empty() || !options.listen_endpoints.is_empty();
    if explicit {
        set("scouting/multicast/enabled", "false")?;
    }
    set("transport/shared_memory/enabled", "false")?;
    Ok(config)
}

fn message(msg_type: &str, time: f64, data: Value) -> Vec<u8> {
    let header = MvizMessage {
        msg_type: msg_type.to_string(),
        timestamp: Some(time),
        data,
        format: None,
        count: None,
    };
    serialize_message(&header, None)
}

fn points_message(time: f64, scan: &sim::LidarScan) -> Vec<u8> {
    let mut xyz = Vec::with_capacity(scan.points.len() * 12);
    for p in &scan.points {
        for v in [p.x, p.y, p.z] {
            xyz.extend_from_slice(&v.to_le_bytes());
        }
    }
    let header = MvizMessage {
        msg_type: "points3d".to_string(),
        timestamp: Some(time),
        data: json!({"radius": 0.03}),
        format: Some(binary_formats::POINTS_XYZ_F32.to_string()),
        count: Some(scan.points.len() as u32),
    };
    serialize_message(&header, Some(&xyz))
}

fn node_definitions() -> Vec<NodeDefinition> {
    let input = |name: &str, source: &str| NodeInputDef {
        name: name.into(),
        source: source.into(),
    };
    let output = |name: &str, to: &str| NodeOutputDef {
        name: name.into(),
        destinations: vec![to.into()],
    };
    vec![
        NodeDefinition {
            id: "sim".into(),
            inputs: Vec::new(),
            outputs: vec![output("scan", "planner"), output("pose", "bridge")],
            operator: Some("fake_publisher".into()),
        },
        NodeDefinition {
            id: "planner".into(),
            inputs: vec![input("scan", "sim/scan")],
            outputs: vec![output("path", "bridge")],
            operator: Some("fake_publisher".into()),
        },
        NodeDefinition {
            id: "bridge".into(),
            inputs: vec![input("pose", "sim/pose"), input("path", "planner/path")],
            outputs: Vec::new(),
            operator: Some("fake_publisher".into()),
        },
    ]
}

fn graph_update(time: f64) -> GraphUpdate {
    let edge = |from: &str, port: &str, to: &str| GraphEdge {
        from_node: from.into(),
        from_port: port.into(),
        to_node: to.into(),
        to_port: port.into(),
    };
    GraphUpdate {
        nodes: NODES
            .iter()
            .map(|id| GraphNode {
                id: (*id).into(),
                status: GraphNodeStatus::Active,
                last_seen: time,
            })
            .collect(),
        edges: vec![
            edge("sim", "scan", "planner"),
            edge("sim", "pose", "bridge"),
            edge("planner", "path", "bridge"),
        ],
        timestamp: time,
    }
}

/// Publishes synthetic data for `duration`, then closes its Zenoh session and
/// returns. Joins the network as a peer on the given endpoints.
pub fn run_for(options: FakeOptions, duration: Duration) -> Result<(), String> {
    if options.prefix.is_empty() {
        return Err("The topic prefix must not be empty".into());
    }
    if !(options.hz.is_finite() && options.hz > 0.0) {
        return Err("hz must be a positive number".into());
    }
    let session = zenoh::open(zenoh_config(&options)?)
        .wait()
        .map_err(|e| format!("Could not open Zenoh: {e}"))?;
    let put = |entity: &str, payload: Vec<u8>| -> Result<(), String> {
        session
            .put(format!("{}/{entity}", options.prefix), payload)
            .congestion_control(CongestionControl::Block)
            .wait()
            .map_err(|e| format!("Could not publish {entity}: {e}"))
    };

    let tick = Duration::from_secs_f64(1.0 / options.hz);
    let steps_per_tick = (SIM_STEP_HZ / options.hz).round().max(1.0) as u32;
    let mut sim = SensorSimulator::new();
    let started = Instant::now();
    let mut next_tick = started;
    let mut next_log = started;
    let mut next_graph = started;
    let result = loop {
        let now = Instant::now();
        if now.duration_since(started) >= duration {
            break Ok(());
        }
        if now < next_tick {
            std::thread::sleep((next_tick - now).min(Duration::from_millis(50)));
            continue;
        }
        next_tick += tick;
        for _ in 0..steps_per_tick {
            sim.step();
        }
        let time = sim.time();
        let pose = sim.generate_pose();
        let imu = sim.generate_imu();
        let strip: Vec<[f32; 3]> = sim.path_history().to_vec();
        let mut sent = vec![
            ("world/lidar", points_message(time, &sim.generate_lidar())),
            (
                "world/vehicle",
                message(
                    "transform3d",
                    time,
                    json!({"translation": pose.position, "rotation": pose.orientation}),
                ),
            ),
            (
                "world/path",
                message("linestrips3d", time, json!({"strips": [strip]})),
            ),
        ];
        for (entity, value) in [
            ("imu/accel_x", imu.linear_acceleration[0]),
            ("imu/accel_y", imu.linear_acceleration[1]),
            ("imu/accel_z", imu.linear_acceleration[2]),
            ("imu/gyro_z", imu.angular_velocity[2]),
        ] {
            sent.push((entity, message("scalar", time, json!({"value": value}))));
        }
        if now >= next_log {
            next_log += LOG_EVERY;
            let line = json!({
                "level": "INFO",
                "message": format!("vehicle at ({:.1}, {:.1})", pose.position[0], pose.position[1]),
                "node_id": "sim",
            });
            sent.push(("logs", message("log", time, line)));
        }
        if now >= next_graph {
            next_graph += GRAPH_EVERY;
            for definition in node_definitions() {
                let id = definition.id.clone();
                let data = serde_json::to_value(definition).map_err(|e| e.to_string())?;
                put(&format!("nodes/{id}"), message("node_definition", time, data))?;
            }
            let data = serde_json::to_value(graph_update(time)).map_err(|e| e.to_string())?;
            sent.push(("graph", message("graph_update", time, data)));
        }
        if let Some(err) = sent.into_iter().find_map(|(entity, payload)| put(entity, payload).err())
        {
            break Err(err);
        }
    };
    session
        .close()
        .wait()
        .map_err(|e| format!("Could not close Zenoh: {e}"))?;
    result
}
