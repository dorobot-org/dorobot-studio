use dorobot_studio_core::api::Backend;
use dorobot_studio_plugin::*;

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
            "dviz_view" => Ok(Box::new(DvizViewPanel::create(vm))),
            _ => Err(format!("Unknown dviz panel {panel}")),
        }
    }

    fn can_disable(&self, _: &dyn Backend) -> Result<(), String> {
        Ok(())
    }
}

/// Placeholder panel: only a title until the real view lands.
struct DvizViewPanel {
    root: WidgetRef,
}

impl DvizViewPanel {
    fn create(vm: &mut ScriptVm) -> Self {
        let value = script_eval!(vm, {
            use mod.prelude.widgets.*
            use mod.widgets.*
            View{width: Fill height: Fill flow: Down padding: 16.0
                Label{text: "Visualization"}
            }
        });
        Self {
            root: WidgetRef::script_from_value(vm, value),
        }
    }
}

impl PluginPanel for DvizViewPanel {
    fn root(&self) -> WidgetRef {
        self.root.clone()
    }

    fn sync(&mut self, _: &mut Cx, _: &PanelContext<'_>) {}

    fn handle_actions(&mut self, _: &mut Cx, _: &Actions, _: &mut dyn Backend) -> bool {
        false
    }
}
