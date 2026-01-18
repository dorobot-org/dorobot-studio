//! Episode List Widget for dataset navigation

use makepad_widgets::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;

    // Episode list item - a clickable view with episode info
    pub EpisodeListItem = {{EpisodeListItem}} {
        width: Fill
        height: 56
        padding: { left: 12, right: 12, top: 8, bottom: 8 }
        flow: Down
        spacing: 4

        // Must have visible background for hit detection
        show_bg: true
        draw_bg: {
            color: #1a1a26
        }

        // Top row with episode number, duration, frame count
        top_row = <View> {
            width: Fill
            height: Fit
            flow: Right
            spacing: 8

            episode_label = <Label> {
                width: Fit
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: #ffffff
                }
                text: "Episode 0"
            }

            duration_label = <Label> {
                width: Fit
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: #888888
                }
                text: "0:00"
            }

            frame_label = <Label> {
                width: Fit
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: #888888
                }
                text: "(0 frames)"
            }
        }

        // Task description
        task_label = <Label> {
            width: Fill
            draw_text: {
                text_style: <TEXT_SMALL> {}
                color: #666666
            }
            text: "Task description"
        }
    }

    // Main episode list widget
    pub EpisodeList = {{EpisodeList}} {
        width: Fill
        height: Fill

        flow: Down

        // Search and filter header
        header = <View> {
            width: Fill
            height: Fit
            padding: 8
            spacing: 8
            flow: Down

            show_bg: true
            draw_bg: { color: (COLOR_BG_HEADER) }

            // Search input
            search_input = <TextInput> {
                width: Fill
                height: 32
                padding: { left: 8, right: 8 }

                text: ""

                draw_bg: {
                    color: (COLOR_BG_INPUT)
                }

                draw_text: {
                    text_style: <TEXT_BODY> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
            }

            // Filter row
            filter_row = <View> {
                width: Fill
                height: Fit
                flow: Right
                spacing: 8
                align: { y: 0.5 }

                filter_label = <Label> {
                    draw_text: {
                        text_style: <TEXT_SMALL> {}
                        color: (COLOR_TEXT_SECONDARY)
                    }
                    text: "Filter:"
                }

                // Task filter dropdown placeholder
                task_filter = <Button> {
                    width: Fit
                    height: 24
                    padding: { left: 8, right: 8 }
                    text: "All Tasks"

                    draw_bg: {
                        color: (COLOR_BG_INPUT)
                        border_radius: 4.0
                    }

                    draw_text: {
                        text_style: <TEXT_SMALL> {}
                        color: (COLOR_TEXT_PRIMARY)
                    }
                }

                <View> { width: Fill }

                // Episode count
                episode_count = <Label> {
                    draw_text: {
                        text_style: <TEXT_SMALL> {}
                        color: (COLOR_TEXT_MUTED)
                    }
                    text: "0 episodes"
                }
            }
        }

        // Scrollable episode list using PortalList for virtual scrolling
        list = <PortalList> {
            width: Fill
            height: Fill
            flow: Down

            EpisodeListItem = <EpisodeListItem> {}
        }
    }
}

#[derive(Clone, Debug)]
pub struct EpisodeInfo {
    pub index: u64,
    pub frame_count: u64,
    pub duration_secs: f64,
    pub task_description: String,
    pub task_index: u64,
}

#[derive(Clone, Debug, DefaultNone)]
pub enum EpisodeListAction {
    EpisodeSelected(u64),
    EpisodeDoubleClicked(u64),
    SearchChanged(String),
    FilterChanged(Option<u64>),  // task_index
    None,
}

#[derive(Live, LiveHook, Widget)]
pub struct EpisodeListItem {
    #[deref]
    view: View,

    #[rust]
    pub episode_index: u64,
    #[rust]
    is_selected: bool,
}

