use dorobot_studio_core::api::Backend;
use dorobot_studio_plugin::*;

pub mod fake;
mod graph;
mod map2d;
mod map_view;
mod nav_panel;
mod nav_scene;
#[cfg(feature = "rerun")]
mod rerun_sink;
mod route;
mod session;
mod view_panel;

pub use graph::{
    hit_test, layout, DvizGraph, DvizGraphAction, DvizGraphRef, DvizGraphWidgetRefExt, GraphLayout,
    Point,
};
pub use map2d::{DvizMap2D, DvizMap2DRef, DvizMap2DWidgetRefExt};
pub use map_view::{
    costmap_rgba, grid_lines, grid_rgba, layer_color, nav2_to_rviz, rviz_costmap_color,
    rviz_map_color, scene_bounds, MapView, NavLayers,
};
pub use nav_panel::{connection_text, nav_status_text, status_is_stale, DvizNavPanel};
pub use nav_scene::{nav_layer, GridLayer, NavLayer, NavScene, Pose2D};
#[cfg(feature = "rerun")]
pub use rerun_sink::{RerunSink, ViewerOptions};
pub use route::{route, CountingSink, EntityInfo, SessionState, Sink, LOG_CAP, LOG_TEXT_CAP};
pub use session::{
    DvizSession, SessionOptions, SessionSnapshot, ViewerState, INBOX_BYTES_CAP, RING_CAP,
};
pub use view_panel::DvizViewPanel;

/// Opens the Rerun 3D viewer and routes every visual message into it.
/// Returns at once: the launch (version check, start, wait for its port)
/// runs on a short-lived thread and reports through the snapshot's
/// `viewer`. Fails only while an earlier launch still runs.
#[cfg(feature = "rerun")]
pub fn open_viewer(session: &DvizSession) -> Result<(), String> {
    open_viewer_with(session, ViewerOptions::default())
}

/// `open_viewer` with another executable or port; for tests.
#[cfg(feature = "rerun")]
fn open_viewer_with(session: &DvizSession, options: ViewerOptions) -> Result<(), String> {
    session.start_viewer(move || {
        let (sink, version) = RerunSink::launch("dorobot-dviz", &options)?;
        Ok((Box::new(sink) as Box<dyn Sink>, format!("Rerun {version}")))
    })
}

/// Without the `rerun` feature there is no viewer to open; the snapshot's
/// `viewer` says so, as it reports any other failed launch.
#[cfg(not(feature = "rerun"))]
pub fn open_viewer(session: &DvizSession) -> Result<(), String> {
    session.start_viewer(|| Err("This build does not include Rerun".to_string()))
}

pub struct DvizPlugin;

impl StudioPlugin for DvizPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "dorobot.dviz",
            title: "3D visualization",
            api_version: API_VERSION,
            panels: &[
                PanelDescriptor {
                    id: "dviz_view",
                    title: "Visualization",
                    legacy_screen: None,
                },
                PanelDescriptor {
                    id: "dviz_nav",
                    title: "Navigation",
                    legacy_screen: None,
                },
            ],
        }
    }

    /// The panels' own widgets; the nav panel's build on the view panel's
    /// themed text and on the map, so those come first.
    fn register(&self, vm: &mut ScriptVm) {
        graph::script_mod(vm);
        view_panel::script_mod(vm);
        map2d::script_mod(vm);
        nav_panel::script_mod(vm);
    }

    fn create(&self, panel: &str, vm: &mut ScriptVm) -> Result<Box<dyn PluginPanel>, String> {
        match panel {
            "dviz_view" => Ok(Box::new(DvizViewPanel::create(vm, DvizSession::global()))),
            "dviz_nav" => Ok(Box::new(DvizNavPanel::create(vm, DvizSession::global()))),
            _ => Err(format!("Unknown dviz panel {panel}")),
        }
    }

    fn can_disable(&self, _: &dyn Backend) -> Result<(), String> {
        Ok(())
    }
}

