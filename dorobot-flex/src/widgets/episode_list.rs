//! Episode List Widget using FileTree for reliable click handling

use makepad_widgets::*;
use makepad_widgets::file_tree::{FileTree, FileTreeAction};
use std::collections::HashMap;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;

    // Main episode list widget using FileTree
    pub EpisodeList = {{EpisodeList}} {
        width: Fill
        height: Fill
        flow: Down

        show_bg: true
        draw_bg: {
            instance dark_mode: 0.0
            fn pixel(self) -> vec4 {
                let light_bg = vec4(0.973, 0.976, 0.988, 1.0);
                let dark_bg = vec4(0.082, 0.082, 0.094, 1.0);
                return mix(light_bg, dark_bg, self.dark_mode);
            }
        }

        // Search and filter header
        header = <View> {
            width: Fill
            height: Fit
            padding: 8
            spacing: 8
            flow: Down

            show_bg: true
            draw_bg: {
                instance dark_mode: 0.0
                fn pixel(self) -> vec4 {
                    let light_bg = vec4(0.94, 0.94, 0.96, 1.0);
                    let dark_bg = vec4(0.10, 0.10, 0.12, 1.0);
                    return mix(light_bg, dark_bg, self.dark_mode);
                }
            }

            filter_row = <View> {
                width: Fill
                height: Fit
                flow: Right
                spacing: 8
                align: { y: 0.5 }

                filter_label = <Label> {
                    draw_text: {
                        text_style: <TEXT_SMALL> {}
                        color: #595966
                    }
                    text: "Episodes"
                }

                <View> { width: Fill }

                episode_count = <Label> {
                    draw_text: {
                        text_style: <TEXT_SMALL> {}
                        color: #737380
                    }
                    text: "0 episodes"
                }
            }
        }

        // FileTree for episode list
        file_tree: <FileTree> {
            width: Fill
            height: Fill

            node_height: 48.0

            scroll_bars: <ScrollBars> {
                show_scroll_x: false
                show_scroll_y: true
            }

            file_node: <FileTreeNode> {
                is_folder: false
                indent_width: 8.0

                draw_bg: {
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.rect(0., 0., self.rect_size.x, self.rect_size.y);
                        sdf.fill(mix(
                            mix(#ffffff, #e8f4fd, self.hover),
                            #d0e8ff,
                            self.active
                        ));
                        return sdf.result;
                    }
                }

                draw_text: {
                    text_style: { font_size: 10.0 }
                    fn get_color(self) -> vec4 {
                        return mix(#555555, #333333, self.active);
                    }
                }

                draw_icon: {
                    fn get_color(self) -> vec4 {
                        return mix(#888888, #666666, self.hover);
                    }
                }
            }

            folder_node: <FileTreeNode> {
                is_folder: true
                indent_width: 8.0

                draw_bg: {
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.rect(0., 0., self.rect_size.x, self.rect_size.y);
                        sdf.fill(mix(
                            mix(#ffffff, #e8f4fd, self.hover),
                            #d0e8ff,
                            self.active
                        ));
                        return sdf.result;
                    }
                }

                draw_text: {
                    text_style: { font_size: 10.0 }
                    fn get_color(self) -> vec4 {
                        return mix(#333333, #222222, self.active);
                    }
                }

                draw_icon: {
                    fn get_color(self) -> vec4 {
                        return mix(#4a90d9, #3080c9, self.hover);
                    }
                }
            }

            filler: {
                fn pixel(self) -> vec4 {
                    return #ffffff;
                }
            }
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

/// Data source info for display in the tree
#[derive(Clone, Debug)]
pub struct DataSourceInfo {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<u64>,
    pub is_video: bool,
}

#[derive(Clone, Debug, DefaultNone)]
pub enum EpisodeListAction {
    EpisodeSelected(u64),
    EpisodeDoubleClicked(u64),
    SearchChanged(String),
    FilterChanged(Option<u64>),
    None,
}

// Internal tree node structure
#[derive(Debug)]
struct FileNode {
    name: String,
    child_edges: Option<Vec<FileEdge>>,
    episode_index: Option<u64>,
}

#[derive(Debug)]
struct FileEdge {
    name: String,
    file_node_id: LiveId,
}

#[derive(Live, LiveHook, Widget)]
pub struct EpisodeList {
    #[deref]
    view: View,

    #[live]
    #[wrap]
    pub file_tree: FileTree,

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
    #[rust]
    file_nodes: LiveIdMap<LiveId, FileNode>,
    #[rust]
    live_id_to_episode: HashMap<LiveId, u64>,
    #[rust]
    live_id_counter: u64,
    #[rust]
    data_sources: Vec<DataSourceInfo>,
}

impl Widget for EpisodeList {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Let view handle events (for header)
        self.view.handle_event(cx, event, scope);

        // Capture FileTree actions
        let actions = cx.capture_actions(|cx| {
            self.file_tree.handle_event(cx, event, scope);
        });

        // Process FileTree actions
        if let Some(item) = actions.find_widget_action(self.file_tree.widget_uid()) {
            let action: FileTreeAction = item.cast();
            match action {
                FileTreeAction::FileClicked(file_id) => {
                    // Map file_id to episode index (for leaf node clicks)
                    if let Some(&episode_idx) = self.live_id_to_episode.get(&file_id) {
                        log!("Episode file clicked: {}", episode_idx);
                        cx.action(EpisodeListAction::EpisodeSelected(episode_idx));
                    }
                }
                FileTreeAction::FolderClicked(file_id) => {
                    // Episode folders also select the episode
                    if let Some(&episode_idx) = self.live_id_to_episode.get(&file_id) {
                        log!("Episode folder clicked: {}", episode_idx);
                        cx.action(EpisodeListAction::EpisodeSelected(episode_idx));
                    }
                }
                _ => {}
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Draw header
        self.view.draw_walk(cx, scope, walk)?;

        // Build tree data if needed
        if !self.initialized && !self.episodes.is_empty() {
            self.build_file_tree_data();
            self.initialized = true;
        }

        // Draw FileTree
        while self.file_tree.draw_walk(cx, scope, walk).is_step() {
            Self::draw_file_node(
                cx,
                live_id!(episodes_root),
                &mut self.file_tree,
                &self.file_nodes,
            );
        }

        DrawStep::done()
    }
}

impl EpisodeList {
    fn draw_file_node(
        cx: &mut Cx2d,
        file_node_id: LiveId,
        file_tree: &mut FileTree,
        file_nodes: &LiveIdMap<LiveId, FileNode>,
    ) {
        if let Some(file_node) = file_nodes.get(&file_node_id) {
            match &file_node.child_edges {
                Some(child_edges) => {
                    if file_tree.begin_folder(cx, file_node_id, &file_node.name).is_ok() {
                        for child_edge in child_edges {
                            Self::draw_file_node(cx, child_edge.file_node_id, file_tree, file_nodes);
                        }
                        file_tree.end_folder();
                    }
                }
                None => {
                    file_tree.file(cx, file_node_id, &file_node.name);
                }
            }
        }
    }

    fn build_file_tree_data(&mut self) {
        self.file_nodes.clear();
        self.live_id_to_episode.clear();
        self.live_id_counter = 1000;

        let mut root_edges = Vec::new();

        // Separate data sources into videos and other data
        let video_sources: Vec<_> = self.data_sources.iter()
            .filter(|ds| ds.is_video)
            .collect();
        let data_sources: Vec<_> = self.data_sources.iter()
            .filter(|ds| !ds.is_video)
            .collect();

        for &idx in &self.filtered_indices {
            let episode = &self.episodes[idx];

            // Generate unique LiveId for this episode folder
            let episode_folder_id = LiveId(self.live_id_counter);
            self.live_id_counter += 1;

            // Map the episode folder to episode index (for folder clicks)
            self.live_id_to_episode.insert(episode_folder_id, episode.index);

            let mut episode_children = Vec::new();

            // Add Videos folder if there are video sources
            if !video_sources.is_empty() {
                let videos_folder_id = LiveId(self.live_id_counter);
                self.live_id_counter += 1;

                let mut video_children = Vec::new();
                for source in &video_sources {
                    let video_id = LiveId(self.live_id_counter);
                    self.live_id_counter += 1;

                    // Extract camera name (e.g., "cam_high" from "observation.images.cam_high")
                    let display_name = source.name.split('.').last().unwrap_or(&source.name);
                    let shape_str = source.shape.iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                        .join("x");

                    self.file_nodes.insert(video_id, FileNode {
                        name: format!("{} [{}]", display_name, shape_str),
                        child_edges: None,
                        episode_index: None,
                    });

                    video_children.push(FileEdge {
                        name: display_name.to_string(),
                        file_node_id: video_id,
                    });
                }

                self.file_nodes.insert(videos_folder_id, FileNode {
                    name: format!("Videos ({})", video_sources.len()),
                    child_edges: Some(video_children),
                    episode_index: None,
                });

                episode_children.push(FileEdge {
                    name: "Videos".to_string(),
                    file_node_id: videos_folder_id,
                });
            }

            // Add Data folder if there are data sources
            if !data_sources.is_empty() {
                let data_folder_id = LiveId(self.live_id_counter);
                self.live_id_counter += 1;

                let mut data_children = Vec::new();
                for source in &data_sources {
                    let data_id = LiveId(self.live_id_counter);
                    self.live_id_counter += 1;

                    // Format shape info
                    let shape_str = if source.shape.is_empty() {
                        "scalar".to_string()
                    } else {
                        source.shape.iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                            .join("x")
                    };

                    self.file_nodes.insert(data_id, FileNode {
                        name: format!("{} [{}]", source.name, shape_str),
                        child_edges: None,
                        episode_index: None,
                    });

                    data_children.push(FileEdge {
                        name: source.name.clone(),
                        file_node_id: data_id,
                    });
                }

                self.file_nodes.insert(data_folder_id, FileNode {
                    name: format!("Data ({})", data_sources.len()),
                    child_edges: Some(data_children),
                    episode_index: None,
                });

                episode_children.push(FileEdge {
                    name: "Data".to_string(),
                    file_node_id: data_folder_id,
                });
            }

            // Format episode folder name
            let duration_str = format!("{}:{:02}",
                (episode.duration_secs / 60.0) as u32,
                (episode.duration_secs % 60.0) as u32);
            let episode_name = format!("Episode {} • {} • {} frames",
                episode.index, duration_str, episode.frame_count);

            self.file_nodes.insert(episode_folder_id, FileNode {
                name: episode_name,
                child_edges: Some(episode_children),
                episode_index: Some(episode.index),
            });

            root_edges.push(FileEdge {
                name: format!("Episode {}", episode.index),
                file_node_id: episode_folder_id,
            });
        }

        // Create root node (always expanded)
        let root_id = live_id!(episodes_root);
        self.file_nodes.insert(root_id, FileNode {
            name: format!("Episodes ({})", root_edges.len()),
            child_edges: Some(root_edges),
            episode_index: None,
        });
    }

    pub fn set_episodes(&mut self, cx: &mut Cx, episodes: Vec<EpisodeInfo>) {
        self.episodes = episodes;
        self.apply_filters();
        self.update_episode_count(cx);
        self.initialized = false;
        self.file_tree.redraw(cx);
    }

    pub fn set_data_sources(&mut self, cx: &mut Cx, data_sources: Vec<DataSourceInfo>) {
        self.data_sources = data_sources;
        self.initialized = false;
        self.file_tree.redraw(cx);
    }

    pub fn set_selected(&mut self, cx: &mut Cx, episode_index: Option<u64>) {
        self.selected_index = episode_index;
        self.file_tree.redraw(cx);
    }

    pub fn set_task_filter(&mut self, cx: &mut Cx, task_index: Option<u64>) {
        self.task_filter = task_index;
        self.apply_filters();
        self.update_episode_count(cx);
        self.initialized = false;
        self.file_tree.redraw(cx);
    }

    fn apply_filters(&mut self) {
        self.filtered_indices = self.episodes.iter()
            .enumerate()
            .filter(|(_, ep)| {
                if let Some(task_idx) = self.task_filter {
                    if ep.task_index != task_idx {
                        return false;
                    }
                }
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

    pub fn set_data_sources(&self, cx: &mut Cx, data_sources: Vec<DataSourceInfo>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_data_sources(cx, data_sources);
        }
    }
}
