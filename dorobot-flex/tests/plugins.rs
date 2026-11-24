use dorobot_flex::api::{mock::MockBackend, Backend, Intent};
use dorobot_plugin_control::screen::HardwareScreenWidgetRefExt;
use dorobot_studio_plugin::*;

fn setup() -> (Cx, PluginRegistry) {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.init_cx_os();
    let registry = dorobot_flex::plugins::builtin_plugins();
    cx.with_vm(|vm| {
        dorobot_studio_core::register_ui(vm);
        registry.register_widgets(vm);
        assert!(vm.take_errors().is_empty());
    });
    (cx, registry)
}

#[test]
fn collection_factory_creates_independent_real_views_and_routes_record_stop_save() {
    let (mut cx, registry) = setup();
    let mut first = cx
        .with_vm(|vm| registry.create_instance("page_record", vm))
        .unwrap();
    let second = cx
        .with_vm(|vm| registry.create_instance("page_record", vm))
        .unwrap();
    assert_ne!(first.root().widget_uid(), second.root().widget_uid());
    let mut backend = MockBackend::new();
    backend.dispatch(Intent::DiscardLast);
    if backend.record().recording {
        backend.dispatch(Intent::RecordStop);
        backend.dispatch(Intent::DiscardLast);
    }
    for (path, recording, saved_delta) in [
        (ids!(transport.btn_record) as &[LiveId], true, 0),
        (ids!(transport.btn_record), false, 0),
        (ids!(transport.btn_save), false, 1),
    ] {
        let previous = backend.record().saved;
        let button = first.root().button(&mut cx, path);
        assert!(!button.is_empty());
        let actions = cx.capture_actions(|cx| {
            cx.widget_action(
                button.widget_uid(),
                ButtonAction::Clicked(KeyModifiers::default()),
            )
        });
        assert!(first.handle_actions(&mut cx, &actions, &mut backend));
        assert_eq!(backend.record().recording, recording);
        assert_eq!(backend.record().saved, previous + saved_delta);
    }
}

#[test]
fn collection_cannot_be_disabled_with_unsaved_take() {
    let mut registry = dorobot_flex::plugins::builtin_plugins();
    let mut backend = MockBackend::new();
    backend.dispatch(Intent::RecordStart);
    assert!(registry
        .set_enabled("dorobot.collection", false, &backend)
        .is_err());
    backend.dispatch(Intent::RecordStop);
    assert!(registry
        .set_enabled("dorobot.collection", false, &backend)
        .is_err());
    backend.dispatch(Intent::SaveEpisode);
    registry
        .set_enabled("dorobot.collection", false, &backend)
        .unwrap();
    assert!(!registry.enabled("dorobot.collection"));
    assert!(registry.enabled("dorobot.control"));
}

#[test]
fn control_factory_preserves_the_hardware_wizard_action() {
    let (mut cx, registry) = setup();
    let mut panel = cx
        .with_vm(|vm| registry.create_instance("page_hardware", vm))
        .unwrap();
    let mut backend = MockBackend::new();
    let before = backend
        .hardware()
        .steps
        .iter()
        .position(|s| s.state == dorobot_flex::api::StepState::Active)
        .unwrap();
    let button = panel.root().button(
        &mut cx,
        ids!(content.camera_a_panel.content.calib.actions.btn_continue),
    );
    assert!(!button.is_empty());
    let actions = cx.capture_actions(|cx| {
        cx.widget_action(
            button.widget_uid(),
            ButtonAction::Clicked(KeyModifiers::default()),
        )
    });
    assert!(panel.handle_actions(&mut cx, &actions, &mut backend));
    assert_eq!(
        backend.hardware().steps[before].state,
        dorobot_flex::api::StepState::Done
    );
}

struct ExtraPlugin;
impl StudioPlugin for ExtraPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "independent.example",
            title: "Example",
            api_version: API_VERSION,
            panels: &[PanelDescriptor {
                id: "independent_panel",
                title: "Independent panel",
                legacy_screen: None,
            }],
        }
    }
    fn register(&self, _: &mut ScriptVm) {}
    fn create(&self, _: &str, vm: &mut ScriptVm) -> Result<Box<dyn PluginPanel>, String> {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{width: Fill height: Fill} });
        Ok(Box::new(ExtraPanel {
            root: WidgetRef::script_from_value(vm, value),
        }))
    }
}
struct ExtraPanel {
    root: WidgetRef,
}
impl PluginPanel for ExtraPanel {
    fn root(&self) -> WidgetRef {
        self.root.clone()
    }
    fn sync(&mut self, _: &mut Cx, _: &PanelContext<'_>) {}
    fn handle_actions(&mut self, _: &mut Cx, _: &Actions, _: &mut dyn Backend) -> bool {
        false
    }
}

#[test]
fn independent_panel_mounts_without_a_new_screen_variant_and_reuses_its_instance() {
    let (mut cx, mut registry) = setup();
    registry.register(Box::new(ExtraPlugin)).unwrap();
    let container = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{} });
        WidgetRef::script_from_value(vm, value)
    });
    registry.mount(&mut cx, &container).unwrap();
    let first = container.widget(&cx, ids!(independent_panel));
    assert!(!first.is_empty());
    registry.mount(&mut cx, &container).unwrap();
    assert_eq!(
        first.widget_uid(),
        container.widget(&cx, ids!(independent_panel)).widget_uid()
    );
    let backend = MockBackend::new();
    registry.select("independent_panel").unwrap();
    registry.sync(
        &mut cx,
        &PanelContext {
            backend: &backend,
            rig_editing: false,
            show_help: false,
            show_details: false,
            live_controls: false,
        },
    );
    assert!(first.visible());
    assert!(!container.widget(&cx, ids!(page_record)).visible());
    registry
        .set_enabled("independent.example", false, &backend)
        .unwrap();
    registry.sync(
        &mut cx,
        &PanelContext {
            backend: &backend,
            rig_editing: false,
            show_help: false,
            show_details: false,
            live_controls: false,
        },
    );
    assert!(!first.visible());
}

