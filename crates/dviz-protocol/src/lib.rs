//! DViz Zenoh Protocol - Universal message format for any application
//!
//! This protocol allows ANY application to publish visualization data
//! without DViz needing application-specific code.
//!
//! ## Topic Structure
//! `{prefix}/{entity_path}` -> logs to Rerun at `{entity_path}`
//!
//! Example: `dviz/world/vehicle/body` -> logs to `world/vehicle/body`
//!
//! ## Message Format
//! JSON header describing the data type, followed by optional binary payload
//!
//! ## Supported Types (maps to Rerun archetypes)
//! - `points3d` - Point clouds
//! - `boxes3d` - 3D boxes
//! - `arrows3d` - Arrows/vectors
//! - `linestrips3d` - Line strips
//! - `transform3d` - Coordinate transforms
//! - `text` - Text annotations
//! - `scalar` - Time-series scalar values
//! - `image` - Images (binary payload)
//! - `log` - System log entries from nodes

use serde::{Deserialize, Serialize};

/// Universal message header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MvizMessage {
    /// Data type (maps to Rerun archetype)
    #[serde(rename = "type")]
    pub msg_type: String,

    /// Optional timestamp (seconds since start)
    #[serde(default)]
    pub timestamp: Option<f64>,

    /// Type-specific data (JSON for simple types)
    #[serde(default)]
    pub data: serde_json::Value,

    /// For binary data: format description
    #[serde(default)]
    pub format: Option<String>,

    /// For binary data: element count
    #[serde(default)]
    pub count: Option<u32>,
}

/// Points3D data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Points3DData {
    /// XYZ positions (inline JSON or binary follows)
    #[serde(default)]
    pub positions: Option<Vec<[f32; 3]>>,

    /// Optional colors [r,g,b,a] 0-255
    #[serde(default)]
    pub colors: Option<Vec<[u8; 4]>>,

    /// Optional single color for all points
    #[serde(default)]
    pub color: Option<[u8; 4]>,

    /// Point radius
    #[serde(default)]
    pub radius: Option<f32>,

    /// Optional radii per point
    #[serde(default)]
    pub radii: Option<Vec<f32>>,
}

/// Boxes3D data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Boxes3DData {
    /// Centers [x, y, z]
    pub centers: Vec<[f32; 3]>,

    /// Sizes [w, h, d] or half-sizes
    pub sizes: Vec<[f32; 3]>,

    /// Use half-sizes instead of full sizes
    #[serde(default)]
    pub half_sizes: bool,

    /// Optional quaternions [x, y, z, w]
    #[serde(default)]
    pub quaternions: Option<Vec<[f32; 4]>>,

    /// Optional colors
    #[serde(default)]
    pub colors: Option<Vec<[u8; 4]>>,

    /// Single color for all boxes
    #[serde(default)]
    pub color: Option<[u8; 4]>,
}

/// Arrows3D data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arrows3DData {
    /// Arrow start points (origins)
    pub origins: Vec<[f32; 3]>,

    /// Arrow direction vectors
    pub vectors: Vec<[f32; 3]>,

    /// Optional colors
    #[serde(default)]
    pub colors: Option<Vec<[u8; 4]>>,

    /// Single color for all arrows
    #[serde(default)]
    pub color: Option<[u8; 4]>,
}

/// LineStrips3D data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineStrips3DData {
    /// List of line strips, each strip is a list of points
    pub strips: Vec<Vec<[f32; 3]>>,

    /// Optional colors per strip
    #[serde(default)]
    pub colors: Option<Vec<[u8; 4]>>,

    /// Single color for all strips
    #[serde(default)]
    pub color: Option<[u8; 4]>,
}

/// Transform3D data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform3DData {
    /// Translation [x, y, z]
    #[serde(default)]
    pub translation: Option<[f32; 3]>,

    /// Rotation as quaternion [x, y, z, w]
    #[serde(default)]
    pub rotation: Option<[f32; 4]>,

    /// Or rotation as 3x3 matrix (row-major)
    #[serde(default)]
    pub rotation_matrix: Option<[[f32; 3]; 3]>,

    /// Or full 4x4 matrix (row-major)
    #[serde(default)]
    pub matrix4x4: Option<[f32; 16]>,
}

/// Text annotation data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextData {
    /// The text content
    pub text: String,

    /// Optional 3D position for TextLog
    #[serde(default)]
    pub position: Option<[f32; 3]>,
}

/// Scalar time-series data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalarData {
    /// The scalar value
    pub value: f64,
}

// ============================================================================
// System Log Types
// ============================================================================

/// Log level for system messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    /// Debug information
    Debug,
    /// General information
    #[default]
    Info,
    /// Warning messages
    Warn,
    /// Error messages
    Error,
}

