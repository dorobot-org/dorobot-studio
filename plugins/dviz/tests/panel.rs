//! The `dviz_view` and `dviz_nav` panels driven through their real widgets,
//! headless (`Cx::new` without a window). Each test has its own leaked `&'static DvizSession`, its
//! own loopback port and topic prefix, and multicast off, as in `loopback.rs`.

use dorobot_plugin_dviz::{
    scene_bounds, DvizGraphAction, DvizGraphWidgetRefExt, DvizMap2DRef, DvizMap2DWidgetRefExt,
    DvizNavPanel, DvizPlugin, DvizSession, DvizViewPanel, GridLayer, MapDraw, MapView, NavLayers,
    NavScene, Pose2D, SessionOptions, ViewerState,
};
use dorobot_studio_core::api::mock::MockBackend;
use dorobot_studio_plugin::*;
use dviz_protocol::{serialize_message, MvizMessage};
use dviz_protocol::{Grid2DHeader, GridKind};
use serde_json::json;
use std::{
    net::TcpListener,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use zenoh::{qos::CongestionControl, Wait};

const SETTLE: Duration = Duration::from_secs(3);

fn setup() -> (Cx, &'static DvizSession) {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.init_cx_os();
    cx.with_vm(|vm| {
        dorobot_studio_core::register_ui(vm);
        // As the host does: the plugin's own widgets (the graph).
        DvizPlugin.register(vm);
        assert!(vm.take_errors().is_empty());
    });
    (cx, Box::leak(Box::new(DvizSession::new())))
}

fn new_panel(cx: &mut Cx, session: &'static DvizSession) -> DvizViewPanel {
    let panel = cx.with_vm(|vm| DvizViewPanel::create(vm, session));
    assert_no_script_errors(cx);
    panel
}

fn assert_no_script_errors(cx: &mut Cx) {
    cx.with_vm(|vm| {
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
    });
}

fn sync(cx: &mut Cx, panel: &mut dyn PluginPanel) {
    let backend = MockBackend::new();
    panel.sync(
        cx,
        &PanelContext {
            backend: &backend,
            rig_editing: false,
            show_help: false,
            show_details: false,
            live_controls: false,
        },
    );
}

fn click(cx: &mut Cx, panel: &mut dyn PluginPanel, path: &[LiveId]) -> bool {
    let button = panel.root().button(cx, path);
    assert!(!button.is_empty(), "no button at {path:?}");
    let actions = cx.capture_actions(|cx| {
        cx.widget_action(
            button.widget_uid(),
            ButtonAction::Clicked(KeyModifiers::default()),
        )
    });
    panel.handle_actions(cx, &actions, &mut MockBackend::new())
}

fn fill(cx: &mut Cx, panel: &DvizViewPanel, path: &[LiveId], text: &str) {
    let input = panel.root().text_input(cx, path);
    assert!(!input.is_empty(), "no text input at {path:?}");
    input.set_text(cx, text);
}

fn text(cx: &mut Cx, panel: &dyn PluginPanel, path: &[LiveId]) -> String {
    let widget = panel.root().widget(cx, path);
    assert!(!widget.is_empty(), "no widget at {path:?}");
    widget.text()
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Runs the test `test` alone in a child process of this test binary with
/// `envs` set (for a process-wide setting such as `PATH`), and asserts that
/// it passed within a minute; a child still running then is killed.
#[cfg(feature = "rerun")]
fn assert_child_passes(test: &str, envs: &[(&str, &std::ffi::OsStr)]) {
    use std::{fs, process::Stdio};
    const TIMEOUT: Duration = Duration::from_secs(60);
    let dir = std::env::temp_dir().join(format!("dviz-panel-child-{}-{test}", std::process::id()));
    fs::create_dir_all(&dir).expect("child output directory");
    let (out, err) = (dir.join("stdout"), dir.join("stderr"));
    let mut command = std::process::Command::new(std::env::current_exe().expect("test binary"));
    command
        .args(["--exact", test])
        .stdin(Stdio::null())
        .stdout(fs::File::create(&out).expect("stdout file"))
        .stderr(fs::File::create(&err).expect("stderr file"));
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("run the test binary");
    let finished = wait_until(TIMEOUT, || matches!(child.try_wait(), Ok(Some(_))));
    if !finished {
        let _ = child.kill();
    }
    let status = child.wait().expect("wait for the child");
    let stdout = fs::read_to_string(&out).unwrap_or_default();
    let stderr = fs::read_to_string(&err).unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    assert!(
        finished,
        "{test} ran longer than {TIMEOUT:?}:\n{stdout}\n{stderr}"
    );
    assert!(
        status.success() && stdout.contains("1 passed"),
        "{test} failed:\n{stdout}\n{stderr}"
    );
}

/// A test peer listening on a fresh loopback port, for the panel to connect to.
fn listening_peer() -> (zenoh::Session, String) {
    let endpoint = format!("tcp/127.0.0.1:{}", free_port());
    let mut config = zenoh::Config::default();
    config.insert_json5("mode", r#""peer""#).unwrap();
    config
        .insert_json5("listen/endpoints", &format!(r#"["{endpoint}"]"#))
        .unwrap();
    config.insert_json5("connect/endpoints", "[]").unwrap();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    config
        .insert_json5("transport/shared_memory/enabled", "false")
        .unwrap();
    (zenoh::open(config).wait().unwrap(), endpoint)
}

/// Fills the panel's inputs and clicks `connect`; returns once the session
/// is linked to the peer at `endpoint`.
fn connect_through_panel(
    cx: &mut Cx,
    panel: &mut DvizViewPanel,
    session: &DvizSession,
    endpoint: &str,
    prefix: &str,
) {
    fill(cx, panel, ids!(mode), "peer");
    fill(cx, panel, ids!(endpoint), endpoint);
    fill(cx, panel, ids!(prefix), prefix);
    assert!(click(cx, panel, ids!(connect)));
    assert!(
        wait_until(SETTLE, || session.snapshot().connected),
        "never connected: {:?}",
        session.snapshot().last_error
    );
}

/// A blocking publisher on `key`, returned once the session's subscriber is
/// known to it, so no put is lost to subscription propagation.
fn matched_publisher(peer: &zenoh::Session, key: String) -> zenoh::pubsub::Publisher<'static> {
    let publisher = peer
        .declare_publisher(key)
        .congestion_control(CongestionControl::Block)
        .wait()
        .unwrap();
    assert!(
        wait_until(SETTLE, || publisher
            .matching_status()
            .wait()
            .is_ok_and(|status| status.matching())),
        "the session's subscriber never reached the publisher"
    );
    publisher
}

fn log_message(level: &str, message: &str) -> Vec<u8> {
    let header = MvizMessage {
        msg_type: "log".to_string(),
        timestamp: Some(1.0),
        data: json!({"level": level, "message": message, "node_id": "planner"}),
        format: None,
        count: None,
    };
    serialize_message(&header, None)
}

/// A `graph_update` with `(id, status, last_seen)` nodes and
/// `(from_node, from_port, to_node, to_port)` edges.
fn graph_message(nodes: &[(&str, &str, f64)], edges: &[(&str, &str, &str, &str)]) -> Vec<u8> {
    let nodes: Vec<_> = nodes
        .iter()
        .map(|(id, status, last_seen)| json!({"id": id, "status": status, "last_seen": last_seen}))
        .collect();
    let edges: Vec<_> = edges
        .iter()
        .map(|(from_node, from_port, to_node, to_port)| {
            json!({
                "from_node": from_node,
                "from_port": from_port,
                "to_node": to_node,
                "to_port": to_port,
            })
        })
        .collect();
    let header = MvizMessage {
        msg_type: "graph_update".to_string(),
        timestamp: Some(2.0),
        data: json!({"nodes": nodes, "edges": edges, "timestamp": 2.0}),
        format: None,
        count: None,
    };
    serialize_message(&header, None)
}

#[test]
fn connect_button_starts_the_session_with_the_entered_endpoint() {
    let (mut cx, session) = setup();
    let mut panel = new_panel(&mut cx, session);
    // Nothing listens there: client mode fails to open after the 1 s connect
    // timeout, and the error names the endpoint that was entered.
    let endpoint = format!("tcp/127.0.0.1:{}", free_port());
    fill(&mut cx, &panel, ids!(mode), "client");
    fill(&mut cx, &panel, ids!(endpoint), &endpoint);

    let started = Instant::now();
    assert!(click(&mut cx, &mut panel, ids!(connect)));
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "connect blocked the UI thread for {:?}",
        started.elapsed()
    );
    assert!(
        wait_until(SETTLE, || session
            .snapshot()
            .last_error
            .is_some_and(|error| error.contains(&endpoint))),
        "{:?}",
        session.snapshot().last_error
    );
    sync(&mut cx, &mut panel);
    let status = text(&mut cx, &panel, ids!(status));
    assert!(status.contains(&endpoint), "{status}");
    session.disconnect();
}

/// What the status adds once the session has waited 2 s without a peer.
const NO_PEER_HINT: &str = "no dviz peer found by multicast; enter the robot's endpoint";

#[test]
fn listening_without_a_peer_shows_listening_then_the_no_peer_hint() {
    let (mut cx, session) = setup();
    let mut panel = new_panel(&mut cx, session);
    // Like the default mode (empty endpoint), but hermetic: listening on
    // loopback with multicast off, so no other Zenoh node can answer.
    session
        .connect(SessionOptions {
            listen_endpoints: vec![format!("tcp/127.0.0.1:{}", free_port())],
            prefix: "dviz_panel_alone".into(),
            ..SessionOptions::default()
        })
        .unwrap();
    let opened = Instant::now();
    assert!(
        wait_until(SETTLE, || {
            let snapshot = session.snapshot();
            snapshot.connected || snapshot.listening_for.is_some()
        }),
        "the session never opened: {:?}",
        session.snapshot()
    );
    sync(&mut cx, &mut panel);
    let status = text(&mut cx, &panel, ids!(status));
    assert!(
        status.starts_with("Listening, no peer found yet"),
        "{status}"
    );
    assert!(!status.contains("Error"), "{status}");
    if opened.elapsed() < Duration::from_millis(1500) {
        assert!(!status.contains(NO_PEER_HINT), "hint too early: {status}");
    }

    thread::sleep(Duration::from_millis(2200).saturating_sub(opened.elapsed()));
    sync(&mut cx, &mut panel);
    let status = text(&mut cx, &panel, ids!(status));
    assert!(status.contains(NO_PEER_HINT), "{status}");
    assert!(!status.contains("Error"), "{status}");
    assert!(!session.snapshot().connected);
    session.disconnect();
}

#[test]
fn disable_keeps_the_session_connected() {
    let (mut cx, session) = setup();
    let mut panel = new_panel(&mut cx, session);
    let (_peer, endpoint) = listening_peer();
    connect_through_panel(
        &mut cx,
        &mut panel,
        session,
        &endpoint,
        "dviz_panel_disable",
    );

    panel.on_disable(&mut cx);
    // `disconnect` would clear `connected` at once; wait a little anyway.
    thread::sleep(Duration::from_millis(200));
    let snapshot = session.snapshot();
    assert!(snapshot.connected, "{:?}", snapshot.last_error);
    assert_eq!(snapshot.receivers_alive, 1);
    session.disconnect();
}

#[test]
fn open_viewer_failure_is_shown_and_session_keeps_running() {
    // With the `rerun` feature a real viewer could start. Run the body in a
    // child process with an empty `PATH` instead, so `rerun` cannot be found
    // and the other tests keep their `PATH`.
    #[cfg(feature = "rerun")]
    if std::env::var_os("DVIZ_PANEL_CHILD").is_none() {
        assert_child_passes(
            "open_viewer_failure_is_shown_and_session_keeps_running",
            &[("PATH", "".as_ref()), ("DVIZ_PANEL_CHILD", "1".as_ref())],
        );
        return;
    }

    let (mut cx, session) = setup();
    let mut panel = new_panel(&mut cx, session);
    let (peer, endpoint) = listening_peer();
    let prefix = "dviz_panel_viewer";
    connect_through_panel(&mut cx, &mut panel, session, &endpoint, prefix);

    assert!(click(&mut cx, &mut panel, ids!(open_viewer)));
    let expected = match launch_outcome(session) {
        ViewerState::Failed(error) => format!("3D window: {error}"),
        other => panic!("no viewer should open here: {other:?}"),
    };
    sync(&mut cx, &mut panel);
    let status = text(&mut cx, &panel, ids!(status));
    assert!(status.contains(&expected), "{status}");

    let logs = matched_publisher(&peer, format!("{prefix}/logs"));
    logs.put(log_message("INFO", "still receiving"))
        .wait()
        .unwrap();
    assert!(
        wait_until(SETTLE, || session
            .snapshot()
            .logs
            .iter()
            .any(|entry| entry.message == "still receiving")),
        "the session stopped receiving"
    );
    sync(&mut cx, &mut panel);
    let shown = text(&mut cx, &panel, ids!(logs));
    assert!(shown.contains("still receiving"), "{shown}");
    let status = text(&mut cx, &panel, ids!(status));
    assert!(status.contains(&expected), "{status}");
    assert!(session.snapshot().connected);
    session.disconnect();
}

/// Waits for the 3D window's launch to end and returns how it ended.
fn launch_outcome(session: &DvizSession) -> ViewerState {
    let mut outcome = ViewerState::Closed;
    let ended = wait_until(Duration::from_secs(15), || {
        outcome = session.snapshot().viewer;
        !matches!(outcome, ViewerState::Closed | ViewerState::Starting)
    });
    assert!(ended, "the launch never ended: {outcome:?}");
    outcome
}

/// The fake `rerun` answers `--version` only after 1 s, and with 0.20.3, so
/// the launch is still running when the clicks return and then fails.
#[cfg(all(feature = "rerun", unix))]
#[test]
fn open_viewer_button_returns_at_once_and_shows_the_launch() {
    use std::{fs, os::unix::fs::PermissionsExt};
    const TEST: &str = "open_viewer_button_returns_at_once_and_shows_the_launch";
    if std::env::var_os("DVIZ_PANEL_FAKE_RERUN").is_none() {
        let dir =
            std::env::temp_dir().join(format!("dviz-panel-fake-rerun-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("rerun");
        fs::write(
            &exe,
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then\n\
             sleep 1\necho 'rerun 0.20.3 [rustc 1.80.0] aarch64-unknown-linux-gnu'\nexit 0\nfi\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let path =
            std::env::join_paths(std::iter::once(dir.clone()).chain(std::env::split_paths(&path)))
                .unwrap();
        let result = std::panic::catch_unwind(|| {
            assert_child_passes(
                TEST,
                &[
                    ("PATH", path.as_os_str()),
                    ("DVIZ_PANEL_FAKE_RERUN", "1".as_ref()),
                ],
            )
        });
        let _ = fs::remove_dir_all(&dir);
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
        return;
    }

    let (mut cx, session) = setup();
    let mut panel = new_panel(&mut cx, session);
    // The second click lands while the first launch still runs.
    for _ in 0..2 {
        let started = Instant::now();
        assert!(click(&mut cx, &mut panel, ids!(open_viewer)));
        let took = started.elapsed();
        assert!(
            took < Duration::from_millis(100),
            "the click blocked the UI thread for {took:?}"
        );
        let status = text(&mut cx, &panel, ids!(status));
        assert!(status.contains("3D window: starting"), "{status}");
    }
    match launch_outcome(session) {
        ViewerState::Failed(error) => assert!(error.contains("0.20.3"), "{error}"),
        other => panic!("a 0.20 viewer was accepted: {other:?}"),
    }
    sync(&mut cx, &mut panel);
    let status = text(&mut cx, &panel, ids!(status));
    assert!(
        status.contains("3D window: ") && status.contains("0.20.3"),
        "{status}"
    );
    assert!(!status.contains("starting"), "{status}");
}

#[test]
fn two_panels_share_one_session() {
    let (mut cx, session) = setup();
    let mut first = new_panel(&mut cx, session);
    let mut second = new_panel(&mut cx, session);
    sync(&mut cx, &mut second);
    let status = text(&mut cx, &second, ids!(status));
    assert!(status.starts_with("Not connected"), "{status}");

    let (_peer, endpoint) = listening_peer();
    connect_through_panel(&mut cx, &mut first, session, &endpoint, "dviz_panel_shared");
    sync(&mut cx, &mut second);
    let status = text(&mut cx, &second, ids!(status));
    assert!(status.starts_with("Connected"), "{status}");
    session.disconnect();
}

#[test]
fn entity_toggle_disables_routing() {
    let (mut cx, session) = setup();
    let mut panel = new_panel(&mut cx, session);
    session.set_entity_enabled("world/lidar", true);
    sync(&mut cx, &mut panel);

    let path = [
        live_id!(entities),
        LiveId::from_str("world/lidar"),
        live_id!(toggle),
    ];
    let toggle = panel.root().widget(&mut cx, &path);
    assert!(!toggle.is_empty(), "no toggle row for world/lidar");
    assert!(toggle.as_check_box().active(&cx));
    let actions = cx
        .capture_actions(|cx| cx.widget_action(toggle.widget_uid(), CheckBoxAction::Change(false)));
    assert!(panel.handle_actions(&mut cx, &actions, &mut MockBackend::new()));
    assert!(!session.snapshot().entities["world/lidar"].enabled);

    // The next sync keeps the same row and shows it switched off.
    sync(&mut cx, &mut panel);
    let synced = panel.root().widget(&mut cx, &path);
    assert_eq!(synced.widget_uid(), toggle.widget_uid());
    assert!(!synced.as_check_box().active(&cx));
    assert_no_script_errors(&mut cx);
}

#[test]
fn entity_list_shows_64_rows_then_how_many_more() {
    let (mut cx, session) = setup();
    let mut panel = new_panel(&mut cx, session);
    let more = panel.root().widget(&mut cx, ids!(more_entities));
    assert!(!more.is_empty(), "no more_entities label");
    for n in 0..70 {
        session.set_entity_enabled(&format!("world/e{n:02}"), true);
    }
    sync(&mut cx, &mut panel);

    let row = |cx: &mut Cx, name: &str| {
        let path = [live_id!(entities), LiveId::from_str(name), live_id!(toggle)];
        panel.root().widget(cx, &path)
    };
    assert!(!row(&mut cx, "world/e63").is_empty(), "no row for the 64th");
    assert!(row(&mut cx, "world/e64").is_empty(), "a 65th row was built");
    assert!(more.visible());
    assert_eq!(more.text(), "+6 more");
    assert_no_script_errors(&mut cx);
}

#[test]
fn graph_view_builds_without_script_errors_and_shows_node_detail_on_select() {
    let (mut cx, session) = setup();
    let mut panel = new_panel(&mut cx, session);
    let (peer, endpoint) = listening_peer();
    let prefix = "dviz_panel_graph";
    connect_through_panel(&mut cx, &mut panel, session, &endpoint, prefix);
    let publisher = matched_publisher(&peer, format!("{prefix}/graph"));
    publisher
        .put(graph_message(
            &[
                ("lidar", "active", 1.0),
                ("planner", "active", 12.34),
                ("controller", "error", 3.0),
            ],
            &[
                ("lidar", "points", "planner", "cloud"),
                ("dora", "timer/millis/100", "planner", "tick"),
                ("planner", "path", "controller", "path"),
            ],
        ))
        .wait()
        .unwrap();
    assert!(
        wait_until(SETTLE, || session.snapshot().nodes.len() == 3),
        "the graph update never arrived"
    );
    sync(&mut cx, &mut panel);

    // The list stays the default view.
    let root = panel.root();
    let list = root.widget(&mut cx, ids!(nodes));
    let graph = root.widget(&mut cx, ids!(graph));
    let detail = root.widget(&mut cx, ids!(node_detail));
    assert!(!graph.is_empty() && !detail.is_empty());
    assert!(list.visible() && !graph.visible() && !detail.visible());
    assert!(list.text().contains("planner · active"), "{}", list.text());

    assert!(click(&mut cx, &mut panel, ids!(node_graph)));
    sync(&mut cx, &mut panel);
    assert!(!list.visible() && graph.visible());
    assert_eq!(
        root.dviz_graph(&mut cx, ids!(graph)).node_ids(),
        ["lidar", "planner", "controller"]
    );
    assert!(!detail.visible(), "no detail before a node is selected");

    // A click on the planner's box.
    let actions = cx.capture_actions(|cx| {
        cx.widget_action(
            graph.widget_uid(),
            DvizGraphAction::Selected("planner".into()),
        )
    });
    assert!(panel.handle_actions(&mut cx, &actions, &mut MockBackend::new()));
    assert!(detail.visible());
    assert_eq!(
        detail.text(),
        "planner · active · seen at t=12.3 s\n\
         Inputs: cloud ← lidar/points, tick ← dora/timer/millis/100\n\
         Outputs: path → controller/path"
    );
    // The selection survives the next sync.
    sync(&mut cx, &mut panel);
    assert!(detail.visible() && detail.text().starts_with("planner"));

    // Back to the list: the detail goes with the graph.
    assert!(click(&mut cx, &mut panel, ids!(node_list)));
    assert!(list.visible() && !graph.visible() && !detail.visible());
    assert_no_script_errors(&mut cx);
    session.disconnect();
}

/// The instance `id` one draw holds, as its shader reads it.
fn instance<const N: usize>(cx: &mut Cx, draw: &DrawVars, id: LiveId) -> [f32; N] {
    let mut value = [f32::NAN; N];
    draw.get_instance(cx, id, &mut value);
    value
}

/// The status line's `stale` (1 grey, 0 ink).
fn status_grey(cx: &mut Cx, panel: &DvizNavPanel) -> f32 {
    let status = panel.root().widget(cx, ids!(status));
    let label = status.borrow::<Label>().expect("status label");
    instance::<1>(cx, &label.draw_text, live_id!(stale))[0]
}

fn new_nav_panel(cx: &mut Cx, session: &'static DvizSession) -> DvizNavPanel {
    let panel = cx.with_vm(|vm| DvizNavPanel::create(vm, session));
    assert_no_script_errors(cx);
    panel
}

/// A message as the nav bridge sends it, with an optional binary part.
fn nav_message(msg_type: &str, data: serde_json::Value, binary: Option<&[u8]>) -> Vec<u8> {
    let header = MvizMessage {
        msg_type: msg_type.to_string(),
        timestamp: Some(1.0),
        data,
        format: None,
        count: None,
    };
    serialize_message(&header, binary)
}

/// The world rectangle `view` shows, as (min, max).
fn visible(view: &MapView) -> ([f64; 2], [f64; 2]) {
    let top_left = view.screen_to_world([0.0, 0.0]);
    let bottom_right = view.screen_to_world(view.size);
    (
        [top_left[0], bottom_right[1]],
        [bottom_right[0], top_left[1]],
    )
}

fn shows(view: &MapView, (min, max): ([f64; 2], [f64; 2])) -> bool {
    let (seen_min, seen_max) = visible(view);
    seen_min[0] <= min[0] && seen_min[1] <= min[1] && seen_max[0] >= max[0] && seen_max[1] >= max[1]
}

#[test]
fn nav_panel_fit_and_follow_buttons_route() {
    let (mut cx, session) = setup();
    let mut panel = new_nav_panel(&mut cx, session);
    let map = panel.root().dviz_map2_d(&cx, ids!(map));
    assert!(!map.is_empty(), "no map widget");
    let (peer, endpoint) = listening_peer();
    let prefix = "dviz_nav_buttons";
    session
        .connect(SessionOptions {
            mode: "peer".into(),
            connect_endpoints: vec![endpoint],
            listen_endpoints: Vec::new(),
            prefix: prefix.into(),
        })
        .unwrap();
    assert!(wait_until(SETTLE, || session.snapshot().connected));

    // A 10 m x 5 m map from (-5, -2); the robot near its right edge.
    let grid = matched_publisher(&peer, format!("{prefix}/tracer/map/grid"));
    let header = json!({"width": 200, "height": 100, "resolution": 0.05,
                        "origin": [-5.0, -2.0, 0.0], "kind": "occupancy"});
    grid.put(nav_message("grid2d", header, Some(&vec![0u8; 200 * 100])))
        .wait()
        .unwrap();
    let pose = matched_publisher(&peer, format!("{prefix}/tracer/robot/pose"));
    pose.put(nav_message(
        "transform3d",
        json!({"translation": [4.0, 2.5, 0.0]}),
        None,
    ))
    .wait()
    .unwrap();
    assert!(
        wait_until(SETTLE, || {
            let nav = session.snapshot().nav;
            nav.grid.is_some() && nav.pose.is_some()
        }),
        "the map and pose never arrived"
    );
    sync(&mut cx, &mut panel);
    let bounds = scene_bounds(&session.snapshot().nav).unwrap();
    assert_eq!(
        text(&mut cx, &panel, ids!(status)),
        "Waiting for nav status"
    );
    assert_eq!(status_grey(&mut cx, &panel), 1.0);

    // A fresh status: the line in ink.
    let status = matched_publisher(&peer, format!("{prefix}/tracer/nav/status"));
    let state = json!({"loc_ok": true, "loc_reason": "", "armed": false, "estop": false,
                       "gate_reason": "disarmed", "v": 0.2});
    status
        .put(nav_message("status", state, None))
        .wait()
        .unwrap();
    assert!(
        wait_until(SETTLE, || session.snapshot().nav.status.is_some()),
        "the status never arrived"
    );
    sync(&mut cx, &mut panel);
    assert_eq!(
        text(&mut cx, &panel, ids!(status)),
        "Localization OK · Gate disarmed (disarmed) · 0.20 m/s"
    );
    assert_eq!(status_grey(&mut cx, &panel), 0.0);

    // Follow: the robot in the middle, at the zoom the view had.
    assert!(!map.follow());
    assert!(click(&mut cx, &mut panel, ids!(follow)));
    assert!(map.follow());
    let view = map.view();
    assert!(
        (view.center[0] - 4.0).abs() < 1e-6 && (view.center[1] - 2.5).abs() < 1e-6,
        "{view:?}"
    );
    assert!(
        !shows(&view, bounds),
        "the map's far side should be out of view: {view:?}"
    );
    sync(&mut cx, &mut panel);
    assert!(map.follow(), "a sync keeps following");

    // Fit map: the whole map in view, and no more following.
    assert!(click(&mut cx, &mut panel, ids!(fit)));
    assert!(!map.follow());
    let view = map.view();
    assert!(shows(&view, bounds), "{view:?} does not show {bounds:?}");
    sync(&mut cx, &mut panel);
    assert!(shows(&map.view(), bounds));

    // The follow button switches following on and off again.
    assert!(click(&mut cx, &mut panel, ids!(follow)));
    assert!(map.follow());
    assert!(click(&mut cx, &mut panel, ids!(follow)));
    assert!(!map.follow());
    assert_no_script_errors(&mut cx);
    session.disconnect();
}

#[test]
fn nav_panel_layer_toggles_reach_the_map() {
    let (mut cx, session) = setup();
    let mut panel = new_nav_panel(&mut cx, session);
    sync(&mut cx, &mut panel);
    let map = panel.root().dviz_map2_d(&cx, ids!(map));
    assert_eq!(map.layers(), NavLayers::default());
    let toggle = |cx: &mut Cx, panel: &mut DvizNavPanel, path: &[LiveId], on: bool| {
        let check = panel.root().widget(cx, path);
        assert!(!check.is_empty(), "no check box at {path:?}");
        let actions = cx
            .capture_actions(|cx| cx.widget_action(check.widget_uid(), CheckBoxAction::Change(on)));
        assert!(panel.handle_actions(cx, &actions, &mut MockBackend::new()));
    };
    toggle(&mut cx, &mut panel, ids!(layer_scan), false);
    toggle(&mut cx, &mut panel, ids!(layer_map), false);
    assert_eq!(
        map.layers(),
        NavLayers {
            scan: false,
            map: false,
            ..NavLayers::default()
        }
    );
    toggle(&mut cx, &mut panel, ids!(layer_map), true);
    assert!(map.layers().map && !map.layers().scan);
    for path in [
        ids!(layer_footprint),
        ids!(layer_plan),
        ids!(layer_trail),
        ids!(layer_goal),
    ] {
        assert!(!panel.root().widget(&cx, path).is_empty(), "{path:?}");
    }
    let note = text(&mut cx, &panel, ids!(disclaimer));
    assert_eq!(
        note,
        "Wi-Fi e-stop is not a safety device; switch the remote to RC."
    );
    // The session is not connected: the line says so and where to connect,
    // in ink (it is not a stale status).
    assert_eq!(
        text(&mut cx, &panel, ids!(status)),
        "Not connected — set Mode and Endpoint on the Visualization tab and press Connect"
    );
    assert_eq!(status_grey(&mut cx, &panel), 0.0, "the connection line is not grey");
    // Each swatch in its layer's (default) colour.
    let light_grey = 220.0 / 255.0;
    for (path, rgb) in [
        (ids!(swatch_plan), [0.0, 180.0 / 255.0, 1.0]),
        (ids!(swatch_trail), [0.0, 220.0 / 255.0, 0.0]),
        (ids!(swatch_footprint), [light_grey; 3]),
        (ids!(swatch_goal), [1.0, 170.0 / 255.0, 0.0]),
        (ids!(swatch_scan), [1.0, 0.0, 0.0]),
    ] {
        let swatch = panel.root().widget(&cx, path);
        let view = swatch.borrow::<View>().expect("swatch");
        let color = instance::<4>(&mut cx, &view.draw_bg, live_id!(color));
        assert_eq!(color, [rgb[0], rgb[1], rgb[2], 1.0], "{path:?}");
    }
    assert_no_script_errors(&mut cx);
}

/// The legend's Costmap row: on by default, a magenta swatch (the costmap
/// palette's lethal colour, not a colour the bridge sends), and switching it
/// off reaches the map, whose frame then leaves the costmap out.
#[test]
fn nav_panel_legend_has_costmap() {
    let (mut cx, session) = setup();
    let mut panel = new_nav_panel(&mut cx, session);
    sync(&mut cx, &mut panel);
    assert_eq!(text(&mut cx, &panel, ids!(layer_costmap)), "Costmap");
    let check = panel.root().check_box(&cx, ids!(layer_costmap));
    assert!(check.active(&cx), "the Costmap layer starts on");
    {
        let swatch = panel.root().widget(&cx, ids!(swatch_costmap));
        let view = swatch.borrow::<View>().expect("costmap swatch");
        let color = instance::<4>(&mut cx, &view.draw_bg, live_id!(color));
        assert_eq!(color, [1.0, 0.0, 1.0, 1.0]);
    }
    let map = panel.root().dviz_map2_d(&cx, ids!(map));
    let set = |cx: &mut Cx, panel: &mut DvizNavPanel, on: bool| {
        let uid = panel.root().widget(cx, ids!(layer_costmap)).widget_uid();
        let actions = cx.capture_actions(|cx| cx.widget_action(uid, CheckBoxAction::Change(on)));
        assert!(panel.handle_actions(cx, &actions, &mut MockBackend::new()));
    };
    set(&mut cx, &mut panel, false);
    assert_eq!(
        map.layers(),
        NavLayers {
            costmap: false,
            ..NavLayers::default()
        }
    );
    let scene = costmap_scene(vec![254; 8], 1);
    map.set_scene(&mut cx, &scene);
    let frame = map.draw_plan();
    assert!(!frame.contains(&MapDraw::Costmap), "{frame:?}");
    assert!(frame.contains(&MapDraw::Map), "{frame:?}");
    set(&mut cx, &mut panel, true);
    assert!(map.layers().costmap);
    map.set_scene(&mut cx, &scene);
    assert!(map.draw_plan().contains(&MapDraw::Costmap));
    assert_no_script_errors(&mut cx);
}

fn grid_scene(cells: Vec<u8>, generation: u64) -> NavScene {
    let header = Grid2DHeader {
        width: 4,
        height: 2,
        resolution: 0.5,
        origin: [0.0, 0.0, 0.0],
        kind: GridKind::Occupancy,
    };
    NavScene {
        grid: Some(GridLayer {
            header,
            cells: Arc::new(cells),
            generation,
        }),
        pose: Some(Pose2D {
            x: 1.0,
            y: 0.5,
            yaw: 0.0,
        }),
        updated: generation,
        ..NavScene::default()
    }
}

fn new_map(cx: &mut Cx) -> DvizMap2DRef {
    let map = cx.with_vm(|vm| {
        let value = script_eval!(vm, {
            use mod.prelude.widgets.*
            use mod.widgets.*
            mod.widgets.DvizMap2D{}
        });
        WidgetRef::script_from_value(vm, value)
    });
    assert_no_script_errors(cx);
    map.as_dviz_map2_d()
}

#[test]
fn map_texture_is_rebuilt_only_when_the_grid_changes() {
    let (mut cx, _) = setup();
    let map = new_map(&mut cx);
    assert!(!map.is_empty());
    assert_eq!(map.texture_builds(), 0);
    let first = grid_scene(vec![0; 8], 1);
    map.set_scene(&mut cx, &first);
    assert_eq!(map.texture_builds(), 1);
    assert!(map.has_texture());
    // The same scene again, and a newer scene with the same grid: no rebuild.
    map.set_scene(&mut cx, &first);
    let mut moved = first.clone();
    moved.pose = Some(Pose2D {
        x: 2.0,
        y: 0.5,
        yaw: 1.0,
    });
    moved.updated += 1;
    map.set_scene(&mut cx, &moved);
    assert_eq!(map.texture_builds(), 1);
    // The bridge sends the static map again: a new generation, same cells.
    map.set_scene(&mut cx, &grid_scene(vec![0; 8], 2));
    assert_eq!(map.texture_builds(), 1);
    // Different cells: rebuilt.
    map.set_scene(&mut cx, &grid_scene(vec![100; 8], 3));
    assert_eq!(map.texture_builds(), 2);
    // A reconnect restarts the generation at 1 with another map: rebuilt,
    // although the generation went back to one seen before.
    map.set_scene(&mut cx, &grid_scene(vec![255; 8], 1));
    assert_eq!(map.texture_builds(), 3);
    // No grid: the texture goes.
    map.set_scene(&mut cx, &NavScene::default());
    assert!(!map.has_texture());
    assert_no_script_errors(&mut cx);
}

/// A map with a 4 x 2 costmap over it; the costmap's cells and generation
/// as given, the map's fixed.
fn costmap_scene(cells: Vec<u8>, generation: u64) -> NavScene {
    let mut scene = grid_scene(vec![0; 8], 1);
    let header = Grid2DHeader {
        kind: GridKind::Costmap,
        ..scene.grid.as_ref().unwrap().header.clone()
    };
    scene.costmap = Some(GridLayer {
        header,
        cells: Arc::new(cells),
        generation,
    });
    scene.updated = 100 + generation;
    scene
}

/// The costmap texture follows the map texture's rules: built again only
/// when its cells change, not for a resend or a newer scene, rebuilt after a
/// reconnect that restarts the generation, dropped without a costmap; and
/// never rebuilds the map texture.
#[test]
fn costmap_texture_is_rebuilt_only_when_it_changes() {
    let (mut cx, _) = setup();
    let map = new_map(&mut cx);
    let first = costmap_scene(vec![0, 1, 252, 253, 254, 255, 0, 0], 1);
    map.set_scene(&mut cx, &first);
    assert_eq!((map.texture_builds(), map.costmap_texture_builds()), (1, 1));
    assert!(map.has_costmap_texture());
    // The same scene again, and a newer scene with the same costmap.
    map.set_scene(&mut cx, &first);
    let mut moved = first.clone();
    moved.pose = Some(Pose2D {
        x: 2.0,
        y: 0.5,
        yaw: 1.0,
    });
    moved.updated += 1;
    map.set_scene(&mut cx, &moved);
    assert_eq!(map.costmap_texture_builds(), 1);
    // The bridge resends it every 5 s: a new generation, same cells.
    let resent = costmap_scene(first.costmap.as_ref().unwrap().cells.to_vec(), 2);
    map.set_scene(&mut cx, &resent);
    assert_eq!(map.costmap_texture_builds(), 1);
    // An obstacle moved: rebuilt, the map's texture kept.
    map.set_scene(&mut cx, &costmap_scene(vec![254; 8], 3));
    assert_eq!((map.texture_builds(), map.costmap_texture_builds()), (1, 2));
    // A reconnect restarts the generation at 1 with other cells: rebuilt.
    map.set_scene(&mut cx, &costmap_scene(vec![0; 8], 1));
    assert_eq!(map.costmap_texture_builds(), 3);
    // Without a costmap the texture goes; the map's stays.
    map.set_scene(&mut cx, &grid_scene(vec![0; 8], 1));
    assert!(!map.has_costmap_texture() && map.has_texture());
    assert_eq!(map.texture_builds(), 1);
    assert_no_script_errors(&mut cx);
}

#[test]
fn map_view_fits_the_first_scene_until_moved() {
    let (mut cx, _) = setup();
    let map = new_map(&mut cx);
    let scene = grid_scene(vec![0; 8], 1);
    let bounds = scene_bounds(&scene).unwrap();
    map.set_scene(&mut cx, &scene);
    assert!(shows(&map.view(), bounds), "{:?}", map.view());
    // Following puts the robot in the middle.
    map.set_follow(&mut cx, true);
    let view = map.view();
    assert!((view.center[0] - 1.0).abs() < 1e-9 && (view.center[1] - 0.5).abs() < 1e-9);
    let mut moved = scene.clone();
    moved.pose = Some(Pose2D {
        x: 1.5,
        y: 0.75,
        yaw: 0.0,
    });
    moved.updated += 1;
    map.set_scene(&mut cx, &moved);
    assert_eq!(map.view().center, [1.5, 0.75]);
    map.fit(&mut cx);
    assert!(!map.follow());
    assert!(shows(&map.view(), bounds));
    let hidden = NavLayers {
        trail: false,
        ..NavLayers::default()
    };
    map.set_layers(&mut cx, hidden);
    assert_eq!(map.layers(), hidden);
}