#[test]
fn every_builtin_factory_and_toolbox_has_no_script_errors() {
    let (mut cx, mut registry) = setup();
    for descriptor in registry.descriptors() {
        for panel in descriptor.panels {
            cx.with_vm(|vm| {
                registry.create_instance(panel.id, vm).unwrap();
                let errors = vm.take_errors();
                assert!(errors.is_empty(), "{}: {errors:?}", panel.id);
            });
        }
    }
    let container = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.prelude.widgets.* use mod.widgets.* View{} });
        WidgetRef::script_from_value(vm, value)
    });
    registry.mount_toolbox(&mut cx, &container).unwrap();
    let tabs = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{} });
        WidgetRef::script_from_value(vm, value)
    });
    registry.mount_tabs(&mut cx, &tabs).unwrap();
    registry.sync_navigation(&mut cx, dorobot_flex::api::Screen::Hardware);
    cx.with_vm(|vm| {
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
    });
}

#[test]
fn toolbox_has_one_entry_per_plugin_instead_of_one_per_panel() {
    let (mut cx, mut registry) = setup();
    let container = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{} });
        WidgetRef::script_from_value(vm, value)
    });
    registry.mount_toolbox(&mut cx, &container).unwrap();
    let children = container.borrow::<View>().unwrap().children.clone();
    assert_eq!(children.len(), 4, "侧栏应按插件分组，不逐个展开面板");
    assert_eq!(children[0].1.text(), "Robot control");
    assert_eq!(children[1].1.text(), "Data collection");
    assert_eq!(children[2].1.text(), "Training & inference");
    assert_eq!(children[3].1.text(), "3D visualization");
}

#[test]
fn dviz_panel_is_registered_and_creates_independent_views() {
    let (mut cx, registry) = setup();
    assert_eq!(registry.panel_owner("dviz_view"), Some("dorobot.dviz"));
    let first = cx
        .with_vm(|vm| registry.create_instance("dviz_view", vm))
        .unwrap();
    let second = cx
        .with_vm(|vm| registry.create_instance("dviz_view", vm))
        .unwrap();
    assert_ne!(first.root().widget_uid(), second.root().widget_uid());
}

#[test]
fn dviz_panel_syncs_in_both_themes_without_script_errors() {
    let (mut cx, registry) = setup();
    let mut panel = cx
        .with_vm(|vm| registry.create_instance("dviz_view", vm))
        .unwrap();
    let backend = MockBackend::new();
    for light in [1.0, 0.0] {
        dorobot_flex::ui::frame::set_light_mode(light);
        panel.sync(
            &mut cx,
            &PanelContext {
                backend: &backend,
                rig_editing: false,
                show_help: false,
                show_details: false,
                live_controls: false,
            },
        );
    }
    let root = panel.root();
    for path in [ids!(connect), ids!(disconnect), ids!(open_viewer)] {
        assert!(!root.button(&mut cx, path).is_empty(), "{path:?}");
    }
    assert_eq!(root.text_input(&mut cx, ids!(mode)).text(), "peer");
    assert_eq!(root.text_input(&mut cx, ids!(endpoint)).text(), "");
    assert_eq!(root.text_input(&mut cx, ids!(prefix)).text(), "dviz");
    let status = root.widget(&mut cx, ids!(status)).text();
    assert!(status.starts_with("Not connected"), "{status}");
    cx.with_vm(|vm| {
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
    });
}

/// The `light` instance one draw holds, as its shader reads it; NaN when the
/// draw declares no `light`, that is when the theme cannot reach it.
fn light_of(cx: &mut Cx, draw: &DrawVars) -> f32 {
    let mut light = [f32::NAN];
    draw.get_instance(cx, live_id!(light), &mut light);
    light[0]
}

