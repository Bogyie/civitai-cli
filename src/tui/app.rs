use crate::api::ImageItem;
use ratatui_image::protocol::StatefulProtocol;
use std::collections::HashMap;
use tokio::sync::mpsc;

pub enum AppMessage {
    // Messages from worker to UI
    ImagesLoaded(Vec<ImageItem>),
    ImageDecoded(u64, StatefulProtocol), // image ID, and its terminal rendering object
    StatusUpdate(String),
}

pub enum WorkerCommand {
    // Commands from UI to worker
    FetchImages,
    DownloadModelForImage(u64), // trigger download via Image ID
    Quit,
}

pub struct App {
    pub images: Vec<ImageItem>,
    pub selected_index: usize,
    pub image_cache: HashMap<u64, StatefulProtocol>,
    pub status: String,
    pub tx: Option<mpsc::Sender<WorkerCommand>>,
}

impl App {
    pub fn new() -> Self {
        Self {
            images: Vec::new(),
            selected_index: 0,
            image_cache: HashMap::new(),
            status: "Initializing... Fetching feed.".to_string(),
            tx: None,
        }
    }

    pub fn set_worker_tx(&mut self, tx: mpsc::Sender<WorkerCommand>) {
        self.tx = Some(tx.clone());
        // Dispatch auto-fetch on start
        let _ = tx.try_send(WorkerCommand::FetchImages);
    }

    pub fn select_next(&mut self) {
        if !self.images.is_empty() && self.selected_index < self.images.len() - 1 {
            self.selected_index += 1;
        }
    }

    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn request_download(&mut self) {
        if let Some(img) = self.images.get(self.selected_index) {
            if let Some(tx) = &self.tx {
                let _ = tx.try_send(WorkerCommand::DownloadModelForImage(img.id));
                self.status = format!("Initiating download for Model tied to Image {}", img.id);
            }
        }
    }
}
