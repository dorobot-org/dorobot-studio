//! Episode List Widget using FileTree for reliable click handling

use makepad_widgets::*;
use makepad_widgets::file_tree::{FileTree, FileTreeAction};
use std::collections::HashMap;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // Main episode list widget using FileTree
    mod.widgets.EpisodeListBase = #(EpisodeList::register_widget(vm))
    mod.widgets.EpisodeList = set_type_default() do mod.widgets.EpisodeListBase{
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

        // Search and filter header
        header := View{
            width: Fill
            height: Fit
            padding: 8
            spacing: 8
            flow: Down

            show_bg: true
            draw_bg +: {
                dark_mode: instance(0.0)
                pixel: fn() {
                    let light_bg = vec4(0.949, 0.941, 0.929, 1.0)
                    let dark_bg = vec4(0.106, 0.098, 0.090, 1.0)
                    return mix(light_bg, dark_bg, self.dark_mode)
                }
            }

            filter_row := View{
                width: Fill
                height: Fit
                flow: Right
                spacing: 8
                align: Align{y: 0.5}

                filter_label := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_SMALL{}
                        color: #x6B625B
                    }
                    text: "Episodes"
                }

                View{ width: Fill }

                episode_count := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_SMALL{}
                        color: #x7A7169
                    }
                    text: "0 episodes"
                }
            }
        }

        // FileTree for episode list
        file_tree: FileTree{
            width: Fill
            height: Fill

            node_height: 48.0

            scroll_bars: ScrollBars{
                show_scroll_x: false
                show_scroll_y: true
            }

            file_node: FileTreeNode{
                is_folder: false
                indent_width: 8.0

                draw_bg +: {
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.rect(0., 0., self.rect_size.x, self.rect_size.y)
                        sdf.fill(mix(
                            mix(#xFFFFFF, #xF2F0ED, self.hover),
                            #xE8E5E1,
                            self.active
                        ))
                        return sdf.result
                    }
                }

                draw_text +: {
                    text_style +: {font_size: 10.0}
                    get_color: fn() {
                        return mix(#x6B625B, #x6B625B, self.active)
                    }
                }

                draw_icon +: {
                    color: #x7A7169
                    color_active: #x7A7169
                }
            }

            folder_node: FileTreeNode{
                is_folder: true
                indent_width: 8.0

                draw_bg +: {
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.rect(0., 0., self.rect_size.x, self.rect_size.y)
                        sdf.fill(mix(
                            mix(#xFFFFFF, #xF2F0ED, self.hover),
                            #xE8E5E1,
                            self.active
                        ))
                        return sdf.result
                    }
                }

                draw_text +: {
                    text_style +: {font_size: 10.0}
                    get_color: fn() {
                        return mix(#x6B625B, #x2A2725, self.active)
                    }
                }

                draw_icon +: {
                    color: #xD15010
                    color_active: #xD15010
                }
            }

            filler +: {
                pixel: fn() {
                    return #xFFFFFF
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
    pub channel_names: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub enum EpisodeListAction {
    EpisodeSelected(u64),
    EpisodeDoubleClicked(u64),
    SearchChanged(String),
    FilterChanged(Option<u64>),
    #[default]
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

#[derive(Script, ScriptHook, Widget)]
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

        // === Data Sources section (shown ONCE, not per episode) ===
        if !self.data_sources.is_empty() {
            let data_sources_folder_id = LiveId(self.live_id_counter);
            self.live_id_counter += 1;

            let mut data_sources_children = Vec::new();

            // Add Videos folder if there are video sources
            if !video_sources.is_empty() {
                let videos_folder_id = LiveId(self.live_id_counter);
                self.live_id_counter += 1;

                let mut video_children = Vec::new();
                for source in &video_sources {
                    let video_id = LiveId(self.live_id_counter);
                    self.live_id_counter += 1;

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

                data_sources_children.push(FileEdge {
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

                    let shape_str = if source.shape.is_empty() {
                        "scalar".to_string()
                    } else {
                        source.shape.iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                            .join("x")
                    };

                    // Check if this data source has channel names (make it a folder)
                    if let Some(ref channel_names) = source.channel_names {
                        let mut channel_children = Vec::new();
                        for (idx, ch_name) in channel_names.iter().enumerate() {
                            let ch_id = LiveId(self.live_id_counter);
                            self.live_id_counter += 1;

                            self.file_nodes.insert(ch_id, FileNode {
                                name: format!("[{}] {}", idx, ch_name),
                                child_edges: None,
                                episode_index: None,
                            });

                            channel_children.push(FileEdge {
                                name: ch_name.clone(),
                                file_node_id: ch_id,
                            });
                        }

                        self.file_nodes.insert(data_id, FileNode {
                            name: format!("{} [{}]", source.name, shape_str),
                            child_edges: Some(channel_children),
                            episode_index: None,
                        });
                    } else {
                        self.file_nodes.insert(data_id, FileNode {
                            name: format!("{} [{}]", source.name, shape_str),
                            child_edges: None,
                            episode_index: None,
                        });
                    }

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

                data_sources_children.push(FileEdge {
                    name: "Data".to_string(),
                    file_node_id: data_folder_id,
                });
            }

            self.file_nodes.insert(data_sources_folder_id, FileNode {
                name: "Data Sources".to_string(),
                child_edges: Some(data_sources_children),
                episode_index: None,
            });

            root_edges.push(FileEdge {
                name: "Data Sources".to_string(),
                file_node_id: data_sources_folder_id,
            });
        }

        // === Episodes section (simple list, no data sources per episode) ===
        let episodes_folder_id = LiveId(self.live_id_counter);
        self.live_id_counter += 1;

        let mut episode_edges = Vec::new();

        for &idx in &self.filtered_indices {
            let episode = &self.episodes[idx];

            // Generate unique LiveId for this episode (as a file, not folder)
            let episode_id = LiveId(self.live_id_counter);
            self.live_id_counter += 1;

            // Map to episode index for selection
            self.live_id_to_episode.insert(episode_id, episode.index);

            // Format episode name
            let duration_str = format!("{}:{:02}",
                (episode.duration_secs / 60.0) as u32,
                (episode.duration_secs % 60.0) as u32);
            let episode_name = format!("Episode {} • {} • {} frames",
                episode.index, duration_str, episode.frame_count);

            // Episode as a simple file node (no children)
            self.file_nodes.insert(episode_id, FileNode {
                name: episode_name,
                child_edges: None,
                episode_index: Some(episode.index),
            });

            episode_edges.push(FileEdge {
                name: format!("Episode {}", episode.index),
                file_node_id: episode_id,
            });
        }

        self.file_nodes.insert(episodes_folder_id, FileNode {
            name: format!("Episodes ({})", episode_edges.len()),
            child_edges: Some(episode_edges),
            episode_index: None,
        });

        root_edges.push(FileEdge {
            name: "Episodes".to_string(),
            file_node_id: episodes_folder_id,
        });

        // Create root node
        let root_id = live_id!(episodes_root);
        self.file_nodes.insert(root_id, FileNode {
            name: "Dataset".to_string(),
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
        self.view.label(cx, ids!(header.filter_row.episode_count)).set_text(cx, &text);
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