/// Where the theme reached the dviz panel: the `connection` card, the
/// `status` label, the `connect` button and, given an entity, the count label
/// of its row.
fn dviz_theme(cx: &mut Cx, root: &WidgetRef, entity: Option<&str>) -> Vec<(&'static str, f32)> {
    let mut seen = Vec::new();
    let card = root.widget(cx, ids!(connection));
    seen.push((
        "connection card",
        light_of(cx, &card.borrow::<View>().unwrap().draw_bg),
    ));
    let status = root.widget(cx, ids!(status));
    seen.push((
        "status label",
        light_of(cx, &status.borrow::<Label>().unwrap().draw_text),
    ));
    let connect = root.widget(cx, ids!(connect));
    seen.push((
        "connect button",
        light_of(cx, &connect.borrow::<Button>().unwrap().draw_bg),
    ));
    if let Some(entity) = entity {
        let count = root.widget(
            cx,
            &[
                live_id!(entities),
                LiveId::from_str(entity),
                live_id!(count),
            ],
        );
        let count = count
            .borrow::<Label>()
            .unwrap_or_else(|| panic!("no row for {entity}"));
        seen.push(("entity row count", light_of(cx, &count.draw_text)));
    }
    seen
}

fn assert_dviz_theme(cx: &mut Cx, root: &WidgetRef, entity: Option<&str>, light: f64) {
    for (part, value) in dviz_theme(cx, root, entity) {
        assert_eq!(value, light as f32, "{part} at light {light}");
    }
    // The connection fields are the stylesheet's own TextInput, as in P1
    // (white field, dark text, in either theme): a themed field could not be
    // typed into on the desktop. They carry no `light`.
    for path in [ids!(mode), ids!(endpoint), ids!(prefix)] {
        let field = root.widget(cx, path);
        let input = field.borrow::<TextInput>().unwrap();
        let (well, text) = (light_of(cx, &input.draw_bg), light_of(cx, &input.draw_text));
        assert!(well.is_nan() && text.is_nan(), "{path:?} carries light {well} / {text}");
    }
}

#[test]
fn dviz_panel_follows_theme_round_trip() {
    let (mut cx, registry) = setup();
    let mut panel = cx
        .with_vm(|vm| registry.create_instance("dviz_view", vm))
        .unwrap();
    let backend = MockBackend::new();
    // Entities in the panel's (shared) session give it rows, as their first
    // messages would. The second arrives with the switch to light, so the
    // rows are made anew in the same sync that changes the theme.
    let session = dorobot_plugin_dviz::DvizSession::global();
    for (light, arriving) in [
        (0.0, Some("theme/first")),
        (1.0, Some("theme/second")),
        (0.0, None),
    ] {
        if let Some(entity) = arriving {
            session.set_entity_enabled(entity, true);
        }
        dorobot_flex::ui::frame::set_light_mode(light);
        panel.sync(
            &mut cx,
            &PanelContext {
                backend: &backend,
                rig_editing: false,
                show_help: false,
                show_details: false,
                live_controls: false,
            },
        );
        assert_dviz_theme(&mut cx, &panel.root(), Some("theme/first"), light);
    }
    cx.with_vm(|vm| {
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
    });
}

/// A theme switch reloads the stylesheet. The host syncs the visible panel at
/// once; the reload (`Event::LiveEdit`, a new `reload_generation`) then
/// replays every panel's source (`PluginRegistry::reapply`), which puts each
/// `light` back to its default, and the host syncs again. The panel must come
/// out of that in the new theme, the List / Graph buttons included.
#[test]
fn dviz_panel_keeps_its_theme_across_the_style_reload() {
    let (mut cx, mut registry) = setup();
    let pages = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{} });
        WidgetRef::script_from_value(vm, value)
    });
    registry.mount(&mut cx, &pages).unwrap();
    registry.select("dviz_view").unwrap();
    let root = pages.widget(&cx, ids!(dviz_view));
    let backend = MockBackend::new();
    let context = PanelContext {
        backend: &backend,
        rig_editing: false,
        show_help: false,
        show_details: false,
        live_controls: false,
    };
    for light in [1.0, 0.0, 1.0] {
        dorobot_flex::ui::frame::set_light_mode(light);
        registry.sync(&mut cx, &context);
        cx.reload_generation += 1;
        registry.reapply(&mut cx);
        registry.sync(&mut cx, &context);
        assert_dviz_theme(&mut cx, &root, None, light);
        let list = root.widget(&cx, ids!(node_list));
        let list = list.borrow::<Button>().unwrap();
        let mut on = [f32::NAN];
        list.draw_bg.get_instance(&mut cx, live_id!(on), &mut on);
        assert_eq!(on[0], 1.0, "List shows as selected at light {light}");
        drop(list);
    }
    cx.with_vm(|vm| {
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
    });
}

