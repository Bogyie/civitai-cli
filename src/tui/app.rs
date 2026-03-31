use crate::api::{ImageItem, Model};
use ratatui_image::protocol::StatefulProtocol;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::sync::mpsc;
use ratatui::widgets::ListState;
use serde_json;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum MainTab {
    Models,
    Bookmarks,
    Images,
    Downloads,
    Settings,
}

#[derive(PartialEq, Clone, Copy)]
pub enum AppMode {
    Browsing,
    SearchForm,
    SearchBookmarks,
    BookmarkPathPrompt,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BookmarkPathAction {
    Export,
    Import,
}

pub struct SearchFormState {
    pub query: String,
    pub focused_field: usize, // 0: Query, 1: Type, 2: Sort, 3: BaseModel
    pub selected_type: usize,
    pub types: Vec<String>,
    pub selected_sort: usize,
    pub sorts: Vec<String>,
    pub selected_base: usize,
    pub bases: Vec<String>,
}

pub struct SettingsFormState {
    pub editing: bool,
    pub focused_field: usize, 
    pub input_buffer: String,
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

impl SearchFormState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            focused_field: 0,
            selected_type: 0,
            types: vec!["All".into(), "Checkpoint".into(), "TextualInversion".into(), "Hypernetwork".into(), "AestheticGradient".into(), "LORA".into(), "Controlnet".into(), "Poses".into()],
            selected_sort: 0,
            sorts: vec!["Highest Rated".into(), "Most Downloaded".into(), "Newest".into()],
            selected_base: 0,
            bases: vec![
                "All".into(),
                "Anima".into(),
                "AuraFlow".into(),
                "Chroma".into(),
                "CogVideoX".into(),
                "Flux.1 S".into(),
                "Flux.1 D".into(),
                "Flux.1 Krea".into(),
                "Flux.1 Kontext".into(),
                "Flux.2 D".into(),
                "Flux.2 Klein 9B".into(),
                "Flux.2 Klein 9B-base".into(),
                "Flux.2 Klein 4B".into(),
                "Flux.2 Klein 4B-base".into(),
                "Grok".into(),
                "HiDream".into(),
                "Hunyuan 1".into(),
                "Hunyuan Video".into(),
                "Illustrious".into(),
                "NoobAI".into(),
                "Kolors".into(),
                "LTXV".into(),
                "LTXV2".into(),
                "LTXV 2.3".into(),
                "Lumina".into(),
                "Mochi".into(),
                "Other".into(),
                "PixArt a".into(),
                "PixArt E".into(),
                "Pony".into(),
                "Pony V7".into(),
                "Qwen".into(),
                "Qwen 2".into(),
                "Wan Video 1.3B t2v".into(),
                "Wan Video 14B t2v".into(),
                "Wan Video 14B i2v 480p".into(),
                "Wan Video 14B i2v 720p".into(),
                "Wan Video 2.2 TI2V-5B".into(),
                "Wan Video 2.2 I2V-A14B".into(),
                "Wan Video 2.2 T2V-A14B".into(),
                "Wan Video 2.5 T2V".into(),
                "Wan Video 2.5 I2V".into(),
                "SD 1.4".into(),
                "SD 1.5".into(),
                "SD 1.5 LCM".into(),
                "SD 1.5 Hyper".into(),
                "SD 2.0".into(),
                "SD 2.1".into(),
                "SDXL 1.0".into(),
                "SDXL Lightning".into(),
                "SDXL Hyper".into(),
                "ZImageTurbo".into(),
                "ZImageBase".into(),
            ],
        }
    }

    pub fn build_options(&self) -> crate::api::client::SearchOptions {
        crate::api::client::SearchOptions {
            query: self.query.clone(),
            limit: 50,
            types: Some(self.types[self.selected_type].clone()),
            sort: Some(self.sorts[self.selected_sort].clone()),
            base_models: Some(self.bases[self.selected_base].clone()),
        }
    }
}

