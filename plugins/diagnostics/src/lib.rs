//! D-03.3 run diagnostics: browse simulation runs and read their `dora.log`.
//!
//! Read-only by design. The plugin does not start/stop Dora, does not mutate a
//! run directory, and does not own replay: opening a dataset hands the path to
//! the existing Library/Replay workflow through `Intent::OpenFolder`.
mod panel;
pub mod runs;

use dorobot_studio_core::api::Backend;
use dorobot_studio_plugin::*;
use panel::DiagnosticsPanel;

pub struct DiagnosticsPlugin;

impl StudioPlugin for DiagnosticsPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "dorobot.diagnostics",
            title: "Run diagnostics",
            api_version: API_VERSION,
            panels: &[PanelDescriptor {
                id: "page_diagnostics",
                title: "Run diagnostics",
                legacy_screen: None,
            }],
        }
    }

    fn register(&self, _vm: &mut ScriptVm) {
        // The panel only composes widgets owned by dorobot-studio-core and
        // dorobot-ux; the host registers those before plugin widgets.
    }

    fn create(&self, panel: &str, vm: &mut ScriptVm) -> Result<Box<dyn PluginPanel>, String> {
        if panel != "page_diagnostics" {
            return Err(format!("Unknown diagnostics panel {panel}"));
        }
        Ok(Box::new(DiagnosticsPanel::create(vm)))
    }

    fn can_disable(&self, _backend: &dyn Backend) -> Result<(), String> {
        // Nothing to stop: the panel only reads files.
        Ok(())
    }
}
