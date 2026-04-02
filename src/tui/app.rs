mod downloads;
mod filters;
mod forms;
mod images;
mod liked_models;
mod models;
mod storage;
mod templates;
pub(crate) mod types;

use self::filters::{
    has_displayable_model_version, liked_model_matches_base_model, liked_model_matches_period,
    liked_model_matches_query, liked_model_matches_type, sort_liked_models,
};
pub use self::forms::{
    ImageSearchFormSection, ImageSearchFormState, MediaRenderRequest, SearchFormMode,
    SearchFormSection, SearchFormState, SearchPeriod, SettingsFormState,
};
use self::storage::{
    collect_paused_sessions_from_history, load_download_history, load_image_tag_catalog,
    load_interrupted_downloads, load_liked_images, load_liked_models, load_search_templates,
    save_download_history_to_file, save_image_tag_catalog_to_file,
    save_interrupted_downloads_to_file, save_liked_images_to_file, save_liked_models_to_file,
    save_search_templates_to_file,
};
pub use self::types::{
    AppMessage, AppMode, DownloadHistoryEntry, DownloadHistoryStatus, DownloadKey, DownloadState,
    DownloadTracker, ImageSearchTemplate, InterruptedDownloadSession, LikedPathAction, MainTab,
    ModelSearchTemplate, NewDownloadHistoryEntry, SearchTemplateKind, SearchTemplateStore,
    SelectedModelCover, SelectedVersionCover, TagViewerColumn, VersionCoverJob, WorkerCommand,
};
use crate::tui::image::{ParsedUsedModel, image_tags, image_used_model_entries, image_used_models};
use crate::tui::model::{
    ParsedModelMetrics, ParsedModelVersion, default_base_model, model_metrics, model_name,
    model_versions,
};
use crate::tui::status::{StatusEvent, StatusHistoryFilter, StatusLevel};
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

pub struct App {
    pub active_tab: MainTab,
    pub mode: AppMode,
    pub config: crate::config::AppConfig,
    pub search_form: SearchFormState,
    pub liked_model_search_form: SearchFormState,
    pub liked_model_search_form_draft: SearchFormState,
    pub image_search_form: ImageSearchFormState,
    pub settings_form: SettingsFormState,

    pub models: Vec<Model>,
    pub model_search_has_more: bool,
    pub model_search_loading_more: bool,
    pub model_search_next_page: Option<u32>,
    pub liked_models: Vec<Model>,
    visible_liked_models_cache: Vec<Model>,
    parsed_model_cache: HashMap<u64, ParsedModelCacheEntry>,
    pub liked_images: Vec<ImageItem>,
    visible_liked_images_cache: Vec<ImageItem>,
    pub image_tag_catalog: Vec<String>,
    pub model_search_templates: Vec<ModelSearchTemplate>,
    pub image_search_templates: Vec<ImageSearchTemplate>,
    pub search_template_file_path: Option<PathBuf>,
    pub show_search_template_modal: bool,
    pub search_template_kind: SearchTemplateKind,
    pub selected_search_template_index: usize,
    pub search_template_name_draft: String,
    pub search_template_name_editing: bool,
    pub show_model_details: bool,
    pub model_list_state: ListState,
    pub liked_model_list_state: ListState,
    pub liked_image_list_state: ListState,
    pub liked_model_query: String,
    pub liked_model_query_draft: String,
    pub liked_image_query: String,
    pub liked_image_query_draft: String,
    pub show_like_confirm_modal: bool,
    pub pending_liked_model_remove_id: Option<u64>,
    pub liked_model_file_path: Option<PathBuf>,
    pub liked_image_file_path: Option<PathBuf>,
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
    pub selected_liked_image_index: usize,
    pub image_cache: HashMap<u64, StatefulProtocol>,
    pub image_bytes_cache: HashMap<u64, Vec<u8>>,
    pub image_request_keys: HashMap<u64, String>,
    pub selected_image_model_index: HashMap<u64, usize>,
    pub image_feed_loaded: bool,
    pub image_feed_loading: bool,
    pub image_feed_next_page: Option<u32>,
    pub image_feed_total_hits: Option<u64>,
    pub image_feed_has_more: bool,
    pub image_advanced_visible: bool,
    pub show_image_prompt_modal: bool,
    pub show_image_tags_modal: bool,
    pub show_image_model_detail_modal: bool,
    pub image_model_detail_model: Option<Model>,
    pub image_prompt_scroll: u16,
    pub image_tags_scroll: u16,
    pub image_tag_modal_column: TagViewerColumn,
    pub image_tag_modal_selected_index: usize,
    pub image_tag_modal_include_pending: HashSet<String>,
    pub image_tag_modal_exclude_pending: HashSet<String>,

