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
use std::{
    io::Read,
    net::{SocketAddr, TcpStream},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// How long `rerun --version` may take.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a starting viewer is checked.
const POLL: Duration = Duration::from_millis(50);

pub struct RerunSink {
    rec: RecordingStream,
}

/// Which viewer `RerunSink::launch` starts, and where it listens.
#[derive(Debug, Clone)]
pub struct ViewerOptions {
    /// The viewer executable, looked up on `PATH` unless it is a path.
    pub executable: String,
    /// The viewer's gRPC port on 127.0.0.1.
    pub port: u16,
    /// How long a started viewer may take to open its port.
    pub bind_timeout: Duration,
}

impl Default for ViewerOptions {
    fn default() -> Self {
        Self {
            executable: "rerun".into(),
            port: rerun::SpawnOptions::default().port,
            bind_timeout: Duration::from_secs(10),
        }
    }
}

impl RerunSink {
    /// Opens a Rerun viewer and streams into it; returns the sink and the
    /// viewer's version. First `<executable> --version` must report the
    /// SDK's major.minor. Then the viewer is started, unless something
    /// already listens on the port (taken for a viewer, as Rerun's own spawn
    /// does), and the sink is made only once the port accepts: a stream to a
    /// viewer that never comes up would queue every message without bound.
    ///
    /// Blocks for up to `VERSION_TIMEOUT` plus `bind_timeout`, so never call
    /// it on the UI thread; `DvizSession::start_viewer` runs it off it.
    pub fn launch(app_id: &str, options: &ViewerOptions) -> Result<(RerunSink, String), String> {
        let version = viewer_version(&options.executable)?;
        let addr = SocketAddr::from(([127, 0, 0, 1], options.port));
        if !accepts(addr) {
            start_viewer(options, addr)?;
        }
        let rec = RecordingStreamBuilder::new(app_id)
            .connect_grpc_opts(format!("rerun+http://{addr}/proxy"))
            .map_err(|e| format!("Could not connect to the Rerun viewer at {addr}: {e}"))?;
        Ok((RerunSink { rec }, version))
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

/// How to install the viewer that matches the SDK.
fn install_hint() -> String {
    let sdk = rerun::build_info().version;
    format!("install it with `cargo install rerun-cli --version {sdk} --locked`")
}

/// The viewer's version, if `<executable> --version` reports the SDK's
/// major.minor (Rerun breaks compatibility on the minor before 1.0).
fn viewer_version(executable: &str) -> Result<String, String> {
    let output = run_version(executable)?;
    let sdk = rerun::build_info().version;
    let Some((major, minor, version)) = parse_version(&output) else {
        let line: String = output
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();
        return Err(format!(
            "Could not read the Rerun viewer version: `{executable} --version` printed {line:?}; \
             Studio needs Rerun {}.{}",
            sdk.major, sdk.minor
        ));
    };
    if (major, minor) != (sdk.major, sdk.minor) {
        return Err(format!(
            "Rerun viewer {version} (`{executable}`) does not match Studio's Rerun SDK {sdk}; {}",
            install_hint()
        ));
    }
    Ok(version)
}

/// What `<executable> --version` prints (at most 4 KiB), waiting at most
/// `VERSION_TIMEOUT`.
fn run_version(executable: &str) -> Result<String, String> {
    let mut child = Command::new(executable)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!(
                "The Rerun viewer `{executable}` is not on PATH; {}",
                install_hint()
            ),
            _ => format!("Could not run `{executable} --version`: {e}"),
        })?;
    let deadline = Instant::now() + VERSION_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "`{executable} --version` did not answer within {} s",
                    VERSION_TIMEOUT.as_secs()
                ));
            }
            Err(e) => return Err(format!("Could not run `{executable} --version`: {e}")),
        }
    }
    let mut output = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        // A read error leaves the output short, which then reads as no version.
        let _ = stdout.take(4096).read_to_end(&mut output);
    }
    Ok(String::from_utf8_lossy(&output).into_owned())
}

