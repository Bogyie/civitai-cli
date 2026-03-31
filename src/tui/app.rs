use crate::api::{ImageItem, Model};
use ratatui_image::protocol::StatefulProtocol;
use std::collections::HashMap;
use tokio::sync::mpsc;
use ratatui::widgets::ListState;

#[derive(PartialEq, Clone, Copy)]
pub enum AppMode {
    SearchForm,
    ModelResults,
    ImageFeed,
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
    DownloadModel(u64),         
    Quit,
}

pub struct App {
    pub mode: AppMode,
    pub search_form: SearchFormState,
    
    pub models: Vec<Model>,
    pub model_list_state: ListState,
    pub model_image_cache: HashMap<u64, StatefulProtocol>,

    pub images: Vec<ImageItem>,
    pub selected_index: usize,
    pub image_cache: HashMap<u64, StatefulProtocol>,
    
    pub active_downloads: HashMap<u64, DownloadTracker>,

    pub status: String,
    pub tx: Option<mpsc::Sender<WorkerCommand>>,
}

impl App {
    pub fn new() -> Self {
        let mut model_list_state = ListState::default();
        model_list_state.select(Some(0));

        Self {
            mode: AppMode::SearchForm,
            search_form: SearchFormState::new(),
            models: Vec::new(),
            model_list_state,
            model_image_cache: HashMap::new(),
            images: Vec::new(),
            selected_index: 0,
            image_cache: HashMap::new(),
            active_downloads: HashMap::new(),
            status: "Initializing App...".to_string(),
            tx: None,
        }
    }

    pub fn set_worker_tx(&mut self, tx: mpsc::Sender<WorkerCommand>) {
        self.tx = Some(tx.clone());
        let _ = tx.try_send(WorkerCommand::FetchImages);
    }

    pub fn select_next(&mut self) {
        if self.mode == AppMode::ImageFeed {
            if !self.images.is_empty() && self.selected_index < self.images.len() - 1 {
                self.selected_index += 1;
            }
        } else if self.mode == AppMode::ModelResults {
            if !self.models.is_empty() {
                let current = self.model_list_state.selected().unwrap_or(0);
                if current < self.models.len() - 1 {
                    self.model_list_state.select(Some(current + 1));
                }
            }
        }
    }

    pub fn select_previous(&mut self) {
        if self.mode == AppMode::ImageFeed {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            }
        } else if self.mode == AppMode::ModelResults {
            let current = self.model_list_state.selected().unwrap_or(0);
            if current > 0 {
                self.model_list_state.select(Some(current - 1));
            }
        }
    }

    pub fn request_download(&mut self) {
        if self.mode == AppMode::ImageFeed {
            if let Some(img) = self.images.get(self.selected_index) {
                if let Some(tx) = &self.tx {
                    let _ = tx.try_send(WorkerCommand::DownloadModelForImage(img.id));
                    self.status = format!("Initiated download search for image {}...", img.id);
                }
            }
        } else if self.mode == AppMode::ModelResults {
            if let Some(current) = self.model_list_state.selected() {
                if let Some(model) = self.models.get(current) {
                    if let Some(tx) = &self.tx {
                        let _ = tx.try_send(WorkerCommand::DownloadModel(model.id));
                        self.status = format!("Initiated download for Model {}...", model.id);
                    }
                }
            }
        }
    }
}
