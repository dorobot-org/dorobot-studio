//! D-03.2 component debug panel: read-only view of a running dataflow through
//! the local `dora` CLI.
//!
//! The plugin never links `dora-node-api` and never mutates a dataflow. Every
//! action is a CLI subprocess with an argv array; the panel shows the resolved
//! binary path and `--version` because Dora forks differ.
mod cli;
mod panel;

use dorobot_studio_core::api::Backend;
use dorobot_studio_plugin::*;
use panel::DoraCliPanel;

pub struct DoraCliPlugin;

impl StudioPlugin for DoraCliPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "dorobot.dora-cli",
            title: "Component debug",
            api_version: API_VERSION,
            panels: &[PanelDescriptor {
                id: "page_dora_cli",
                title: "Component debug",
                legacy_screen: None,
            }],
        }
    }

    fn register(&self, _vm: &mut ScriptVm) {
        // The panel composes existing dorobot-ux / core widgets only.
    }

    fn create(&self, panel: &str, vm: &mut ScriptVm) -> Result<Box<dyn PluginPanel>, String> {
        if panel != "page_dora_cli" {
            return Err(format!("Unknown dora-cli panel {panel}"));
        }
        Ok(Box::new(DoraCliPanel::create(vm)))
    }

    fn can_disable(&self, _backend: &dyn Backend) -> Result<(), String> {
        // Read-only; the panel kills a running stream in `on_disable`.
        Ok(())
    }
}
