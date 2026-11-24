//! `dorobot-nav-bridge <config.json>`: the Dora node. Wiring only; the logic is in the
//! library (`run`, `worker`, `logs`, `publish`). See README.md.

use dora_node_api::{DoraNode, Event};
use dorobot_nav_bridge::config::load_config;
use dorobot_nav_bridge::logs::LogSources;
use dorobot_nav_bridge::run::{config_arg, input_bytes, log_dir, Bridge, Flow, NodeEvent, USAGE};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How long after Stop the bridge waits for its worker, its command channel and the
/// Zenoh session close to finish.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(1);

fn main() {
    let code = run();
    exit_now(code);
}

fn run() -> i32 {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if matches!(args.as_slice(), [arg] if arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return 0;
    }
    // Dora first, before anything that can fail: Dora 1.1 fails the start of every
    // node of the dataflow when one node stops before it subscribes, whatever its
    // exit code. Bad arguments are handled below, as a node that idles.
    let (node, mut events) = match DoraNode::init_from_env_force() {
        Ok(node) => node,
        Err(e) => {
            eprintln!("nav-bridge: not started by Dora: {e}\n{USAGE}");
            return 1;
        }
    };
    // From here on the node is part of a dataflow and must not fail it: whatever
    // goes wrong while starting is reported once and the node only drains events.
    // DORA_NAV's autostart.sh takes this line in the node's Dora log as the sign that
    // the dataflow got past Dora's start (every node subscribed): keep its wording.
    println!("nav-bridge: started by Dora (dataflow {}, node {})", node.dataflow_id(), node.id());
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let logs = log_dir(std::env::var_os("NAV_VIZ_LOG_DIR"), &cwd, &node.dataflow_id().to_string());
    let dataflow = std::env::var_os("NAV_VIZ_DATAFLOW").filter(|v| !v.is_empty()).map(PathBuf::from);
    let started = config_arg(&args)
        .and_then(|config| load_config(&config).map_err(|e| format!("config: {e}")))
        .and_then(|cfg| {
            println!(
                "nav-bridge: robot {}, listening on {}, commands on dviz/{}/cmd to {}, node logs from {}, graph from {}",
                cfg.robot_id,
                cfg.listen.join(", "),
                cfg.robot_id,
                cfg.cmd.forward,
                logs.display(),
                dataflow.as_ref().map_or_else(|| "nowhere (NAV_VIZ_DATAFLOW not set)".into(), |p| p.display().to_string())
            );
            let sources = LogSources { log_dir: Some(logs), dataflow, own_node: Some(node.id().to_string()) };
            Bridge::start(cfg, sources)
        });
    let mut bridge = started.unwrap_or_else(|cause| {
        eprintln!("nav-bridge: ERROR: {cause}; publishing nothing, Dora inputs are consumed and dropped until Stop");
        Bridge::disabled()
    });

    let mut stderr = std::io::stderr();
    while let Some(event) = events.recv() {
        let event = match event {
            Event::Input { id, data, .. } => NodeEvent::Input { id: id.into(), data: input_bytes(&data) },
            Event::Stop(_) => NodeEvent::Stop,
            Event::Error(e) => {
                eprintln!("nav-bridge: dora: {e}");
                NodeEvent::Other
            }
            _ => NodeEvent::Other,
        };
        if bridge.on_event(event, &mut stderr) == Flow::Stop {
            break;
        }
    }

    // Stop (or the end of the event stream). The worker leaves its loop at once while
    // Dora's side is torn down here; `wait` then ends the command channel and closes Zenoh.
    println!("nav-bridge: stopping");
    let deadline = Instant::now() + SHUTDOWN_WAIT;
    bridge.request_stop();
    drop(events);
    drop(node);
    let closed = bridge.wait(deadline);
    println!("{}", bridge.summary());
    if !closed {
        println!("nav-bridge: Zenoh still closing after {SHUTDOWN_WAIT:?} (a stalled Studio reader?); exiting without it");
    }
    0
}

/// Ends the process at once: flushes stdout and stderr, then `_exit(2)`.
///
/// After Stop the worker has left its loop, so all that can still be running is the
/// command channel's end (queued commands answered) and the Zenoh session close, which a
/// connected but stalled Studio reader holds up for seconds (zenoh-link-tcp sets a 10 s
/// `SO_LINGER`). Neither returning from `main` nor `std::process::exit` is safe then:
/// both run the exit-time destructors of `zenoh-shm`'s statics (compiled in by
/// dora-node-api) while the close still uses them, which panics. `_exit` runs no
/// destructors; the kernel closes the sockets without lingering. Nothing else is
/// pending: the bridge writes no files, and stdio is flushed.
fn exit_now(code: i32) -> ! {
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    extern "C" {
        fn _exit(status: std::os::raw::c_int) -> !;
    }
    // SAFETY: `_exit` takes a plain int and never returns; skipping atexit handlers
    // and destructors is the point (see above).
    unsafe { _exit(code) }
}