#[test]
fn nav_panel_registered_and_independent() {
    let (mut cx, registry) = setup();
    let dviz = registry
        .descriptors()
        .into_iter()
        .find(|plugin| plugin.id == "dorobot.dviz")
        .unwrap();
    // The dviz plugin now has two panels: the P1 view and the 2D map.
    let panels: Vec<_> = dviz.panels.iter().map(|p| (p.id, p.title)).collect();
    assert_eq!(
        panels,
        [("dviz_view", "Visualization"), ("dviz_nav", "Navigation")]
    );
    assert!(dviz.panels.iter().all(|p| p.legacy_screen.is_none()));
    assert_eq!(registry.panel_owner("dviz_nav"), Some("dorobot.dviz"));
    let first = cx
        .with_vm(|vm| registry.create_instance("dviz_nav", vm))
        .unwrap();
    let second = cx
        .with_vm(|vm| registry.create_instance("dviz_nav", vm))
        .unwrap();
    assert_ne!(first.root().widget_uid(), second.root().widget_uid());
    cx.with_vm(|vm| {
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
    });
}

/// Where the theme reached the nav panel: the legend card and its label, the
/// heading, the status line, the disclaimer and the two buttons. The map is
/// not themed: its canvas is the same dark grey in both themes, as RViz's.
fn nav_theme(cx: &mut Cx, root: &WidgetRef) -> Vec<(&'static str, f32)> {
    let mut seen = Vec::new();
    let legend = root.widget(cx, ids!(legend));
    seen.push((
        "legend card",
        light_of(cx, &legend.borrow::<View>().unwrap().draw_bg),
    ));
    for (part, path) in [
        ("heading", ids!(heading)),
        ("status line", ids!(status)),
        ("disclaimer", ids!(disclaimer)),
        ("legend label", ids!(legend_label)),
    ] {
        let label = root.widget(cx, path);
        let label = label
            .borrow::<Label>()
            .unwrap_or_else(|| panic!("no label {part}"));
        seen.push((part, light_of(cx, &label.draw_text)));
    }
    for (part, path) in [("fit button", ids!(fit)), ("follow button", ids!(follow))] {
        let button = root.widget(cx, path);
        let button = button.borrow::<Button>().unwrap();
        seen.push((part, light_of(cx, &button.draw_bg)));
        seen.push((part, light_of(cx, &button.draw_text)));
    }
    seen
}

/// Like the dviz view: the theme is on every part after a switch, and again
/// after the style reload that replays the panel's source.
#[test]
fn nav_panel_syncs_in_both_themes() {
    let (mut cx, mut registry) = setup();
    let pages = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{} });
        WidgetRef::script_from_value(vm, value)
    });
    registry.mount(&mut cx, &pages).unwrap();
    registry.select("dviz_nav").unwrap();
    let root = pages.widget(&cx, ids!(dviz_nav));
    assert!(!root.is_empty());
    let backend = MockBackend::new();
    let context = PanelContext {
        backend: &backend,
        rig_editing: false,
        show_help: false,
        show_details: false,
        live_controls: false,
    };
    for light in [1.0, 0.0, 1.0] {
        dorobot_flex::ui::frame::set_light_mode(light);
        registry.sync(&mut cx, &context);
        for (part, value) in nav_theme(&mut cx, &root) {
            assert_eq!(value, light as f32, "{part} at light {light}");
        }
        cx.reload_generation += 1;
        registry.reapply(&mut cx);
        registry.sync(&mut cx, &context);
        for (part, value) in nav_theme(&mut cx, &root) {
            assert_eq!(
                value, light as f32,
                "{part} at light {light} after the reload"
            );
        }
        // The session is not connected: the line says so, and where to connect.
        let status = root.widget(&cx, ids!(status)).text();
        assert_eq!(
            status,
            "Not connected — set Mode and Endpoint on the Visualization tab and press Connect"
        );
    }
    cx.with_vm(|vm| {
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
    });
}

#[test]
fn plugin_tabs_follow_navigation_and_group_entries_restore_the_last_tab() {
    use dorobot_flex::api::Screen;
    let (mut cx, mut registry) = setup();
    let root = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{toolbox := View{} tabs := View{}} });
        WidgetRef::script_from_value(vm, value)
    });
    let toolbox = root.widget(&cx, ids!(toolbox));
    let tabs = root.widget(&cx, ids!(tabs));
    registry.mount_toolbox(&mut cx, &toolbox).unwrap();
    registry.mount_tabs(&mut cx, &tabs).unwrap();
    let setup = root.widget(&cx, ids!(plugin_tab_page_hardware));
    let operate = root.widget(&cx, ids!(plugin_tab_hardware_teleop));
    let record = root.widget(&cx, ids!(plugin_tab_page_record));
    registry.sync_navigation(&mut cx, Screen::Hardware);
    assert!(setup.visible() && operate.visible());
    assert!(!record.visible());
    let operate_click = cx.capture_actions(|cx| {
        cx.widget_action(
            operate.widget_uid(),
            ButtonAction::Clicked(KeyModifiers::default()),
        )
    });
    assert_eq!(
        registry
            .tab_selection(&operate_click)
            .unwrap()
            .legacy_screen,
        Some(Screen::Live)
    );
    registry.sync_navigation(&mut cx, Screen::Live);
    registry.sync_navigation(&mut cx, Screen::Record);
    assert!(!setup.visible() && !operate.visible());
    assert!(record.visible());
    assert!(
        registry.tab_selection(&operate_click).is_none(),
        "隐藏标签不能触发跳转"
    );
    let control = root.widget(&cx, &[LiveId::from_str("toolbox_dorobot.control")]);
    let group_click = cx.capture_actions(|cx| {
        cx.widget_action(
            control.widget_uid(),
            ButtonAction::Clicked(KeyModifiers::default()),
        )
    });
    assert_eq!(
        registry
            .toolbox_selection(&mut cx, &group_click)
            .unwrap()
            .id,
        "hardware_teleop"
    );
    registry.sync_navigation(&mut cx, Screen::Play);
    assert!(!setup.visible() && !operate.visible() && !record.visible());
    cx.with_vm(|vm| assert!(vm.take_errors().is_empty()));
}

