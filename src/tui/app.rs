mod bookmarks;
mod downloads;
mod filters;
mod forms;
mod images;
mod models;
mod storage;
pub(crate) mod types;

use self::filters::{
    bookmark_matches_base_model, bookmark_matches_period, bookmark_matches_query,
    bookmark_matches_type, has_displayable_model_version, sort_bookmarks,
};
pub use self::forms::{
    ImageSearchFormSection, ImageSearchFormState, MediaRenderRequest, SearchFormMode,
    SearchFormSection, SearchFormState, SearchPeriod, SettingsFormState,
};
use self::storage::{
    collect_paused_sessions_from_history, load_bookmarks, load_download_history,
    load_image_bookmarks, load_image_tag_catalog, load_interrupted_downloads,
    save_bookmarks_to_file, save_download_history_to_file, save_image_bookmarks_to_file,
    save_image_tag_catalog_to_file, save_interrupted_downloads_to_file,
};
pub use self::types::{
    AppMessage, AppMode, BookmarkPathAction, DownloadHistoryEntry, DownloadHistoryStatus,
    DownloadKey, DownloadState, DownloadTracker, InterruptedDownloadSession, MainTab,
    NewDownloadHistoryEntry, SelectedModelCover, SelectedVersionCover, VersionCoverJob,
    WorkerCommand,
};
use crate::tui::image::{ParsedUsedModel, image_tags, image_used_model_entries, image_used_models};
use crate::tui::model::{
    ParsedModelMetrics, ParsedModelVersion, default_base_model, model_metrics, model_name,
    model_versions,
};
use civitai_cli::sdk::{
    ModelSearchSortBy, ModelSearchState, SearchImageHit as ImageItem, SearchModelHit as Model,
};
use ratatui::widgets::ListState;
use ratatui_image::protocol::StatefulProtocol;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Clone, Default)]
struct ParsedModelCacheEntry {
    metrics: ParsedModelMetrics,
    versions: Vec<ParsedModelVersion>,
    default_base_model: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StatusHistoryEntry {
    pub message: String,
}

pub struct App {
    pub active_tab: MainTab,
    pub mode: AppMode,
    pub config: crate::config::AppConfig,
    pub search_form: SearchFormState,
    pub bookmark_search_form: SearchFormState,
    pub bookmark_search_form_draft: SearchFormState,
    pub image_search_form: ImageSearchFormState,
    pub settings_form: SettingsFormState,

    pub models: Vec<Model>,
    pub model_search_has_more: bool,
    pub model_search_loading_more: bool,
    pub model_search_next_page: Option<u32>,
    pub bookmarks: Vec<Model>,
    visible_bookmarks_cache: Vec<Model>,
    parsed_model_cache: HashMap<u64, ParsedModelCacheEntry>,
    pub image_bookmarks: Vec<ImageItem>,
    visible_image_bookmarks_cache: Vec<ImageItem>,
    pub image_tag_catalog: Vec<String>,
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
    pub model_version_image_bytes_cache: HashMap<u64, Vec<Vec<u8>>>,
    pub model_version_image_request_keys: HashMap<u64, String>,
    pub model_version_image_failed: HashSet<u64>,

    pub images: Vec<ImageItem>,
    pub selected_index: usize,
    pub selected_image_bookmark_index: usize,
    pub image_cache: HashMap<u64, StatefulProtocol>,
    pub image_bytes_cache: HashMap<u64, Vec<u8>>,
    pub image_request_keys: HashMap<u64, String>,
    pub selected_image_model_index: HashMap<u64, usize>,
    pub image_feed_loaded: bool,
    pub image_feed_loading: bool,
    pub image_feed_next_page: Option<u32>,
    pub image_feed_has_more: bool,
    pub image_advanced_visible: bool,
    pub show_image_prompt_modal: bool,
    pub show_image_model_detail_modal: bool,
    pub image_model_detail_model: Option<Model>,
    pub image_prompt_scroll: u16,

    pub active_downloads: HashMap<DownloadKey, DownloadTracker>,
    pub active_download_order: Vec<DownloadKey>,
    pub selected_download_index: usize,
    pub selected_history_index: usize,
    pub download_history: Vec<DownloadHistoryEntry>,

