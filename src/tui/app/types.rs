use civitai_cli::sdk::{
    ImageSearchState, ModelSearchState, SearchImageHit as ImageItem, SearchModelHit as Model,
};
use ratatui_image::protocol::StatefulProtocol;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::{PersistedImageFilterState, PersistedModelFilterState};
use crate::tui::app::MediaRenderRequest;
use crate::tui::status::StatusEvent;

pub type ImageDimensions = (u32, u32);
pub type CoverImageUrl = Option<String>;
pub type CoverSourceDimensions = Option<ImageDimensions>;
pub type VersionCoverJob = (u64, CoverImageUrl, CoverSourceDimensions);
pub type SelectedModelCover = (u64, u64, CoverImageUrl, CoverSourceDimensions);
pub type SelectedVersionCover = (u64, CoverImageUrl, CoverSourceDimensions);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DownloadKey {
    pub model_id: u64,
    pub version_id: u64,
    pub filename: String,
}

impl DownloadKey {
    pub fn new(model_id: u64, version_id: u64, filename: impl Into<String>) -> Self {
        Self {
            model_id,
            version_id,
            filename: filename.into(),
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum MainTab {
    Models,
    SavedModels,
    Images,
    SavedImages,
    Downloads,
    Settings,
}

#[derive(PartialEq, Clone, Copy)]
pub enum AppMode {
    Browsing,
    SearchForm,
    SearchImages,
    SearchSavedModels,
    SearchSavedImages,
    BookmarkPathPrompt,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BookmarkPathAction {
    Export,
    Import,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum SearchTemplateKind {
    Model,
    Image,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TagViewerColumn {
    Include,
    Current,
    Exclude,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ModelSearchTemplate {
    pub name: String,
    pub state: PersistedModelFilterState,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ImageSearchTemplate {
    pub name: String,
    pub state: PersistedImageFilterState,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SearchTemplateStore {
    pub model_templates: Vec<ModelSearchTemplate>,
    pub image_templates: Vec<ImageSearchTemplate>,
}

pub enum AppMessage {
    ImagesLoaded(Vec<ImageItem>, bool, Option<u32>, Option<u64>),
    ImageDetailEnriched(ImageItem),
    ImageDecoded(u64, StatefulProtocol, Vec<u8>, String),
    ModelsSearchedChunk(Vec<Model>, bool, bool, Option<u32>),
    ModelDetailLoaded(Box<Model>, Option<u64>),
    ModelSidebarDetailLoaded(Box<Model>),
    ModelCoverDecoded(u64, StatefulProtocol, Vec<u8>, String),
    ModelCoversDecoded(u64, Vec<(StatefulProtocol, Vec<u8>)>, String),
    ModelCoverLoadFailed(u64),
    StatusUpdate(StatusEvent),
    DownloadStarted(DownloadKey, String, u64, Option<PathBuf>),
    DownloadProgress(DownloadKey, f64, u64, u64),
    DownloadPaused(DownloadKey),
    DownloadResumed(DownloadKey),
    DownloadCompleted(DownloadKey),
    DownloadFailed(DownloadKey, String),
    DownloadCancelled(DownloadKey),
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

pub struct NewDownloadHistoryEntry {
    pub model_id: u64,
    pub version_id: u64,
    pub filename: String,
    pub model_name: String,
    pub file_path: Option<PathBuf>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub status: DownloadHistoryStatus,
    pub progress: f64,
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
    PrefetchModelCovers(Vec<VersionCoverJob>, MediaRenderRequest),
    DownloadImage(ImageItem),
    DownloadModel(Box<Model>, u64, usize),
    PauseDownload(DownloadKey),
    ResumeDownload(DownloadKey),
    CancelDownload(DownloadKey),
    ResumeDownloadModel(u64, u64, Option<PathBuf>, u64, u64),
    Quit,
    UpdateConfig(crate::config::AppConfig),
}
