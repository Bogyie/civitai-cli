use crate::api::ImageItem;
use civitai_cli::sdk::{
    ModelBaseModel, ModelSearchSortBy, ModelSearchState, ModelType, SearchModelHit as Model,
};
use crate::tui::model::{model_name, model_versions, preview_image_url, selected_version};
use ratatui::widgets::ListState;
use ratatui_image::protocol::StatefulProtocol;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum MainTab {
    Models,
    Bookmarks,
    Images,
    ImageBookmarks,
    Downloads,
    Settings,
}

#[derive(PartialEq, Clone, Copy)]
pub enum AppMode {
    Browsing,
    SearchForm,
    SearchImages,
    SearchBookmarks,
    SearchImageBookmarks,
    BookmarkPathPrompt,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BookmarkPathAction {
    Export,
    Import,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchFormMode {
    Quick,
    Builder,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchFormSection {
    Query,
    Sort,
    Type,
    BaseModel,
    Period,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchPeriod {
    AllTime,
    Year,
    Month,
    Week,
    Day,
}

impl SearchPeriod {
    pub fn all() -> Vec<Self> {
        vec![Self::AllTime, Self::Year, Self::Month, Self::Week, Self::Day]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::AllTime => "AllTime",
            Self::Year => "Year",
            Self::Month => "Month",
            Self::Week => "Week",
            Self::Day => "Day",
        }
    }
}

pub struct SearchFormState {
    pub query: String,
    pub mode: SearchFormMode,
    pub focused_section: SearchFormSection,
    pub sort_options: Vec<ModelSearchSortBy>,
    pub selected_sort: usize,
    pub type_options: Vec<ModelType>,
    pub type_cursor: usize,
    pub selected_types: BTreeSet<ModelType>,
    pub base_options: Vec<ModelBaseModel>,
    pub base_cursor: usize,
    pub selected_base_models: BTreeSet<ModelBaseModel>,
    pub periods: Vec<SearchPeriod>,
    pub selected_period: usize,
}

pub struct SettingsFormState {
    pub editing: bool,
    pub focused_field: usize,
    pub input_buffer: String,
}

pub struct ImageSearchFormState {
    pub focused_field: usize, // 0: NSFW, 1: Sort, 2: Period, 3: ModelVersionId, 4: Tag
    pub selected_nsfw: usize,
    pub nsfw_options: Vec<String>,
    pub selected_sort: usize,
    pub sort_options: Vec<String>,
    pub selected_period: usize,
    pub period_options: Vec<String>,
    pub model_version_id: String,
    pub tag_text: String,
}

impl SettingsFormState {
    pub fn new() -> Self {
        Self {
            editing: false,
            focused_field: 0,
            input_buffer: String::new(),
        }
    }
}

impl ImageSearchFormState {
    pub fn new() -> Self {
        Self {
            focused_field: 0,
            selected_nsfw: 0,
            nsfw_options: vec![
                "All".into(),
                "None".into(),
                "Soft".into(),
                "Mature".into(),
                "X".into(),
            ],
            selected_sort: 0,
            sort_options: vec![
                "Most Collected".into(),
                "Most Reactions".into(),
                "Most Comments".into(),
                "Newest".into(),
                "Oldest".into(),
            ],
            selected_period: 0,
            period_options: vec![
                "AllTime".into(),
                "Year".into(),
                "Month".into(),
                "Week".into(),
                "Day".into(),
            ],
            model_version_id: String::new(),
            tag_text: String::new(),
        }
    }

    pub fn build_options(&self) -> crate::api::client::ImageSearchOptions {
        crate::api::client::ImageSearchOptions {
            limit: 10,
            nsfw: Some(self.nsfw_options[self.selected_nsfw].clone()),
            sort: Some(self.sort_options[self.selected_sort].clone()),
            period: Some(self.period_options[self.selected_period].clone()),
            model_version_id: self.model_version_id.trim().parse::<u64>().ok(),
            tags: image_tag_to_id(self.tag_text.trim()),
        }
    }
}

impl SearchFormState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            mode: SearchFormMode::Quick,
            focused_section: SearchFormSection::Query,
            sort_options: ModelSearchSortBy::all(),
            selected_sort: ModelSearchSortBy::all()
                .iter()
                .position(|sort| *sort == ModelSearchSortBy::Relevance)
                .unwrap_or(0),
            type_options: ModelType::all(),
            type_cursor: 0,
            selected_types: BTreeSet::new(),
            base_options: ModelBaseModel::all(),
            base_cursor: 0,
            selected_base_models: BTreeSet::new(),
            periods: SearchPeriod::all(),
            selected_period: 0,
        }
    }

    pub fn build_options(&self) -> ModelSearchState {
        ModelSearchState {
            query: (!self.query.trim().is_empty()).then(|| self.query.trim().to_string()),
            sort_by: self
                .sort_options
                .get(self.selected_sort)
                .cloned()
                .unwrap_or_default(),
            base_models: self.selected_base_models.iter().cloned().collect(),
            types: self.selected_types.iter().cloned().collect(),
            created_at: self
                .periods
                .get(self.selected_period)
                .map(|period| period_to_created_at(period.label()))
                .unwrap_or(None),
            limit: Some(50),
            ..Default::default()
        }
    }

    pub fn begin_quick_search(&mut self) {
        self.mode = SearchFormMode::Quick;
        self.focused_section = SearchFormSection::Query;
    }

    pub fn begin_builder(&mut self) {
        self.mode = SearchFormMode::Builder;
        if self.focused_section == SearchFormSection::Query {
            self.focused_section = SearchFormSection::Sort;
        }
    }
}

fn image_tag_to_id(value: &str) -> Option<u64> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => None,
        "animal" => Some(111768),
        "architecture" => Some(414),
        "armor" => Some(5169),
        "astronomy" => Some(111767),
        "car" => Some(111805),
        "cartoon" => Some(5186),
        "cat" => Some(5132),
        "celebrity" => Some(5188),
        "city" => Some(55),
        "clothing" => Some(5193),
        "comics" => Some(2397),
        "costume" => Some(2435),
        "dog" => Some(2539),
        "dragon" => Some(5499),
        "fantasy" => Some(5207),
        "food" => Some(3915),
        "game character" => Some(5211),
        "landscape" => Some(8363),
        "latex clothing" => Some(111935),
        "man" => Some(5232),
        "modern art" => Some(617),
        "outdoors" => Some(111763),
        "photography" => Some(5241),
        "photorealistic" => Some(172),
        "post apocalyptic" => Some(213),
        "realistic" => Some(5248),
        "robot" => Some(6594),
        "sci-fi" => Some(3060),
        "sports car" => Some(111833),
        "swimwear" => Some(111943),
        "transportation" => Some(111757),
        "nude" => Some(304),
        "woman" => Some(5133),
        _ => normalized.parse::<u64>().ok(),
    }
}