#[test]
fn independent_plugin_navigation_survives_theme_reapply_and_blocks_disabled_entries() {
    use dorobot_flex::api::Screen;
    let (mut cx, mut registry) = setup();
    registry.register(Box::new(ExtraPlugin)).unwrap();
    let root = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{toolbox := View{} tabs := View{}} });
        WidgetRef::script_from_value(vm, value)
    });
    let toolbox = root.widget(&cx, ids!(toolbox));
    let tabs = root.widget(&cx, ids!(tabs));
    registry.mount_toolbox(&mut cx, &toolbox).unwrap();
    registry.mount_tabs(&mut cx, &tabs).unwrap();
    registry.select("independent_panel").unwrap();
    registry.sync_navigation(&mut cx, Screen::Play);
    assert_eq!(
        registry.active_plugin(Screen::Play).unwrap().id,
        "independent.example"
    );
    let tab = root.widget(&cx, ids!(plugin_tab_independent_panel));
    assert!(tab.visible());
    registry.reapply(&mut cx);
    toolbox.borrow_mut::<View>().unwrap().children.clear();
    tabs.borrow_mut::<View>().unwrap().children.clear();
    registry.mount_toolbox(&mut cx, &toolbox).unwrap();
    registry.mount_tabs(&mut cx, &tabs).unwrap();
    registry.sync_navigation(&mut cx, Screen::Play);
    assert_eq!(
        root.widget(&cx, ids!(plugin_tab_independent_panel))
            .widget_uid(),
        tab.widget_uid()
    );
    assert!(tab.visible());
    let group = root.widget(&cx, &[LiveId::from_str("toolbox_independent.example")]);
    let click = cx.capture_actions(|cx| {
        cx.widget_action(
            group.widget_uid(),
            ButtonAction::Clicked(KeyModifiers::default()),
        )
    });
    assert_eq!(
        registry.toolbox_selection(&mut cx, &click).unwrap().id,
        "independent_panel"
    );
    registry
        .set_enabled("independent.example", false, &MockBackend::new())
        .unwrap();
    registry.sync_navigation(&mut cx, Screen::Play);
    assert!(!tab.visible());
    assert!(registry.toolbox_selection(&mut cx, &click).is_none());
    cx.with_vm(|vm| assert!(vm.take_errors().is_empty()));
}

#[test]
fn plugin_panel_named_play_cannot_shadow_the_hosts_replay_tab() {
    struct NamedPlay;
    impl StudioPlugin for NamedPlay {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: "example.play",
                title: "Example",
                api_version: API_VERSION,
                panels: &[PanelDescriptor {
                    id: "play",
                    title: "Example play",
                    legacy_screen: None,
                }],
            }
        }
        fn register(&self, _: &mut ScriptVm) {}
        fn create(&self, panel: &str, vm: &mut ScriptVm) -> Result<Box<dyn PluginPanel>, String> {
            ExtraPlugin.create(panel, vm)
        }
    }
    let (mut cx, mut registry) = setup();
    registry.register(Box::new(NamedPlay)).unwrap();
    let root = cx.with_vm(|vm| {
        let value = script_eval!(vm, {
            use mod.prelude.widgets.* use mod.widgets.*
            View{plugin_tabs := View{} data_tabs := View{tab_play := mod.widgets.ux.WorkspaceTab{text: "Replay"}}}
        });
        WidgetRef::script_from_value(vm, value)
    });
    let tabs = root.widget(&cx, ids!(plugin_tabs));
    registry.mount_tabs(&mut cx, &tabs).unwrap();
    let replay = root.widget(&cx, ids!(data_tabs.tab_play));
    assert_eq!(
        root.widget(&cx, ids!(tab_play)).widget_uid(),
        replay.widget_uid()
    );
}

#[test]
fn toolbox_controls_cannot_shadow_panel_paths() {
    let (mut cx, mut registry) = setup();
    let root = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{ toolbox := View{} pages := View{} } });
        WidgetRef::script_from_value(vm, value)
    });
    let toolbox = root.widget(&cx, ids!(toolbox));
    let pages = root.widget(&cx, ids!(pages));
    registry.mount_toolbox(&mut cx, &toolbox).unwrap();
    registry.mount(&mut cx, &pages).unwrap();
    assert!(root
        .widget(&cx, ids!(page_hardware))
        .borrow::<dorobot_plugin_control::screen::HardwareScreen>()
        .is_some());
    assert!(root
        .widget(&cx, ids!(page_record))
        .borrow::<dorobot_plugin_collection::screen::RecordScreen>()
        .is_some());
}