pub enum AppMessage {
    ImagesLoaded(Vec<ImageItem>),
    ImageDecoded(u64, StatefulProtocol),
    ModelsSearched(Vec<Model>),
    ModelCoverDecoded(u64, StatefulProtocol), // version_id, protocol
    ModelCoverLoadFailed(u64),
    StatusUpdate(String),
    DownloadStarted(u64, String, u64, String, u64, Option<PathBuf>), // model_id, filename, version_id, model_name, total_bytes, file_path
    DownloadProgress(u64, String, f64, u64, u64),   // model_id, filename, percentage, downloaded_bytes, total_bytes
    DownloadPaused(u64),
    DownloadResumed(u64),
    DownloadCompleted(u64),                    // model_id
    DownloadFailed(u64, String),               // model_id, reason
    DownloadCancelled(u64),                    // model_id
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

pub enum DownloadHistoryStatus {
    Completed,
    Failed(String),
    Cancelled,
}

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

pub enum WorkerCommand {
    FetchImages,
    SearchModels(crate::api::client::SearchOptions, Option<u64>, Option<u64>),
    PrioritizeModelCover(u64, Option<String>),
    DownloadModelForImage(u64),
    DownloadModel(u64, u64), // model_id, version_id
    PauseDownload(u64),      // model_id
    ResumeDownload(u64),     // model_id
    CancelDownload(u64),     // model_id
    Quit,
    UpdateConfig(crate::config::AppConfig),
}

pub struct App {
    pub active_tab: MainTab,
    pub mode: AppMode,
    pub config: crate::config::AppConfig,
    pub search_form: SearchFormState,
    pub settings_form: SettingsFormState,
    
    pub models: Vec<Model>,
    pub bookmarks: Vec<Model>,
    pub show_model_details: bool,
    pub model_list_state: ListState,
    pub bookmark_list_state: ListState,
    pub bookmark_query: String,
    pub bookmark_query_draft: String,
    pub show_bookmark_confirm_modal: bool,
    pub pending_bookmark_remove_id: Option<u64>,
    pub bookmark_file_path: Option<PathBuf>,
    pub selected_version_index: HashMap<u64, usize>,
    pub model_version_image_cache: HashMap<u64, StatefulProtocol>,
    pub model_version_image_failed: HashSet<u64>,

    pub images: Vec<ImageItem>,
    pub selected_index: usize,
    pub image_cache: HashMap<u64, StatefulProtocol>,
    
    pub active_downloads: HashMap<u64, DownloadTracker>,
    pub active_download_order: Vec<u64>,
    pub selected_download_index: usize,
    pub selected_history_index: usize,
    pub download_history: Vec<DownloadHistoryEntry>,

    pub status: String,
    pub last_error: Option<String>,
    pub show_status_modal: bool,
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
        let mut bookmark_list_state = ListState::default();
        if !bookmarks.is_empty() {
            bookmark_list_state.select(Some(0));
        }
        let mut model_list_state = ListState::default();
        model_list_state.select(Some(0));

        Self {
            active_tab: MainTab::Models,
            mode: AppMode::Browsing,
            config,
            search_form: SearchFormState::new(),
            settings_form: SettingsFormState::new(),
            models: Vec::new(),
            bookmarks,
            show_model_details: false,
            model_list_state,
            bookmark_list_state,
            bookmark_query: String::new(),
            bookmark_query_draft: String::new(),
            show_bookmark_confirm_modal: false,
            pending_bookmark_remove_id: None,
            bookmark_file_path,
            selected_version_index: HashMap::new(),
            model_version_image_cache: HashMap::new(),
            model_version_image_failed: HashSet::new(),
            images: Vec::new(),
            selected_index: 0,
            image_cache: HashMap::new(),
            active_downloads: HashMap::new(),
            active_download_order: Vec::new(),
            selected_download_index: 0,
            selected_history_index: 0,
            download_history: Vec::new(),
            status: "Initializing App...".to_string(),
            last_error: None,
            show_status_modal: false,
            bookmark_path_prompt_action: None,
            bookmark_path_draft: String::new(),
            tx: None,
        }
    }

    pub fn set_worker_tx(&mut self, tx: mpsc::Sender<WorkerCommand>) {
        self.tx = Some(tx.clone());
        let _ = tx.try_send(WorkerCommand::FetchImages);
    }

