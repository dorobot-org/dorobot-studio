//! Left sidebar content with dataset info and episode list
//!
//! Contains:
//! - Dataset name and info
//! - Load Dataset button
//! - Episode list (scrollable)

use makepad_widgets::*;
use crate::app_data::AppData;
use crate::widgets::episode_list::{EpisodeListAction, EpisodeListWidgetExt};
use crate::shared::styles::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;
    use crate::widgets::episode_list::EpisodeList;

    pub SidebarContent = {{SidebarContent}} {
        width: Fill
        height: Fill
        flow: Down

        show_bg: true
        draw_bg: { color: (COLOR_BG_SIDEBAR) }

        // Dataset section
        dataset_section = <View> {
            width: Fill
            height: Fit
            padding: 12
            flow: Down
            spacing: 8

            <Label> {
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_MUTED)
                }
                text: "DATASET"
            }

            dataset_name = <Label> {
                draw_text: {
                    text_style: <TEXT_SUBTITLE> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
                text: "No dataset loaded"
            }

            dataset_info = <Label> {
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_SECONDARY)
                }
                text: ""
            }

            load_btn = <Button> {
                width: Fill
                height: 36
                text: "Load Dataset"

                draw_bg: {
                    color: (COLOR_ACCENT)
                }
                draw_text: {
                    color: #ffffff
                    text_style: <TEXT_BODY> {}
                }
            }
        }

        <View> {
            width: Fill
            height: 1
            show_bg: true
            draw_bg: { color: (COLOR_DIVIDER) }
        }

        // Episode section header
        episode_header = <View> {
            width: Fill
            height: 36
            padding: { left: 12, right: 12 }
            align: { y: 0.5 }

            <Label> {
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_MUTED)
                }
                text: "EPISODES"
            }
        }

        // Episode list (fills remaining space)
        episode_list = <EpisodeList> {}
    }
}

#[derive(Clone, Debug, DefaultNone)]
pub enum SidebarAction {
    LoadDataset,
    EpisodeSelected(u64),
    None,
}

#[derive(Live, LiveHook, Widget)]
pub struct SidebarContent {
    #[deref]
    view: View,
}

impl Widget for SidebarContent {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Handle load button click
        if self.view.button(id!(dataset_section.load_btn)).clicked(&actions) {
            cx.widget_action(self.widget_uid(), &scope.path, SidebarAction::LoadDataset);
        }

        // Handle episode list actions
        for action in actions.iter() {
            if let Some(EpisodeListAction::EpisodeSelected(idx)) = action.downcast_ref() {
                cx.widget_action(self.widget_uid(), &scope.path, SidebarAction::EpisodeSelected(*idx));
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Update from AppData
        if let Some(data) = scope.data.get::<AppData>() {
            // Update dataset info
            if data.dataset.is_some() {
                self.view.label(id!(dataset_section.dataset_name))
                    .set_text(cx, &data.dataset_name);
                self.view.label(id!(dataset_section.dataset_info))
                    .set_text(cx, &data.dataset_info);
            } else {
                self.view.label(id!(dataset_section.dataset_name))
                    .set_text(cx, "No dataset loaded");
                self.view.label(id!(dataset_section.dataset_info))
                    .set_text(cx, "");
            }

            // Update episode list
            if !data.episodes.is_empty() {
                self.view.episode_list(id!(episode_list))
                    .set_episodes(cx, data.episodes.clone());

                // Highlight selected episode
                if let Some(selected) = data.current_episode {
                    self.view.episode_list(id!(episode_list))
                        .set_selected(cx, Some(selected));
                }
            }
        }

        self.view.draw_walk(cx, scope, walk)
    }
}

