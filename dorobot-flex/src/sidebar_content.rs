//! Left sidebar content with dataset info and episode list
//!
//! Contains:
//! - Dataset name and info
//! - Load Dataset button
//! - Episode list (scrollable)

use makepad_widgets::*;
use makepad_app_shell::theme::get_global_dark_mode;
use crate::app_data::AppData;
use crate::widgets::episode_list::EpisodeListWidgetExt;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.SidebarContentBase = #(SidebarContent::register_widget(vm))
    mod.widgets.SidebarContent = set_type_default() do mod.widgets.SidebarContentBase{
        width: Fill
        height: Fill
        flow: Down

        show_bg: true
        draw_bg +: {
            dark_mode: instance(0.0)
            pixel: fn() {
                let light_bg = vec4(0.984, 0.980, 0.973, 1.0)
                let dark_bg = vec4(0.078, 0.075, 0.071, 1.0)
                return mix(light_bg, dark_bg, self.dark_mode)
            }
        }

        // Dataset section
        dataset_section := View{
            width: Fill
            height: Fit
            padding: 12
            flow: Down
            spacing: 8

            dataset_label := Label{
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SMALL{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.478, 0.443, 0.412, 1.0)
                        let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: "DATASET"
            }

            dataset_name := Label{
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SUBTITLE{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                        let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: "No dataset loaded"
            }

            dataset_info := Label{
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SMALL{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.420, 0.384, 0.357, 1.0)
                        let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: ""
            }

            load_btn := Button{
                width: Fill
                height: 36
                text: "Load Dataset"

                draw_bg +: {
                    color: #xD15010
                    pixel: fn() {
                        let base = self.color
                        let hover_color = vec4(0.820, 0.314, 0.063, 1.0)
                        let pressed_color = vec4(0.937, 0.435, 0.180, 1.0)
                        let color = mix(base, hover_color, self.hover)
                        return mix(color, pressed_color, self.down)
                    }
                }
                draw_text +: {
                    color: #xFFFFFF
                    text_style: mod.widgets.flex.TEXT_BODY{}
                }
            }

            // Error message display (hidden by default)
            error_view := View{
                width: Fill
                height: Fit
                visible: false
                padding: Inset{top: 8.}

                error_container := RoundedView{
                    width: Fill
                    height: Fit
                    padding: 8
                    draw_bg +: {
                        color: #xE8E5E1  // Light red background
                        border_radius: 0.0
                    }

                    error_label := Label{
                        width: Fill
                        draw_text +: {
                            text_style: mod.widgets.flex.TEXT_SMALL{}
                            color: #xC43B36  // Red text
                        }
                        text: ""
                    }
                }
            }
        }

        divider := View{
            width: Fill
            height: 1
            show_bg: true
            draw_bg +: {
                dark_mode: instance(0.0)
                pixel: fn() {
                    let light_div = vec4(0.910, 0.898, 0.882, 1.0)
                    let dark_div = vec4(0.165, 0.153, 0.145, 1.0)
                    return mix(light_div, dark_div, self.dark_mode)
                }
            }
        }

        // Episode section header
        episode_header := View{
            width: Fill
            height: 36
            padding: Inset{left: 12. right: 12.}
            align: Align{y: 0.5}

            episode_label := Label{
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SMALL{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.478, 0.443, 0.412, 1.0)
                        let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: "EPISODES"
            }
        }

        // Episode list (fills remaining space)
        episode_list := EpisodeList{}
    }
}

#[derive(Clone, Debug, Default)]
pub enum SidebarAction {
    LoadDataset,
    EpisodeSelected(u64),
    #[default]
    None,
}

#[derive(Script, ScriptHook, Widget)]
pub struct SidebarContent {
    #[deref]
    view: View,
}

impl Widget for SidebarContent {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Let events propagate to children
        self.view.handle_event(cx, event, scope);

        // The Load Dataset button is a stock Button; its Clicked widget action
        // is consumed in app.rs handle_actions (manual area hit-testing no
        // longer coexists with the button's own capture in makepad dev).
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Apply theme
        let dm = get_global_dark_mode();
        self.apply_theme(cx, dm);

        // Update from AppData
        if let Some(data) = scope.data.get::<AppData>() {
            if data.dataset.is_some() {
                self.view.label(cx, ids!(dataset_section.dataset_name))
                    .set_text(cx, &data.dataset_name);
                self.view.label(cx, ids!(dataset_section.dataset_info))
                    .set_text(cx, &data.dataset_info);
            } else {
                self.view.label(cx, ids!(dataset_section.dataset_name))
                    .set_text(cx, "No dataset loaded");
                self.view.label(cx, ids!(dataset_section.dataset_info))
                    .set_text(cx, "");
            }

            // Show/hide error message
            if let Some(ref error_msg) = data.error_message {
                self.view.view(cx, ids!(dataset_section.error_view)).set_visible(cx, true);
                self.view.label(cx, ids!(dataset_section.error_view.error_container.error_label))
                    .set_text(cx, error_msg);
            } else {
                self.view.view(cx, ids!(dataset_section.error_view)).set_visible(cx, false);
            }

            if !data.episodes.is_empty() {
                self.view.episode_list(cx, ids!(episode_list))
                    .set_episodes(cx, data.episodes.clone());

                if let Some(selected) = data.current_episode {
                    self.view.episode_list(cx, ids!(episode_list))
                        .set_selected(cx, Some(selected));
                }
            }
        }

        self.view.draw_walk(cx, scope, walk)
    }
}

impl SidebarContent {
    fn apply_theme(&mut self, cx: &mut Cx, dm: f64) {
        script_apply_eval!(cx, self.view, {
            draw_bg +: { dark_mode: #(dm) }
        });
        let mut divider = self.view.view(cx, ids!(divider));
        script_apply_eval!(cx, divider, {
            draw_bg +: { dark_mode: #(dm) }
        });
        // The episode list carries its own dorobot-ux pair but is not reached by
        // the push above — it is a widget, not a plain view, so it needs one of
        // its own or it stays light while everything around it goes dark.
        let mut episodes = self.view.episode_list(cx, ids!(episode_list));
        script_apply_eval!(cx, episodes, {
            draw_bg +: { dark_mode: #(dm) }
        });
        for path in [
            ids!(dataset_section.dataset_label),
            ids!(dataset_section.dataset_name),
            ids!(dataset_section.dataset_info),
            ids!(episode_header.episode_label),
        ] {
            let mut label = self.view.label(cx, path);
            script_apply_eval!(cx, label, {
                draw_text +: { dark_mode: #(dm) }
            });
        }
        // load_btn uses a fixed blue accent color, no dark_mode needed
    }
}