#[test]
fn remount_after_container_reapply_restores_the_same_panel_instances() {
    let (mut cx, mut registry) = setup();
    let container = cx.with_vm(|vm| {
        let value = script_eval!(vm, { use mod.prelude.widgets.* use mod.widgets.* View{} });
        WidgetRef::script_from_value(vm, value)
    });
    registry.mount(&mut cx, &container).unwrap();
    let original = container.widget(&cx, ids!(page_record));
    container.borrow_mut::<View>().unwrap().children.clear();
    cx.widget_tree_mark_dirty(container.widget_uid());
    registry.mount(&mut cx, &container).unwrap();
    assert_eq!(
        container.widget(&cx, ids!(page_record)).widget_uid(),
        original.widget_uid()
    );
}

#[test]
fn disconnected_operate_panel_respects_the_hosts_setup_gate() {
    let (mut cx, registry) = setup();
    let mut panel = cx
        .with_vm(|vm| registry.create_instance("hardware_teleop", vm))
        .unwrap();
    let root = std::env::temp_dir().join(format!("dorobot-plugin-empty-{}", std::process::id()));
    let mut backend = dorobot_flex::api::files::FileBackend::new(root);
    backend.dispatch(Intent::Navigate(dorobot_flex::api::Screen::Live));
    assert!(backend.playback().live.is_some());
    let context = PanelContext {
        backend: &backend,
        rig_editing: false,
        show_help: false,
        show_details: false,
        live_controls: false,
    };
    assert_eq!(panel.visible_in(&context), Some(false));
    panel.sync(&mut cx, &context);
}

