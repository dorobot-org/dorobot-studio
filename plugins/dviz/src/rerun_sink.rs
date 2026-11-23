//! Writes dviz messages into a Rerun recording. Ported from dviz-shell's
//! `log_vis_data_to_rerun`; compiled only with the `rerun` feature.
//!
//! `Sink::log` runs on the session's receive thread, which does not catch
//! panics, so nothing here may panic on message data: every shape is checked
//! through serde or by length, and bad input comes back as `Err`.

use crate::route::Sink;
use dviz_protocol::{
    binary_formats, parse_points_xyz_f32, Arrows3DData, Boxes3DData, LineStrips3DData, MvizMessage,
    Points3DData, ScalarData, TextData, Transform3DData,
};
use rerun::sink::MemorySinkStorage;
use rerun::{RecordingStream, RecordingStreamBuilder};

pub struct RerunSink {
    rec: RecordingStream,
}

impl RerunSink {
    /// Spawns a Rerun viewer (needs the `rerun` executable on `PATH`) and
    /// streams into it.
    pub fn spawn(app_id: &str) -> Result<RerunSink, String> {
        let rec = RecordingStreamBuilder::new(app_id)
            .spawn()
            .map_err(|e| format!("Could not start the Rerun viewer: {e}"))?;
        Ok(RerunSink { rec })
    }

    /// Records into memory; for tests.
    pub fn memory(app_id: &str) -> (RerunSink, MemorySinkStorage) {
        let (rec, storage) = RecordingStreamBuilder::new(app_id)
            .memory()
            .expect("an in-memory recording needs no resources");
        (RerunSink { rec }, storage)
    }

    /// Waits until everything logged so far has reached the sink.
    pub fn flush(&self) {
        let _ = self.rec.flush_blocking(); // a failed flush only means less data is visible
    }
}

const POINTS_COLOR: [u8; 4] = [100, 200, 255, 255];
const POINTS_JSON_COLOR: [u8; 4] = [200, 200, 200, 255];
const BOXES_COLOR: [u8; 4] = [100, 150, 200, 255];
const ARROWS_COLOR: [u8; 4] = [255, 100, 100, 255];
const STRIPS_COLOR: [u8; 4] = [100, 200, 255, 255];

fn rgba(c: [u8; 4]) -> rerun::Color {
    rerun::Color::from_unmultiplied_rgba(c[0], c[1], c[2], c[3])
}

/// Deserializes the message `data` into `$ty`, naming the message type on failure.
macro_rules! parse {
    ($msg:expr, $ty:ty) => {
        serde_json::from_value::<$ty>($msg.data.clone())
            .map_err(|e| format!("bad {} data: {e}", $msg.msg_type))?
    };
}

/// Optional `color` of a binary point cloud header: 3 or 4 numbers 0..=255.
/// Anything else keeps the default, as in dviz-shell.
fn lenient_color(data: &serde_json::Value, default: [u8; 4]) -> [u8; 4] {
    let Some(items) = data.get("color").and_then(|v| v.as_array()) else {
        return default;
    };
    if items.len() < 3 {
        return default;
    }
    let mut color = default;
    for (slot, item) in color.iter_mut().zip(items) {
        if let Some(v) = item.as_u64().and_then(|v| u8::try_from(v).ok()) {
            *slot = v;
        }
    }
    color
}

