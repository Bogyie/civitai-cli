use crate::api::{ImageItem, Model};
use ratatui_image::protocol::StatefulProtocol;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::sync::mpsc;
use ratatui::widgets::ListState;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum MainTab {
    Models,
    Images,
    Downloads,
    Settings,
}

#[derive(PartialEq, Clone, Copy)]
pub enum AppMode {
    Browsing,
    SearchForm,
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
    pub show_model_details: bool,
    pub model_list_state: ListState,
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
    pub tx: Option<mpsc::Sender<WorkerCommand>>,
}

impl App {
    pub fn new(config: crate::config::AppConfig) -> Self {
        let mut model_list_state = ListState::default();
        model_list_state.select(Some(0));

        Self {
            active_tab: MainTab::Models,
            mode: AppMode::Browsing,
            config,
            search_form: SearchFormState::new(),
            settings_form: SettingsFormState::new(),
            models: Vec::new(),
            show_model_details: false,
            model_list_state,
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
        }
    }

    pub fn select_next_version(&mut self) {
        if self.active_tab == MainTab::Models {
            let current = self.model_list_state.selected().unwrap_or(0);
            if let Some(model) = self.models.get(current) {
                let v_idx = self.selected_version_index.entry(model.id).or_insert(0);
                if *v_idx < model.model_versions.len().saturating_sub(1) {
                    *v_idx += 1;
                }
            }
        }
    }

    pub fn select_previous_version(&mut self) {
        if self.active_tab == MainTab::Models {
            let current = self.model_list_state.selected().unwrap_or(0);
            if let Some(model) = self.models.get(current) {
                let v_idx = self.selected_version_index.entry(model.id).or_insert(0);
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
        } else if self.active_tab == MainTab::Models {
            if let Some(current) = self.model_list_state.selected() {
                if let Some(model) = self.models.get(current) {
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
        let idx = self.model_list_state.selected()?;
        let model = self.models.get(idx)?;
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        let version = model.model_versions.get(version_index)?;
        Some((model.id, version.id))
    }

    pub fn selected_model_version_with_cover_url(&self) -> Option<(u64, u64, Option<String>)> {
        let idx = self.model_list_state.selected()?;
        let model = self.models.get(idx)?;
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        let version = model.model_versions.get(version_index)?;
        Some((
            model.id,
            version.id,
            version.images.first().map(|image| image.url.clone()),
        ))
    }
}