impl LogLevel {
    /// Parse log level from string (case-insensitive)
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "DEBUG" => Self::Debug,
            "INFO" => Self::Info,
            "WARN" | "WARNING" => Self::Warn,
            "ERROR" | "ERR" => Self::Error,
            _ => Self::Info,
        }
    }

    /// Get display name
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    /// Get color for UI display [r, g, b, a]
    pub fn color(&self) -> [u8; 4] {
        match self {
            Self::Debug => [128, 128, 128, 255], // Gray
            Self::Info => [100, 180, 255, 255],  // Blue
            Self::Warn => [255, 200, 50, 255],   // Yellow/Orange
            Self::Error => [255, 80, 80, 255],   // Red
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// System log entry from a dora node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level
    pub level: LogLevel,
    /// Log message content
    pub message: String,
    /// Source node ID (e.g., "bicycle_model", "simple_planner")
    pub node_id: String,
    /// Timestamp in seconds since dataflow start
    #[serde(default)]
    pub timestamp: f64,
    /// Optional metadata key-value pairs
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(level: LogLevel, message: impl Into<String>, node_id: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            node_id: node_id.into(),
            timestamp: 0.0,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Create with timestamp
    pub fn with_timestamp(mut self, timestamp: f64) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Format as display string for UI
    pub fn format_display(&self) -> String {
        format!(
            "[{:.2}s] [{}] [{}] {}",
            self.timestamp,
            self.level.as_str(),
            self.node_id,
            self.message
        )
    }
}

/// Log data payload in MvizMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogData {
    /// Log level
    pub level: String,
    /// Log message
    pub message: String,
    /// Source node ID
    pub node_id: String,
    /// Optional port name (for I/O activity logs)
    #[serde(default)]
    pub port: Option<String>,
    /// Optional port type: "input" or "output" (for I/O activity logs)
    #[serde(default)]
    pub port_type: Option<String>,
    /// Optional metadata
    #[serde(default)]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

// ============================================================================
// Node Definition Types (for dataflow graph inspection)
// ============================================================================

/// Node input port definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInputDef {
    /// Input port name
    pub name: String,
    /// Source in format "node_id/output_name"
    pub source: String,
}

/// Node output port definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeOutputDef {
    /// Output port name
    pub name: String,
    /// Destination node IDs that receive this output
    #[serde(default)]
    pub destinations: Vec<String>,
}

/// Complete node definition from dataflow YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDefinition {
    /// Node ID
    pub id: String,
    /// Input ports
    #[serde(default)]
    pub inputs: Vec<NodeInputDef>,
    /// Output ports
    #[serde(default)]
    pub outputs: Vec<NodeOutputDef>,
    /// Node operator path (Python/Rust)
    #[serde(default)]
    pub operator: Option<String>,
}

/// Full dataflow definition with all nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataflowDefinition {
    /// Dataflow name
    #[serde(default)]
    pub name: String,
    /// All nodes in the dataflow
    pub nodes: Vec<NodeDefinition>,
}

// ============================================================================
// Dynamic Graph Discovery Types (for runtime graph visualization)
// ============================================================================

/// Node status in the dataflow graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GraphNodeStatus {
    /// Node is active (receiving/sending messages)
    Active,
    /// Node is idle (no recent activity)
    #[default]
    Idle,
    /// Node has errors
    Error,
}

/// A node in the dynamically discovered dataflow graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Node ID
    pub id: String,
    /// Current status
    #[serde(default)]
    pub status: GraphNodeStatus,
    /// Last activity timestamp (seconds since start)
    #[serde(default)]
    pub last_seen: f64,
}

/// An edge (connection) in the dataflow graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Source node ID
    pub from_node: String,
    /// Source output port name
    pub from_port: String,
    /// Target node ID
    pub to_node: String,
    /// Target input port name
    pub to_port: String,
}

/// Graph update message - sent periodically by the bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphUpdate {
    /// All discovered nodes
    pub nodes: Vec<GraphNode>,
    /// All discovered edges
    pub edges: Vec<GraphEdge>,
    /// Update timestamp
    #[serde(default)]
    pub timestamp: f64,
}

/// Binary payload formats
pub mod binary_formats {
    /// Points: x,y,z as f32 (12 bytes per point)
    pub const POINTS_XYZ_F32: &str = "xyz_f32";

    /// Points: x,y,z,r,g,b,a as f32,f32,f32,u8,u8,u8,u8 (16 bytes per point)
    pub const POINTS_XYZRGBA: &str = "xyzrgba";

    /// Image: raw RGB bytes
    pub const IMAGE_RGB8: &str = "rgb8";

    /// Image: raw RGBA bytes
    pub const IMAGE_RGBA8: &str = "rgba8";
}

/// Parse an `xyz_f32` payload (little-endian, 12 bytes per point) into points.
///
/// Returns `None` when `binary` holds fewer than `count` points. Any trailing
/// bytes beyond `count` points are ignored.
pub fn parse_points_xyz_f32(binary: &[u8], count: u32) -> Option<Vec<[f32; 3]>> {
    let needed = (count as usize).checked_mul(12)?;
    if binary.len() < needed {
        return None;
    }
    Some(
        binary[..needed]
            .chunks_exact(12)
            .map(|c| {
                let f = |i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
                [f(0), f(4), f(8)]
            })
            .collect(),
    )
}