    pub active_downloads: HashMap<DownloadKey, DownloadTracker>,
    pub active_download_order: Vec<DownloadKey>,
    pub selected_download_index: usize,
    pub selected_history_index: usize,
    pub download_history: Vec<DownloadHistoryEntry>,

    pub status: String,
    pub status_detail: Option<String>,
    pub status_level: StatusLevel,
    pub status_recorded_at: std::time::SystemTime,
    pub last_error: Option<String>,
    pub show_help_modal: bool,
    pub show_status_modal: bool,
    pub show_status_history_modal: bool,
    pub show_exit_confirm_modal: bool,
    pub show_resume_download_modal: bool,
    pub status_history: Vec<StatusEvent>,
    pub selected_status_history_index: usize,
    pub status_history_filter: StatusHistoryFilter,
    pub liked_model_path_prompt_action: Option<LikedPathAction>,
    pub liked_model_path_draft: String,
    pub tx: Option<mpsc::Sender<WorkerCommand>>,
}

impl App {
    pub fn new(config: crate::config::AppConfig) -> Self {
        let liked_model_file_path = config
            .liked_model_file_path
            .clone()
            .or_else(crate::config::AppConfig::liked_model_path);
        let liked_models = load_liked_models(liked_model_file_path.as_deref());
        let liked_image_file_path = config
            .liked_image_file_path
            .clone()
            .or_else(crate::config::AppConfig::liked_image_path);
        let liked_images = load_liked_images(liked_image_file_path.as_deref());
        let image_tag_catalog = load_image_tag_catalog(config.image_tag_catalog_path().as_deref());
        let search_template_file_path = config.search_templates_path();
        let search_templates = load_search_templates(search_template_file_path.as_deref());
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
        let mut liked_model_list_state = ListState::default();
        if !liked_models.is_empty() {
            liked_model_list_state.select(Some(0));
        }
        let mut liked_image_list_state = ListState::default();
        if !liked_images.is_empty() {
            liked_image_list_state.select(Some(0));
        }
        let mut model_list_state = ListState::default();
        model_list_state.select(Some(0));

        let initial_status = StatusEvent::info("Initializing App...");
        let mut app = Self {
            active_tab: MainTab::Models,
            mode: AppMode::Browsing,
            config,
            search_form: SearchFormState::new(),
            liked_model_search_form: SearchFormState::new(),
            liked_model_search_form_draft: SearchFormState::new(),
            image_search_form: ImageSearchFormState::new(),
            settings_form: SettingsFormState::new(),
            models: Vec::new(),
            model_search_has_more: true,
            model_search_loading_more: false,
            model_search_next_page: None,
            liked_models,
            visible_liked_models_cache: Vec::new(),
            parsed_model_cache: HashMap::new(),
            liked_images,
            visible_liked_images_cache: Vec::new(),
            image_tag_catalog,
            model_search_templates: search_templates.model_templates,
            image_search_templates: search_templates.image_templates,
            search_template_file_path,
            show_search_template_modal: false,
            search_template_kind: SearchTemplateKind::Model,
            selected_search_template_index: 0,
            search_template_name_draft: String::new(),
            search_template_name_editing: false,
            show_model_details: false,
            model_list_state,
            liked_model_list_state,
            liked_image_list_state,
            liked_model_query: String::new(),
            liked_model_query_draft: String::new(),
            liked_image_query: String::new(),
            liked_image_query_draft: String::new(),
            show_like_confirm_modal: false,
            pending_liked_model_remove_id: None,
            liked_model_file_path,
            liked_image_file_path,
            download_history_file_path,
            selected_version_index: HashMap::new(),
            selected_file_index: HashMap::new(),
            model_version_image_cache: HashMap::new(),
            model_version_image_bytes_cache: HashMap::new(),
            model_version_image_request_keys: HashMap::new(),
            model_version_image_failed: HashSet::new(),
            images: Vec::new(),
            selected_index: 0,
            selected_liked_image_index: 0,
            image_cache: HashMap::new(),
            image_bytes_cache: HashMap::new(),
            image_request_keys: HashMap::new(),
            selected_image_model_index: HashMap::new(),
            image_feed_loaded: false,
            image_feed_loading: false,
            image_feed_next_page: None,
            image_feed_total_hits: None,
            image_feed_has_more: true,
            image_advanced_visible: false,
            show_image_prompt_modal: false,
            show_image_tags_modal: false,
            show_image_model_detail_modal: false,
            image_model_detail_model: None,
            image_prompt_scroll: 0,
            image_tags_scroll: 0,
            image_tag_modal_column: TagViewerColumn::Current,
            image_tag_modal_selected_index: 0,
            image_tag_modal_include_pending: HashSet::new(),
            image_tag_modal_exclude_pending: HashSet::new(),
            active_downloads: HashMap::new(),
            active_download_order: Vec::new(),
            selected_download_index: 0,
            selected_history_index: 0,
            download_history,
            interrupted_download_file_path,
            interrupted_download_sessions: interrupted_sessions_for_state,
            show_resume_download_modal,
            status: initial_status.summary.clone(),
            status_detail: initial_status.detail.clone(),
            status_level: initial_status.level,
            status_recorded_at: initial_status.recorded_at,
            last_error: None,
            show_help_modal: false,
            show_status_modal: false,
            show_status_history_modal: false,
            show_exit_confirm_modal: false,
            status_history: vec![initial_status],
            selected_status_history_index: 0,
            status_history_filter: StatusHistoryFilter::All,
            liked_model_path_prompt_action: None,
            liked_model_path_draft: String::new(),
            tx: None,
        };

        let model_filter_state = app.config.model_filter_state.clone();
        let image_filter_state = app.config.image_filter_state.clone();
        app.search_form.apply_persisted_state(&model_filter_state);
        app.image_search_form
            .apply_persisted_state(&image_filter_state);
        app.selected_index = app.config.image_selection_index;
        app.selected_liked_image_index = app.config.liked_image_selection_index;

        app.rebuild_parsed_model_cache();
        app.refresh_visible_liked_models_cache();
        app.refresh_visible_liked_images_cache();
        app.clamp_liked_image_selection();
        if !app.images.is_empty() && app.selected_index >= app.images.len() {
            app.selected_index = app.images.len() - 1;
        }

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

    pub fn sync_filter_state_to_config(&mut self) {
        self.config.model_filter_state = self.search_form.persisted_state();
        self.config.image_filter_state = self.image_search_form.persisted_state();
        self.config.image_selection_index = self.selected_index;
        self.config.liked_image_selection_index = self.selected_liked_image_index;
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

    pub fn apply_status(&mut self, event: StatusEvent) {
        let is_duplicate = self
            .status_history
            .first()
            .map(|entry| {
                entry.level == event.level
                    && entry.summary == event.summary
                    && entry.detail == event.detail
            })
            .unwrap_or(false);

        self.status = event.summary.clone();
        self.status_detail = event.detail.clone();
        self.status_level = event.level;
        self.status_recorded_at = event.recorded_at;
        self.last_error = if event.level == StatusLevel::Error {
            Some(
                event
                    .detail
                    .clone()
                    .unwrap_or_else(|| event.summary.clone()),
            )
        } else {
            None
        };
        self.show_status_modal = event.show_modal;

        if !is_duplicate {
            self.status_history.insert(0, event);
            const STATUS_HISTORY_LIMIT: usize = 200;
            if self.status_history.len() > STATUS_HISTORY_LIMIT {
                self.status_history.truncate(STATUS_HISTORY_LIMIT);
            }
        }
        self.clamp_selected_status_history_index();
    }

    pub fn sync_status_history_from_fields(&mut self) {
        let level = if self.last_error.is_some() {
            StatusLevel::Error
        } else {
            self.status_level
        };
        let detail = if level == StatusLevel::Error {
            self.last_error
                .clone()
                .or_else(|| self.status_detail.clone())
        } else {
            self.status_detail.clone()
        };
        let differs = self
            .status_history
            .first()
            .map(|entry| {
                entry.level != level || entry.summary != self.status || entry.detail != detail
            })
            .unwrap_or(true);
        if !differs || self.status.trim().is_empty() {
            return;
        }

        self.apply_status(StatusEvent {
            level,
            summary: self.status.clone(),
            detail,
            recorded_at: std::time::SystemTime::now(),
            show_modal: self.show_status_modal,
        });
    }

    pub fn set_status(&mut self, summary: impl Into<String>) {
        self.apply_status(StatusEvent::info(summary));
    }

    pub fn set_status_detail(&mut self, summary: impl Into<String>, detail: impl Into<String>) {
        self.apply_status(StatusEvent::info_detail(summary, detail));
    }

    pub fn set_warn(&mut self, summary: impl Into<String>) {
        self.apply_status(StatusEvent::warn(summary));
    }

    pub fn set_error(&mut self, summary: impl Into<String>, detail: impl Into<String>) {
        self.apply_status(StatusEvent::error_detail(summary, detail));
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
        let len = self.filtered_status_history().len();
        if len == 0 {
            self.selected_status_history_index = 0;
        } else if self.selected_status_history_index >= len {
            self.selected_status_history_index = len - 1;
        }
    }

    pub fn select_next_status_history(&mut self) {
        if self.selected_status_history_index + 1 < self.filtered_status_history().len() {
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
        let len = self.filtered_status_history().len();
        if len > 0 {
            self.selected_status_history_index = len - 1;
        }
    }

    pub fn set_status_history_filter(&mut self, filter: StatusHistoryFilter) {
        self.status_history_filter = filter;
        self.selected_status_history_index = 0;
        self.clamp_selected_status_history_index();
    }

    pub fn filtered_status_history(&self) -> Vec<&StatusEvent> {
        self.status_history
            .iter()
            .filter(|entry| self.status_history_filter.matches(entry.level))
            .collect()
    }

    pub fn selected_status_history_entry(&self) -> Option<&StatusEvent> {
        self.filtered_status_history()
            .get(self.selected_status_history_index)
            .copied()
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
            liked_model_file_path: Some(root.join("liked_models.json")),
            liked_image_file_path: Some(root.join("liked_images.json")),
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
    fn liked_visibility_cache_updates_when_query_is_applied() {
        let mut app = App::new(isolated_config());
        app.liked_models = vec![
            model(json!({ "id": 1, "name": "Flux Portrait" })),
            model(json!({ "id": 2, "name": "Anime Landscape" })),
        ];
        app.refresh_visible_liked_models_cache();

        assert_eq!(app.visible_liked_models().len(), 2);

        app.liked_model_search_form_draft.query = "flux".to_string();
        app.apply_liked_model_query();

        assert_eq!(app.visible_liked_models().len(), 1);
        assert_eq!(app.visible_liked_models()[0].id, 1);
    }

    #[test]
    fn image_liked_visibility_cache_updates_when_query_changes() {
        let mut app = App::new(isolated_config());
        app.liked_images = vec![
            image(json!({ "id": 10, "baseModel": "Flux.1 D" })),
            image(json!({ "id": 20, "baseModel": "SDXL" })),
        ];
        app.refresh_visible_liked_images_cache();

        assert_eq!(app.visible_liked_images().len(), 2);

        app.liked_image_query_draft = "flux".to_string();
        app.apply_liked_image_query();

        assert_eq!(app.visible_liked_images().len(), 1);
        assert_eq!(app.visible_liked_images()[0].id, 10);
    }

    #[test]
    fn status_history_records_new_snapshots_without_duplicates() {
        let mut app = App::new(isolated_config());

        app.set_status("Searching models");
        app.set_status("Searching models");
        app.set_error("Search failed", "network");

        assert_eq!(app.status_history[0].summary, "Search failed");
        assert_eq!(app.status_history[0].detail.as_deref(), Some("network"));
        assert_eq!(app.status_history[1].summary, "Searching models");
    }

    #[test]
    fn status_history_selection_clamps_to_bounds() {
        let mut app = App::new(isolated_config());
        app.status_history = vec![
            crate::tui::status::StatusEvent::info("Newest"),
            crate::tui::status::StatusEvent::warn("Older"),
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
    fn liked_selection_clamps_after_filtered_removal() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::LikedModels;
        app.liked_models = vec![
            model(json!({ "id": 1, "name": "Flux Portrait" })),
            model(json!({ "id": 2, "name": "Flux Landscape" })),
        ];
        app.refresh_visible_liked_models_cache();
        app.liked_model_search_form.query = "flux".to_string();
        app.refresh_visible_liked_models_cache();
        app.liked_model_list_state.select(Some(1));

        let selected = app.liked_models[1].clone();
        app.toggle_like_for_selected_model(&selected);

        assert_eq!(app.visible_liked_models().len(), 1);
        assert_eq!(app.liked_model_list_state.selected(), Some(0));
    }

    #[test]
    fn image_liked_selection_clamps_after_filtered_removal() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::LikedImages;
        app.liked_images = vec![
            image(json!({ "id": 10, "baseModel": "Flux.1 D" })),
            image(json!({ "id": 20, "baseModel": "Flux.1 D" })),
        ];
        app.refresh_visible_liked_images_cache();
        app.liked_image_query = "flux".to_string();
        app.refresh_visible_liked_images_cache();
        app.selected_liked_image_index = 1;
        app.liked_image_list_state.select(Some(1));

        let selected = app.liked_images[1].clone();
        app.toggle_like_for_selected_image(&selected);

        assert_eq!(app.visible_liked_images().len(), 1);
        assert_eq!(app.selected_liked_image_index, 0);
        assert_eq!(app.liked_image_list_state.selected(), Some(0));
    }

    #[test]
    fn image_selection_indices_round_trip_through_config_and_clamp() {
        let mut config = isolated_config();
        config.image_selection_index = 5;
        config.liked_image_selection_index = 7;
        let liked_image_path = config
            .liked_image_file_path
            .clone()
            .expect("liked image path");
        let liked_images = vec![
            image(json!({ "id": 10, "baseModel": "Flux.1 D" })),
            image(json!({ "id": 20, "baseModel": "Flux.1 D" })),
            image(json!({ "id": 30, "baseModel": "Flux.1 D" })),
        ];
        save_liked_images_to_file(&liked_image_path, &liked_images).expect("save liked images");

        let mut app = App::new(config);
        app.images = vec![
            image(json!({ "id": 1, "baseModel": "Flux.1 D" })),
            image(json!({ "id": 2, "baseModel": "Flux.1 D" })),
        ];
        if app.selected_index >= app.images.len() {
            app.selected_index = app.images.len() - 1;
        }

        assert_eq!(app.selected_index, 1);
        assert_eq!(app.selected_liked_image_index, 2);
        assert_eq!(app.liked_image_list_state.selected(), Some(2));

        app.selected_index = 1;
        app.selected_liked_image_index = 2;
        app.sync_filter_state_to_config();

        assert_eq!(app.config.image_selection_index, 1);
        assert_eq!(app.config.liked_image_selection_index, 2);
    }
}