    pub status: String,
    pub last_error: Option<String>,
    pub show_help_modal: bool,
    pub show_status_modal: bool,
    pub show_status_history_modal: bool,
    pub show_exit_confirm_modal: bool,
    pub show_resume_download_modal: bool,
    pub status_history: Vec<StatusHistoryEntry>,
    pub selected_status_history_index: usize,
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
        let image_tag_catalog = load_image_tag_catalog(config.image_tag_catalog_path().as_deref());
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
        for session in &interrupted_download_sessions {
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
            bookmark_search_form: SearchFormState::new(),
            bookmark_search_form_draft: SearchFormState::new(),
            image_search_form: ImageSearchFormState::new(),
            settings_form: SettingsFormState::new(),
            models: Vec::new(),
            model_search_has_more: true,
            model_search_loading_more: false,
            model_search_next_page: None,
            bookmarks,
            visible_bookmarks_cache: Vec::new(),
            parsed_model_cache: HashMap::new(),
            image_bookmarks,
            visible_image_bookmarks_cache: Vec::new(),
            image_tag_catalog,
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
            model_version_image_bytes_cache: HashMap::new(),
            model_version_image_request_keys: HashMap::new(),
            model_version_image_failed: HashSet::new(),
            images: Vec::new(),
            selected_index: 0,
            selected_image_bookmark_index: 0,
            image_cache: HashMap::new(),
            image_bytes_cache: HashMap::new(),
            image_request_keys: HashMap::new(),
            selected_image_model_index: HashMap::new(),
            image_feed_loaded: false,
            image_feed_loading: false,
            image_feed_next_page: None,
            image_feed_has_more: true,
            image_advanced_visible: false,
            show_image_prompt_modal: false,
            show_image_model_detail_modal: false,
            image_model_detail_model: None,
            image_prompt_scroll: 0,
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
            show_help_modal: false,
            show_status_modal: false,
            show_status_history_modal: false,
            show_exit_confirm_modal: false,
            status_history: vec![StatusHistoryEntry {
                message: "Initializing App...".to_string(),
            }],
            selected_status_history_index: 0,
            bookmark_path_prompt_action: None,
            bookmark_path_draft: String::new(),
            tx: None,
        };

        app.rebuild_parsed_model_cache();
        app.refresh_visible_bookmarks_cache();
        app.refresh_visible_image_bookmarks_cache();

