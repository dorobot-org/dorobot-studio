//! Writes dviz messages into a Rerun recording. Ported from dviz-shell's
//! `log_vis_data_to_rerun`; compiled only with the `rerun` feature.
//!
//! `Sink::log` and `Sink::clear` run on the session's receive thread, where a
//! panic costs the 3D window (the session swaps in a discarding sink), so
//! nothing here may panic on message data: every shape is checked through
//! serde or by length, and bad input comes back as `Err`.

use crate::map_view::{grid_rgba, nav2_to_rviz, rviz_costmap_color};
use crate::nav_scene::{nav_layer, NavLayer};
use crate::route::Sink;
use dviz_protocol::{
    binary_formats, nav_entities, parse_grid2d, parse_points_xyz_f32, Arrows3DData, Boxes3DData,
    Grid2DHeader, GridKind, LineStrips3DData, MvizMessage, Points3DData, ScalarData, TextData,
    Transform3DData,
};
use rerun::sink::MemorySinkStorage;
use rerun::{RecordingStream, RecordingStreamBuilder};
use std::{
    collections::{BTreeMap, BTreeSet},
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
    /// Entity paths this sink logged static data to; see `clear`.
    statics: BTreeSet<String>,
    /// Per grid lines path, the map extent (whole metres) drawn there.
    grid_lines: BTreeMap<String, [i64; 4]>,
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
        Ok((RerunSink::new(rec), version))
    }

    /// Wraps `rec`, whose world is Z-up (as the robot's map frame is).
    fn new(rec: RecordingStream) -> Self {
        // A failure only leaves Rerun's default (Y-up) view, so it is not fatal.
        let _ = rec.log_static("/", &rerun::ViewCoordinates::RIGHT_HAND_Z_UP());
        Self {
            rec,
            statics: BTreeSet::new(),
            grid_lines: BTreeMap::new(),
        }
    }

    /// Records into memory; for tests.
    pub fn memory(app_id: &str) -> (RerunSink, MemorySinkStorage) {
        let (rec, storage) = RecordingStreamBuilder::new(app_id)
            .memory()
            .expect("an in-memory recording needs no resources");
        (RerunSink::new(rec), storage)
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
/// Line radius (m) of strips that do not give one.
const STRIPS_RADIUS: f32 = 0.03;
/// Height (m) that gets the first and the last colour of a coloured cloud.
const CLOUD_Z_MIN: f32 = -0.5;
const CLOUD_Z_MAX: f32 = 2.5;
/// The live cloud's points: small and 0.6 opaque, so the robot and the
/// lines stay visible inside it.
const CLOUD_RADIUS: f32 = 0.01;
const CLOUD_ALPHA: u8 = 153;
/// Length (m) of the axes drawn at every `transform3d` entity.
const TRANSFORM_AXIS_LENGTH: f32 = 0.3;
/// The robot pose's heading arrow, `<pose>/heading`.
const HEADING_LENGTH: f32 = 0.4;
const HEADING_RADIUS: f32 = 0.015;
const HEADING_COLOR: [u8; 4] = [255, 25, 0, 255];
/// The goal's ring, `<goal>/ring`, in the goal's colour (this one when the
/// message has none) and line radius (this one when it has none).
const GOAL_RING_RADIUS: f32 = 0.15;
const GOAL_RING_SEGMENTS: usize = 24;
const GOAL_COLOR: [u8; 4] = [255, 170, 0, 255];
const GOAL_LINE_RADIUS: f32 = 0.015;
/// The costmap lies this far (m) above the map, which is at z = 0.
const COSTMAP_Z: f32 = 0.01;
/// Most quads in one costmap mesh (4 vertices each, about 90 bytes): a
/// costmap that would need more is refused, not drawn.
const COSTMAP_MAX_QUADS: usize = 100_000;
/// The ground grid, `<robot>/view/grid`: lines every whole metre over the
/// map's extent, just above the map and under the costmap.
const GRID_LINES_Z: f32 = 0.002;
const GRID_LINES_RADIUS: f32 = 0.003;
const GRID_LINES_COLOR: [u8; 4] = [160, 160, 164, 128];
/// More lines than this (a map over about 1 km across) are not drawn.
const GRID_LINES_MAX: usize = 2000;

/// The turbo colour map at `t` in 0..=1 (Mikhailov's polynomial fit).
fn turbo(t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let channel = |c: [f32; 6]| {
        let v = c[0] + t * (c[1] + t * (c[2] + t * (c[3] + t * (c[4] + t * c[5]))));
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    [
        channel([
            0.135_721_38,
            4.615_392_6,
            -42.660_322_58,
            132.131_082_34,
            -152.942_393_96,
            59.286_379_43,
        ]),
        channel([
            0.091_402_61,
            2.194_188_39,
            4.842_966_58,
            -14.185_033_33,
            4.277_298_57,
            2.829_566_04,
        ]),
        channel([
            0.106_673_3,
            12.641_946_08,
            -60.582_048_36,
            110.362_767_71,
            -89.903_109_12,
            27.348_249_73,
        ]),
        255,
    ]
}

/// Turbo colour of each point by height, from `CLOUD_Z_MIN` to `CLOUD_Z_MAX`,
/// at the cloud's alpha.
fn height_colors(points: &[[f32; 3]]) -> Vec<rerun::Color> {
    points
        .iter()
        .map(|p| {
            // A NaN height takes the lowest colour.
            let z = if p[2].is_nan() { CLOUD_Z_MIN } else { p[2] };
            let [r, g, b, _] = turbo((z - CLOUD_Z_MIN) / (CLOUD_Z_MAX - CLOUD_Z_MIN));
            rgba([r, g, b, CLOUD_ALPHA])
        })
        .collect()
}

/// The `grid2d` picture as a flat two-triangle quad on the ground (z = 0):
/// the lower-left corner at `origin`, `width * resolution` along x and
/// `height * resolution` along y, turned by the origin's yaw.
fn grid_mesh(header: &Grid2DHeader, cells: &[u8]) -> Result<rerun::Mesh3D, String> {
    let [ox, oy, yaw] = header.origin;
    if !header.origin.iter().all(|v| v.is_finite()) {
        return Err("grid2d origin is not finite".to_string());
    }
    let w = header.width as f32 * header.resolution;
    let h = header.height as f32 * header.resolution;
    let (sin, cos) = yaw.sin_cos();
    let corner = |x: f32, y: f32| [ox + x * cos - y * sin, oy + x * sin + y * cos, 0.0];
    // The picture's row 0 is the largest y (see `grid_rgba`), so v = 1 is y = 0.
    Ok(rerun::Mesh3D::new([
        corner(0.0, 0.0),
        corner(w, 0.0),
        corner(w, h),
        corner(0.0, h),
    ])
    .with_triangle_indices([[0, 1, 2], [0, 2, 3]])
    .with_vertex_texcoords([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
    .with_albedo_texture(
        rerun::components::ImageFormat::rgba8([header.width, header.height]),
        grid_rgba(header, cells),
    ))
}

/// A Nav2 costmap as one flat mesh at `COSTMAP_Z`: a quad per run of
/// neighbouring cells in a row with the same RViz costmap colour, coloured
/// per vertex; free cells (RViz 0, transparent) are left out. Vertex colours
/// and not a texture, because Rerun 0.28 draws a texture without its alpha
/// and the costmap must let the map show through. Row 0 is the smallest y,
/// as on the wire. More than `COSTMAP_MAX_QUADS` quads is an error.
fn costmap_mesh(header: &Grid2DHeader, cells: &[u8]) -> Result<rerun::Mesh3D, String> {
    let [ox, oy, yaw] = header.origin;
    if !header.origin.iter().all(|v| v.is_finite()) {
        return Err("grid2d origin is not finite".to_string());
    }
    let palette: [[u8; 4]; 256] =
        std::array::from_fn(|cost| rviz_costmap_color(nav2_to_rviz(cost as u8)));
    let res = header.resolution;
    let (sin, cos) = yaw.sin_cos();
    let corner = |x: f32, y: f32| [ox + x * cos - y * sin, oy + x * sin + y * cos, COSTMAP_Z];
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut triangles = Vec::new();
    for (row, costs) in cells.chunks(header.width as usize).enumerate() {
        let (y0, y1) = (row as f32 * res, (row + 1) as f32 * res);
        let mut start = 0;
        while start < costs.len() {
            let color = palette[usize::from(costs[start])];
            let mut end = start + 1;
            while end < costs.len() && palette[usize::from(costs[end])] == color {
                end += 1;
            }
            let (x0, x1) = (start as f32 * res, end as f32 * res);
            start = end;
            if color[3] == 0 {
                continue; // free
            }
            if positions.len() / 4 >= COSTMAP_MAX_QUADS {
                return Err(format!(
                    "costmap needs more than {COSTMAP_MAX_QUADS} quads; not drawn"
                ));
            }
            let first = positions.len() as u32;
            positions.extend([
                corner(x0, y0),
                corner(x1, y0),
                corner(x1, y1),
                corner(x0, y1),
            ]);
            colors.extend([rgba(color); 4]);
            triangles.extend([[first, first + 1, first + 2], [first, first + 2, first + 3]]);
        }
    }
    Ok(rerun::Mesh3D::new(positions)
        .with_triangle_indices(triangles)
        .with_vertex_colors(colors))
}

/// The map's extent rounded out to whole metres, `[x0, x1, y0, y1]`; `None`
/// if it is not finite or would need more than `GRID_LINES_MAX` lines.
fn grid_extent(header: &Grid2DHeader) -> Option<[i64; 4]> {
    let [ox, oy, yaw] = header.origin.map(f64::from);
    let w = f64::from(header.width) * f64::from(header.resolution);
    let h = f64::from(header.height) * f64::from(header.resolution);
    let (sin, cos) = yaw.sin_cos();
    let corners = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
        .map(|[x, y]| [ox + x * cos - y * sin, oy + x * sin + y * cos]);
    if !corners.iter().flatten().all(|v| v.is_finite()) {
        return None;
    }
    let low = |i: usize| corners.iter().map(|c| c[i]).fold(f64::INFINITY, f64::min);
    let high = |i: usize| {
        corners
            .iter()
            .map(|c| c[i])
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let extent = [
        low(0).floor(),
        high(0).ceil(),
        low(1).floor(),
        high(1).ceil(),
    ];
    let lines = (extent[1] - extent[0]) + (extent[3] - extent[2]) + 2.0;
    (lines <= GRID_LINES_MAX as f64).then(|| extent.map(|v| v as i64))
}

/// A line at every whole metre of `extent` (see `grid_extent`), each
/// spanning it, at `GRID_LINES_Z`.
fn grid_lines([x0, x1, y0, y1]: [i64; 4]) -> Vec<[[f32; 3]; 2]> {
    let z = GRID_LINES_Z;
    let vertical = (x0..=x1).map(|x| [[x as f32, y0 as f32, z], [x as f32, y1 as f32, z]]);
    let horizontal = (y0..=y1).map(|y| [[x0 as f32, y as f32, z], [x1 as f32, y as f32, z]]);
    vertical.chain(horizontal).collect()
}

/// The goal's ring around `center`, closed (the first point again at the end).
fn goal_ring(center: [f32; 3]) -> Vec<[f32; 3]> {
    let step = std::f32::consts::TAU / GOAL_RING_SEGMENTS as f32;
    (0..=GOAL_RING_SEGMENTS)
        .map(|i| {
            let (sin, cos) = (step * (i % GOAL_RING_SEGMENTS) as f32).sin_cos();
            [
                center[0] + GOAL_RING_RADIUS * cos,
                center[1] + GOAL_RING_RADIUS * sin,
                center[2],
            ]
        })
        .collect()
}

/// A line radius from a message, if it gave a usable one.
fn line_radius(radius: Option<f32>) -> Option<f32> {
    radius.filter(|r| r.is_finite())
}

/// Whether the entity path `path` is `entity` or below it.
fn at_or_under(path: &str, entity: &str) -> bool {
    path.strip_prefix(entity)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

fn rgba(c: [u8; 4]) -> rerun::Color {
    rerun::Color::from_unmultiplied_rgba(c[0], c[1], c[2], c[3])
}

fn log_failed(e: rerun::RecordingStreamError) -> String {
    format!("Rerun log failed: {e}")
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

impl RerunSink {
    /// Logs `what` at `entity` on the timeline.
    fn log_now(&self, entity: &str, what: &impl rerun::AsComponents) -> Result<(), String> {
        self.rec.log(entity, what).map_err(log_failed)
    }

    /// Logs `what` at `entity` as static (on every timeline, also for a
    /// viewer that connects later) and remembers the path for `clear`.
    fn log_static(&mut self, entity: &str, what: &impl rerun::AsComponents) -> Result<(), String> {
        self.statics.insert(entity.to_string());
        self.rec.log_static(entity, what).map_err(log_failed)
    }

    /// The 1 m ground grid over the extent of the map at `entity`, at
    /// `<robot>/view/grid`. Static, and logged again only when the extent
    /// (in whole metres) changes, not for every resend of the map.
    fn log_grid_lines(&mut self, entity: &str, header: &Grid2DHeader) -> Result<(), String> {
        let robot = entity.strip_suffix(nav_entities::MAP_GRID).unwrap_or("");
        let path = format!("{robot}view/grid");
        let Some(extent) = grid_extent(header) else {
            return Ok(()); // a map too large for lines still shows
        };
        if self.grid_lines.get(&path) == Some(&extent) {
            return Ok(());
        }
        let lines = rerun::LineStrips3D::new(grid_lines(extent))
            .with_radii([GRID_LINES_RADIUS])
            .with_colors([rgba(GRID_LINES_COLOR)]);
        self.log_static(&path, &lines)?;
        self.grid_lines.insert(path, extent);
        Ok(())
    }
}

impl Sink for RerunSink {
    fn log(
        &mut self,
        entity: &str,
        msg: &MvizMessage,
        binary: Option<&[u8]>,
    ) -> Result<(), String> {
        let layer = nav_layer(entity);
        match msg.msg_type.as_str() {
            "points3d" => {
                let (points, radius, color) = match (binary, msg.format.as_deref(), msg.count) {
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
                        (points, radius, lenient_color(&msg.data, POINTS_COLOR))
                    }
                    _ => {
                        let data = parse!(msg, Points3DData);
                        let positions = data
                            .positions
                            .ok_or("points3d needs positions or a binary payload")?;
                        let color = data.color.unwrap_or(POINTS_JSON_COLOR);
                        (positions, data.radius.unwrap_or(0.05), color)
                    }
                };
                let points = if layer == Some(NavLayer::Cloud) {
                    let colors = height_colors(&points);
                    rerun::Points3D::new(points)
                        .with_radii([CLOUD_RADIUS])
                        .with_colors(colors)
                } else {
                    rerun::Points3D::new(points)
                        .with_radii([radius])
                        .with_colors([rgba(color)])
                };
                self.log_now(entity, &points)
            }
            "grid2d" => {
                let header: Grid2DHeader = parse!(msg, Grid2DHeader);
                let cells = parse_grid2d(&header, binary.ok_or("grid2d needs a binary payload")?)
                    .ok_or("grid2d payload does not match its header")?;
                let mesh = match header.kind {
                    GridKind::Costmap => costmap_mesh(&header, cells)?,
                    GridKind::Occupancy => grid_mesh(&header, cells)?,
                };
                // Static, so a viewer that connects later still gets the map.
                self.log_static(entity, &mesh)?;
                if layer == Some(NavLayer::Grid) {
                    self.log_grid_lines(entity, &header)?;
                }
                Ok(())
            }
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
                self.log_now(
                    entity,
                    &boxes.with_colors([rgba(data.color.unwrap_or(BOXES_COLOR))]),
                )
            }
            "arrows3d" => {
                let data = parse!(msg, Arrows3DData);
                let goal = layer == Some(NavLayer::Goal);
                let color = data
                    .color
                    .unwrap_or(if goal { GOAL_COLOR } else { ARROWS_COLOR });
                let mut arrows = rerun::Arrows3D::from_vectors(data.vectors)
                    .with_origins(data.origins.iter().copied())
                    .with_colors([rgba(color)]);
                if let Some(radius) = line_radius(data.radius) {
                    arrows = arrows.with_radii([radius]);
                }
                self.log_now(entity, &arrows)?;
                if !goal {
                    return Ok(());
                }
                let ring = format!("{entity}/ring");
                if data.origins.is_empty() {
                    // No goal (the bridge sends no arrows), so no ring either.
                    return self.log_now(&ring, &rerun::Clear::flat());
                }
                let radius = line_radius(data.radius).unwrap_or(GOAL_LINE_RADIUS);
                let rings = data.origins.iter().map(|origin| goal_ring(*origin));
                self.log_now(
                    &ring,
                    &rerun::LineStrips3D::new(rings)
                        .with_radii([radius])
                        .with_colors([rgba(color)]),
                )
            }
            "linestrips3d" => {
                let data = parse!(msg, LineStrips3DData);
                self.log_now(
                    entity,
                    &rerun::LineStrips3D::new(data.strips)
                        .with_radii([line_radius(data.radius).unwrap_or(STRIPS_RADIUS)])
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
                self.log_now(entity, &transform)?;
                // Rerun 0.28 draws nothing for a bare Transform3D; the axes make it visible.
                self.log_now(entity, &rerun::TransformAxes3D::new(TRANSFORM_AXIS_LENGTH))?;
                if layer == Some(NavLayer::Pose) {
                    // Below the pose, so it moves with it; on the timeline, so
                    // a Clear of the pose takes it too.
                    let heading = rerun::Arrows3D::from_vectors([[HEADING_LENGTH, 0.0, 0.0]])
                        .with_origins([[0.0; 3]])
                        .with_colors([rgba(HEADING_COLOR)])
                        .with_radii([HEADING_RADIUS]);
                    self.log_now(&format!("{entity}/heading"), &heading)?;
                }
                Ok(())
            }
            "scalar" => {
                // A time series: Rerun plots it as a curve over log time.
                let data = parse!(msg, ScalarData);
                self.log_now(entity, &rerun::Scalars::single(data.value))
            }
            "text" => {
                let data = parse!(msg, TextData);
                self.log_now(entity, &rerun::TextLog::new(data.text))
            }
            _ => Ok(()),
        }
    }

    /// A recursive `Clear` at `entity` on the timeline. That leaves static
    /// data up (Rerun ranks static above any time), so every static path at
    /// or below `entity` (map and costmap meshes, grid lines) also gets a
    /// static `Clear` of its own, which its next static log overrides. Flat:
    /// a static recursive `Clear` would hide the timeline data of the
    /// path's children for good.
    fn clear(&mut self, entity: &str) -> Result<(), String> {
        self.log_now(entity, &rerun::Clear::recursive())?;
        let cleared: Vec<String> = self
            .statics
            .iter()
            .filter(|path| at_or_under(path, entity))
            .cloned()
            .collect();
        for path in cleared {
            self.rec
                .log_static(path.as_str(), &rerun::Clear::flat())
                .map_err(log_failed)?;
            self.statics.remove(&path);
            // Grid lines cleared this way come back with the next map.
            self.grid_lines.remove(&path);
        }
        Ok(())
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

    /// Every batch recorded so far for `entity`.
    fn recorded_batches(
        storage: &MemorySinkStorage,
        entity: &str,
    ) -> Vec<rerun::external::arrow::array::RecordBatch> {
        storage
            .take()
            .into_iter()
            .filter_map(|msg| match msg {
                rerun::log::LogMsg::ArrowMsg(_, arrow) => Some(arrow.batch.clone()),
                _ => None,
            })
            .filter(|b| format!("{:?}", b.schema()).contains(&format!("\"/{entity}\"")))
            .collect()
    }

    /// The innermost values of every column whose name starts with `prefix`.
    fn leaves(
        batches: &[rerun::external::arrow::array::RecordBatch],
        prefix: &str,
    ) -> Vec<rerun::external::arrow::array::ArrayRef> {
        use rerun::external::arrow::array::{Array, FixedSizeListArray, ListArray};
        let mut out = Vec::new();
        for b in batches {
            for (f, c) in b.schema().fields().iter().zip(b.columns()) {
                if !f.name().starts_with(prefix) {
                    continue;
                }
                let mut a = c.clone();
                loop {
                    if let Some(l) = a.as_any().downcast_ref::<ListArray>() {
                        a = l.values().clone();
                    } else if let Some(l) = a.as_any().downcast_ref::<FixedSizeListArray>() {
                        a = l.values().clone();
                    } else {
                        break;
                    }
                }
                out.push(a);
            }
        }
        out
    }

    fn f32_leaves(
        batches: &[rerun::external::arrow::array::RecordBatch],
        prefix: &str,
    ) -> Vec<Vec<f32>> {
        use rerun::external::arrow::array::Float32Array;
        leaves(batches, prefix)
            .iter()
            .map(|a| {
                a.as_any()
                    .downcast_ref::<Float32Array>()
                    .expect("f32 leaf")
                    .values()
                    .to_vec()
            })
            .collect()
    }

    fn grid_message(width: u32, height: u32, origin: [f32; 3]) -> MvizMessage {
        msg(
            "grid2d",
            json!({
                "width": width, "height": height, "resolution": 0.5,
                "origin": origin, "kind": "occupancy"
            }),
        )
    }

    #[test]
    fn grid2d_logs_textured_mesh() {
        let (mut sink, storage) = RerunSink::memory("test");
        let cells = vec![0u8; 12];
        let m = grid_message(4, 3, [1.0, 2.0, 0.0]);
        assert_eq!(sink.log("tracer/map/grid", &m, Some(&cells)), Ok(()));
        sink.flush();
        let batches = recorded_batches(&storage, "tracer/map/grid");
        let schemas: Vec<String> = batches
            .iter()
            .map(|b| format!("{:?}", b.schema()))
            .collect();
        for component in [
            "Mesh3D:vertex_positions",
            "Mesh3D:vertex_texcoords",
            "Mesh3D:albedo_texture_buffer",
            "Mesh3D:albedo_texture_format",
        ] {
            assert!(
                schemas.iter().any(|s| s.contains(component)),
                "no {component}: {schemas:?}"
            );
        }
        let positions = f32_leaves(&batches, "Mesh3D:vertex_positions");
        assert_eq!(positions.len(), 1);
        let xyz = &positions[0];
        assert_eq!(xyz.len(), 12, "a quad has four vertices");
        assert!(xyz.chunks(3).all(|p| p[2] == 0.0), "not flat: {xyz:?}");
        // Unrotated: the quad spans origin .. origin + (4, 3) * 0.5.
        let xs: Vec<f32> = xyz.chunks(3).map(|p| p[0]).collect();
        let ys: Vec<f32> = xyz.chunks(3).map(|p| p[1]).collect();
        assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), 1.0);
        assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 3.0);
        assert_eq!(ys.iter().cloned().fold(f32::MAX, f32::min), 2.0);
        assert_eq!(ys.iter().cloned().fold(f32::MIN, f32::max), 3.5);
        // Static: it must survive a viewer connecting late.
        assert!(
            schemas.iter().all(|s| !s.contains("log_time")),
            "mesh was logged on a timeline: {schemas:?}"
        );
    }

    #[test]
    fn grid2d_yaw_rotates_the_corners() {
        let (mut sink, storage) = RerunSink::memory("test");
        let m = grid_message(2, 2, [0.0, 0.0, std::f32::consts::FRAC_PI_2]);
        assert_eq!(sink.log("map/grid", &m, Some(&[0u8; 4])), Ok(()));
        sink.flush();
        let batches = recorded_batches(&storage, "map/grid");
        let xyz = &f32_leaves(&batches, "Mesh3D:vertex_positions")[0];
        // A 1 m square turned a quarter turn about its corner spans x in -1..0.
        let max_x = xyz.chunks(3).map(|p| p[0]).fold(f32::MIN, f32::max);
        let min_x = xyz.chunks(3).map(|p| p[0]).fold(f32::MAX, f32::min);
        assert!(max_x.abs() < 1e-5 && (min_x + 1.0).abs() < 1e-5, "{xyz:?}");
    }

    #[test]
    fn bad_grid2d_is_an_error_not_a_panic() {
        let (mut sink, _storage) = RerunSink::memory("test");
        let m = grid_message(4, 3, [0.0; 3]);
        assert!(sink.log("map/grid", &m, Some(&[0u8; 5])).is_err());
        assert!(sink.log("map/grid", &m, None).is_err());
        let bad = msg("grid2d", json!({"width": "x"}));
        assert!(sink.log("map/grid", &bad, Some(&[])).is_err());
        let nan = msg(
            "grid2d",
            json!({"width": 1, "height": 1, "resolution": 0.5, "origin": [1e39, 0, 0],
                   "kind": "occupancy"}),
        );
        assert!(sink.log("map/grid", &nan, Some(&[0])).is_err());
    }

    fn cloud_message() -> (MvizMessage, Vec<u8>) {
        let bin = xyz_bytes(&[[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]]);
        let mut m = msg("points3d", json!({}));
        m.format = Some(binary_formats::POINTS_XYZ_F32.to_string());
        m.count = Some(3);
        (m, bin)
    }

    #[test]
    fn cloud_colored_by_height() {
        let (mut sink, storage) = RerunSink::memory("test");
        let (m, bin) = cloud_message();
        assert_eq!(sink.log("tracer/sensors/cloud", &m, Some(&bin)), Ok(()));
        sink.flush();
        let batches = recorded_batches(&storage, "tracer/sensors/cloud");
        let colors = leaves(&batches, "Points3D:colors");
        assert_eq!(colors.len(), 1);
        let colors = colors[0]
            .as_any()
            .downcast_ref::<rerun::external::arrow::array::UInt32Array>()
            .expect("u32 colors")
            .values()
            .to_vec();
        assert_eq!(colors.len(), 3, "one color per point");
        assert!(
            colors[0] != colors[1] && colors[1] != colors[2] && colors[0] != colors[2],
            "{colors:?}"
        );
    }

    #[test]
    fn other_clouds_keep_one_color() {
        let (mut sink, storage) = RerunSink::memory("test");
        let (m, bin) = cloud_message();
        assert_eq!(sink.log("tracer/sensors/scan", &m, Some(&bin)), Ok(()));
        sink.flush();
        let batches = recorded_batches(&storage, "tracer/sensors/scan");
        assert_eq!(leaves(&batches, "Points3D:colors")[0].len(), 1);
    }

    /// Every batch recorded so far, for any entity.
    fn take_batches(
        storage: &MemorySinkStorage,
    ) -> Vec<rerun::external::arrow::array::RecordBatch> {
        storage
            .take()
            .into_iter()
            .filter_map(|msg| match msg {
                rerun::log::LogMsg::ArrowMsg(_, arrow) => Some(arrow.batch.clone()),
                _ => None,
            })
            .collect()
    }

    /// The batches of `batches` that belong to `entity` exactly.
    fn of_entity(
        batches: &[rerun::external::arrow::array::RecordBatch],
        entity: &str,
    ) -> Vec<rerun::external::arrow::array::RecordBatch> {
        batches
            .iter()
            .filter(|b| format!("{:?}", b.schema()).contains(&format!("\"/{entity}\"")))
            .cloned()
            .collect()
    }

    fn has_column(batches: &[rerun::external::arrow::array::RecordBatch], prefix: &str) -> bool {
        batches.iter().any(|b| {
            b.schema()
                .fields()
                .iter()
                .any(|f| f.name().starts_with(prefix))
        })
    }

    /// One entry per logged row (the batcher may put several `log` calls in
    /// one batch) of the first column named `prefix…`: its innermost values.
    fn rows(
        batches: &[rerun::external::arrow::array::RecordBatch],
        prefix: &str,
    ) -> Vec<rerun::external::arrow::array::ArrayRef> {
        use rerun::external::arrow::array::{Array, FixedSizeListArray, ListArray};
        let mut out = Vec::new();
        for b in batches {
            let Some(column) = b
                .schema()
                .fields()
                .iter()
                .position(|f| f.name().starts_with(prefix))
                .map(|i| b.column(i).clone())
            else {
                continue;
            };
            let column = column
                .as_any()
                .downcast_ref::<ListArray>()
                .expect("a component column is a list per row")
                .clone();
            for row in 0..column.len() {
                if column.is_null(row) {
                    continue;
                }
                let mut a = column.value(row);
                loop {
                    if let Some(l) = a.as_any().downcast_ref::<ListArray>() {
                        a = l.values().clone();
                    } else if let Some(l) = a.as_any().downcast_ref::<FixedSizeListArray>() {
                        a = l.values().clone();
                    } else {
                        break;
                    }
                }
                out.push(a);
            }
        }
        out
    }

    fn f32_rows(
        batches: &[rerun::external::arrow::array::RecordBatch],
        prefix: &str,
    ) -> Vec<Vec<f32>> {
        use rerun::external::arrow::array::Float32Array;
        rows(batches, prefix)
            .iter()
            .map(|a| {
                a.as_any()
                    .downcast_ref::<Float32Array>()
                    .expect("f32 leaf")
                    .values()
                    .to_vec()
            })
            .collect()
    }

    /// Static batches carry no timeline column.
    fn is_static(batch: &rerun::external::arrow::array::RecordBatch) -> bool {
        !format!("{:?}", batch.schema()).contains("log_time")
    }

    fn u32_leaves(
        batches: &[rerun::external::arrow::array::RecordBatch],
        prefix: &str,
    ) -> Vec<u32> {
        use rerun::external::arrow::array::UInt32Array;
        leaves(batches, prefix)
            .iter()
            .flat_map(|a| {
                a.as_any()
                    .downcast_ref::<UInt32Array>()
                    .expect("u32 leaf")
                    .values()
                    .to_vec()
            })
            .collect()
    }

    fn bool_rows(
        batches: &[rerun::external::arrow::array::RecordBatch],
        prefix: &str,
    ) -> Vec<bool> {
        use rerun::external::arrow::array::BooleanArray;
        rows(batches, prefix)
            .iter()
            .flat_map(|a| {
                let a = a
                    .as_any()
                    .downcast_ref::<BooleanArray>()
                    .expect("bool leaf");
                (0..a.len()).map(|i| a.value(i)).collect::<Vec<_>>()
            })
            .collect()
    }

    /// An RGBA colour as Rerun stores it (0xRRGGBBAA).
    fn packed(c: [u8; 4]) -> u32 {
        u32::from_be_bytes(c)
    }

    #[test]
    fn strips_use_radius() {
        let (mut sink, storage) = RerunSink::memory("test");
        let plan = msg(
            "linestrips3d",
            json!({"strips": [[[0, 0, 0], [1, 0, 0]]], "radius": 0.025}),
        );
        assert_eq!(sink.log("tracer/nav/plan", &plan, None), Ok(()));
        let goal = msg(
            "arrows3d",
            json!({"origins": [[0, 0, 0]], "vectors": [[1, 0, 0]], "radius": 0.015}),
        );
        assert_eq!(sink.log("tracer/nav/goal", &goal, None), Ok(()));
        sink.flush();
        let batches = take_batches(&storage);
        assert_eq!(
            f32_leaves(
                &of_entity(&batches, "tracer/nav/plan"),
                "LineStrips3D:radii"
            ),
            [[0.025]]
        );
        assert_eq!(
            f32_leaves(&of_entity(&batches, "tracer/nav/goal"), "Arrows3D:radii"),
            [[0.015]]
        );
    }

    #[test]
    fn cloud_points_small_and_translucent() {
        let (mut sink, storage) = RerunSink::memory("test");
        let (m, bin) = cloud_message();
        assert_eq!(sink.log("tracer/sensors/cloud", &m, Some(&bin)), Ok(()));
        let json_cloud = msg(
            "points3d",
            json!({"positions": [[0, 0, 0], [0, 0, 1]], "radius": 0.2}),
        );
        assert_eq!(
            sink.log("cloud_json/sensors/cloud", &json_cloud, None),
            Ok(())
        );
        sink.flush();
        let batches = take_batches(&storage);
        for entity in ["tracer/sensors/cloud", "cloud_json/sensors/cloud"] {
            let cloud = of_entity(&batches, entity);
            assert_eq!(f32_leaves(&cloud, "Points3D:radii"), [[0.01]], "{entity}");
            let colors = u32_leaves(&cloud, "Points3D:colors");
            assert!(!colors.is_empty(), "{entity}");
            assert!(
                colors.iter().all(|c| c & 0xFF == 153),
                "{entity}: alpha not 153 in {colors:x?}"
            );
        }
    }

    fn costmap_message(width: u32, height: u32) -> MvizMessage {
        costmap_at(width, height, [0.0; 3])
    }

    fn costmap_at(width: u32, height: u32, origin: [f32; 3]) -> MvizMessage {
        msg(
            "grid2d",
            json!({
                "width": width, "height": height, "resolution": 0.5,
                "origin": origin, "kind": "costmap"
            }),
        )
    }

    #[test]
    fn costmap_rows_go_up_from_the_origin() {
        // Row 0 is the smallest y (the texture of the map is flipped; this is not).
        let (mut sink, storage) = RerunSink::memory("test");
        let m = costmap_at(1, 2, [1.0, 2.0, 0.0]);
        assert_eq!(
            sink.log("tracer/costmap/global", &m, Some(&[254, 0])),
            Ok(())
        );
        let turned = costmap_at(1, 1, [0.0, 0.0, std::f32::consts::FRAC_PI_2]);
        assert_eq!(
            sink.log("other/costmap/global", &turned, Some(&[254])),
            Ok(())
        );
        sink.flush();
        let batches = take_batches(&storage);
        let xyz = f32_leaves(
            &of_entity(&batches, "tracer/costmap/global"),
            "Mesh3D:vertex_positions",
        )
        .concat();
        let xs: Vec<f32> = xyz.chunks(3).map(|p| p[0]).collect();
        let ys: Vec<f32> = xyz.chunks(3).map(|p| p[1]).collect();
        let span = |v: &[f32]| {
            (
                v.iter().cloned().fold(f32::MAX, f32::min),
                v.iter().cloned().fold(f32::MIN, f32::max),
            )
        };
        assert_eq!(span(&xs), (1.0, 1.5), "{xyz:?}");
        assert_eq!(span(&ys), (2.0, 2.5), "{xyz:?}");
        // A quarter turn about the origin puts the cell at x -0.5..0.
        let xyz = f32_leaves(
            &of_entity(&batches, "other/costmap/global"),
            "Mesh3D:vertex_positions",
        )
        .concat();
        let xs: Vec<f32> = xyz.chunks(3).map(|p| p[0]).collect();
        let (min_x, max_x) = span(&xs);
        assert!((min_x + 0.5).abs() < 1e-6 && max_x.abs() < 1e-6, "{xyz:?}");
    }

    #[test]
    fn costmap_is_a_vertex_colored_mesh() {
        let (mut sink, storage) = RerunSink::memory("test");
        let m = costmap_message(3, 1);
        assert_eq!(
            sink.log("tracer/costmap/global", &m, Some(&[0, 254, 253])),
            Ok(())
        );
        sink.flush();
        let batches = of_entity(&take_batches(&storage), "tracer/costmap/global");
        assert!(!batches.is_empty(), "nothing recorded for the costmap");
        let xyz = f32_leaves(&batches, "Mesh3D:vertex_positions").concat();
        assert_eq!(
            xyz.len(),
            8 * 3,
            "two quads (the free cell is left out): {xyz:?}"
        );
        assert!(
            xyz.chunks(3).all(|p| p[2] == 0.01),
            "not at z 0.01: {xyz:?}"
        );
        // The free first cell (x 0..0.5) is not drawn.
        let min_x = xyz.chunks(3).map(|p| p[0]).fold(f32::MAX, f32::min);
        let max_x = xyz.chunks(3).map(|p| p[0]).fold(f32::MIN, f32::max);
        assert_eq!((min_x, max_x), (0.5, 1.5), "{xyz:?}");
        let colors = u32_leaves(&batches, "Mesh3D:vertex_colors");
        assert_eq!(colors.len(), 8, "{colors:x?}");
        assert!(
            colors.contains(&packed([255, 0, 255, 128])),
            "no magenta: {colors:x?}"
        );
        assert!(
            colors.contains(&packed([0, 255, 255, 128])),
            "no cyan: {colors:x?}"
        );
        let triangles = leaves(&batches, "Mesh3D:triangle_indices");
        assert_eq!(triangles.iter().map(|a| a.len()).sum::<usize>(), 4 * 3);
        // Vertex colours, never the map's texture.
        assert!(!has_column(&batches, "Mesh3D:albedo_texture"), "textured");
        assert!(batches.iter().all(is_static), "not static");
    }

    #[test]
    fn costmap_runs_of_one_colour_share_a_quad() {
        let (mut sink, storage) = RerunSink::memory("test");
        // Lethal x3, free, inscribed, unknown: three runs to draw.
        let cells = [254, 254, 254, 0, 253, 255];
        let m = costmap_message(6, 1);
        assert_eq!(sink.log("tracer/costmap/global", &m, Some(&cells)), Ok(()));
        // An all-free costmap replaces it with an empty mesh.
        assert_eq!(sink.log("tracer/costmap/global", &m, Some(&[0; 6])), Ok(()));
        sink.flush();
        let batches = of_entity(&take_batches(&storage), "tracer/costmap/global");
        let xyz = f32_rows(&batches, "Mesh3D:vertex_positions");
        assert_eq!(xyz.len(), 2, "{xyz:?}");
        assert_eq!(xyz[0].len(), 3 * 4 * 3, "{:?}", xyz[0]);
        assert!(xyz[1].is_empty(), "{:?}", xyz[1]);
    }

    #[test]
    fn oversized_costmap_is_an_error_not_a_hog() {
        let (mut sink, _storage) = RerunSink::memory("test");
        // Every cell differs from its neighbour: one quad per cell, too many.
        let width = 1000u32;
        let height = (COSTMAP_MAX_QUADS as u32).div_ceil(width) + 1;
        let cells: Vec<u8> = (0..width * height)
            .map(|i| if i % 2 == 0 { 254 } else { 253 })
            .collect();
        let m = costmap_message(width, height);
        let err = sink
            .log("tracer/costmap/global", &m, Some(&cells))
            .expect_err("a costmap over the quad cap was drawn");
        assert!(err.contains("costmap"), "{err}");
    }

    #[test]
    fn grid_lines_logged_once_per_map() {
        let (mut sink, storage) = RerunSink::memory("test");
        // 4 x 3 cells of 0.5 m at (1, 2): x 1..3, y 2..3.5.
        let m = grid_message(4, 3, [1.0, 2.0, 0.0]);
        for _ in 0..2 {
            assert_eq!(sink.log("tracer/map/grid", &m, Some(&[0u8; 12])), Ok(()));
        }
        // A costmap is no map: no grid lines for it.
        let costmap = costmap_message(1, 1);
        assert_eq!(
            sink.log("tracer/costmap/global", &costmap, Some(&[254])),
            Ok(())
        );
        sink.flush();
        let lines = of_entity(&take_batches(&storage), "tracer/view/grid");
        let logged = rows(&lines, "LineStrips3D:strips").len();
        assert_eq!(logged, 1, "grid lines logged {logged} times");
        assert!(lines.iter().all(is_static), "not static");
        assert_eq!(f32_leaves(&lines, "LineStrips3D:radii"), [[0.003]]);
        assert_eq!(
            u32_leaves(&lines, "LineStrips3D:colors"),
            [packed([160, 160, 164, 128])]
        );
        let xyz = f32_leaves(&lines, "LineStrips3D:strips").concat();
        assert!(
            xyz.chunks(3).all(|p| p[2] == 0.002),
            "not at z 0.002: {xyz:?}"
        );
        // Every metre from x 1 to 3 and y 2 to 4 (3.5 rounded up).
        let points: Vec<[f32; 3]> = xyz.chunks(3).map(|p| [p[0], p[1], p[2]]).collect();
        let vertical: Vec<f32> = points
            .chunks(2)
            .filter(|l| l[0][0] == l[1][0])
            .map(|l| l[0][0])
            .collect();
        let horizontal: Vec<f32> = points
            .chunks(2)
            .filter(|l| l[0][1] == l[1][1])
            .map(|l| l[0][1])
            .collect();
        assert_eq!(vertical, [1.0, 2.0, 3.0], "{points:?}");
        assert_eq!(horizontal, [2.0, 3.0, 4.0], "{points:?}");

        // Another map extent: drawn again.
        let moved = grid_message(4, 3, [-3.0, 2.0, 0.0]);
        assert_eq!(
            sink.log("tracer/map/grid", &moved, Some(&[0u8; 12])),
            Ok(())
        );
        sink.flush();
        let lines = of_entity(&take_batches(&storage), "tracer/view/grid");
        let logged = rows(&lines, "LineStrips3D:strips").len();
        assert_eq!(logged, 1, "a new map extent was not drawn");
    }

    #[test]
    fn goal_ring_and_pose_heading() {
        let (mut sink, storage) = RerunSink::memory("test");
        let goal = msg(
            "arrows3d",
            json!({"origins": [[1, 2, 0]], "vectors": [[0.5, 0, 0]],
                   "color": [255, 170, 0, 255], "radius": 0.015}),
        );
        assert_eq!(sink.log("tracer/nav/goal", &goal, None), Ok(()));
        let pose = msg(
            "transform3d",
            json!({"translation": [1, 2, 0], "rotation": [0, 0, 0, 1]}),
        );
        assert_eq!(sink.log("tracer/robot/pose", &pose, None), Ok(()));
        // Neither for an arrow or transform outside the navigation layers.
        assert_eq!(sink.log("world/arrows", &goal, None), Ok(()));
        assert_eq!(sink.log("world/vehicle", &pose, None), Ok(()));
        sink.flush();
        let batches = take_batches(&storage);

        let ring = of_entity(&batches, "tracer/nav/goal/ring");
        let xyz = f32_leaves(&ring, "LineStrips3D:strips").concat();
        assert_eq!(xyz.len(), 25 * 3, "24 segments, closed: {xyz:?}");
        for p in xyz.chunks(3) {
            let r = (p[0] - 1.0).hypot(p[1] - 2.0);
            assert!((r - 0.15).abs() < 1e-5 && p[2] == 0.0, "{p:?} at {r}");
        }
        assert_eq!(xyz[..3], xyz[xyz.len() - 3..], "not closed");
        assert_eq!(f32_leaves(&ring, "LineStrips3D:radii"), [[0.015]]);
        assert_eq!(
            u32_leaves(&ring, "LineStrips3D:colors"),
            [packed([255, 170, 0, 255])]
        );

        let heading = of_entity(&batches, "tracer/robot/pose/heading");
        assert_eq!(
            f32_leaves(&heading, "Arrows3D:vectors").concat(),
            [0.4, 0.0, 0.0]
        );
        assert_eq!(
            f32_leaves(&heading, "Arrows3D:origins").concat(),
            [0.0, 0.0, 0.0]
        );
        assert_eq!(
            u32_leaves(&heading, "Arrows3D:colors"),
            [packed([255, 25, 0, 255])]
        );
        assert_eq!(f32_leaves(&heading, "Arrows3D:radii"), [[0.015]]);
        // Logged on the timeline with the pose, so a Clear of the pose takes it too.
        assert!(heading.iter().all(|b| !is_static(b)), "static heading");

        assert!(of_entity(&batches, "world/arrows/ring").is_empty());
        assert!(of_entity(&batches, "world/vehicle/heading").is_empty());
    }

    #[test]
    fn goal_without_origins_clears_the_ring() {
        let (mut sink, storage) = RerunSink::memory("test");
        let none = msg("arrows3d", json!({"origins": [], "vectors": []}));
        assert_eq!(sink.log("tracer/nav/goal", &none, None), Ok(()));
        sink.flush();
        let ring = of_entity(&take_batches(&storage), "tracer/nav/goal/ring");
        assert_eq!(bool_rows(&ring, "Clear:is_recursive"), [false], "no Clear");
        assert!(
            !has_column(&ring, "LineStrips3D:strips"),
            "a ring was drawn"
        );
    }

    #[test]
    fn clear_logs_a_recursive_clear() {
        let (mut sink, storage) = RerunSink::memory("test");
        assert_eq!(sink.clear("tracer/sensors/cloud"), Ok(()));
        sink.flush();
        let cleared = of_entity(&take_batches(&storage), "tracer/sensors/cloud");
        assert_eq!(bool_rows(&cleared, "Clear:is_recursive"), [true]);
        assert!(cleared.iter().all(|b| !is_static(b)), "static Clear");
    }

    /// Rerun ranks static data above any time, so a Clear on the timeline
    /// leaves the static map and costmap meshes up: they get a static Clear.
    #[test]
    fn clear_also_hides_static_layers() {
        let (mut sink, storage) = RerunSink::memory("test");
        let costmap = costmap_message(1, 1);
        assert_eq!(
            sink.log("tracer/costmap/global", &costmap, Some(&[254])),
            Ok(())
        );
        let map = grid_message(4, 3, [1.0, 2.0, 0.0]);
        assert_eq!(sink.log("tracer/map/grid", &map, Some(&[0u8; 12])), Ok(()));
        sink.flush();
        let _ = storage.take();

        assert_eq!(sink.clear("tracer/nav/plan"), Ok(()));
        assert_eq!(sink.clear("tracer/costmap/global"), Ok(()));
        sink.flush();
        let batches = take_batches(&storage);
        let (fixed, timed): (Vec<_>, Vec<_>) = of_entity(&batches, "tracer/costmap/global")
            .into_iter()
            .partition(is_static);
        assert_eq!(
            bool_rows(&timed, "Clear:is_recursive"),
            [true],
            "no Clear on the timeline"
        );
        assert_eq!(
            bool_rows(&fixed, "Clear:is_recursive"),
            [false],
            "no static Clear for the static costmap"
        );
        // Nothing else was cleared: not the map, not the grid lines.
        assert!(of_entity(&batches, "tracer/map/grid").is_empty());
        assert!(of_entity(&batches, "tracer/view/grid").is_empty());

        // The next costmap shows again (a later static log), and a second
        // switch-off clears it again.
        assert_eq!(
            sink.log("tracer/costmap/global", &costmap, Some(&[254])),
            Ok(())
        );
        assert_eq!(sink.clear("tracer/costmap/global"), Ok(()));
        sink.flush();
        let fixed: Vec<_> = of_entity(&take_batches(&storage), "tracer/costmap/global")
            .into_iter()
            .filter(is_static)
            .collect();
        assert_eq!(rows(&fixed, "Mesh3D:vertex_positions").len(), 1);
        assert_eq!(bool_rows(&fixed, "Clear:is_recursive"), [false]);
    }

    #[test]
    fn root_is_z_up() {
        let (sink, storage) = RerunSink::memory("test");
        sink.flush();
        let batches = recorded_batches(&storage, "");
        let schemas: Vec<String> = batches
            .iter()
            .map(|b| format!("{:?}", b.schema()))
            .collect();
        assert!(
            schemas.iter().any(|s| s.contains("ViewCoordinates:")),
            "no ViewCoordinates at the root: {schemas:?}"
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