/// Runs a test again in a child process of the test binary, for tests that
/// need a process-wide setting such as `PATH` (only the Rerun ones do).
#[cfg(all(test, feature = "rerun"))]
pub(crate) mod test_child {
    use std::{
        env, fs,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    /// Longest a child may run before it is killed and its test fails.
    const TIMEOUT: Duration = Duration::from_secs(60);

    /// Runs the test `test` (its full path) alone in a child with `envs`
    /// set, and asserts that it passed within `TIMEOUT`.
    pub fn assert_passes(test: &str, envs: &[(&str, &std::ffi::OsStr)]) {
        let dir = env::temp_dir().join(format!(
            "dviz-child-{}-{}",
            std::process::id(),
            test.replace("::", "-")
        ));
        fs::create_dir_all(&dir).expect("child output directory");
        let (out, err) = (dir.join("stdout"), dir.join("stderr"));
        let mut command = Command::new(env::current_exe().expect("test binary"));
        command
            .args(["--exact", test])
            .stdin(Stdio::null())
            .stdout(fs::File::create(&out).expect("stdout file"))
            .stderr(fs::File::create(&err).expect("stderr file"));
        for (key, value) in envs {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("run the test binary");
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait for the child") {
                break Some(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            thread::sleep(Duration::from_millis(50));
        };
        let stdout = fs::read_to_string(&out).unwrap_or_default();
        let stderr = fs::read_to_string(&err).unwrap_or_default();
        let _ = fs::remove_dir_all(&dir);
        let Some(status) = status else {
            panic!("{test} ran longer than {TIMEOUT:?}:\n{stdout}\n{stderr}");
        };
        assert!(
            status.success() && stdout.contains("1 passed"),
            "{test} failed:\n{stdout}\n{stderr}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Waits for the 3D window's launch to end and returns how it ended.
    fn launch_outcome(session: &DvizSession) -> ViewerState {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let viewer = session.snapshot().viewer;
            if !matches!(viewer, ViewerState::Closed | ViewerState::Starting) {
                return viewer;
            }
            assert!(
                Instant::now() < deadline,
                "the launch never ended: {viewer:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(not(feature = "rerun"))]
    #[test]
    fn open_viewer_without_rerun_says_so() {
        let session = DvizSession::new();
        assert_eq!(open_viewer(&session), Ok(()));
        match launch_outcome(&session) {
            ViewerState::Failed(error) => assert!(error.contains("Rerun"), "{error}"),
            other => panic!("{other:?}"),
        }
    }

    /// With the feature, a missing viewer executable is an error that names Rerun.
    /// Run in a child process with an empty `PATH` so other tests keep theirs.
    #[cfg(feature = "rerun")]
    #[test]
    fn open_viewer_without_rerun_binary_reports_an_error() {
        const TEST: &str = "tests::open_viewer_without_rerun_binary_reports_an_error";
        if std::env::var_os("DVIZ_VIEWER_CHILD").is_some() {
            let session = DvizSession::new();
            assert_eq!(open_viewer(&session), Ok(()));
            match launch_outcome(&session) {
                ViewerState::Failed(error) => {
                    assert!(error.to_lowercase().contains("rerun"), "{error}")
                }
                other => panic!("{other:?}"),
            }
            return;
        }
        test_child::assert_passes(
            TEST,
            &[("PATH", "".as_ref()), ("DVIZ_VIEWER_CHILD", "1".as_ref())],
        );
    }

    /// The fake answers `--version` only after 1 s, so the launch is still
    /// running when `open_viewer` returns.
    #[cfg(all(feature = "rerun", unix))]
    #[test]
    fn open_viewer_returns_at_once_and_reports_a_wrong_version() {
        use rerun_sink::fake_rerun;
        const TEST: &str = "tests::open_viewer_returns_at_once_and_reports_a_wrong_version";
        let on_version = format!("sleep 1\n{}", fake_rerun::version_reply("0.20.3"));
        let script = fake_rerun::script(&on_version, "exit 0");
        let Some(dir) = fake_rerun::run_in_child(TEST, &script) else {
            return;
        };
        let session = DvizSession::new();
        let started = Instant::now();
        assert_eq!(open_viewer(&session), Ok(()));
        let took = started.elapsed();
        assert!(
            took < Duration::from_millis(100),
            "open_viewer blocked for {took:?}"
        );
        assert_eq!(session.snapshot().viewer, ViewerState::Starting);

        // A second click while the first launch runs is refused and changes nothing.
        let started = Instant::now();
        assert!(open_viewer(&session).is_err());
        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(session.snapshot().viewer, ViewerState::Starting);

        match launch_outcome(&session) {
            ViewerState::Failed(error) => {
                let sdk = fake_rerun::sdk_version();
                assert!(error.contains("0.20.3") && error.contains(&sdk), "{error}");
            }
            other => panic!("a 0.20 viewer was accepted: {other:?}"),
        }
        assert!(
            !dir.join("started").exists(),
            "the wrong viewer was started"
        );
    }

    #[cfg(all(feature = "rerun", unix))]
    #[test]
    fn open_viewer_reports_a_viewer_that_exits_at_once() {
        use rerun_sink::fake_rerun;
        const TEST: &str = "tests::open_viewer_reports_a_viewer_that_exits_at_once";
        let reply = fake_rerun::version_reply(&fake_rerun::sdk_version());
        let script = fake_rerun::script(&reply, "exit 3");
        if fake_rerun::run_in_child(TEST, &script).is_none() {
            return;
        }
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let session = DvizSession::new();
        let options = ViewerOptions {
            port,
            ..ViewerOptions::default()
        };
        assert_eq!(open_viewer_with(&session, options), Ok(()));
        match launch_outcome(&session) {
            ViewerState::Failed(error) => assert!(error.contains("exited"), "{error}"),
            other => panic!("a dead viewer was accepted: {other:?}"),
        }
    }
}