        if !interrupted_download_sessions.is_empty() {
            for session in &interrupted_download_sessions {
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

    pub fn can_request_more_images(&self, threshold: usize) -> bool {
        self.active_tab == MainTab::Images
            && !self.images.is_empty()
            && self.image_feed_has_more
            && !self.image_feed_loading
            && self.selected_index + threshold >= self.images.len()
    }

    pub fn next_image_feed_page(&self) -> Option<u32> {
        self.image_feed_next_page
    }

    pub fn current_status_snapshot(&self) -> String {
        if let Some(error) = self.last_error.as_deref() {
            format!("{} | ERROR: {}", self.status, error)
        } else {
            self.status.clone()
        }
    }

    pub fn record_status_snapshot_if_needed(&mut self) {
        let snapshot = self.current_status_snapshot();
        if snapshot.trim().is_empty() {
            return;
        }

        if self
            .status_history
            .first()
            .map(|entry| entry.message == snapshot)
            .unwrap_or(false)
        {
            return;
        }

        self.status_history
            .insert(0, StatusHistoryEntry { message: snapshot });
        const STATUS_HISTORY_LIMIT: usize = 200;
        if self.status_history.len() > STATUS_HISTORY_LIMIT {
            self.status_history.truncate(STATUS_HISTORY_LIMIT);
        }
        self.clamp_selected_status_history_index();
    }

    pub fn begin_status_history_modal(&mut self) {
        self.show_status_history_modal = true;
        self.selected_status_history_index = 0;
        self.clamp_selected_status_history_index();
    }

    pub fn close_status_history_modal(&mut self) {
        self.show_status_history_modal = false;
    }

    pub fn clamp_selected_status_history_index(&mut self) {
        if self.status_history.is_empty() {
            self.selected_status_history_index = 0;
        } else if self.selected_status_history_index >= self.status_history.len() {
            self.selected_status_history_index = self.status_history.len() - 1;
        }
    }

    pub fn select_next_status_history(&mut self) {
        if self.selected_status_history_index + 1 < self.status_history.len() {
            self.selected_status_history_index += 1;
        }
    }

    pub fn select_previous_status_history(&mut self) {
        if self.selected_status_history_index > 0 {
            self.selected_status_history_index -= 1;
        }
    }

    pub fn select_first_status_history(&mut self) {
        self.selected_status_history_index = 0;
    }

    pub fn select_last_status_history(&mut self) {
        if !self.status_history.is_empty() {
            self.selected_status_history_index = self.status_history.len() - 1;
        }
    }

    pub fn selected_status_history_entry(&self) -> Option<&StatusHistoryEntry> {
        self.status_history.get(self.selected_status_history_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn isolated_config() -> AppConfig {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("civitai-cli-app-tests-{unique}"));
        AppConfig {
            bookmark_file_path: Some(root.join("bookmarks.json")),
            image_bookmark_file_path: Some(root.join("image_bookmarks.json")),
            download_history_file_path: Some(root.join("download_history.json")),
            interrupted_download_file_path: Some(root.join("interrupted_downloads.json")),
            ..AppConfig::default()
        }
    }

    fn model(value: serde_json::Value) -> Model {
        serde_json::from_value(value).expect("valid model fixture")
    }

    fn image(value: serde_json::Value) -> ImageItem {
        serde_json::from_value(value).expect("valid image fixture")
    }

    #[test]
    fn bookmark_visibility_cache_updates_when_query_is_applied() {
        let mut app = App::new(isolated_config());
        app.bookmarks = vec![
            model(json!({ "id": 1, "name": "Flux Portrait" })),
            model(json!({ "id": 2, "name": "Anime Landscape" })),
        ];
        app.refresh_visible_bookmarks_cache();

        assert_eq!(app.visible_bookmarks().len(), 2);

        app.bookmark_search_form_draft.query = "flux".to_string();
        app.apply_bookmark_query();

        assert_eq!(app.visible_bookmarks().len(), 1);
        assert_eq!(app.visible_bookmarks()[0].id, 1);
    }

    #[test]
    fn image_bookmark_visibility_cache_updates_when_query_changes() {
        let mut app = App::new(isolated_config());
        app.image_bookmarks = vec![
            image(json!({ "id": 10, "baseModel": "Flux.1 D" })),
            image(json!({ "id": 20, "baseModel": "SDXL" })),
        ];
        app.refresh_visible_image_bookmarks_cache();

        assert_eq!(app.visible_image_bookmarks().len(), 2);

        app.image_bookmark_query_draft = "flux".to_string();
        app.apply_image_bookmark_query();

        assert_eq!(app.visible_image_bookmarks().len(), 1);
        assert_eq!(app.visible_image_bookmarks()[0].id, 10);
    }

    #[test]
    fn status_history_records_new_snapshots_without_duplicates() {
        let mut app = App::new(isolated_config());

        app.status = "Searching models".into();
        app.record_status_snapshot_if_needed();
        app.record_status_snapshot_if_needed();
        app.last_error = Some("network".into());
        app.record_status_snapshot_if_needed();

        assert_eq!(app.status_history[0].message, "Searching models | ERROR: network");
        assert_eq!(app.status_history[1].message, "Searching models");
    }

    #[test]
    fn status_history_selection_clamps_to_bounds() {
        let mut app = App::new(isolated_config());
        app.status_history = vec![
            StatusHistoryEntry {
                message: "Newest".into(),
            },
            StatusHistoryEntry {
                message: "Older".into(),
            },
        ];

        app.select_next_status_history();
        app.select_next_status_history();
        assert_eq!(app.selected_status_history_index, 1);

        app.select_previous_status_history();
        assert_eq!(app.selected_status_history_index, 0);

        app.selected_status_history_index = 99;
        app.clamp_selected_status_history_index();
        assert_eq!(app.selected_status_history_index, 1);
    }

    #[test]
    fn bookmark_selection_clamps_after_filtered_removal() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Bookmarks;
        app.bookmarks = vec![
            model(json!({ "id": 1, "name": "Flux Portrait" })),
            model(json!({ "id": 2, "name": "Flux Landscape" })),
        ];
        app.refresh_visible_bookmarks_cache();
        app.bookmark_search_form.query = "flux".to_string();
        app.refresh_visible_bookmarks_cache();
        app.bookmark_list_state.select(Some(1));

        let selected = app.bookmarks[1].clone();
        app.toggle_bookmark_for_selected_model(&selected);

        assert_eq!(app.visible_bookmarks().len(), 1);
        assert_eq!(app.bookmark_list_state.selected(), Some(0));
    }

    #[test]
    fn image_bookmark_selection_clamps_after_filtered_removal() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::ImageBookmarks;
        app.image_bookmarks = vec![
            image(json!({ "id": 10, "baseModel": "Flux.1 D" })),
            image(json!({ "id": 20, "baseModel": "Flux.1 D" })),
        ];
        app.refresh_visible_image_bookmarks_cache();
        app.image_bookmark_query = "flux".to_string();
        app.refresh_visible_image_bookmarks_cache();
        app.selected_image_bookmark_index = 1;
        app.image_bookmark_list_state.select(Some(1));

        let selected = app.image_bookmarks[1].clone();
        app.toggle_bookmark_for_selected_image(&selected);

        assert_eq!(app.visible_image_bookmarks().len(), 1);
        assert_eq!(app.selected_image_bookmark_index, 0);
        assert_eq!(app.image_bookmark_list_state.selected(), Some(0));
    }
}
