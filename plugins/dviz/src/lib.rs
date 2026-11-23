use dorobot_studio_core::api::Backend;
use dorobot_studio_plugin::*;

#[cfg(feature = "rerun")]
mod rerun_sink;
mod route;
mod session;
mod view_panel;

#[cfg(feature = "rerun")]
pub use rerun_sink::RerunSink;
pub use route::{route, CountingSink, EntityInfo, SessionState, Sink, LOG_CAP};
pub use session::{DvizSession, SessionOptions, SessionSnapshot, RING_CAP};
pub use view_panel::DvizViewPanel;

/// Opens the Rerun 3D viewer and routes every visual message into it.
#[cfg(feature = "rerun")]
pub fn open_viewer(session: &DvizSession) -> Result<(), String> {
    let sink = RerunSink::spawn("dorobot-dviz")?;
    session.set_sink(Box::new(sink));
    Ok(())
}

/// Without the `rerun` feature there is no viewer to open.
#[cfg(not(feature = "rerun"))]
pub fn open_viewer(_: &DvizSession) -> Result<(), String> {
    Err("This build does not include Rerun".to_string())
}

pub struct DvizPlugin;

impl StudioPlugin for DvizPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "dorobot.dviz",
            title: "3D visualization",
            api_version: API_VERSION,
            panels: &[PanelDescriptor {
                id: "dviz_view",
                title: "Visualization",
                legacy_screen: None,
            }],
        }
    }

    fn register(&self, _: &mut ScriptVm) {}

    fn create(&self, panel: &str, vm: &mut ScriptVm) -> Result<Box<dyn PluginPanel>, String> {
        match panel {
            "dviz_view" => Ok(Box::new(DvizViewPanel::create(vm, DvizSession::global()))),
            _ => Err(format!("Unknown dviz panel {panel}")),
        }
    }

    fn can_disable(&self, _: &dyn Backend) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "rerun"))]
    #[test]
    fn open_viewer_without_rerun_says_so() {
        let err = open_viewer(&DvizSession::new()).unwrap_err();
        assert!(err.contains("Rerun"), "{err}");
    }

    /// With the feature, a missing viewer executable is an error that names Rerun.
    /// Run in a child process with an empty `PATH` so other tests keep theirs.
    #[cfg(feature = "rerun")]
    #[test]
    fn open_viewer_without_rerun_binary_reports_an_error() {
        const TEST: &str = "tests::open_viewer_without_rerun_binary_reports_an_error";
        if std::env::var_os("DVIZ_VIEWER_CHILD").is_some() {
            let err = open_viewer(&DvizSession::new()).unwrap_err();
            assert!(err.to_lowercase().contains("rerun"), "{err}");
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", TEST])
            .env("PATH", "")
            .env("DVIZ_VIEWER_CHILD", "1")
            .output()
            .expect("run the test binary");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