/// Parse a Zenoh message into header and optional binary payload
pub fn parse_message(bytes: &[u8]) -> Option<(MvizMessage, Option<&[u8]>)> {
    // Find newline separator (if present, binary data follows)
    if let Some(newline_pos) = bytes.iter().position(|&b| b == b'\n') {
        let header_str = std::str::from_utf8(&bytes[..newline_pos]).ok()?;
        let header: MvizMessage = serde_json::from_str(header_str).ok()?;
        let binary = if newline_pos + 1 < bytes.len() {
            Some(&bytes[newline_pos + 1..])
        } else {
            None
        };
        Some((header, binary))
    } else {
        // No binary data, entire message is JSON
        let json_str = std::str::from_utf8(bytes).ok()?;
        let header: MvizMessage = serde_json::from_str(json_str).ok()?;
        Some((header, None))
    }
}

/// Serialize a message with optional binary payload
pub fn serialize_message(header: &MvizMessage, binary: Option<&[u8]>) -> Vec<u8> {
    let mut result = serde_json::to_string(header).unwrap_or_default().into_bytes();
    if let Some(data) = binary {
        result.push(b'\n');
        result.extend_from_slice(data);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_data_with_port_info() {
        // Simulate what the bridge sends for I/O activity
        let io_log_json = serde_json::json!({
            "level": "INFO",
            "message": "[0.12, 3.45]",
            "node_id": "dviz_bridge",
            "port": "imu_msg",
            "port_type": "input",
        });

        let log_data: LogData = serde_json::from_value(io_log_json).unwrap();
        assert_eq!(log_data.level, "INFO");
        assert_eq!(log_data.node_id, "dviz_bridge");
        assert_eq!(log_data.port, Some("imu_msg".to_string()));
        assert_eq!(log_data.port_type, Some("input".to_string()));

        let has_port_info = log_data.port.is_some() && log_data.port_type.is_some();
        assert!(has_port_info, "Should detect port info in I/O activity log");
    }

    #[test]
    fn test_log_data_without_port_info() {
        // Regular log without port info
        let log_json = serde_json::json!({
            "level": "INFO",
            "message": "Test message",
            "node_id": "bicycle_model",
        });

        let log_data: LogData = serde_json::from_value(log_json).unwrap();
        assert_eq!(log_data.port, None);
        assert_eq!(log_data.port_type, None);

        let has_port_info = log_data.port.is_some() && log_data.port_type.is_some();
        assert!(!has_port_info, "Regular log should not have port info");
    }

    #[test]
    fn test_full_dviz_message_with_log() {
        // Test the full MvizMessage -> LogData flow
        let msg = MvizMessage {
            msg_type: "log".to_string(),
            timestamp: Some(1.5),
            data: serde_json::json!({
                "level": "INFO",
                "message": "[1.23, 4.56, 7.89]",
                "node_id": "simple_planner",
                "port": "waypoints",
                "port_type": "output",
            }),
            format: None,
            count: None,
        };

        // Serialize and parse back
        let bytes = serialize_message(&msg, None);
        let (parsed_msg, _binary) = parse_message(&bytes).unwrap();

        assert_eq!(parsed_msg.msg_type, "log");

        let log_data: LogData = serde_json::from_value(parsed_msg.data).unwrap();
        assert_eq!(log_data.port, Some("waypoints".to_string()));
        assert_eq!(log_data.port_type, Some("output".to_string()));
    }

    fn header(msg_type: &str, count: Option<u32>, format: Option<&str>) -> MvizMessage {
        MvizMessage {
            msg_type: msg_type.to_string(),
            timestamp: Some(1.5),
            data: serde_json::Value::Null,
            format: format.map(str::to_string),
            count,
        }
    }

    #[test]
    fn round_trip_with_binary() {
        let payload: Vec<u8> = (0u8..24).collect();
        let h = header("points3d", Some(2), Some(binary_formats::POINTS_XYZ_F32));
        let bytes = serialize_message(&h, Some(&payload));
        let (parsed, bin) = parse_message(&bytes).expect("parse");
        assert_eq!(parsed.msg_type, "points3d");
        assert_eq!(parsed.timestamp, Some(1.5));
        assert_eq!(parsed.count, Some(2));
        assert_eq!(parsed.format.as_deref(), Some(binary_formats::POINTS_XYZ_F32));
        assert_eq!(bin, Some(payload.as_slice()));
    }

    #[test]
    fn round_trip_json_only() {
        let h = header("log", None, None);
        let bytes = serialize_message(&h, None);
        let (parsed, bin) = parse_message(&bytes).expect("parse");
        assert_eq!(parsed.msg_type, "log");
        assert!(bin.is_none());
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_message(b"\xff\xfe").is_none());
        assert!(parse_message(b"{not json").is_none());
    }

    fn le(vals: &[f32]) -> Vec<u8> {
        vals.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn parse_points_ok() {
        let bin = le(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(bin.len(), 24);
        assert_eq!(
            parse_points_xyz_f32(&bin, 2),
            Some(vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]])
        );
    }

    #[test]
    fn parse_points_rejects_short_binary() {
        assert_eq!(parse_points_xyz_f32(&[0u8; 20], 2), None);
    }
}