pub enum AppMessage {
    ImagesLoaded(Vec<ImageItem>, bool, Option<String>),
    ImageDecoded(u64, StatefulProtocol),
    ModelsSearchedChunk(Vec<Model>, bool, bool, Option<u32>),
    ModelCoverDecoded(u64, StatefulProtocol), // version_id, protocol
    ModelCoversDecoded(u64, Vec<StatefulProtocol>), // version_id, protocols
    ModelCoverLoadFailed(u64),
    StatusUpdate(String),
    DownloadStarted(u64, String, u64, String, u64, Option<PathBuf>), // model_id, filename, version_id, model_name, total_bytes, file_path
    DownloadProgress(u64, String, f64, u64, u64), // model_id, filename, percentage, downloaded_bytes, total_bytes
    DownloadPaused(u64),
    DownloadResumed(u64),
    DownloadCompleted(u64),      // model_id
    DownloadFailed(u64, String), // model_id, reason
    DownloadCancelled(u64),      // model_id
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    Running,
    Paused,
}

pub struct DownloadTracker {
    pub filename: String,
    pub progress: f64, // 0.0 to 100.0
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub file_path: Option<PathBuf>,
    pub model_name: String,
    pub version_id: u64,
    pub state: DownloadState,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum DownloadHistoryStatus {
    Completed,
    Failed(String),
    Paused,
    Cancelled,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DownloadHistoryEntry {
    pub model_id: u64,
    pub version_id: u64,
    pub filename: String,
    pub model_name: String,
    pub file_path: Option<PathBuf>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub status: DownloadHistoryStatus,
    pub progress: f64,
    pub created_at: std::time::SystemTime,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct InterruptedDownloadSession {
    pub model_id: u64,
    pub version_id: u64,
    pub filename: String,
    pub model_name: String,
    pub file_path: Option<PathBuf>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub created_at: std::time::SystemTime,
}

pub enum WorkerCommand {
    FetchImages(crate::api::client::ImageSearchOptions, Option<String>),
    SearchModels(
        ModelSearchState,
        Option<u64>,
        Option<u64>,
        bool,
        bool,
        Option<u32>,
    ),
    ClearSearchCache,
    PrioritizeModelCover(u64, Option<String>),
    PrefetchModelCovers(Vec<(u64, Option<String>)>),
    DownloadModelForImage(u64),
    DownloadModel(Model, u64, usize), // selected model hit, version_id, file_index
    PauseDownload(u64),      // model_id
    ResumeDownload(u64),     // model_id
    CancelDownload(u64),     // model_id
    ResumeDownloadModel(u64, u64, Option<PathBuf>, u64, u64), // model_id, version_id, file_path, downloaded_bytes, total_bytes
    Quit,
    UpdateConfig(crate::config::AppConfig),
}

pub struct App {
    pub active_tab: MainTab,
    pub mode: AppMode,
    pub config: crate::config::AppConfig,
    pub search_form: SearchFormState,
    pub image_search_form: ImageSearchFormState,
    pub settings_form: SettingsFormState,

    pub models: Vec<Model>,
    pub model_search_has_more: bool,
    pub model_search_loading_more: bool,
    pub model_search_next_page: Option<u32>,
    pub bookmarks: Vec<Model>,
    pub image_bookmarks: Vec<ImageItem>,
    pub show_model_details: bool,
    pub model_list_state: ListState,
    pub bookmark_list_state: ListState,
    pub image_bookmark_list_state: ListState,
    pub bookmark_query: String,
    pub bookmark_query_draft: String,
    pub image_bookmark_query: String,
    pub image_bookmark_query_draft: String,
    pub show_bookmark_confirm_modal: bool,
    pub pending_bookmark_remove_id: Option<u64>,
    pub bookmark_file_path: Option<PathBuf>,
    pub image_bookmark_file_path: Option<PathBuf>,
    pub download_history_file_path: Option<PathBuf>,
    pub interrupted_download_file_path: Option<PathBuf>,
    pub interrupted_download_sessions: Vec<InterruptedDownloadSession>,
    pub selected_version_index: HashMap<u64, usize>,
    pub selected_file_index: HashMap<u64, usize>,
    pub model_version_image_cache: HashMap<u64, Vec<StatefulProtocol>>,
    pub model_version_image_failed: HashSet<u64>,

    pub images: Vec<ImageItem>,
    pub selected_index: usize,
    pub selected_image_bookmark_index: usize,
    pub image_cache: HashMap<u64, StatefulProtocol>,
    pub image_feed_loaded: bool,
    pub image_feed_loading: bool,
    pub image_feed_next_page: Option<String>,
    pub image_feed_has_more: bool,

    pub active_downloads: HashMap<u64, DownloadTracker>,
    pub active_download_order: Vec<u64>,
    pub selected_download_index: usize,
    pub selected_history_index: usize,
    pub download_history: Vec<DownloadHistoryEntry>,

    pub status: String,
    pub last_error: Option<String>,
    pub show_status_modal: bool,
    pub show_exit_confirm_modal: bool,
    pub show_resume_download_modal: bool,
    pub bookmark_path_prompt_action: Option<BookmarkPathAction>,
    pub bookmark_path_draft: String,
    pub tx: Option<mpsc::Sender<WorkerCommand>>,
}

impl App {
    pub fn new(config: crate::config::AppConfig) -> Self {
        let bookmark_file_path = config
            .bookmark_file_path
            .clone()
            .or_else(crate::config::AppConfig::bookmark_path);
        let bookmarks = load_bookmarks(bookmark_file_path.as_deref());
        let image_bookmark_file_path = config
            .image_bookmark_file_path
            .clone()
            .or_else(crate::config::AppConfig::image_bookmark_path);
        let image_bookmarks = load_image_bookmarks(image_bookmark_file_path.as_deref());
        let download_history_file_path = config
            .download_history_file_path
            .clone()
            .or_else(|| config.download_history_path());
        let download_history = load_download_history(download_history_file_path.as_deref());
        let interrupted_from_history =
            collect_paused_sessions_from_history(download_history.as_slice());
        let interrupted_download_file_path = config
            .interrupted_download_file_path
            .clone()
            .or_else(|| config.interrupted_download_path());
        let mut interrupted_download_sessions =
            load_interrupted_downloads(interrupted_download_file_path.as_deref());
        let mut seen = HashSet::new();
        for session in interrupted_download_sessions.iter() {
            seen.insert((session.model_id, session.version_id));
        }
        for session in interrupted_from_history {
            let key = (session.model_id, session.version_id);
            if !seen.contains(&key) {
                seen.insert(key);
                interrupted_download_sessions.push(session);
            }
        }
        let interrupted_sessions_for_state = interrupted_download_sessions.clone();
        let show_resume_download_modal = !interrupted_download_sessions.is_empty();
        let mut bookmark_list_state = ListState::default();
        if !bookmarks.is_empty() {
            bookmark_list_state.select(Some(0));
        }
        let mut image_bookmark_list_state = ListState::default();
        if !image_bookmarks.is_empty() {
            image_bookmark_list_state.select(Some(0));
        }
        let mut model_list_state = ListState::default();
        model_list_state.select(Some(0));

        let mut app = Self {
            active_tab: MainTab::Models,
            mode: AppMode::Browsing,
            config,
            search_form: SearchFormState::new(),
            image_search_form: ImageSearchFormState::new(),
            settings_form: SettingsFormState::new(),
            models: Vec::new(),
            model_search_has_more: true,
            model_search_loading_more: false,
            model_search_next_page: None,
            bookmarks,
            image_bookmarks,
            show_model_details: false,
            model_list_state,
            bookmark_list_state,
            image_bookmark_list_state,
            bookmark_query: String::new(),
            bookmark_query_draft: String::new(),
            image_bookmark_query: String::new(),
            image_bookmark_query_draft: String::new(),
            show_bookmark_confirm_modal: false,
            pending_bookmark_remove_id: None,
            bookmark_file_path,
            image_bookmark_file_path,
            download_history_file_path,
            selected_version_index: HashMap::new(),
            selected_file_index: HashMap::new(),
            model_version_image_cache: HashMap::new(),
            model_version_image_failed: HashSet::new(),
            images: Vec::new(),
            selected_index: 0,
            selected_image_bookmark_index: 0,
            image_cache: HashMap::new(),
            image_feed_loaded: false,
            image_feed_loading: false,
            image_feed_next_page: None,
            image_feed_has_more: true,
            active_downloads: HashMap::new(),
            active_download_order: Vec::new(),
            selected_download_index: 0,
            selected_history_index: 0,
            download_history,
            interrupted_download_file_path,
            interrupted_download_sessions: interrupted_sessions_for_state,
            show_resume_download_modal,
            status: "Initializing App...".to_string(),
            last_error: None,
            show_status_modal: false,
            show_exit_confirm_modal: false,
            bookmark_path_prompt_action: None,
            bookmark_path_draft: String::new(),
            tx: None,
        };

        if !interrupted_download_sessions.is_empty() {
            for session in interrupted_download_sessions.iter() {
                let existing_paused = app.download_history.iter().any(|entry| {
                    entry.model_id == session.model_id
                        && entry.version_id == session.version_id
                        && matches!(entry.status, DownloadHistoryStatus::Paused)
                });
                if !existing_paused {
                    app.record_interrupted_session_to_history(session);
                }
            }
        }

        app
    }

    pub fn set_worker_tx(&mut self, tx: mpsc::Sender<WorkerCommand>) {
        self.tx = Some(tx.clone());
    }

    pub fn can_request_more_models(&self) -> bool {
        self.active_tab == MainTab::Models
            && !self.models.is_empty()
            && self.model_search_has_more
            && !self.model_search_loading_more
    }

    pub fn can_request_more_images(&self, threshold: usize) -> bool {
        self.active_tab == MainTab::Images
            && !self.images.is_empty()
            && self.image_feed_has_more
            && !self.image_feed_loading
            && !self.images.is_empty()
            && self.selected_index + threshold >= self.images.len()
    }

    pub fn next_image_feed_page(&self) -> Option<String> {
        self.image_feed_next_page.clone()
    }

    pub fn visible_image_bookmarks(&self) -> Vec<ImageItem> {
        let query = self.image_bookmark_query.trim().to_ascii_lowercase();
        if query.is_empty() {
            self.image_bookmarks.clone()
        } else {
            self.image_bookmarks
                .iter()
                .filter(|image| {
                    let username = image
                        .username
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    let base_model = image
                        .base_model
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    image.id.to_string().contains(&query)
                        || username.contains(&query)
                        || base_model.contains(&query)
                        || image
                            .meta
                            .as_ref()
                            .map(|meta| meta.to_string().to_ascii_lowercase().contains(&query))
                            .unwrap_or(false)
                })
                .cloned()
                .collect()
        }
    }

    pub fn clamp_image_bookmark_selection(&mut self) {
        let visible = self.visible_image_bookmarks();
        if visible.is_empty() {
            self.selected_image_bookmark_index = 0;
            self.image_bookmark_list_state.select(None);
            return;
        }

        if self.selected_image_bookmark_index >= visible.len() {
            self.selected_image_bookmark_index = visible.len() - 1;
        }
        self.image_bookmark_list_state
            .select(Some(self.selected_image_bookmark_index));
    }

    pub fn selected_image_in_active_view(&self) -> Option<&ImageItem> {
        match self.active_tab {
            MainTab::Images => self.images.get(self.selected_index),
            MainTab::ImageBookmarks => {
                let visible = self.visible_image_bookmarks();
                let selected = self.selected_image_bookmark_index;
                let id = visible.get(selected)?.id;
                self.image_bookmarks.iter().find(|image| image.id == id)
            }
            _ => None,
        }
    }

    pub fn active_image_items(&self) -> Vec<ImageItem> {
        match self.active_tab {
            MainTab::Images => self.images.clone(),
            MainTab::ImageBookmarks => self.visible_image_bookmarks(),
            _ => Vec::new(),
        }
    }

    pub fn active_image_selected_index(&self) -> usize {
        match self.active_tab {
            MainTab::Images => self.selected_index,
            MainTab::ImageBookmarks => self.selected_image_bookmark_index,
            _ => 0,
        }
    }

    pub fn set_image_feed_results(
        &mut self,
        mut images: Vec<ImageItem>,
        next_page: Option<String>,
    ) {
        self.image_feed_next_page = next_page;
        self.image_feed_has_more = self.image_feed_next_page.is_some();
        self.image_feed_loaded = true;
        self.image_feed_loading = false;
        self.images = std::mem::take(&mut images);
        if self.images.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.images.len() {
            self.selected_index = self.images.len() - 1;
        }
    }

    pub fn append_image_feed_results(
        &mut self,
        mut images: Vec<ImageItem>,
        next_page: Option<String>,
    ) {
        if !self.images.is_empty() && !images.is_empty() {
            let known_ids: HashSet<u64> = self.images.iter().map(|item| item.id).collect();
            images.retain(|item| !known_ids.contains(&item.id));
        }

        self.images.append(&mut images);
        self.image_feed_next_page = next_page;
        self.image_feed_has_more = self.image_feed_next_page.is_some();
        self.image_feed_loading = false;
        if !self.images.is_empty() && self.selected_index >= self.images.len() {
            self.selected_index = self.images.len() - 1;
        }
    }

    pub fn next_model_search_options_if_needed(
        &mut self,
    ) -> Option<(ModelSearchState, Option<u32>)> {
        if !self.can_request_more_models() {
            return None;
        }

        self.model_search_loading_more = true;
        Some((
            self.search_form.build_options(),
            self.model_search_next_page.clone(),
        ))
    }

    pub fn select_next(&mut self) {
        if self.active_tab == MainTab::Images {
            if !self.images.is_empty() && self.selected_index < self.images.len() - 1 {
                self.selected_index += 1;
            }
        } else if self.active_tab == MainTab::ImageBookmarks {
            let visible = self.visible_image_bookmarks();
            if !visible.is_empty() && self.selected_image_bookmark_index < visible.len() - 1 {
                self.selected_image_bookmark_index += 1;
                self.image_bookmark_list_state
                    .select(Some(self.selected_image_bookmark_index));
            }
        } else if self.active_tab == MainTab::Models {
            if !self.models.is_empty() {
                let current = self.model_list_state.selected().unwrap_or(0);
                if current < self.models.len() - 1 {
                    self.model_list_state.select(Some(current + 1));
                }
            }
        } else if self.active_tab == MainTab::Bookmarks {
            let visible = self.visible_bookmarks();
            if let Some(current) = self.bookmark_list_state.selected() {
                if current < visible.len().saturating_sub(1) {
                    self.bookmark_list_state.select(Some(current + 1));
                }
            } else if !visible.is_empty() {
                self.bookmark_list_state.select(Some(0));
            }
        }
    }

    pub fn select_previous(&mut self) {
        if self.active_tab == MainTab::Images {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            }
        } else if self.active_tab == MainTab::ImageBookmarks {
            if self.selected_image_bookmark_index > 0 {
                self.selected_image_bookmark_index -= 1;
                self.image_bookmark_list_state
                    .select(Some(self.selected_image_bookmark_index));
            }
        } else if self.active_tab == MainTab::Models {
            let current = self.model_list_state.selected().unwrap_or(0);
            if current > 0 {
                self.model_list_state.select(Some(current - 1));
            }
        } else if self.active_tab == MainTab::Bookmarks {
            let current = self.bookmark_list_state.selected().unwrap_or(0);
            if current > 0 {
                self.bookmark_list_state.select(Some(current - 1));
            }
        }
    }

    pub fn move_list_selection_by(&mut self, delta: isize) {
        match self.active_tab {
            MainTab::Models => {
                if self.models.is_empty() {
                    self.model_list_state.select(None);
                    return;
                }
                let current = self.model_list_state.selected().unwrap_or(0) as isize;
                let max = self.models.len().saturating_sub(1) as isize;
                let next = (current + delta).clamp(0, max) as usize;
                self.model_list_state.select(Some(next));
            }
            MainTab::Bookmarks => {
                let visible = self.visible_bookmarks();
                if visible.is_empty() {
                    self.bookmark_list_state.select(None);
                    return;
                }
                let current = self.bookmark_list_state.selected().unwrap_or(0) as isize;
                let max = visible.len().saturating_sub(1) as isize;
                let next = (current + delta).clamp(0, max) as usize;
                self.bookmark_list_state.select(Some(next));
            }
            _ => {}
        }
    }

    pub fn select_list_first(&mut self) {
        match self.active_tab {
            MainTab::Models => {
                if self.models.is_empty() {
                    self.model_list_state.select(None);
                } else {
                    self.model_list_state.select(Some(0));
                }
            }
            MainTab::Bookmarks => {
                if self.visible_bookmarks().is_empty() {
                    self.bookmark_list_state.select(None);
                } else {
                    self.bookmark_list_state.select(Some(0));
                }
            }
            _ => {}
        }
    }

    pub fn select_list_last(&mut self) {
        match self.active_tab {
            MainTab::Models => {
                if self.models.is_empty() {
                    self.model_list_state.select(None);
                } else {
                    self.model_list_state
                        .select(Some(self.models.len().saturating_sub(1)));
                }
            }
            MainTab::Bookmarks => {
                let visible = self.visible_bookmarks();
                if visible.is_empty() {
                    self.bookmark_list_state.select(None);
                } else {
                    self.bookmark_list_state
                        .select(Some(visible.len().saturating_sub(1)));
                }
            }
            _ => {}
        }
    }

    pub fn append_models_results(
        &mut self,
        new_models: Vec<Model>,
        has_more: bool,
        next_page: Option<u32>,
    ) {
        if new_models.is_empty() {
            self.model_search_has_more = has_more;
            self.model_search_loading_more = false;
            self.model_search_next_page = next_page;
            return;
        }

        let mut seen_ids = self.models.iter().map(|model| model.id).collect::<HashSet<_>>();
        for model in new_models {
            if seen_ids.insert(model.id) {
                self.models.push(model);
            }
        }
        self.model_search_has_more = has_more;
        self.model_search_loading_more = false;
        self.model_search_next_page = next_page;
    }

    pub fn set_models_results(
        &mut self,
        models: Vec<Model>,
        has_more: bool,
        next_page: Option<u32>,
    ) {
        self.models = models;
        self.model_search_has_more = has_more;
        self.model_search_loading_more = false;
        self.model_search_next_page = next_page;
        self.model_list_state.select(Some(0));
    }

    pub fn select_next_version(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view().cloned() {
                let model_id = model.id;
                let version_len = model_versions(&model).len();
                let v_idx = self.selected_version_index.entry(model_id).or_insert(0);
                if *v_idx < version_len.saturating_sub(1) {
                    *v_idx += 1;
                    if let Some(version) = selected_version(&model, *v_idx) {
                        self.selected_file_index.entry(version.id).or_insert(0);
                    }
                }
            }
        }
    }

    pub fn select_previous_version(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view().cloned() {
                let model_id = model.id;
                let v_idx = self.selected_version_index.entry(model_id).or_insert(0);
                if *v_idx > 0 {
                    *v_idx -= 1;
                    if let Some(version) = selected_version(&model, *v_idx) {
                        self.selected_file_index.entry(version.id).or_insert(0);
                    }
                }
            }
        }
    }