impl Widget for EpisodeListItem {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        // Check hits against the view's area
        match event.hits(cx, self.view.area()) {
            Hit::FingerDown(_fe) => {
                cx.action(EpisodeListAction::EpisodeSelected(self.episode_index));
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl EpisodeListItem {
    pub fn set_episode_index(&mut self, index: u64) {
        self.episode_index = index;
    }

    pub fn set_selected(&mut self, cx: &mut Cx, selected: bool) {
        self.is_selected = selected;
        self.view.redraw(cx);
    }
}

#[derive(Live, Widget)]
pub struct EpisodeList {
    #[deref]
    view: View,

    #[rust]
    episodes: Vec<EpisodeInfo>,
    #[rust]
    filtered_indices: Vec<usize>,
    #[rust]
    selected_index: Option<u64>,
    #[rust]
    search_query: String,
    #[rust]
    task_filter: Option<u64>,
    #[rust]
    initialized: bool,
}

impl LiveHook for EpisodeList {
    fn after_apply(&mut self, _cx: &mut Cx, _apply: &mut Apply, _index: usize, _nodes: &[LiveNode]) {
        // Initialize with demo data if empty
        if !self.initialized {
            self.initialized = true;
            // Add demo episodes for testing
            self.episodes = (0..20)
                .map(|i| EpisodeInfo {
                    index: i,
                    frame_count: 150 + (i % 100),
                    duration_secs: (150 + (i % 100)) as f64 / 30.0,
                    task_description: format!("Demo episode {} - test task", i),
                    task_index: i % 3,
                })
                .collect();
            self.filtered_indices = (0..self.episodes.len()).collect();
        }
    }
}

impl Widget for EpisodeList {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Let events propagate to child widgets (including PortalList items)
        // Actions from EpisodeListItem will bubble up to parent directly
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, self.filtered_indices.len());

                while let Some(item_id) = list.next_visible_item(cx) {
                    if item_id < self.filtered_indices.len() {
                        let episode_idx = self.filtered_indices[item_id];
                        let episode = &self.episodes[episode_idx];

                        // Get the item widget and configure it
                        let item = list.item(cx, item_id, live_id!(EpisodeListItem));

                        // Set episode index on the item widget for click handling
                        if let Some(mut item_inner) = item.borrow_mut::<EpisodeListItem>() {
                            item_inner.episode_index = episode.index;
                        }

                        // Set labels using correct paths from live_design
                        item.label(id!(top_row.episode_label))
                            .set_text(cx, &format!("Episode {}", episode.index));

                        let duration_str = format!("{}:{:02}",
                            (episode.duration_secs / 60.0) as u32,
                            (episode.duration_secs % 60.0) as u32);
                        item.label(id!(top_row.duration_label)).set_text(cx, &duration_str);

                        item.label(id!(top_row.frame_label))
                            .set_text(cx, &format!("({} frames)", episode.frame_count));

                        item.label(id!(task_label)).set_text(cx, &episode.task_description);

                        item.draw_all(cx, &mut Scope::empty());
                    }
                }
            }
        }
        DrawStep::done()
    }
}

impl EpisodeList {
    /// Set the list of episodes
    pub fn set_episodes(&mut self, cx: &mut Cx, episodes: Vec<EpisodeInfo>) {
        self.episodes = episodes;
        self.apply_filters();
        self.update_episode_count(cx);
        self.view.redraw(cx);
    }

    /// Set the selected episode
    pub fn set_selected(&mut self, cx: &mut Cx, episode_index: Option<u64>) {
        self.selected_index = episode_index;
        self.view.redraw(cx);
    }

    /// Set task filter
    pub fn set_task_filter(&mut self, cx: &mut Cx, task_index: Option<u64>) {
        self.task_filter = task_index;
        self.apply_filters();
        self.update_episode_count(cx);
        self.view.redraw(cx);
    }

    fn apply_filters(&mut self) {
        self.filtered_indices = self.episodes.iter()
            .enumerate()
            .filter(|(_, ep)| {
                // Apply task filter
                if let Some(task_idx) = self.task_filter {
                    if ep.task_index != task_idx {
                        return false;
                    }
                }

                // Apply search filter
                if !self.search_query.is_empty() {
                    let query = self.search_query.to_lowercase();
                    if !ep.task_description.to_lowercase().contains(&query) {
                        return false;
                    }
                }

                true
            })
            .map(|(i, _)| i)
            .collect();
    }

    fn update_episode_count(&mut self, cx: &mut Cx) {
        let text = format!("{} episodes", self.filtered_indices.len());
        self.view.label(id!(header.filter_row.episode_count)).set_text(cx, &text);
    }

    /// Get the currently filtered episodes
    pub fn filtered_episodes(&self) -> Vec<&EpisodeInfo> {
        self.filtered_indices.iter()
            .map(|&i| &self.episodes[i])
            .collect()
    }
}

impl EpisodeListRef {
    pub fn set_episodes(&self, cx: &mut Cx, episodes: Vec<EpisodeInfo>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_episodes(cx, episodes);
        }
    }

    pub fn set_selected(&self, cx: &mut Cx, episode_index: Option<u64>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_selected(cx, episode_index);
        }
    }
}