impl Sink for RerunSink {
    fn log(
        &mut self,
        entity: &str,
        msg: &MvizMessage,
        binary: Option<&[u8]>,
    ) -> Result<(), String> {
        let rec = &self.rec;
        let logged = match msg.msg_type.as_str() {
            "points3d" => match (binary, msg.format.as_deref(), msg.count) {
                (Some(binary), Some(format), Some(count)) => {
                    if format != binary_formats::POINTS_XYZ_F32 {
                        return Err(format!("unsupported points3d binary format {format:?}"));
                    }
                    let points = parse_points_xyz_f32(binary, count).ok_or_else(|| {
                        format!(
                            "points3d binary holds {} bytes, fewer than {count} points",
                            binary.len()
                        )
                    })?;
                    let radius = msg
                        .data
                        .get("radius")
                        .and_then(|v| v.as_f64())
                        .map_or(0.03, |v| v as f32);
                    let color = lenient_color(&msg.data, POINTS_COLOR);
                    rec.log(
                        entity,
                        &rerun::Points3D::new(points)
                            .with_radii([radius])
                            .with_colors([rgba(color)]),
                    )
                }
                _ => {
                    let data = parse!(msg, Points3DData);
                    let positions = data
                        .positions
                        .ok_or("points3d needs positions or a binary payload")?;
                    rec.log(
                        entity,
                        &rerun::Points3D::new(positions)
                            .with_radii([data.radius.unwrap_or(0.05)])
                            .with_colors([rgba(data.color.unwrap_or(POINTS_JSON_COLOR))]),
                    )
                }
            },
            "boxes3d" => {
                let data = parse!(msg, Boxes3DData);
                let mut boxes = if data.half_sizes {
                    rerun::Boxes3D::from_centers_and_half_sizes(data.centers, data.sizes)
                } else {
                    rerun::Boxes3D::from_centers_and_sizes(data.centers, data.sizes)
                };
                if let Some(quats) = data.quaternions {
                    boxes = boxes.with_quaternions(
                        quats
                            .into_iter()
                            .map(rerun::Quaternion::from_xyzw)
                            .collect::<Vec<_>>(),
                    );
                }
                rec.log(
                    entity,
                    &boxes.with_colors([rgba(data.color.unwrap_or(BOXES_COLOR))]),
                )
            }
            "arrows3d" => {
                let data = parse!(msg, Arrows3DData);
                rec.log(
                    entity,
                    &rerun::Arrows3D::from_vectors(data.vectors)
                        .with_origins(data.origins)
                        .with_colors([rgba(data.color.unwrap_or(ARROWS_COLOR))]),
                )
            }
            "linestrips3d" => {
                let data = parse!(msg, LineStrips3DData);
                rec.log(
                    entity,
                    &rerun::LineStrips3D::new(data.strips)
                        .with_radii([0.03])
                        .with_colors([rgba(data.color.unwrap_or(STRIPS_COLOR))]),
                )
            }
            "transform3d" => {
                let data = parse!(msg, Transform3DData);
                if let Some(m) = data.matrix4x4 {
                    // Row-major 4x4: the upper-left 3x3 rotation and the last column.
                    rec.log(
                        entity,
                        &rerun::Transform3D::from_mat3x3([
                            [m[0], m[1], m[2]],
                            [m[4], m[5], m[6]],
                            [m[8], m[9], m[10]],
                        ])
                        .with_translation([m[3], m[7], m[11]]),
                    )
                } else if let (Some(t), Some(r)) = (data.translation, data.rotation) {
                    rec.log(
                        entity,
                        &rerun::Transform3D::from_translation(t)
                            .with_quaternion(rerun::Quaternion::from_xyzw(r)),
                    )
                } else if let Some(t) = data.translation {
                    rec.log(entity, &rerun::Transform3D::from_translation(t))
                } else {
                    return Err("transform3d needs a matrix4x4 or a translation".to_string());
                }
            }
            "scalar" => {
                let data = parse!(msg, ScalarData);
                rec.log(entity, &rerun::TextLog::new(format!("{:.4}", data.value)))
            }
            "text" => {
                let data = parse!(msg, TextData);
                rec.log(entity, &rerun::TextLog::new(data.text))
            }
            _ => return Ok(()),
        };
        logged.map_err(|e| format!("Rerun log failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn msg(msg_type: &str, data: Value) -> MvizMessage {
        MvizMessage {
            msg_type: msg_type.to_string(),
            timestamp: None,
            data,
            format: None,
            count: None,
        }
    }

    fn xyz_bytes(points: &[[f32; 3]]) -> Vec<u8> {
        points
            .iter()
            .flatten()
            .flat_map(|f| f.to_le_bytes())
            .collect()
    }

    #[test]
    fn each_supported_type_logs_without_error() {
        let (mut sink, storage) = RerunSink::memory("test");
        sink.flush();
        let baseline = storage.num_msgs(); // store info is already in there
        let bin = xyz_bytes(&[[0.0, 1.0, 2.0], [3.0, 4.0, 5.0]]);
        let mut binary_points = msg("points3d", json!({"radius": 0.1, "color": [255, 0, 0]}));
        binary_points.format = Some(binary_formats::POINTS_XYZ_F32.to_string());
        binary_points.count = Some(2);
        let cases: Vec<(&str, MvizMessage, Option<&[u8]>)> = vec![
            ("pts_bin", binary_points, Some(&bin)),
            (
                "pts_json",
                msg("points3d", json!({"positions": [[0, 0, 0], [1, 1, 1]]})),
                None,
            ),
            (
                "boxes",
                msg(
                    "boxes3d",
                    json!({
                        "centers": [[0, 0, 0]], "sizes": [[1, 1, 1]], "quaternions": [[0, 0, 0, 1]]
                    }),
                ),
                None,
            ),
            (
                "arrows",
                msg(
                    "arrows3d",
                    json!({"origins": [[0, 0, 0]], "vectors": [[1, 0, 0]]}),
                ),
                None,
            ),
            (
                "strips",
                msg("linestrips3d", json!({"strips": [[[0, 0, 0], [1, 1, 1]]]})),
                None,
            ),
            (
                "tf_mat",
                msg(
                    "transform3d",
                    json!({"matrix4x4": [
                1, 0, 0, 5, 0, 1, 0, 6, 0, 0, 1, 7, 0, 0, 0, 1]}),
                ),
                None,
            ),
            (
                "tf_quat",
                msg(
                    "transform3d",
                    json!({
                "translation": [1, 2, 3], "rotation": [0, 0, 0, 1]}),
                ),
                None,
            ),
            (
                "tf_trans",
                msg("transform3d", json!({"translation": [1, 2, 3]})),
                None,
            ),
            ("scalar", msg("scalar", json!({"value": 1.5})), None),
            ("text", msg("text", json!({"text": "hello"})), None),
        ];
        for (entity, m, binary) in &cases {
            let result = sink.log(entity, m, *binary);
            assert_eq!(result, Ok(()), "{entity}");
        }
        sink.flush();
        assert!(
            storage.num_msgs() > baseline,
            "nothing reached the recording"
        );
    }

    #[test]
    fn short_point_binary_is_an_error_not_a_panic() {
        let (mut sink, _storage) = RerunSink::memory("test");
        let mut m = msg("points3d", json!({}));
        m.format = Some(binary_formats::POINTS_XYZ_F32.to_string());
        m.count = Some(10);
        assert!(sink.log("p", &m, Some(&[0u8; 5])).is_err());
    }

    #[test]
    fn binary_points_without_a_payload_fall_back_to_json_and_fail_cleanly() {
        let (mut sink, _storage) = RerunSink::memory("test");
        let mut m = msg("points3d", json!({}));
        m.format = Some(binary_formats::POINTS_XYZ_F32.to_string());
        m.count = Some(2);
        assert!(sink.log("p", &m, None).is_err());
    }

    #[test]
    fn unknown_type_is_ignored() {
        let (mut sink, storage) = RerunSink::memory("test");
        sink.flush();
        let baseline = storage.num_msgs();
        assert_eq!(sink.log("x", &msg("foo", json!({})), None), Ok(()));
        sink.flush();
        assert_eq!(storage.num_msgs(), baseline);
    }

    #[test]
    fn malformed_data_never_panics() {
        let (mut sink, _storage) = RerunSink::memory("test");
        let bad: Vec<Value> = vec![
            Value::Null,
            json!(7),
            json!("text"),
            json!([]),
            json!({}),
            json!({"positions": "x"}),
            json!({"positions": [[1, 2]]}),
            json!({"positions": [[1, 2, 3, 4]]}),
            json!({"centers": [[0, 0, 0]]}),
            json!({"centers": [[0, 0]], "sizes": [[1, 1, 1]]}),
            json!({"centers": [[0, 0, 0]], "sizes": [[1, 1, 1]], "quaternions": [[0, 0, 1]]}),
            json!({"origins": [[0, 0, 0]]}),
            json!({"origins": [[0, 0, 0]], "vectors": "x"}),
            json!({"strips": [[[0, 0]]]}),
            json!({"strips": 3}),
            json!({"matrix4x4": [1, 2, 3]}),
            json!({"translation": [1, 2]}),
            json!({"translation": [1, 2, 3], "rotation": [1]}),
            json!({"text": 5}),
            json!({"value": "high"}),
            json!({"color": "red", "radius": "big"}),
            json!({"color": [1, 2], "positions": [[0, 0, 0]]}),
        ];
        for ty in [
            "points3d",
            "boxes3d",
            "arrows3d",
            "linestrips3d",
            "transform3d",
            "scalar",
            "text",
        ] {
            for data in &bad {
                // Ok or Err are both acceptable; reaching the next line is the test.
                let _ = sink.log("bad", &msg(ty, data.clone()), None);
            }
        }
        let mut binary = msg("points3d", Value::Null);
        binary.format = Some(binary_formats::POINTS_XYZ_F32.to_string());
        binary.count = Some(u32::MAX);
        assert!(sink.log("bad", &binary, Some(&[0u8; 24])).is_err());
    }

    #[test]
    fn wrong_shapes_are_errors() {
        let (mut sink, _storage) = RerunSink::memory("test");
        for (ty, data) in [
            ("points3d", json!({"positions": [[1, 2]]})),
            (
                "boxes3d",
                json!({"centers": [[0, 0]], "sizes": [[1, 1, 1]]}),
            ),
            ("arrows3d", json!({"origins": [[0, 0, 0]]})),
            ("linestrips3d", json!({"strips": 3})),
            ("transform3d", json!({"translation": [1, 2]})),
            ("scalar", json!({"value": "high"})),
            ("text", json!({"text": 5})),
        ] {
            assert!(sink.log("bad", &msg(ty, data), None).is_err(), "{ty}");
        }
    }

    /// Runs in a child process with an empty `PATH` so other tests keep theirs.
    #[test]
    fn spawn_without_rerun_binary_returns_clear_error() {
        if std::env::var_os("DVIZ_SPAWN_CHILD").is_some() {
            let err = RerunSink::spawn("dorobot-dviz-test")
                .err()
                .expect("no viewer on PATH");
            assert!(err.to_lowercase().contains("rerun"), "unclear error: {err}");
            return;
        }
        let exe = std::env::current_exe().expect("test binary path");
        let output = std::process::Command::new(exe)
            .args([
                "--exact",
                "rerun_sink::tests::spawn_without_rerun_binary_returns_clear_error",
            ])
            .env("PATH", "")
            .env("DVIZ_SPAWN_CHILD", "1")
            .output()
            .expect("run the test binary");
        assert!(
            output.status.success(),
            "child failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
