use civitai_cli::sdk::{ImageSearchState, ModelSearchState, SearchImageHit as ImageItem, SearchModelHit as Model};
use ratatui_image::protocol::StatefulProtocol;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::tui::app::MediaRenderRequest;

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

pub enum AppMessage {
    ImagesLoaded(Vec<ImageItem>, bool, Option<u32>),
    ImageDecoded(u64, StatefulProtocol, Vec<u8>, String),
    ModelsSearchedChunk(Vec<Model>, bool, bool, Option<u32>),
    ModelDetailLoaded(Model, Option<u64>),
    ModelCoverDecoded(u64, StatefulProtocol, Vec<u8>, String),
    ModelCoversDecoded(u64, Vec<(StatefulProtocol, Vec<u8>)>, String),
    ModelCoverLoadFailed(u64),
    StatusUpdate(String),
    DownloadStarted(u64, String, u64, String, u64, Option<PathBuf>),
    DownloadProgress(u64, String, f64, u64, u64),
    DownloadPaused(u64),
    DownloadResumed(u64),
    DownloadCompleted(u64),
    DownloadFailed(u64, String),
    DownloadCancelled(u64),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    Running,
    Paused,
}

pub struct DownloadTracker {
    pub filename: String,
    pub progress: f64,
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
    FetchImages(ImageSearchState, Option<u32>, MediaRenderRequest),
    LoadImage(ImageItem, MediaRenderRequest),
    RebuildImageProtocol(u64, Vec<u8>),
    RebuildModelCover(u64, Vec<u8>),
    SearchModels(
        ModelSearchState,
        Option<u64>,
        Option<u64>,
        bool,
        bool,
        Option<u32>,
    ),
    FetchModelDetail(u64, Option<u64>, String),
    ClearSearchCache,
    ClearAllCaches,
    PrioritizeModelCover(u64, Option<String>, Option<(u32, u32)>, MediaRenderRequest),
    PrefetchModelCovers(
        Vec<(u64, Option<String>, Option<(u32, u32)>)>,
        MediaRenderRequest,
    ),
    DownloadImage(ImageItem),
    DownloadModel(Model, u64, usize),
    PauseDownload(u64),
    ResumeDownload(u64),
    CancelDownload(u64),
    ResumeDownloadModel(u64, u64, Option<PathBuf>, u64, u64),
    Quit,
    UpdateConfig(crate::config::AppConfig),
}
