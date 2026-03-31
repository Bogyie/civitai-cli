use crate::api::{ImageItem, Model};
use ratatui_image::protocol::StatefulProtocol;
use std::collections::HashMap;
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
            bases: vec!["All".into(), "SD 1.4".into(), "SD 1.5".into(), "SD 2.0".into(), "SD 2.1".into(), "SDXL 1.0".into(), "Pony".into(), "SD 3.5".into(), "Flux.1 D".into()],
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

pub struct DownloadTracker {
    pub filename: String,
    pub progress: f64, // 0.0 to 100.0
}

pub enum AppMessage {
    ImagesLoaded(Vec<ImageItem>),
    ImageDecoded(u64, StatefulProtocol), 
    ModelsSearched(Vec<Model>),
    ModelCoverDecoded(u64, StatefulProtocol), // id, protocol
    StatusUpdate(String),
    DownloadProgress(u64, String, f64), // model_id, filename, percentage
}

pub enum WorkerCommand {
    FetchImages,
    SearchModels(crate::api::client::SearchOptions),
    DownloadModelForImage(u64), 
    DownloadModel(u64, u64), // model_id, version_id
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
    pub model_image_cache: HashMap<u64, StatefulProtocol>,

    pub images: Vec<ImageItem>,
    pub selected_index: usize,
    pub image_cache: HashMap<u64, StatefulProtocol>,
    
    pub active_downloads: HashMap<u64, DownloadTracker>,

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
            mode: AppMode::SearchForm,
            config,
            search_form: SearchFormState::new(),
            settings_form: SettingsFormState::new(),
            models: Vec::new(),
            show_model_details: false,
            model_list_state,
            selected_version_index: HashMap::new(),
            model_image_cache: HashMap::new(),
            images: Vec::new(),
            selected_index: 0,
            image_cache: HashMap::new(),
            active_downloads: HashMap::new(),
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
}