    pub fn select_next(&mut self) {
        if self.active_tab == MainTab::Images {
            if !self.images.is_empty() && self.selected_index < self.images.len() - 1 {
                self.selected_index += 1;
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

    pub fn select_next_version(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view() {
                let model_id = model.id;
                let version_len = model.model_versions.len();
                let v_idx = self.selected_version_index.entry(model_id).or_insert(0);
                if *v_idx < version_len.saturating_sub(1) {
                    *v_idx += 1;
                }
            }
        }
    }

    pub fn select_previous_version(&mut self) {
        if self.active_tab == MainTab::Models || self.active_tab == MainTab::Bookmarks {
            if let Some(model) = self.selected_model_in_active_view() {
                let model_id = model.id;
                let v_idx = self.selected_version_index.entry(model_id).or_insert(0);
                if *v_idx > 0 {
                    *v_idx -= 1;
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
                if let Some(version) = model.model_versions.get(v_idx) {
                    if let Some(tx) = &self.tx {
                        let _ = tx.try_send(WorkerCommand::DownloadModel(model.id, version.id));
                        self.status = format!("Initiated download for {} (v: {})", model.name, version.name);
                    }
                }
            }
        }
    }

    pub fn select_next_download(&mut self) {
        if !self.active_download_order.is_empty() && self.selected_download_index + 1 < self.active_download_order.len() {
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
        self.active_download_order.get(self.selected_download_index).copied()
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
    }

    pub fn selected_history_entry_index(&self) -> Option<usize> {
        if self.download_history.is_empty() {
            return None;
        }
        let idx = self.selected_history_index.min(self.download_history.len().saturating_sub(1));
        Some(self.download_history.len() - 1 - idx)
    }

    pub fn selected_history(&self) -> Option<&DownloadHistoryEntry> {
        let idx = self.selected_history_entry_index()?;
        self.download_history.get(idx)
    }

    pub fn remove_selected_history(&mut self) -> Option<DownloadHistoryEntry> {
        let idx = self.selected_history_entry_index()?;
        let removed = self.download_history.remove(idx);
        self.clamp_selected_history_index();
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
        let version = model.model_versions.get(version_index)?;
        Some((model.id, version.id))
    }

    pub fn selected_model_version_with_cover_url(&self) -> Option<(u64, u64, Option<String>)> {
        let model = self.selected_model_in_active_view()?;
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        let version = model.model_versions.get(version_index)?;
        Some((
            model.id,
            version.id,
            version.images.first().map(|image| image.url.clone()),
        ))
    }

    pub fn visible_bookmarks(&self) -> Vec<Model> {
        let query = self.bookmark_query.trim().to_ascii_lowercase();
        if query.is_empty() {
            self.bookmarks.clone()
        } else {
            self.bookmarks
                .iter()
                .filter(|model| model.name.to_ascii_lowercase().contains(&query))
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
                    if model.name.to_ascii_lowercase().contains(&query) {
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

    pub fn is_model_bookmarked(&self, model_id: u64) -> bool {
        self.bookmarks.iter().any(|model| model.id == model_id)
    }

    pub fn toggle_bookmark_for_selected_model(&mut self, model: &Model) {
        if self.is_model_bookmarked(model.id) {
            self.bookmarks.retain(|item| item.id != model.id);
            self.status = format!("Removed bookmark: {}", model.name);
            if self.active_tab == MainTab::Bookmarks {
                self.clamp_bookmark_selection();
            }
        } else {
            self.bookmarks.push(model.clone());
            self.status = format!("Added bookmark: {}", model.name);
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
            let name = self.bookmarks[pos].name.clone();
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

    pub fn effective_bookmark_file_path_text(&self) -> String {
        self.effective_bookmark_file_path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "Not configured".to_string())
    }

    pub fn set_bookmark_file_path(&mut self, path: PathBuf) {
        self.bookmark_file_path = Some(path.clone());
        self.config.bookmark_file_path = Some(path);
    }

    pub fn is_bookmark_export_prompt(&self) -> bool {
        matches!(self.bookmark_path_prompt_action, Some(BookmarkPathAction::Export))
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

    pub fn export_bookmarks_to_file(&mut self) {
        let Some(path) = self.effective_bookmark_file_path() else {
            self.status = "No bookmark export path available.".to_string();
            return;
        };
        self.export_bookmarks_to_path(path)
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

    pub fn import_bookmarks_from_file(&mut self) {
        let Some(path) = self.effective_bookmark_file_path() else {
            self.status = "No bookmark import path available.".to_string();
            return;
        };
        self.import_bookmarks_from_path(path)
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
            self.status = format!("Imported {} new bookmark(s).", self.bookmarks.len() - before);
            self.last_error = None;
        } else {
            self.status = "Import completed, no new bookmarks.".to_string();
        }
    }

    fn deduplicate_bookmarks(&mut self) {
        let mut seen = HashSet::new();
        self.bookmarks.retain(|model| seen.insert(model.id));
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
