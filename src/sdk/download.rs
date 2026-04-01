use anyhow::{Context, Result};
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_RANGE};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use super::image_search::SearchImageHit;
use super::model_search::{ModelDownloadAuth, SearchModelHit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadKind {
    Model,
    Image,
    Video,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadSpec {
    pub url: String,
    pub kind: DownloadKind,
    pub file_name: Option<String>,
    pub auth: Option<ModelDownloadAuth>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadEvent {
    Started {
        path: PathBuf,
        total_bytes: Option<u64>,
        resumed: bool,
    },
    Progress {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        percent: Option<f64>,
    },
    Paused {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Resumed {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Completed {
        path: PathBuf,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Cancelled {
        path: PathBuf,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadControl {
    Pause,
    Resume,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadDestination {
    File(PathBuf),
    Directory(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadOptions {
    pub destination: DownloadDestination,
    pub overwrite: bool,
    pub resume: bool,
    pub create_parent_dirs: bool,
    pub progress_step_percent: f64,
}

impl DownloadOptions {
    pub fn to_file(path: impl Into<PathBuf>) -> Self {
        Self {
            destination: DownloadDestination::File(path.into()),
            ..Self::default()
        }
    }

    pub fn to_directory(path: impl Into<PathBuf>) -> Self {
        Self {
            destination: DownloadDestination::Directory(path.into()),
            ..Self::default()
        }
    }
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            destination: DownloadDestination::Directory(PathBuf::from(".")),
            overwrite: true,
            resume: true,
            create_parent_dirs: true,
            progress_step_percent: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadResult {
    pub path: PathBuf,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub resumed: bool,
    pub content_type: Option<String>,
}

impl DownloadSpec {
    pub fn new(url: impl Into<String>, kind: DownloadKind) -> Self {
        Self {
            url: url.into(),
            kind,
            file_name: None,
            auth: None,
        }
    }

    pub fn with_file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }

    pub fn with_auth(mut self, auth: ModelDownloadAuth) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn suggested_file_name(&self) -> String {
        self.file_name.clone().unwrap_or_else(|| {
            file_name_from_url(&self.url).unwrap_or_else(|| match self.kind {
                DownloadKind::Model => "civitai-model-download.bin".to_string(),
                DownloadKind::Image => "civitai-image-download.bin".to_string(),
                DownloadKind::Video => "civitai-video-download.bin".to_string(),
                DownloadKind::Other => "civitai-download.bin".to_string(),
            })
        })
    }
}

impl SearchImageHit {
    pub fn download_kind(&self) -> DownloadKind {
        match self.r#type.as_deref() {
            Some(kind) if kind.eq_ignore_ascii_case("video") => DownloadKind::Video,
            Some(kind) if kind.eq_ignore_ascii_case("image") => DownloadKind::Image,
            _ => DownloadKind::Image,
        }
    }

    pub fn default_download_file_name(&self) -> String {
        match self.download_kind() {
            DownloadKind::Video => format!("civitai-video-{}", self.id),
            _ => format!("civitai-image-{}", self.id),
        }
    }
}

impl SearchModelHit {
    pub fn default_download_file_name(&self) -> String {
        let base = self
            .name
            .as_deref()
            .map(sanitize_file_name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("civitai-model-{}", self.id));
        match self.primary_model_version_id() {
            Some(version_id) => format!("{base}-v{version_id}"),
            None => base,
        }
    }
}

pub(crate) fn sanitize_file_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn file_name_from_url(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let segment = parsed.path_segments()?.next_back()?;
    if segment.is_empty() {
        return None;
    }
    Some(segment.to_string())
}

pub(crate) fn file_name_from_content_disposition(
    value: Option<&reqwest::header::HeaderValue>,
) -> Option<String> {
    let raw = value?.to_str().ok()?;
    for part in raw.split(';') {
        let trimmed = part.trim();
        if let Some(file_name) = trimmed.strip_prefix("filename=") {
            return Some(file_name.trim_matches('"').to_string());
        }
    }
    None
}

pub(crate) async fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create parent directory for {}", path.display()))?;
    }
    Ok(())
}

pub(crate) async fn emit_event(tx: &Option<mpsc::Sender<DownloadEvent>>, event: DownloadEvent) {
    if let Some(sender) = tx {
        let _ = sender.send(event).await;
    }
}

pub(crate) fn range_total_from_header(value: Option<&reqwest::header::HeaderValue>) -> Option<u64> {
    value
        .and_then(|header| header.to_str().ok())
        .and_then(|raw| raw.split('/').next_back())
        .and_then(|value| value.parse::<u64>().ok())
}

pub(crate) fn authorization_header_value(token: &str) -> Result<reqwest::header::HeaderValue> {
    reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
        .context("Failed to build authorization header")
}

pub(crate) fn content_disposition_file_name(
    headers: &reqwest::header::HeaderMap,
) -> Option<String> {
    file_name_from_content_disposition(headers.get(CONTENT_DISPOSITION))
}

pub(crate) fn content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    range_total_from_header(headers.get(CONTENT_RANGE))
}