#[test]
fn hardware_model_switch_returns_before_parsing_and_only_displays_the_latest_choice() {
    let (mut cx, registry) = setup();
    let panel = cx
        .with_vm(|vm| registry.create_instance("page_hardware", vm))
        .unwrap();
    let root = panel.root();
    let stage = root.widget(&cx, ids!(content.robot_panel.content.mirror.viewport));
    let dir = std::env::temp_dir().join(format!("dorobot-async-model-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["first", "latest"] {
        std::fs::write(dir.join(format!("{name}.urdf")), format!(
            "<robot name=\"{name}\"><link name=\"base\"/><link name=\"arm\"><visual><geometry><box size=\"0.1 0.1 0.1\"/></geometry></visual></link><joint name=\"joint\" type=\"revolute\"><parent link=\"base\"/><child link=\"arm\"/><axis xyz=\"0 0 1\"/><limit lower=\"-1\" upper=\"1\" effort=\"1\" velocity=\"1\"/></joint></robot>"
        )).unwrap();
    }
    let mut state = MockBackend::new().hardware().clone();
    state.robot_urdf = Some((dir.join("first.urdf"), dir.clone()));
    root.as_hardware_screen().sync(&mut cx, &state);
    let status = root.widget(&cx, ids!(content.robot_panel.content.mirror.model_status));
    assert!(!status.is_empty() && status.visible());
    assert!(
        stage
            .borrow::<makepad_urdf_player::robot_view::RobotView>()
            .unwrap()
            .robot()
            .is_none(),
        "界面同步必须先返回，模型解析不能阻塞按钮反馈"
    );
    state.robot_urdf = Some((dir.join("latest.urdf"), dir.clone()));
    state.live_angles = vec![0.7];
    root.as_hardware_screen().sync(&mut cx, &state);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        root.handle_event(&mut cx, &Event::Signal, &mut Scope::empty());
        let ready = stage
            .borrow::<makepad_urdf_player::robot_view::RobotView>()
            .unwrap()
            .robot()
            .map(|r| r.name.clone());
        assert!(
            ready.is_none() || ready.as_deref() == Some("latest"),
            "不应挂载过期模型：{ready:?}"
        );
        if ready.as_deref() == Some("latest") {
            assert_eq!(
                stage
                    .borrow::<makepad_urdf_player::robot_view::RobotView>()
                    .unwrap()
                    .joint_angles(),
                [0.7]
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "最新模型未完成后台加载：{ready:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cx.with_vm(|vm| assert!(vm.take_errors().is_empty()));
    std::fs::remove_dir_all(dir).unwrap();
}

fn check_submenu_model_loading(panel_id: &str) {
    use dorobot_flex::ui::play::PlayScreenWidgetRefExt;
    use dorobot_plugin_collection::screen::RecordScreenWidgetRefExt;
    let (mut cx, registry) = setup();
    let panel = cx
        .with_vm(|vm| registry.create_instance(panel_id, vm))
        .unwrap();
    let root = panel.root();
    let dir =
        std::env::temp_dir().join(format!("dorobot-submenu-{panel_id}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("model.urdf");
    std::fs::write(
        &path,
        "<robot name=\"submenu\"><link name=\"base\"/><link name=\"arm\"><visual><geometry><box size=\"0.1 0.1 0.1\"/></geometry></visual></link><joint name=\"joint\" type=\"revolute\"><parent link=\"base\"/><child link=\"arm\"/><axis xyz=\"0 0 1\"/><limit lower=\"-1\" upper=\"1\" effort=\"1\" velocity=\"1\"/></joint></robot>",
    )
    .unwrap();
    let model = Some((path.clone(), path.parent().unwrap().to_path_buf()));
    let backend = MockBackend::new();
    let stage = if panel_id == "page_record" {
        let mut state = backend.record().clone();
        state.robot_urdf = model;
        state.last_take = None;
        state.pose = vec![0.4];
        root.as_record_screen().sync(&mut cx, &state);
        root.widget(&cx, ids!(body.right.view3d.v3_body.robot_view))
    } else {
        let mut state = backend.playback().clone();
        state.robot_urdf = model;
        use dorobot_flex::api::scene::{BasePose, SceneFrame, SceneReplay};
        state.current_time = 0.0;
        state.stats.frames = 1;
        state.state_names = vec!["joint".into()];
        state.joint_frames = vec![vec![0.4]];
        state.scene = Some(SceneReplay {
            version: 2,
            objects: Vec::new(),
            frames: vec![SceneFrame {
                timestamp: 0.,
                sim_time_ns: 0,
                transforms: Vec::new(),
                base_pose: Some(BasePose {
                    position: [1., 2., 0.],
                    quat_xyzw: [0., 0., 0., 1.],
                }),
            }],
        });
        state.video_paths.clear();
        root.as_play_screen().sync(&mut cx, &state);
        root.widget(&cx, ids!(stage_3d))
    };
    assert!(
        stage
            .borrow::<makepad_urdf_player::robot_view::RobotView>()
            .unwrap()
            .robot()
            .is_none(),
        "{panel_id} 的首次同步不应阻塞解析模型"
    );
    assert!(!stage.visible(), "后台加载时应显示占位提示");
    let camera_before = stage
        .borrow::<makepad_urdf_player::robot_view::RobotView>()
        .unwrap()
        .camera_target();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        root.handle_event(&mut cx, &Event::Signal, &mut Scope::empty());
        if stage
            .borrow::<makepad_urdf_player::robot_view::RobotView>()
            .unwrap()
            .robot()
            .is_some()
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let viewer = stage
        .borrow::<makepad_urdf_player::robot_view::RobotView>()
        .unwrap();
    assert_eq!(viewer.joint_angles(), [0.4], "模型首帧应立即使用当前姿态");
    if panel_id != "page_record" {
        assert_eq!(
            viewer.camera_target(),
            camera_before,
            "异步加载不能重置场景相机"
        );
        assert_eq!(
            &viewer.robot().unwrap().base_pose.to_cols_array()[12..15],
            &[1., 2., 0.]
        );
    }
    drop(viewer);
    cx.with_vm(|vm| assert!(vm.take_errors().is_empty()));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn collection_submenu_loads_models_without_blocking_its_first_sync() {
    check_submenu_model_loading("page_record");
}

#[test]
fn operate_and_replay_submenu_loads_models_without_blocking_its_first_sync() {
    check_submenu_model_loading("hardware_teleop");
}

#[test]
fn live_mirror_registers_without_replacing_operate() {
    let (mut cx, registry) = setup();
    assert!(cx.with_vm(|vm| registry.create_instance("hardware_teleop", vm)).is_ok());
    let mirror = cx.with_vm(|vm| registry.create_instance("hardware_live_mirror", vm)).unwrap();
    assert!(!mirror.root().button(&mut cx, ids!(next_source)).is_empty());
}

#[test]
fn one_policy_plugin_contains_training_and_rollout_with_native_actions() {
    let (mut cx, registry) = setup();
    let plugins = registry.descriptors();
    let policy = plugins.iter().find(|descriptor| descriptor.id == "dorobot.policy")
        .expect("policy toolbox is registered");
    let tabs: Vec<_> = policy.panels.iter().map(|panel| panel.id).collect();
    assert_eq!(tabs, ["nexus_train", "nexus_rollout"]);
    for (id, action) in [
        ("nexus_train", ids!(train) as &[LiveId]),
        ("nexus_rollout", ids!(run)),
    ] {
        let panel = cx.with_vm(|vm| registry.create_instance(id, vm)).unwrap();
        assert!(!panel.root().button(&mut cx, action).is_empty());
    }
}

/// The panel mounted and drawn as the host does it, headless (no window): the
/// root holding every mounted panel, the panel selected, a pass to draw into.
/// Input goes through `Cx::dispatch_studio_msg`, the platform's own path for
/// a hosted app's mouse and keyboard: the finger bookkeeping before and after
/// each event, the event to the app (here: the root), then the key focus
/// asked for while handling it.
struct Drawn {
    cx: Cx,
    registry: PluginRegistry,
    pages: WidgetRef,
    target: ui_checks::UiCheckTarget,
    time: f64,
}

const DRAWN_SIZE: Vec2d = Vec2d { x: 1200.0, y: 900.0 };

impl Drawn {
    fn new(panel: &str) -> Drawn {
        let slot: std::rc::Rc<std::cell::RefCell<Option<WidgetRef>>> = Default::default();
        let app = slot.clone();
        let mut cx = Cx::new(Box::new(move |cx, event| {
            let root = app.borrow().clone();
            if let Some(root) = root {
                root.handle_event(cx, event, &mut Scope::empty());
            }
        }));
        cx.init_cx_os();
        let mut registry = dorobot_flex::plugins::builtin_plugins();
        cx.with_vm(|vm| {
            dorobot_studio_core::register_ui(vm);
            registry.register_widgets(vm);
            assert!(vm.take_errors().is_empty());
        });
        let pages = cx.with_vm(|vm| {
            let value = script_eval!(vm, {
                use mod.prelude.widgets.* use mod.widgets.* View{width: Fill height: Fill}
            });
            WidgetRef::script_from_value(vm, value)
        });
        registry.mount(&mut cx, &pages).unwrap();
        registry.select(panel).unwrap();
        *slot.borrow_mut() = Some(pages.clone());
        let target = ui_checks::UiCheckTarget::new(&mut cx);
        let mut drawn = Drawn { cx, registry, pages, target, time: 1.0 };
        drawn.frame();
        drawn
    }

    /// One host frame: the visible panel's sync, then a draw.
    fn frame(&mut self) {
        let backend = MockBackend::new();
        let context = PanelContext {
            backend: &backend,
            rig_editing: false,
            show_help: false,
            show_details: false,
            live_controls: false,
        };
        self.registry.sync(&mut self.cx, &context);
        self.target.draw(&mut self.cx, &self.pages, DRAWN_SIZE);
        self.target.draw(&mut self.cx, &self.pages, DRAWN_SIZE);
    }

    fn send(&mut self, msg: studio::StudioToApp) {
        self.cx.dispatch_studio_msg(msg, CxWindowPool::id_zero(), Vec2d::default());
    }

    /// A left click in the middle of `path` (from the panel's root).
    fn click(&mut self, panel: &[LiveId], path: &[LiveId]) {
        let root = self.pages.widget(&self.cx, panel);
        let rect = root.widget(&self.cx, path).area().rect(&self.cx);
        assert!(rect.size.x > 0.0 && rect.size.y > 0.0, "{path:?} not drawn: {rect:?}");
        let at = rect.pos + rect.size / 2.0;
        // Well apart, so no click counts as a double click.
        self.time += 2.0;
        let button_raw_bits = MouseButton::PRIMARY.bits();
        self.send(studio::StudioToApp::MouseDown(studio::RemoteMouseDown {
            button_raw_bits,
            x: at.x,
            y: at.y,
            time: self.time,
            modifiers: Default::default(),
        }));
        self.send(studio::StudioToApp::MouseUp(studio::RemoteMouseUp {
            time: self.time + 0.05,
            button_raw_bits,
            x: at.x,
            y: at.y,
            modifiers: Default::default(),
        }));
    }

    fn type_text(&mut self, text: &str) {
        self.send(studio::StudioToApp::TextInput(TextInputEvent {
            input: text.into(),
            ..Default::default()
        }));
    }

    /// What the host does on `Event::LiveEdit` / `ScriptReapply` (a theme
    /// switch, or the appearance sync at startup): a new reload generation,
    /// every mounted panel's source replayed, the panels mounted again.
    fn style_reload(&mut self) {
        self.cx.reload_generation += 1;
        self.registry.reapply(&mut self.cx);
        self.registry.mount(&mut self.cx, &self.pages).unwrap();
        self.frame();
    }
}

/// The user's desktop check: a click on a connection field, then typing,
/// must change its text, with the host's syncs and draws in between, in both
/// themes, before and after a style reload.
#[test]
fn dviz_connection_fields_take_typed_text() {
    for (light, reload) in [(0.0, false), (1.0, false), (1.0, true), (0.0, true)] {
        dorobot_flex::ui::frame::set_light_mode(light);
        let mut host = Drawn::new("dviz_view");
        if reload {
            host.style_reload();
        }
        let panel = ids!(dviz_view);
        for (field, typed) in [
            (ids!(endpoint), "tcp/192.168.1.160:7450"),
            (ids!(mode), "x"),
            (ids!(prefix), "y"),
        ] {
            host.click(panel, field);
            host.frame();
            host.type_text(typed);
            host.frame();
            let root = host.pages.widget(&host.cx, panel);
            let text = root.text_input(&mut host.cx, field).text();
            assert!(text.contains(typed), "{field:?} at light {light}, reload {reload}: {text:?}");
        }
        host.cx.with_vm(|vm| {
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "{errors:?}");
        });
    }
}
