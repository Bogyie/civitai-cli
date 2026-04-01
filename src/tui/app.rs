mod filters;
mod forms;
mod storage;
mod types;

use self::filters::{
    bookmark_matches_base_model, bookmark_matches_period, bookmark_matches_query,
    bookmark_matches_type, has_displayable_model_version, sort_bookmarks,
};
use self::storage::{
    collect_paused_sessions_from_history, load_bookmarks, load_download_history,
    load_image_bookmarks, load_image_tag_catalog, load_interrupted_downloads,
    save_bookmarks_to_file, save_download_history_to_file, save_image_bookmarks_to_file,
    save_image_tag_catalog_to_file, save_interrupted_downloads_to_file,
};
pub use self::forms::{
    ImageSearchFormSection, ImageSearchFormState, MediaRenderRequest, SearchFormMode,
    SearchFormSection, SearchFormState, SearchPeriod, SettingsFormState,
};
pub use self::types::{
    AppMessage, AppMode, BookmarkPathAction, DownloadHistoryEntry, DownloadHistoryStatus,
    DownloadState, DownloadTracker, InterruptedDownloadSession, MainTab, WorkerCommand,
};
use civitai_cli::sdk::{
    ModelSearchSortBy, ModelSearchState, SearchImageHit as ImageItem, SearchModelHit as Model,
};
use crate::tui::image::{image_tags, image_used_model_entries, image_used_models, ParsedUsedModel};
use crate::tui::model::{model_name, model_versions, preview_image_info, selected_version};
use ratatui::widgets::ListState;
use ratatui_image::protocol::StatefulProtocol;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::sync::mpsc;

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

    pub active_downloads: HashMap<u64, DownloadTracker>,
    pub active_download_order: Vec<u64>,
    pub selected_download_index: usize,
    pub selected_history_index: usize,
    pub download_history: Vec<DownloadHistoryEntry>,

    pub status: String,
    pub last_error: Option<String>,
    pub show_help_modal: bool,
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
            show_exit_confirm_modal: false,
            bookmark_path_prompt_action: None,
            bookmark_path_draft: String::new(),
            tx: None,
        };

        app.refresh_visible_bookmarks_cache();
        app.refresh_visible_image_bookmarks_cache();

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

    pub fn next_image_feed_page(&self) -> Option<u32> {
        self.image_feed_next_page.clone()
    }

    pub fn visible_image_bookmarks(&self) -> &[ImageItem] {
        &self.visible_image_bookmarks_cache
    }

    fn refresh_visible_image_bookmarks_cache(&mut self) {
        let query = self.image_bookmark_query.trim().to_ascii_lowercase();
        self.visible_image_bookmarks_cache = if query.is_empty() {
            self.image_bookmarks.clone()
        } else {
            self.image_bookmarks
                .iter()
                .filter(|image| {
                    let username = image
                        .user
                        .as_ref()
                        .and_then(|user| user.username.as_deref())
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
                            .metadata
                            .as_ref()
                            .map(|meta| meta.to_string().to_ascii_lowercase().contains(&query))
                            .unwrap_or(false)
                })
                .cloned()
                .collect()
        };
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
            MainTab::ImageBookmarks => self
                .visible_image_bookmarks()
                .get(self.selected_image_bookmark_index),
            _ => None,
        }
    }

    pub fn active_image_items(&self) -> &[ImageItem] {
        match self.active_tab {
            MainTab::Images => &self.images,
            MainTab::ImageBookmarks => self.visible_image_bookmarks(),
            _ => &[],
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
        next_page: Option<u32>,
    ) {
        let new_ids = images.iter().map(|item| item.id).collect::<HashSet<_>>();
        self.image_cache.retain(|id, _| new_ids.contains(id));
        self.image_bytes_cache.retain(|id, _| new_ids.contains(id));
        self.image_request_keys.retain(|id, _| new_ids.contains(id));
        self.selected_image_model_index
            .retain(|id, _| new_ids.contains(id));
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
        next_page: Option<u32>,
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

    pub fn has_cached_image_request(&self, image_id: u64, request_key: &str) -> bool {
        self.image_cache.contains_key(&image_id)
            && self
                .image_request_keys
                .get(&image_id)
                .is_some_and(|key| key == request_key)
    }

    pub fn has_cached_model_cover_request(&self, version_id: u64, request_key: &str) -> bool {
        self.model_version_image_cache.contains_key(&version_id)
            && self
                .model_version_image_request_keys
                .get(&version_id)
                .is_some_and(|key| key == request_key)
    }

    pub fn merge_image_tag_catalog_from_hits(&mut self, images: &[ImageItem]) {
        let mut existing = self
            .image_tag_catalog
            .iter()
            .map(|tag| tag.to_lowercase())
            .collect::<HashSet<_>>();
        let mut changed = false;

        for tag in images
            .iter()
            .flat_map(image_tags)
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
        {
            if existing.insert(tag.to_lowercase()) {
                self.image_tag_catalog.push(tag);
                changed = true;
            }
        }

        if changed {
            self.image_tag_catalog
                .sort_by_key(|tag| tag.to_lowercase());
            self.persist_image_tag_catalog();
        }
    }

    pub fn image_tag_suggestions(&self, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }

        let query = self.image_search_form.tag_query.as_str();
        let prefix = query
            .rsplit(',')
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .to_lowercase();
        let selected = query
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_lowercase())
            .collect::<HashSet<_>>();

        let mut suggestions = self
            .image_tag_catalog
            .iter()
            .filter(|tag| !selected.contains(&tag.to_lowercase()))
            .filter(|tag| prefix.is_empty() || tag.to_lowercase().starts_with(&prefix))
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();

        if suggestions.len() >= limit || prefix.is_empty() {
            return suggestions;
        }

        for tag in self
            .image_tag_catalog
            .iter()
            .filter(|tag| !selected.contains(&tag.to_lowercase()))
            .filter(|tag| tag.to_lowercase().contains(&prefix))
        {
            if suggestions.len() >= limit {
                break;
            }
            if !suggestions.iter().any(|existing| existing.eq_ignore_ascii_case(tag)) {
                suggestions.push(tag.clone());
            }
        }

        suggestions
    }

    pub fn accept_image_tag_suggestion(&mut self) -> bool {
        let Some(suggestion) = self.image_tag_suggestions(1).into_iter().next() else {
            return false;
        };

        let mut tags = self
            .image_search_form
            .tag_query
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect::<Vec<_>>();

        if self.image_search_form.tag_query.trim().is_empty()
            || self.image_search_form.tag_query.trim_end().ends_with(',')
        {
            tags.push(suggestion);
        } else if let Some(last) = tags.last_mut() {
            *last = suggestion;
        } else {
            tags.push(suggestion);
        }

        let mut seen = HashSet::new();
        tags.retain(|tag| seen.insert(tag.to_lowercase()));
        self.image_search_form.tag_query = tags.join(", ");
        true
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
        mut new_models: Vec<Model>,
        has_more: bool,
        next_page: Option<u32>,
    ) {
        if new_models.is_empty() {
            self.model_search_has_more = has_more;
            self.model_search_loading_more = false;
            self.model_search_next_page = next_page;
            return;
        }

        new_models.sort_by_key(|model| !has_displayable_model_version(model));
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
        mut models: Vec<Model>,
        has_more: bool,
        next_page: Option<u32>,
    ) {
        models.sort_by_key(|model| !has_displayable_model_version(model));
        let known_version_ids = models
            .iter()
            .flat_map(model_versions)
            .map(|version| version.id)
            .collect::<HashSet<_>>();
        self.model_version_image_cache
            .retain(|version_id, _| known_version_ids.contains(version_id));
        self.model_version_image_bytes_cache
            .retain(|version_id, _| known_version_ids.contains(version_id));
        self.model_version_image_request_keys
            .retain(|version_id, _| known_version_ids.contains(version_id));
        self.model_version_image_failed
            .retain(|version_id| known_version_ids.contains(version_id));
        self.models = models;
        self.model_search_has_more = has_more;
        self.model_search_loading_more = false;
        self.model_search_next_page = next_page;
        self.model_list_state.select(Some(0));
    }

    pub fn select_next_version(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view().cloned() {
                self.select_next_version_for_model(&model);
            }
        }
    }

    pub fn select_previous_version(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view().cloned() {
                self.select_previous_version_for_model(&model);
            }
        }
    }

    pub fn select_next_file(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view().cloned() {
                self.select_next_file_for_model(&model);
            }
        }
    }

    pub fn select_previous_file(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view().cloned() {
                self.select_previous_file_for_model(&model);
            }
        }
    }

    pub fn request_download(&mut self) {
        if self.active_tab == MainTab::Images {
            if let Some(img) = self.images.get(self.selected_index) {
                if let Some(tx) = &self.tx {
                    let _ = tx.try_send(WorkerCommand::DownloadImage(img.clone()));
                    self.status = format!("Downloading image {}...", img.id);
                }
            }
        } else if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view().map(|m| m.clone()) {
                self.request_download_for_model(&model);
            }
        }
    }

    pub fn select_next_version_for_model(&mut self, model: &Model) {
        let model_id = model.id;
        let version_len = model_versions(model).len();
        let v_idx = self.selected_version_index.entry(model_id).or_insert(0);
        if *v_idx < version_len.saturating_sub(1) {
            *v_idx += 1;
            if let Some(version) = selected_version(model, *v_idx) {
                self.selected_file_index.entry(version.id).or_insert(0);
            }
        }
    }

    pub fn select_previous_version_for_model(&mut self, model: &Model) {
        let model_id = model.id;
        let v_idx = self.selected_version_index.entry(model_id).or_insert(0);
        if *v_idx > 0 {
            *v_idx -= 1;
            if let Some(version) = selected_version(model, *v_idx) {
                self.selected_file_index.entry(version.id).or_insert(0);
            }
        }
    }

    pub fn select_next_file_for_model(&mut self, model: &Model) {
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

    pub fn select_previous_file_for_model(&mut self, model: &Model) {
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        if let Some(version) = selected_version(model, version_index) {
            let file_idx = self.selected_file_index.entry(version.id).or_insert(0);
            if *file_idx > 0 {
                *file_idx -= 1;
            }
        }
    }

    pub fn request_download_for_model(&mut self, model: &Model) {
        let v_idx = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        if let Some(version) = selected_version(model, v_idx) {
            let file_idx = *self.selected_file_index.get(&version.id).unwrap_or(&0);
            if let Some(tx) = &self.tx {
                let _ = tx.try_send(WorkerCommand::DownloadModel(
                    model.clone(),
                    version.id,
                    file_idx,
                ));
                self.status = format!(
                    "Initiated download for {} (v: {}, file: {})",
                    model_name(model),
                    version.name,
                    file_idx + 1
                );
            }
        }
    }

    pub fn begin_image_model_detail_modal_loading(&mut self) {
        self.show_image_model_detail_modal = true;
        self.image_model_detail_model = None;
    }

    pub fn select_next_image_model(&mut self) {
        if !(self.active_tab == MainTab::Images || self.active_tab == MainTab::ImageBookmarks) {
            return;
        }
        if let Some(image) = self.selected_image_in_active_view() {
            let items = image_used_models(image);
            if items.is_empty() {
                return;
            }
            let index = self.selected_image_model_index.entry(image.id).or_insert(0);
            if *index < items.len().saturating_sub(1) {
                *index += 1;
            }
        }
    }

    pub fn select_previous_image_model(&mut self) {
        if !(self.active_tab == MainTab::Images || self.active_tab == MainTab::ImageBookmarks) {
            return;
        }
        if let Some(image) = self.selected_image_in_active_view() {
            let index = self.selected_image_model_index.entry(image.id).or_insert(0);
            if *index > 0 {
                *index -= 1;
            }
        }
    }

    pub fn selected_image_used_model(&self) -> Option<ParsedUsedModel> {
        let image = self.selected_image_in_active_view()?;
        let entries = image_used_model_entries(image);
        let index = self
            .selected_image_model_index
            .get(&image.id)
            .copied()
            .unwrap_or(0)
            .min(entries.len().saturating_sub(1));
        entries.get(index).cloned()
    }

    pub fn open_image_model_detail_modal(&mut self, model: Model, version_id: Option<u64>) {
        if let Some(version_id) = version_id
            && let Some(index) = model_versions(&model)
                .iter()
                .position(|version| version.id == version_id)
        {
            self.selected_version_index.insert(model.id, index);
        }
        self.image_model_detail_model = Some(model);
        self.show_image_model_detail_modal = true;
    }

    pub fn close_image_model_detail_modal(&mut self) {
        self.show_image_model_detail_modal = false;
        self.image_model_detail_model = None;
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
        if let Some(version) = selected_version(model, version_index) {
            Some((model.id, version.id))
        } else {
            model.primary_model_version_id().map(|version_id| (model.id, version_id))
        }
    }

    pub fn selected_model_version_with_cover_url(
        &self,
    ) -> Option<(u64, u64, Option<String>, Option<(u32, u32)>)> {
        let model = self.selected_model_in_active_view()?;
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        let version = selected_version(model, version_index)?;
        if self.model_version_image_failed.contains(&version.id) {
            return None;
        }
        let preview = preview_image_info(model, version_index);
        Some((
            model.id,
            version.id,
            preview.as_ref().map(|image| image.url.clone()),
            preview
                .and_then(|image| Some((image.width?, image.height?))),
        ))
    }

    pub fn selected_model_neighbor_cover_urls(
        &self,
        radius: usize,
    ) -> Vec<(u64, Option<String>, Option<(u32, u32)>)> {
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
                let preview = preview_image_info(model, idx);
                (
                    version.id,
                    version.images.first().map(|image| image.url.clone()).or_else(|| {
                        preview.as_ref().map(|image| image.url.clone())
                    }),
                    version
                        .images
                        .first()
                        .and_then(|image| Some((image.width?, image.height?)))
                        .or_else(|| {
                            preview
                                .and_then(|image| Some((image.width?, image.height?)))
                        }),
                )
            })
            .collect()
    }

    pub fn visible_bookmarks(&self) -> &[Model] {
        &self.visible_bookmarks_cache
    }

    fn refresh_visible_bookmarks_cache(&mut self) {
        let query = self.bookmark_search_form.query.trim().to_ascii_lowercase();
        let mut items = self
            .bookmarks
            .iter()
            .filter(|model| {
                bookmark_matches_query(model, &query)
                    && bookmark_matches_type(model, &self.bookmark_search_form.selected_types)
                    && bookmark_matches_base_model(
                        model,
                        &self.bookmark_search_form.selected_base_models,
                    )
                    && bookmark_matches_period(
                        model,
                        self.bookmark_search_form
                            .periods
                            .get(self.bookmark_search_form.selected_period),
                    )
            })
            .cloned()
            .collect::<Vec<_>>();

        sort_bookmarks(
            &mut items,
            self.bookmark_search_form
                .sort_options
                .get(self.bookmark_search_form.selected_sort)
                .unwrap_or(&ModelSearchSortBy::Relevance),
        );
        self.visible_bookmarks_cache = items;
    }

    pub fn clamp_bookmark_selection(&mut self) {
        let visible = self.visible_bookmarks();
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
            MainTab::Bookmarks => self
                .visible_bookmarks()
                .get(self.bookmark_list_state.selected().unwrap_or(0)),
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
        self.refresh_visible_image_bookmarks_cache();
        self.persist_image_bookmarks();
    }

    pub fn begin_image_bookmark_search(&mut self) {
        self.image_bookmark_query_draft = self.image_bookmark_query.clone();
        self.mode = AppMode::SearchImageBookmarks;
        self.status = "Search image bookmarks. Enter apply, Esc cancel".to_string();
    }

    pub fn apply_image_bookmark_query(&mut self) {
        self.image_bookmark_query = self.image_bookmark_query_draft.clone();
        self.refresh_visible_image_bookmarks_cache();
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
        self.refresh_visible_bookmarks_cache();
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
            self.refresh_visible_bookmarks_cache();
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
        self.bookmark_search_form_draft = self.bookmark_search_form.clone();
        self.bookmark_query_draft = self.bookmark_search_form.query.clone();
        self.mode = AppMode::SearchBookmarks;
        self.status = "Filter bookmarks. Enter apply, Esc cancel".to_string();
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
        self.bookmark_search_form = self.bookmark_search_form_draft.clone();
        self.bookmark_query = self.bookmark_search_form.query.clone();
        self.bookmark_query_draft = self.bookmark_query.clone();
        self.refresh_visible_bookmarks_cache();
        self.mode = AppMode::Browsing;
        self.clamp_bookmark_selection();
        self.status = format!(
            "Bookmark filter applied: {}",
            if self.bookmark_search_form.query.is_empty() {
                "<all>".to_string()
            } else {
                self.bookmark_search_form.query.clone()
            }
        );
    }

    pub fn cancel_bookmark_search(&mut self) {
        self.bookmark_search_form_draft = self.bookmark_search_form.clone();
        self.bookmark_query_draft = self.bookmark_search_form.query.clone();
        self.mode = AppMode::Browsing;
        self.status = "Bookmark filter cancelled.".to_string();
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
        self.refresh_visible_bookmarks_cache();
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

    pub fn persist_image_tag_catalog(&mut self) {
        let Some(path) = self.config.image_tag_catalog_path() else {
            return;
        };

        if let Err(err) = save_image_tag_catalog_to_file(path.as_path(), &self.image_tag_catalog) {
            self.last_error = Some(err.to_string());
        } else {
            self.last_error = None;
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
}