/// Major, minor and the whole version from a `--version` line such as
/// `rerun-cli 0.28.2 [rustc 1.88.0, LLVM 20.1.5] aarch64-unknown-linux-gnu …`.
fn parse_version(output: &str) -> Option<(u8, u8, String)> {
    let version = output.split_whitespace().nth(1)?;
    let mut numbers = version.split('.');
    let major = numbers.next()?.parse().ok()?;
    let minor = numbers.next()?.parse().ok()?;
    Some((major, minor, version.to_string()))
}

/// Starts the viewer and waits until `addr` accepts. Fails if the viewer
/// exits first, or does not open its port within `bind_timeout` (it is then
/// killed).
fn start_viewer(options: &ViewerOptions, addr: SocketAddr) -> Result<(), String> {
    // The same arguments as Rerun's own spawn.
    let defaults = rerun::SpawnOptions::default();
    let mut command = Command::new(&options.executable);
    command
        .stdin(Stdio::null())
        .arg(format!("--port={}", options.port))
        .arg(format!("--memory-limit={}", defaults.memory_limit))
        .arg(format!(
            "--server-memory-limit={}",
            defaults.server_memory_limit
        ))
        .arg("--expect-data-soon");
    // Its own process group, as Rerun detaches it: a Ctrl-C meant for
    // Studio leaves the viewer open.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| {
        format!(
            "Could not start the Rerun viewer `{}`: {e}",
            options.executable
        )
    })?;
    let deadline = Instant::now() + options.bind_timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(format!(
                    "The Rerun viewer exited ({status}) before opening port {}",
                    options.port
                ))
            }
            Ok(None) => {}
            Err(e) => return Err(format!("Lost track of the Rerun viewer: {e}")),
        }
        if accepts(addr) {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "The Rerun viewer did not open port {} within {} s",
                options.port,
                options.bind_timeout.as_secs()
            ));
        }
        thread::sleep(POLL);
    }
    // Reaps the viewer once it is closed, so it does not linger as a zombie.
    let _ = thread::Builder::new()
        .name("dviz-viewer-wait".into())
        .spawn(move || child.wait());
    Ok(())
}

fn accepts(addr: SocketAddr) -> bool {
    TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
}