    pub fn select_next_file(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view() {
                let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
                if let Some(version) = selected_version(model, version_index) {
                    let file_len = version.files.len();
                    if file_len > 0 {
                        let file_idx = self.selected_file_index.entry(version.id).or_insert(0);
                        if *file_idx < file_len.saturating_sub(1) {
                            *file_idx += 1;
                        }
                    }
                }
            }
        }
    }

    pub fn select_previous_file(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view() {
                let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
                if let Some(version) = selected_version(model, version_index) {
                    let file_idx = self.selected_file_index.entry(version.id).or_insert(0);
                    if *file_idx > 0 {
                        *file_idx -= 1;
                    }
                }
            }
        }
    }

    pub fn request_download(&mut self) {
        if self.active_tab == MainTab::Images {
            if let Some(img) = self.images.get(self.selected_index) {
                if let Some(tx) = &self.tx {
                    let _ = tx.try_send(WorkerCommand::DownloadModelForImage(img.id));
                    self.status = format!("Initiated download search for image {}...", img.id);
                }
            }
        } else if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view().map(|m| m.clone()) {
                let v_idx = *self.selected_version_index.get(&model.id).unwrap_or(&0);
                if let Some(version) = selected_version(&model, v_idx) {
                    let file_idx = *self.selected_file_index.get(&version.id).unwrap_or(&0);
                    if let Some(tx) = &self.tx {
                        let _ = tx.try_send(WorkerCommand::DownloadModel(
                            model.clone(),
                            version.id,
                            file_idx,
                        ));
                        self.status = format!(
                            "Initiated download for {} (v: {}, file: {})",
                            model_name(&model),
                            version.name,
                            file_idx + 1
                        );
                    }
                }
            }
        }
    }

    pub fn select_next_download(&mut self) {
        if !self.active_download_order.is_empty()
            && self.selected_download_index + 1 < self.active_download_order.len()
        {
            self.selected_download_index += 1;
        }
    }

    pub fn select_previous_download(&mut self) {
        if self.selected_download_index > 0 {
            self.selected_download_index -= 1;
        }
    }

    pub fn select_next_history(&mut self) {
        if self.selected_history_index + 1 < self.download_history.len() {
            self.selected_history_index += 1;
        }
    }

    pub fn select_previous_history(&mut self) {
        if self.selected_history_index > 0 {
            self.selected_history_index -= 1;
        }
    }

    pub fn selected_download_id(&mut self) -> Option<u64> {
        if self.active_download_order.is_empty() {
            self.selected_download_index = 0;
            return None;
        }
        if self.selected_download_index >= self.active_download_order.len() {
            self.selected_download_index = self.active_download_order.len() - 1;
        }
        self.active_download_order
            .get(self.selected_download_index)
            .copied()
    }

    pub fn push_download_history(
        &mut self,
        model_id: u64,
        version_id: u64,
        filename: String,
        model_name: String,
        file_path: Option<PathBuf>,
        downloaded_bytes: u64,
        total_bytes: u64,
        status: DownloadHistoryStatus,
        progress: f64,
    ) {
        self.download_history.push(DownloadHistoryEntry {
            model_id,
            version_id,
            filename,
            model_name,
            file_path,
            downloaded_bytes,
            total_bytes,
            status,
            progress,
            created_at: std::time::SystemTime::now(),
        });
        if self.download_history.len() > 200 {
            let extra = self.download_history.len() - 200;
            self.download_history.drain(0..extra);
            self.clamp_selected_history_index();
        }
        self.persist_download_history();
    }

    pub fn selected_history_entry_index(&self) -> Option<usize> {
        if self.download_history.is_empty() {
            return None;
        }
        let idx = self
            .selected_history_index
            .min(self.download_history.len().saturating_sub(1));
        Some(self.download_history.len() - 1 - idx)
    }

    pub fn remove_selected_history(&mut self) -> Option<DownloadHistoryEntry> {
        let idx = self.selected_history_entry_index()?;
        let removed = self.download_history.remove(idx);
        self.clamp_selected_history_index();
        self.persist_download_history();
        Some(removed)
    }

    pub fn clamp_selected_download_index(&mut self) {
        if self.active_download_order.is_empty() {
            self.selected_download_index = 0;
            return;
        }
        if self.selected_download_index >= self.active_download_order.len() {
            self.selected_download_index = self.active_download_order.len() - 1;
        }
    }

    pub fn clamp_selected_history_index(&mut self) {
        if self.download_history.is_empty() {
            self.selected_history_index = 0;
            return;
        }
        if self.selected_history_index >= self.download_history.len() {
            self.selected_history_index = self.download_history.len() - 1;
        }
    }

    pub fn selected_model_version(&self) -> Option<(u64, u64)> {
        let model = self.selected_model_in_active_view()?;
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        let version = selected_version(model, version_index)?;
        Some((model.id, version.id))
    }

    pub fn selected_model_version_with_cover_url(&self) -> Option<(u64, u64, Option<String>)> {
        let model = self.selected_model_in_active_view()?;
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        let version = selected_version(model, version_index)?;
        if self.model_version_image_cache.contains_key(&version.id)
            || self.model_version_image_failed.contains(&version.id)
        {
            return None;
        }
        Some((
            model.id,
            version.id,
            preview_image_url(model, version_index),
        ))
    }

    pub fn selected_model_neighbor_cover_urls(&self, radius: usize) -> Vec<(u64, Option<String>)> {
        let Some(model) = self.selected_model_in_active_view() else {
            return Vec::new();
        };
        let versions = model_versions(model);
        if versions.is_empty() {
            return Vec::new();
        }

        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        let center = version_index.min(versions.len().saturating_sub(1));
        let start = center.saturating_sub(radius);
        let end = (center + radius).min(versions.len().saturating_sub(1));

        versions
            .iter()
            .enumerate()
            .filter(|(_, version)| {
                !self.model_version_image_cache.contains_key(&version.id)
                    && !self.model_version_image_failed.contains(&version.id)
            })
            .filter(|(idx, _)| *idx != center && *idx >= start && *idx <= end)
            .map(|(idx, version)| {
                (
                    version.id,
                    version.images.first().map(|image| image.url.clone()).or_else(|| {
                        preview_image_url(model, idx)
                    }),
                )
            })
            .collect()
    }

    pub fn visible_bookmarks(&self) -> Vec<Model> {
        let query = self.bookmark_query.trim().to_ascii_lowercase();
        if query.is_empty() {
            self.bookmarks.clone()
        } else {
            self.bookmarks
                .iter()
                .filter(|model| model_name(model).to_ascii_lowercase().contains(&query))
                .cloned()
                .collect()
        }
    }

    pub fn visible_bookmark_indices(&self) -> Vec<usize> {
        let query = self.bookmark_query.trim().to_ascii_lowercase();
        if query.is_empty() {
            (0..self.bookmarks.len()).collect()
        } else {
            self.bookmarks
                .iter()
                .enumerate()
                .filter_map(|(idx, model)| {
                    if model_name(model).to_ascii_lowercase().contains(&query) {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .collect()
        }
    }

    pub fn clamp_bookmark_selection(&mut self) {
        let visible = self.visible_bookmark_indices();
        if visible.is_empty() {
            self.bookmark_list_state.select(None);
            return;
        }

        let selected = self.bookmark_list_state.selected().unwrap_or(0);
        if selected >= visible.len() {
            self.bookmark_list_state.select(Some(visible.len() - 1));
        }
    }

    pub fn selected_model_in_active_view(&self) -> Option<&Model> {
        match self.active_tab {
            MainTab::Models => self
                .models
                .get(self.model_list_state.selected().unwrap_or(0)),
            MainTab::Bookmarks => {
                let visible = self.visible_bookmark_indices();
                let selected = self.bookmark_list_state.selected().unwrap_or(0);
                self.bookmarks.get(*visible.get(selected)?)
            }
            _ => None,
        }
    }

    pub fn is_image_bookmarked(&self, image_id: u64) -> bool {
        self.image_bookmarks
            .iter()
            .any(|image| image.id == image_id)
    }

    pub fn toggle_bookmark_for_selected_image(&mut self, image: &ImageItem) {
        if self.is_image_bookmarked(image.id) {
            self.image_bookmarks.retain(|item| item.id != image.id);
            self.status = format!("Removed image bookmark: {}", image.id);
            if self.active_tab == MainTab::ImageBookmarks {
                self.clamp_image_bookmark_selection();
            }
        } else {
            self.image_bookmarks.push(image.clone());
            self.status = format!("Added image bookmark: {}", image.id);
        }
        self.deduplicate_image_bookmarks();
        self.persist_image_bookmarks();
    }

    pub fn begin_image_bookmark_search(&mut self) {
        self.image_bookmark_query_draft = self.image_bookmark_query.clone();
        self.mode = AppMode::SearchImageBookmarks;
        self.status = "Search image bookmarks. Enter apply, Esc cancel".to_string();
    }

    pub fn apply_image_bookmark_query(&mut self) {
        self.image_bookmark_query = self.image_bookmark_query_draft.clone();
        self.mode = AppMode::Browsing;
        self.clamp_image_bookmark_selection();
        self.status = format!(
            "Image bookmark query applied: {}",
            if self.image_bookmark_query.is_empty() {
                "<all>".to_string()
            } else {
                self.image_bookmark_query.clone()
            }
        );
    }

    pub fn cancel_image_bookmark_search(&mut self) {
        self.image_bookmark_query_draft = self.image_bookmark_query.clone();
        self.mode = AppMode::Browsing;
        self.status = "Image bookmark search cancelled.".to_string();
    }

    pub fn is_model_bookmarked(&self, model_id: u64) -> bool {
        self.bookmarks.iter().any(|model| model.id == model_id)
    }

    pub fn toggle_bookmark_for_selected_model(&mut self, model: &Model) {
        if self.is_model_bookmarked(model.id) {
            self.bookmarks.retain(|item| item.id != model.id);
            self.status = format!("Removed bookmark: {}", model_name(model));
            if self.active_tab == MainTab::Bookmarks {
                self.clamp_bookmark_selection();
            }
        } else {
            self.bookmarks.push(model.clone());
            self.status = format!("Added bookmark: {}", model_name(model));
        }
        self.deduplicate_bookmarks();
        self.persist_bookmarks();
    }

    pub fn confirm_remove_selected_bookmark(&mut self) {
        let Some(model_id) = self.pending_bookmark_remove_id.take() else {
            self.show_bookmark_confirm_modal = false;
            return;
        };

        if let Some(pos) = self.bookmarks.iter().position(|model| model.id == model_id) {
            let name = model_name(&self.bookmarks[pos]);
            self.bookmarks.remove(pos);
            self.persist_bookmarks();
            self.clamp_bookmark_selection();
            self.status = format!("Removed bookmark: {}", name);
        } else {
            self.status = "Bookmark already removed.".to_string();
        }

        self.show_bookmark_confirm_modal = false;
        self.pending_bookmark_remove_id = None;
    }

    pub fn cancel_bookmark_remove(&mut self) {
        self.show_bookmark_confirm_modal = false;
        self.pending_bookmark_remove_id = None;
    }

    pub fn request_bookmark_remove_selected(&mut self) {
        if self.active_tab != MainTab::Bookmarks {
            return;
        }

        if let Some(model) = self.selected_model_in_active_view() {
            self.pending_bookmark_remove_id = Some(model.id);
            self.show_bookmark_confirm_modal = true;
        } else {
            self.status = "No bookmark selected".to_string();
        }
    }

    pub fn begin_bookmark_search(&mut self) {
        self.bookmark_query_draft = self.bookmark_query.clone();
        self.mode = AppMode::SearchBookmarks;
        self.status = "Search bookmarks. Enter apply, Esc cancel".to_string();
    }

    pub fn begin_bookmark_export_prompt(&mut self) {
        self.bookmark_path_draft = self
            .effective_bookmark_file_path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        self.bookmark_path_prompt_action = Some(BookmarkPathAction::Export);
        self.mode = AppMode::BookmarkPathPrompt;
        self.status = "Bookmark export path. Enter to confirm, Esc to cancel.".to_string();
    }

    pub fn begin_bookmark_import_prompt(&mut self) {
        self.bookmark_path_draft = self
            .effective_bookmark_file_path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        self.bookmark_path_prompt_action = Some(BookmarkPathAction::Import);
        self.mode = AppMode::BookmarkPathPrompt;
        self.status = "Bookmark import path. Enter to confirm, Esc to cancel.".to_string();
    }

    pub fn cancel_bookmark_path_prompt(&mut self) {
        self.bookmark_path_prompt_action = None;
        self.mode = AppMode::Browsing;
        self.status = "Bookmark path input cancelled.".to_string();
    }

    pub fn apply_bookmark_path_prompt(&mut self) {
        let action = self.bookmark_path_prompt_action.take();
        if action.is_none() {
            self.mode = AppMode::Browsing;
            return;
        }

        self.mode = AppMode::Browsing;

        let path = {
            let trimmed = self.bookmark_path_draft.trim();
            if trimmed.is_empty() {
                self.effective_bookmark_file_path()
            } else {
                Some(PathBuf::from(trimmed))
            }
        };

        let Some(path) = path else {
            self.status = "No bookmark file path configured.".to_string();
            return;
        };

        self.set_bookmark_file_path(path.clone());

        match action {
            Some(BookmarkPathAction::Export) => self.export_bookmarks_to_path(path),
            Some(BookmarkPathAction::Import) => self.import_bookmarks_from_path(path),
            None => {}
        }
    }

    pub fn effective_bookmark_file_path(&self) -> Option<PathBuf> {
        self.bookmark_file_path
            .clone()
            .or_else(crate::config::AppConfig::bookmark_path)
    }

    pub fn set_bookmark_file_path(&mut self, path: PathBuf) {
        self.bookmark_file_path = Some(path.clone());
        self.config.bookmark_file_path = Some(path);
    }

    pub fn set_download_history_file_path(&mut self, path: PathBuf) {
        self.download_history_file_path = Some(path.clone());
        self.config.download_history_file_path = Some(path);
        self.persist_download_history();
    }

    pub fn has_active_download(&self) -> bool {
        !self.active_downloads.is_empty()
    }

    pub fn collect_interrupt_sessions_from_active(&self) -> Vec<InterruptedDownloadSession> {
        self.active_download_order
            .iter()
            .filter_map(|model_id| {
                self.active_downloads
                    .get(model_id)
                    .map(|tracker| (*model_id, tracker))
            })
            .map(|(model_id, tracker)| InterruptedDownloadSession {
                model_id,
                version_id: tracker.version_id,
                filename: tracker.filename.clone(),
                model_name: tracker.model_name.clone(),
                file_path: tracker.file_path.clone(),
                downloaded_bytes: tracker.downloaded_bytes,
                total_bytes: tracker.total_bytes,
                created_at: std::time::SystemTime::now(),
            })
            .collect()
    }

    pub fn selected_download_history_entry(&self) -> Option<&DownloadHistoryEntry> {
        let idx = self.selected_history_entry_index()?;
        self.download_history.get(idx)
    }

    pub fn upsert_download_history(&mut self, entry: DownloadHistoryEntry) {
        self.download_history.retain(|existing| {
            !(existing.model_id == entry.model_id
                && existing.version_id == entry.version_id
                && matches!(existing.status, DownloadHistoryStatus::Paused))
        });

        self.download_history.push(entry);

        if self.download_history.len() > 200 {
            let extra = self.download_history.len() - 200;
            self.download_history.drain(0..extra);
            self.clamp_selected_history_index();
        }

        self.persist_download_history();
    }

    pub fn record_interrupted_session_to_history(&mut self, session: &InterruptedDownloadSession) {
        let progress = if session.total_bytes > 0 {
            (session.downloaded_bytes as f64 / session.total_bytes as f64) * 100.0
        } else {
            0.0
        };
        self.upsert_download_history(DownloadHistoryEntry {
            model_id: session.model_id,
            version_id: session.version_id,
            filename: session.filename.clone(),
            model_name: session.model_name.clone(),
            file_path: session.file_path.clone(),
            downloaded_bytes: session.downloaded_bytes,
            total_bytes: session.total_bytes,
            status: DownloadHistoryStatus::Paused,
            progress,
            created_at: session.created_at,
        });
    }

    pub fn cancel_resume_download_modal(&mut self) {
        self.show_resume_download_modal = false;
    }

    pub fn clear_interrupted_download_sessions(&mut self) {
        self.interrupted_download_sessions.clear();
        self.show_resume_download_modal = false;
        self.persist_interrupted_downloads();
    }

    pub fn remove_history_for_session(&mut self, model_id: u64, version_id: u64) -> usize {
        let before = self.download_history.len();
        self.download_history
            .retain(|entry| !(entry.model_id == model_id && entry.version_id == version_id));
        if before != self.download_history.len() {
            self.persist_download_history();
            self.clamp_selected_history_index();
        }
        before.saturating_sub(self.download_history.len())
    }

    pub fn persist_interrupted_downloads(&mut self) {
        if let Some(path) = &self.interrupted_download_file_path {
            if let Err(err) =
                save_interrupted_downloads_to_file(path, &self.interrupted_download_sessions)
            {
                self.last_error = Some(err.to_string());
            } else {
                self.last_error = None;
            }
        }
    }

    pub fn begin_exit_confirm_modal(&mut self) {
        self.show_exit_confirm_modal = true;
        self.status = format!(
            "Active downloads detected ({}). Confirm exit: [Y] Save and exit, [D] Delete and exit, [N] Cancel.",
            self.active_downloads.len()
        );
    }

    pub fn cancel_exit_confirm_modal(&mut self) {
        self.show_exit_confirm_modal = false;
    }

    pub fn is_bookmark_export_prompt(&self) -> bool {
        matches!(
            self.bookmark_path_prompt_action,
            Some(BookmarkPathAction::Export)
        )
    }

    pub fn apply_bookmark_query(&mut self) {
        self.bookmark_query = self.bookmark_query_draft.clone();
        self.mode = AppMode::Browsing;
        self.clamp_bookmark_selection();
        self.status = format!(
            "Bookmark query applied: {}",
            if self.bookmark_query.is_empty() {
                "<all>".to_string()
            } else {
                self.bookmark_query.clone()
            }
        );
    }

    pub fn cancel_bookmark_search(&mut self) {
        self.bookmark_query_draft = self.bookmark_query.clone();
        self.mode = AppMode::Browsing;
        self.status = "Bookmark search cancelled.".to_string();
    }

    pub fn export_bookmarks_to_path(&mut self, path: PathBuf) {
        self.set_bookmark_file_path(path.clone());

        if let Err(err) = save_bookmarks_to_file(&path, &self.bookmarks) {
            self.last_error = Some(err.to_string());
            self.status = "Failed to export bookmarks".to_string();
            return;
        }

        self.last_error = None;
        self.status = format!(
            "Exported {} bookmarks to {}",
            self.bookmarks.len(),
            path.display()
        );
    }

    pub fn import_bookmarks_from_path(&mut self, path: PathBuf) {
        self.set_bookmark_file_path(path.clone());
        let mut imported = load_bookmarks(Some(path.as_path()));
        if imported.is_empty() {
            self.status = "No bookmarks found in import file.".to_string();
            return;
        }

        let before = self.bookmarks.len();
        self.bookmarks.append(&mut imported);
        self.deduplicate_bookmarks();
        self.clamp_bookmark_selection();
        self.persist_bookmarks();

        if self.bookmarks.len() > before {
            self.status = format!(
                "Imported {} new bookmark(s).",
                self.bookmarks.len() - before
            );
            self.last_error = None;
        } else {
            self.status = "Import completed, no new bookmarks.".to_string();
        }
    }

    fn deduplicate_bookmarks(&mut self) {
        let mut seen = HashSet::new();
        self.bookmarks.retain(|model| seen.insert(model.id));
    }

    fn deduplicate_image_bookmarks(&mut self) {
        let mut seen = HashSet::new();
        self.image_bookmarks.retain(|image| seen.insert(image.id));
    }

    pub fn persist_bookmarks(&mut self) {
        if let Some(path) = &self.bookmark_file_path {
            if let Err(err) = save_bookmarks_to_file(path, &self.bookmarks) {
                self.last_error = Some(err.to_string());
            } else {
                self.last_error = None;
            }
        }
    }

    pub fn persist_image_bookmarks(&mut self) {
        if let Some(path) = &self.image_bookmark_file_path {
            if let Err(err) = save_image_bookmarks_to_file(path, &self.image_bookmarks) {
                self.last_error = Some(err.to_string());
            } else {
                self.last_error = None;
            }
        }
    }

    pub fn persist_download_history(&mut self) {
        if let Some(path) = &self.download_history_file_path {
            if let Err(err) = save_download_history_to_file(path, &self.download_history) {
                self.last_error = Some(err.to_string());
            } else {
                self.last_error = None;
            }
        }
    }
}

fn load_bookmarks(path: Option<&Path>) -> Vec<Model> {
    let Some(path) = path else {
        return Vec::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut models: Vec<Model> = match serde_json::from_str(&content) {
        Ok(models) => models,
        Err(_) => Vec::new(),
    };

    let mut seen = HashSet::new();
    models.retain(|model| seen.insert(model.id));
    models
}

fn period_to_created_at(period: &str) -> Option<String> {
    let end = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs();

    let start = match period {
        "Day" => end.saturating_sub(24 * 60 * 60),
        "Week" => end.saturating_sub(7 * 24 * 60 * 60),
        "Month" => end.saturating_sub(30 * 24 * 60 * 60),
        "Year" => end.saturating_sub(365 * 24 * 60 * 60),
        _ => return None,
    };

    Some(format!("{start}-{end}"))
}

fn load_image_bookmarks(path: Option<&Path>) -> Vec<ImageItem> {
    let Some(path) = path else {
        return Vec::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut images: Vec<ImageItem> = serde_json::from_str(&content).unwrap_or_default();
    let mut seen = HashSet::new();
    images.retain(|image| seen.insert(image.id));
    images
}

fn save_bookmarks_to_file(path: &Path, bookmarks: &[Model]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let mut normalized = bookmarks.to_vec();
    let mut seen = HashSet::new();
    normalized.retain(|model| seen.insert(model.id));

    let json = serde_json::to_string_pretty(&normalized).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}

fn save_image_bookmarks_to_file(path: &Path, bookmarks: &[ImageItem]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let mut normalized = bookmarks.to_vec();
    let mut seen = HashSet::new();
    normalized.retain(|image| seen.insert(image.id));

    let json = serde_json::to_string_pretty(&normalized).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}

fn load_download_history(path: Option<&Path>) -> Vec<DownloadHistoryEntry> {
    let Some(path) = path else {
        return Vec::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut history: Vec<DownloadHistoryEntry> = match serde_json::from_str(&content) {
        Ok(history) => history,
        Err(_) => Vec::new(),
    };

    if history.len() > 200 {
        let extra = history.len() - 200;
        history.drain(0..extra);
    }

    history
}

fn collect_paused_sessions_from_history(
    history: &[DownloadHistoryEntry],
) -> Vec<InterruptedDownloadSession> {
    let mut sessions = Vec::new();
    let mut seen = HashSet::new();

    for entry in history.iter().rev() {
        if !matches!(entry.status, DownloadHistoryStatus::Paused) {
            continue;
        }

        if entry.downloaded_bytes == 0 {
            continue;
        }

        if let Some(total_bytes) = if entry.total_bytes == 0 {
            None
        } else {
            Some(entry.total_bytes)
        } {
            if entry.downloaded_bytes >= total_bytes {
                continue;
            }
        }

        if let Some(file_path) = &entry.file_path {
            if !file_path.exists() {
                continue;
            }
        } else {
            continue;
        }

        if seen.contains(&(entry.model_id, entry.version_id)) {
            continue;
        }
        seen.insert((entry.model_id, entry.version_id));

        sessions.push(InterruptedDownloadSession {
            model_id: entry.model_id,
            version_id: entry.version_id,
            filename: entry.filename.clone(),
            model_name: entry.model_name.clone(),
            file_path: entry.file_path.clone(),
            downloaded_bytes: entry.downloaded_bytes,
            total_bytes: entry.total_bytes,
            created_at: entry.created_at,
        });
    }

    sessions.reverse();
    sessions
}

fn load_interrupted_downloads(path: Option<&Path>) -> Vec<InterruptedDownloadSession> {
    let Some(path) = path else {
        return Vec::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    serde_json::from_str::<Vec<InterruptedDownloadSession>>(&content).unwrap_or_default()
}

fn save_download_history_to_file(
    path: &Path,
    history: &[DownloadHistoryEntry],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let json = serde_json::to_string_pretty(&history).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}

fn save_interrupted_downloads_to_file(
    path: &Path,
    sessions: &[InterruptedDownloadSession],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    if sessions.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }

    let json = serde_json::to_string_pretty(sessions).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}
