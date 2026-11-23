//! The `dviz_view` panel driven through its real widgets, headless (`Cx::new`
//! without a window). Each test has its own leaked `&'static DvizSession`, its
//! own loopback port and topic prefix, and multicast off, as in `loopback.rs`.

use dorobot_plugin_dviz::{open_viewer, DvizSession, DvizViewPanel};
use dorobot_studio_core::api::mock::MockBackend;
use dorobot_studio_plugin::*;
use dviz_protocol::{serialize_message, MvizMessage};
use serde_json::json;
use std::{
    net::TcpListener,
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

fn sync(cx: &mut Cx, panel: &mut DvizViewPanel) {
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

fn click(cx: &mut Cx, panel: &mut DvizViewPanel, path: &[LiveId]) -> bool {
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

fn text(cx: &mut Cx, panel: &DvizViewPanel, path: &[LiveId]) -> String {
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
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "open_viewer_failure_is_shown_and_session_keeps_running",
            ])
            .env("PATH", "")
            .env("DVIZ_PANEL_CHILD", "1")
            .output()
            .expect("run the test binary");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let expected = open_viewer(&DvizSession::new()).unwrap_err();
    let (mut cx, session) = setup();
    let mut panel = new_panel(&mut cx, session);
    let (peer, endpoint) = listening_peer();
    let prefix = "dviz_panel_viewer";
    connect_through_panel(&mut cx, &mut panel, session, &endpoint, prefix);

    assert!(click(&mut cx, &mut panel, ids!(open_viewer)));
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