const POINTS_COLOR: [u8; 4] = [100, 200, 255, 255];
const POINTS_JSON_COLOR: [u8; 4] = [200, 200, 200, 255];
const BOXES_COLOR: [u8; 4] = [100, 150, 200, 255];
const ARROWS_COLOR: [u8; 4] = [255, 100, 100, 255];
const STRIPS_COLOR: [u8; 4] = [100, 200, 255, 255];
/// Length (m) of the axes drawn at every `transform3d` entity.
const TRANSFORM_AXIS_LENGTH: f32 = 0.3;

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
                let transform = if let Some(m) = data.matrix4x4 {
                    // Row-major 4x4: the upper-left 3x3 rotation and the last column.
                    rerun::Transform3D::from_mat3x3([
                        [m[0], m[1], m[2]],
                        [m[4], m[5], m[6]],
                        [m[8], m[9], m[10]],
                    ])
                    .with_translation([m[3], m[7], m[11]])
                } else if let (Some(t), Some(r)) = (data.translation, data.rotation) {
                    rerun::Transform3D::from_translation(t)
                        .with_quaternion(rerun::Quaternion::from_xyzw(r))
                } else if let Some(t) = data.translation {
                    rerun::Transform3D::from_translation(t)
                } else {
                    return Err("transform3d needs a matrix4x4 or a translation".to_string());
                };
                // Rerun 0.28 draws nothing for a bare Transform3D; the axes make it visible.
                rec.log(entity, &transform).and_then(|()| {
                    rec.log(entity, &rerun::TransformAxes3D::new(TRANSFORM_AXIS_LENGTH))
                })
            }
            "scalar" => {
                // A time series: Rerun plots it as a curve over log time.
                let data = parse!(msg, ScalarData);
                rec.log(entity, &rerun::Scalars::single(data.value))
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

    /// The Arrow schemas (with their metadata) of every chunk recorded so far
    /// for `entity`, as text.
    fn recorded_schemas(storage: &MemorySinkStorage, entity: &str) -> Vec<String> {
        storage
            .take()
            .iter()
            .filter_map(|msg| match msg {
                rerun::log::LogMsg::ArrowMsg(_, arrow) => {
                    Some(format!("{:?}", arrow.batch.schema()))
                }
                _ => None,
            })
            .filter(|schema| schema.contains(&format!("\"/{entity}\"")))
            .collect()
    }

    #[test]
    fn scalar_is_logged_as_a_time_series() {
        let (mut sink, storage) = RerunSink::memory("test");
        assert_eq!(
            sink.log("imu/accel_x", &msg("scalar", json!({"value": 1.5})), None),
            Ok(())
        );
        sink.flush();
        let schemas = recorded_schemas(&storage, "imu/accel_x");
        assert!(!schemas.is_empty(), "nothing recorded for imu/accel_x");
        assert!(
            schemas
                .iter()
                .any(|schema| schema.contains("Scalars:scalars")),
            "no Scalars component: {schemas:?}"
        );
        assert!(
            !schemas.iter().any(|schema| schema.contains("TextLog")),
            "logged as text: {schemas:?}"
        );
    }

    #[test]
    fn transform_logs_axes_too() {
        // Rerun 0.28 draws nothing for a bare Transform3D; TransformAxes3D does.
        let cases = [
            json!({"matrix4x4": [1, 0, 0, 5, 0, 1, 0, 6, 0, 0, 1, 7, 0, 0, 0, 1]}),
            json!({"translation": [1, 2, 3], "rotation": [0, 0, 0, 1]}),
            json!({"translation": [1, 2, 3]}),
        ];
        for data in cases {
            let (mut sink, storage) = RerunSink::memory("test");
            let m = msg("transform3d", data.clone());
            assert_eq!(sink.log("robot/pose", &m, None), Ok(()));
            sink.flush();
            let batches: Vec<_> = storage
                .take()
                .into_iter()
                .filter_map(|msg| match msg {
                    rerun::log::LogMsg::ArrowMsg(_, arrow) => Some(arrow.batch.clone()),
                    _ => None,
                })
                .filter(|b| format!("{:?}", b.schema()).contains("\"/robot/pose\""))
                .collect();
            let schemas: Vec<String> = batches
                .iter()
                .map(|b| format!("{:?}", b.schema()))
                .collect();
            let axes: Vec<String> = batches
                .iter()
                .flat_map(|b| {
                    b.schema()
                        .fields()
                        .iter()
                        .zip(b.columns())
                        .filter(|(f, _)| f.name().starts_with("TransformAxes3D:"))
                        .map(|(_, c)| format!("{c:?}"))
                        .collect::<Vec<_>>()
                })
                .collect();
            assert!(
                axes.iter().any(|c| c.contains("0.3")),
                "axis length 0.3 not recorded for {data}: {axes:?}"
            );
            assert!(
                schemas.iter().any(|s| s.contains("TransformAxes3D:")),
                "no TransformAxes3D component for {data}: {schemas:?}"
            );
            assert!(
                schemas.iter().any(|s| s.contains("Transform3D:")),
                "transform itself missing for {data}: {schemas:?}"
            );
        }
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
    /// (Named after the plan's Review Focus 4; the launch replaced `spawn`.)
    #[test]
    fn spawn_without_rerun_binary_returns_clear_error() {
        if std::env::var_os("DVIZ_SPAWN_CHILD").is_some() {
            let err = RerunSink::launch("dorobot-dviz-test", &options(free_port()))
                .err()
                .expect("no viewer on PATH");
            assert!(err.to_lowercase().contains("rerun"), "unclear error: {err}");
            assert!(err.contains("PATH"), "unclear error: {err}");
            return;
        }
        crate::test_child::assert_passes(
            "rerun_sink::tests::spawn_without_rerun_binary_returns_clear_error",
            &[("PATH", "".as_ref()), ("DVIZ_SPAWN_CHILD", "1".as_ref())],
        );
    }

    #[test]
    fn viewer_versions_are_read_from_the_version_line() {
        assert_eq!(
            parse_version("rerun-cli 0.28.2 [rustc 1.88.0, LLVM 20.1.5] aarch64-unknown-linux-gnu"),
            Some((0, 28, "0.28.2".to_string()))
        );
        assert_eq!(
            parse_version("rerun 0.28.0-rc.1\n"),
            Some((0, 28, "0.28.0-rc.1".to_string()))
        );
        assert_eq!(
            parse_version("rerun 1.2.3"),
            Some((1, 2, "1.2.3".to_string()))
        );
        for unreadable in [
            "",
            "rerun",
            "Rerun!",
            "rerun x.y.z",
            "rerun 0",
            "rerun 0.999.1",
        ] {
            assert_eq!(parse_version(unreadable), None, "{unreadable:?}");
        }
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// A free port, so no viewer already running on 9876 gets in the way.
    fn options(port: u16) -> ViewerOptions {
        ViewerOptions {
            port,
            bind_timeout: Duration::from_secs(5),
            ..ViewerOptions::default()
        }
    }

    #[cfg(unix)]
    #[test]
    fn launch_refuses_a_viewer_of_another_version() {
        const TEST: &str = "rerun_sink::tests::launch_refuses_a_viewer_of_another_version";
        let script = fake_rerun::script(&fake_rerun::version_reply("0.20.3"), "exit 0");
        let Some(dir) = fake_rerun::run_in_child(TEST, &script) else {
            return;
        };
        let err = RerunSink::launch("t", &options(free_port()))
            .err()
            .expect("a 0.20 viewer was accepted");
        let sdk = fake_rerun::sdk_version();
        assert!(err.contains("0.20.3") && err.contains(&sdk), "{err}");
        assert!(
            !dir.join("started").exists(),
            "the wrong viewer was started"
        );
    }

    #[cfg(unix)]
    #[test]
    fn launch_refuses_an_unreadable_version() {
        const TEST: &str = "rerun_sink::tests::launch_refuses_an_unreadable_version";
        let script = fake_rerun::script("echo 'Rerun!'", "exit 0");
        let Some(dir) = fake_rerun::run_in_child(TEST, &script) else {
            return;
        };
        let err = RerunSink::launch("t", &options(free_port()))
            .err()
            .expect("a viewer of unknown version was accepted");
        assert!(err.contains("version") && err.contains("Rerun!"), "{err}");
        assert!(!dir.join("started").exists(), "the viewer was started");
    }

    #[cfg(unix)]
    #[test]
    fn launch_reports_a_viewer_that_exits_at_once() {
        const TEST: &str = "rerun_sink::tests::launch_reports_a_viewer_that_exits_at_once";
        let reply = fake_rerun::version_reply(&fake_rerun::sdk_version());
        let script = fake_rerun::script(&reply, "exit 3");
        let Some(dir) = fake_rerun::run_in_child(TEST, &script) else {
            return;
        };
        let port = free_port();
        let err = RerunSink::launch("t", &options(port))
            .err()
            .expect("a dead viewer was accepted");
        assert!(err.contains("exited") && err.contains('3'), "{err}");
        assert!(err.contains(&port.to_string()), "{err}");
        assert!(dir.join("started").exists(), "the viewer never ran");
    }

    #[cfg(unix)]
    #[test]
    fn launch_gives_up_on_a_viewer_that_never_opens_its_port() {
        const TEST: &str =
            "rerun_sink::tests::launch_gives_up_on_a_viewer_that_never_opens_its_port";
        let reply = fake_rerun::version_reply(&fake_rerun::sdk_version());
        let script = fake_rerun::script(&reply, "exec sleep 30");
        if fake_rerun::run_in_child(TEST, &script).is_none() {
            return;
        }
        let started = std::time::Instant::now();
        let err = RerunSink::launch(
            "t",
            &ViewerOptions {
                bind_timeout: Duration::from_secs(1),
                ..options(free_port())
            },
        )
        .err()
        .expect("a viewer without a port was accepted");
        assert!(err.contains("did not open port"), "{err}");
        let took = started.elapsed();
        assert!(took < Duration::from_secs(4), "took {took:?}");
    }

    #[cfg(unix)]
    #[test]
    fn launch_streams_into_a_viewer_already_listening() {
        const TEST: &str = "rerun_sink::tests::launch_streams_into_a_viewer_already_listening";
        let reply = fake_rerun::version_reply(&fake_rerun::sdk_version());
        let script = fake_rerun::script(&reply, "exit 0");
        let Some(dir) = fake_rerun::run_in_child(TEST, &script) else {
            return;
        };
        // Stands in for a viewer that is already open.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sink, version) = RerunSink::launch("t", &options(port)).expect("launch");
        // First, so a failed assertion cannot hang: dropping would flush
        // towards a listener that never speaks gRPC, without end.
        std::mem::forget(sink);
        assert_eq!(version, fake_rerun::sdk_version());
        assert!(!dir.join("started").exists(), "a second viewer was started");
    }
}

/// A fake `rerun` for tests. `PATH` is process-wide, so a test that needs
/// it runs itself again in a child process of the test binary, with the fake
/// first on `PATH`; other tests keep theirs.
#[cfg(all(test, unix))]
pub(crate) mod fake_rerun {
    use std::{env, fs, os::unix::fs::PermissionsExt, path::PathBuf};

    /// Set in the child: the directory that holds the fake.
    const DIR_VAR: &str = "DVIZ_FAKE_RERUN_DIR";

    /// The Rerun SDK version this crate is built with, e.g. `0.28.2`.
    pub fn sdk_version() -> String {
        rerun::build_info().version.to_string()
    }

    /// Shell that prints a `rerun --version` line for `version`.
    pub fn version_reply(version: &str) -> String {
        format!("echo 'rerun {version} [rustc 1.80.0, LLVM 18.1.7] aarch64-unknown-linux-gnu'")
    }

    /// A `rerun` that runs `on_version` for `--version`; started as a viewer
    /// it touches `started` next to itself, then runs `as_viewer`.
    pub fn script(on_version: &str, as_viewer: &str) -> String {
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then\n{on_version}\nexit 0\nfi\n\
             touch \"$(dirname \"$0\")/started\"\n{as_viewer}\n"
        )
    }

    /// In the test process: writes `script` as `rerun` into a fresh
    /// directory, runs the test `test` (its full path) alone in a child with
    /// that directory first on `PATH`, asserts that it passed in time and
    /// returns `None`. In the child: returns the directory; the test goes on.
    pub fn run_in_child(test: &str, script: &str) -> Option<PathBuf> {
        if let Some(dir) = env::var_os(DIR_VAR) {
            return Some(dir.into());
        }
        let dir = env::temp_dir().join(format!(
            "dviz-fake-rerun-{}-{}",
            std::process::id(),
            test.replace("::", "-")
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create the fake rerun directory");
        let exe = dir.join("rerun");
        fs::write(&exe, script).expect("write the fake rerun");
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).expect("chmod");
        let path = env::var_os("PATH").unwrap_or_default();
        let path = env::join_paths(std::iter::once(dir.clone()).chain(env::split_paths(&path)))
            .expect("PATH");
        // Removes the directory even when the child fails.
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(dir.clone());
        crate::test_child::assert_passes(
            test,
            &[("PATH", path.as_os_str()), (DIR_VAR, dir.as_os_str())],
        );
        None
    }
}
