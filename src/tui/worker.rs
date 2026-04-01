use anyhow::Result;
use ratatui_image::picker::Picker;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, Semaphore, mpsc};

use crate::api::types::PaginationMetadata;
use crate::api::CivitaiClient;
use crate::config::AppConfig;
use civitai_cli::sdk::{
    ApiImageSearchOptions, DownloadControl, DownloadDestination, DownloadEvent, DownloadKind,
    DownloadOptions, DownloadSpec, ModelDownloadAuth, ModelSearchState, SdkClientBuilder,
    SearchModelHit as Model,
};
use crate::tui::app::{AppMessage, WorkerCommand};
use crate::tui::model::{
    ParsedModelFile, build_download_file_name, model_name, model_versions,
    resolve_download_target_dir,
};

type DownloadControlMap = HashMap<u64, mpsc::Sender<DownloadControl>>;
type SearchCache = HashMap<String, CachedSearchResult>;
type ImageSearchCache = HashMap<String, CachedImageSearchResult>;

#[derive(Serialize, Deserialize, Clone)]
struct CachedSearchResult {
    cache_key: String,
    models: Vec<Model>,
    has_more: bool,
    next_page: Option<u32>,
    cached_at_unix_secs: u64,
}

#[derive(Serialize, Deserialize, Clone)]
struct CachedImageSearchResult {
    cache_key: String,
    items: Vec<crate::api::ImageItem>,
    #[serde(default)]
    metadata: Option<PaginationMetadata>,
    cached_at_unix_secs: u64,
}

#[derive(Serialize, Deserialize, Clone)]
struct CachedBinaryBlob {
    cached_at_unix_secs: u64,
    bytes: Vec<u8>,
}

enum CoverQueueCommand {
    Enqueue(Vec<(u64, String)>),
    Prioritize(u64, Option<String>),
    Prefetch(Vec<(u64, Option<String>)>),
}

const MODEL_VERSION_COVER_IMAGE_LIMIT: usize = 1;

fn cache_key_for_options(opts: &ModelSearchState) -> String {
    let mut parts = Vec::new();
    parts.push(format!(
        "q={}",
        normalize_cache_segment(opts.query.as_deref().unwrap_or_default())
    ));
    parts.push(format!(
        "sort={}",
        normalize_cache_segment(opts.sort_by.to_query_value().as_ref())
    ));
    parts.push(format!(
        "page={}",
        opts.page.map(|value| value.to_string()).unwrap_or_default()
    ));
    parts.push(format!(
        "limit={}",
        opts.limit.map(|value| value.to_string()).unwrap_or_default()
    ));
    parts.push(format!(
        "base={}",
        opts.base_models
            .iter()
            .map(|value| value.as_query_value())
            .collect::<Vec<_>>()
            .join(",")
    ));
    parts.push(format!(
        "types={}",
        opts.types
            .iter()
            .map(|value| value.as_query_value())
            .collect::<Vec<_>>()
            .join(",")
    ));
    parts.push(format!(
        "created={}",
        normalize_cache_segment(opts.created_at.as_deref().unwrap_or_default())
    ));
    parts.join("|")
}

fn normalize_cache_segment(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_search_options_for_cache(mut opts: ModelSearchState) -> ModelSearchState {
    opts.query = opts
        .query
        .take()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty());
    opts.created_at = opts
        .created_at
        .take()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty());
    opts
}

fn build_search_url(opts: &ModelSearchState) -> String {
    opts.to_web_url("https://civitai.com/search/models")
        .map(|url| url.to_string())
        .unwrap_or_else(|_| "https://civitai.com/search/models".to_string())
}

fn build_image_search_url(opts: &crate::api::client::ImageSearchOptions) -> String {
    let mut url = format!("https://civitai.com/api/v1/images?limit={}", opts.limit);

    if let Some(nsfw) = &opts.nsfw {
        match nsfw.as_str() {
            "All" => {}
            "None" => url.push_str("&nsfw=false"),
            "Soft" | "Mature" | "X" => url.push_str("&nsfw=true"),
            other => url.push_str(&format!("&nsfw={}", other.replace(' ', "%20"))),
        }
    }
    if let Some(sort) = &opts.sort {
        url.push_str(&format!("&sort={}", sort.replace(' ', "%20")));
    }
    if let Some(period) = &opts.period {
        url.push_str(&format!("&period={}", period.replace(' ', "%20")));
    }
    if let Some(model_version_id) = opts.model_version_id {
        url.push_str(&format!("&modelVersionId={}", model_version_id));
    }
    if let Some(tags) = opts.tags {
        url.push_str(&format!("&tags={}", tags));
    }

    url
}

fn filter_image_items_by_requested_nsfw(
    items: Vec<crate::api::ImageItem>,
    requested_nsfw: Option<&str>,
) -> Vec<crate::api::ImageItem> {
    match requested_nsfw {
        None | Some("All") => items,
        Some("None") => items
            .into_iter()
            .filter(|item| item.nsfw_level.as_deref() == Some("None") || item.nsfw == Some(false))
            .collect(),
        Some(level @ ("Soft" | "Mature" | "X")) => items
            .into_iter()
            .filter(|item| item.nsfw_level.as_deref() == Some(level))
            .collect(),
        Some(_) => items,
    }
}

fn build_model_url(model: &Model, version_id: u64) -> String {
    build_model_page_url(model, Some(version_id))
}

fn build_model_page_url(model: &Model, version_id: Option<u64>) -> String {
    crate::tui::model::build_model_url(model, version_id)
}

fn status_with_url(action: &str, url: &str) -> String {
    format!("{} | {}", action, url)
}

fn error_status_with_url(action: &str, url: &str, err: &str) -> String {
    format!("{} | {} | {}", action, url, err)
}

fn estimated_file_size_bytes(file: &ParsedModelFile) -> Option<u64> {
    file.size_kb.and_then(|size_kb| {
        if size_kb.is_finite() && size_kb > 0.0 {
            Some((size_kb * 1024.0).round() as u64)
        } else {
            None
        }
    })
}

async fn forward_download_events(
    mut progress_rx: mpsc::Receiver<DownloadEvent>,
    tx_msg: mpsc::Sender<AppMessage>,
    model_id: u64,
    version_id: u64,
    model_name: String,
    filename: String,
) {
    while let Some(event) = progress_rx.recv().await {
        match event {
            DownloadEvent::Started {
                path,
                total_bytes,
                ..
            } => {
                let _ = tx_msg
                    .send(AppMessage::DownloadStarted(
                        model_id,
                        filename.clone(),
                        version_id,
                        model_name.clone(),
                        total_bytes.unwrap_or(0),
                        Some(path),
                    ))
                    .await;
            }
            DownloadEvent::Progress {
                downloaded_bytes,
                total_bytes,
                percent,
            } => {
                let _ = tx_msg
                    .send(AppMessage::DownloadProgress(
                        model_id,
                        filename.clone(),
                        percent.unwrap_or(0.0),
                        downloaded_bytes,
                        total_bytes.unwrap_or(0),
                    ))
                    .await;
            }
            DownloadEvent::Paused {
                downloaded_bytes,
                total_bytes,
            } => {
                let percent = total_bytes
                    .filter(|value| *value > 0)
                    .map(|value| (downloaded_bytes as f64 / value as f64) * 100.0)
                    .unwrap_or(0.0);
                let _ = tx_msg
                    .send(AppMessage::DownloadProgress(
                        model_id,
                        filename.clone(),
                        percent,
                        downloaded_bytes,
                        total_bytes.unwrap_or(0),
                    ))
                    .await;
                let _ = tx_msg.send(AppMessage::DownloadPaused(model_id)).await;
            }
            DownloadEvent::Resumed {
                downloaded_bytes,
                total_bytes,
            } => {
                let percent = total_bytes
                    .filter(|value| *value > 0)
                    .map(|value| (downloaded_bytes as f64 / value as f64) * 100.0)
                    .unwrap_or(0.0);
                let _ = tx_msg
                    .send(AppMessage::DownloadProgress(
                        model_id,
                        filename.clone(),
                        percent,
                        downloaded_bytes,
                        total_bytes.unwrap_or(0),
                    ))
                    .await;
                let _ = tx_msg.send(AppMessage::DownloadResumed(model_id)).await;
            }
            DownloadEvent::Completed {
                downloaded_bytes,
                total_bytes,
                ..
            }
            | DownloadEvent::Cancelled {
                downloaded_bytes,
                total_bytes,
                ..
            } => {
                let percent = total_bytes
                    .filter(|value| *value > 0)
                    .map(|value| (downloaded_bytes as f64 / value as f64) * 100.0)
                    .unwrap_or(0.0);
                let _ = tx_msg
                    .send(AppMessage::DownloadProgress(
                        model_id,
                        filename.clone(),
                        percent,
                        downloaded_bytes,
                        total_bytes.unwrap_or(0),
                    ))
                    .await;
            }
        }
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs()
}

fn is_cache_valid(cached_at_unix_secs: u64, ttl_hours: u64) -> bool {
    if ttl_hours == 0 {
        return false;
    }

    let ttl_secs = ttl_hours.saturating_mul(3600);
    now_unix_secs().saturating_sub(cached_at_unix_secs) < ttl_secs
}

fn is_cache_valid_minutes(cached_at_unix_secs: u64, ttl_minutes: u64) -> bool {
    if ttl_minutes == 0 {
        return false;
    }

    let ttl_secs = ttl_minutes.saturating_mul(60);
    now_unix_secs().saturating_sub(cached_at_unix_secs) < ttl_secs
}

fn is_cache_valid_minutes_or_persistent(cached_at_unix_secs: u64, ttl_minutes: u64) -> bool {
    if ttl_minutes == 0 {
        return true;
    }

    let ttl_secs = ttl_minutes.saturating_mul(60);
    now_unix_secs().saturating_sub(cached_at_unix_secs) < ttl_secs
}

fn model_search_cache_path(config: &AppConfig) -> Option<PathBuf> {
    config.search_cache_path()
}

fn model_cover_cache_root(config: &AppConfig) -> Option<PathBuf> {
    config.model_cover_cache_path()
}

fn image_search_cache_root(config: &AppConfig) -> Option<PathBuf> {
    config
        .search_cache_path()
        .map(|root| root.join("image_search"))
}

fn image_bytes_cache_root(config: &AppConfig) -> Option<PathBuf> {
    config.image_cache_path()
}

fn debug_fetch_log_path(config: &AppConfig) -> Option<PathBuf> {
    AppConfig::config_dir()
        .or_else(|| config.search_cache_path())
        .map(|dir| dir.join("fetch_debug.log"))
}

fn debug_fetch_log_to_file(log_path: Option<&Path>, message: &str) {
    if !cfg!(debug_assertions) {
        return;
    }

    let path = match log_path {
        Some(path) => path,
        None => return,
    };

    if let Some(parent) = path.parent() {
        let _ = create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|dur| dur.as_secs())
            .unwrap_or_default();
        let _ = writeln!(file, "[{}] {}", ts, message);
    }
}

fn debug_fetch_log(config: &AppConfig, message: &str) {
    let path = match debug_fetch_log_path(config) {
        Some(path) => path,
        None => return,
    };

    debug_fetch_log_to_file(Some(path.as_path()), message);
}

fn use_search_cache() -> bool {
    !cfg!(debug_assertions)
}

fn load_search_cache(path: Option<&Path>, ttl_hours: u64) -> SearchCache {
    let Some(path) = path else {
        return HashMap::new();
    };

    let cache_root = path.to_path_buf();
    if path.exists() && !path.is_dir() {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::create_dir_all(&cache_root);

    let mut cache = HashMap::new();
    let Ok(entries) = std::fs::read_dir(&cache_root) else {
        return cache;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let config = bincode::config::standard();
        let Ok((entry, _)) =
            bincode::serde::decode_from_slice::<CachedSearchResult, _>(&bytes, config)
        else {
            continue;
        };
        if !entry.cache_key.is_empty() {
            cache.insert(entry.cache_key.clone(), entry);
        }
    }
    let _ = prune_search_cache(&mut cache, ttl_hours);
    save_search_cache(Some(&cache_root), &cache, ttl_hours);
    cache
}

fn save_search_cache(path: Option<&Path>, cache: &SearchCache, ttl_hours: u64) {
    let Some(path) = path else {
        return;
    };

    if path.exists() && !path.is_dir() {
        let _ = std::fs::remove_file(path);
    }

    let mut normalized_cache = cache.to_owned();
    let _ = prune_search_cache(&mut normalized_cache, ttl_hours);
    let _ = std::fs::remove_dir_all(path);
    let _ = std::fs::create_dir_all(path);

    for entry in normalized_cache.values() {
        let data_path = cache_entry_path(path, &entry.cache_key);
        let config = bincode::config::standard();
        if let Ok(bytes) = bincode::serde::encode_to_vec(entry, config) {
            let _ = std::fs::write(data_path, bytes);
        }
    }
}

fn persist_search_cache_entry(cache_root: &Path, entry: &CachedSearchResult) {
    let _ = std::fs::create_dir_all(cache_root);
    let data_path = cache_entry_path(cache_root, &entry.cache_key);
    let config = bincode::config::standard();
    if let Ok(bytes) = bincode::serde::encode_to_vec(entry, config) {
        let _ = std::fs::write(data_path, bytes);
    }
}

fn prune_search_cache(cache: &mut SearchCache, ttl_hours: u64) -> usize {
    if ttl_hours == 0 {
        let removed = cache.len();
        cache.clear();
        return removed;
    }

    let now = now_unix_secs();
    let before = cache.len();
    cache.retain(|_, entry| {
        now.saturating_sub(entry.cached_at_unix_secs) < ttl_hours.saturating_mul(3600)
    });

    before.saturating_sub(cache.len())
}

fn cache_entry_path(cache_root: &Path, cache_key: &str) -> PathBuf {
    let key = cache_key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '=' || c == '_' || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in cache_key.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    cache_root.join(format!(
        "{:x}_{}.bin",
        hash,
        key.chars().take(32).collect::<String>()
    ))
}

fn clear_cached_search_cache(cache_root: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(cache_root) else {
        return 0;
    };
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            removed = removed.saturating_add(1);
        }
    }
    removed
}

fn model_cover_cache_path(cache_root: &Path, version_id: u64, image_url: &str) -> PathBuf {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in image_url.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let safe_name = image_url
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(24)
        .collect::<String>();
    cache_root.join(format!("v{version_id}_{hash:016x}_{safe_name}.bin"))
}

fn load_cached_model_cover(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        let _ = std::fs::remove_file(path);
        None
    } else {
        Some(bytes)
    }
}

fn decode_model_cover(
    bytes: &[u8],
    picker: &Picker,
) -> Option<ratatui_image::protocol::StatefulProtocol> {
    let img = image::load_from_memory(bytes).ok()?;
    Some(picker.new_resize_protocol(img))
}

fn persist_model_cover_cache(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, bytes);
}

fn load_cached_search_entry(cache_root: &Path, cache_key: &str) -> Option<CachedSearchResult> {
    let cache_path = cache_entry_path(cache_root, cache_key);
    let bytes = std::fs::read(cache_path).ok()?;
    let config = bincode::config::standard();
    bincode::serde::decode_from_slice::<CachedSearchResult, _>(&bytes, config)
        .ok()
        .map(|(entry, _)| entry)
}

fn has_more_from_response(limit: Option<u32>, offset: Option<u32>, total: Option<u64>) -> bool {
    let limit = limit.unwrap_or(50) as u64;
    let offset = offset.unwrap_or(0) as u64;
    total.is_some_and(|total| offset.saturating_add(limit) < total)
}

fn remove_cached_search_entry(cache_root: &Path, cache_key: &str) {
    let cache_path = cache_entry_path(cache_root, cache_key);
    let _ = std::fs::remove_file(cache_path);
}

fn load_image_search_cache(path: Option<&Path>, ttl_minutes: u64) -> ImageSearchCache {
    let Some(path) = path else {
        return HashMap::new();
    };

    let cache_root = path.to_path_buf();
    if path.exists() && !path.is_dir() {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::create_dir_all(&cache_root);

    let mut cache = HashMap::new();
    let Ok(entries) = std::fs::read_dir(&cache_root) else {
        return cache;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let config = bincode::config::standard();
        let Ok((entry, _)) =
            bincode::serde::decode_from_slice::<CachedImageSearchResult, _>(&bytes, config)
        else {
            continue;
        };
        if !entry.cache_key.is_empty() {
            cache.insert(entry.cache_key.clone(), entry);
        }
    }
    let _ = prune_image_search_cache(&mut cache, ttl_minutes);
    save_image_search_cache(Some(&cache_root), &cache, ttl_minutes);
    cache
}

fn save_image_search_cache(path: Option<&Path>, cache: &ImageSearchCache, ttl_minutes: u64) {
    let Some(path) = path else {
        return;
    };

    if path.exists() && !path.is_dir() {
        let _ = std::fs::remove_file(path);
    }

    let mut normalized_cache = cache.to_owned();
    let _ = prune_image_search_cache(&mut normalized_cache, ttl_minutes);
    let _ = std::fs::remove_dir_all(path);
    let _ = std::fs::create_dir_all(path);

    for entry in normalized_cache.values() {
        let data_path = cache_entry_path(path, &entry.cache_key);
        let config = bincode::config::standard();
        if let Ok(bytes) = bincode::serde::encode_to_vec(entry, config) {
            let _ = std::fs::write(data_path, bytes);
        }
    }
}

fn prune_image_search_cache(cache: &mut ImageSearchCache, ttl_minutes: u64) -> usize {
    if ttl_minutes == 0 {
        let removed = cache.len();
        cache.clear();
        return removed;
    }

    let now = now_unix_secs();
    let before = cache.len();
    cache.retain(|_, entry| {
        now.saturating_sub(entry.cached_at_unix_secs) < ttl_minutes.saturating_mul(60)
    });

    before.saturating_sub(cache.len())
}

fn load_cached_image_search_entry(
    cache_root: &Path,
    cache_key: &str,
) -> Option<CachedImageSearchResult> {
    let cache_path = cache_entry_path(cache_root, cache_key);
    let bytes = std::fs::read(cache_path).ok()?;
    let config = bincode::config::standard();
    bincode::serde::decode_from_slice::<CachedImageSearchResult, _>(&bytes, config)
        .ok()
        .map(|(entry, _)| entry)
}

fn image_bytes_cache_path(cache_root: &Path, image_id: u64, image_url: &str) -> PathBuf {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in image_url.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let safe_name = image_url
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(24)
        .collect::<String>();
    cache_root.join(format!("img{image_id}_{hash:016x}_{safe_name}.bin"))
}

fn load_cached_feed_image(path: &Path, ttl_minutes: u64) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    let config = bincode::config::standard();
    let Ok((entry, _)) = bincode::serde::decode_from_slice::<CachedBinaryBlob, _>(&bytes, config)
    else {
        let _ = std::fs::remove_file(path);
        return None;
    };

    if !is_cache_valid_minutes_or_persistent(entry.cached_at_unix_secs, ttl_minutes)
        || entry.bytes.is_empty()
    {
        let _ = std::fs::remove_file(path);
        None
    } else {
        Some(entry.bytes)
    }
}

fn persist_cached_feed_image(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let entry = CachedBinaryBlob {
        cached_at_unix_secs: now_unix_secs(),
        bytes: bytes.to_vec(),
    };
    let config = bincode::config::standard();
    if let Ok(encoded) = bincode::serde::encode_to_vec(&entry, config) {
        let _ = std::fs::write(path, encoded);
    }
}

fn pop_next_job(
    queue: &mut VecDeque<(u64, String)>,
    focus_version: Option<u64>,
) -> Option<(u64, String)> {
    if let Some(focus_version) = focus_version {
        if let Some(pos) = queue.iter().position(|(id, _)| *id == focus_version) {
            return queue.remove(pos);
        }
    }

    queue.pop_front()
}

fn upsert_job(
    queue: &mut VecDeque<(u64, String)>,
    queued_ids: &mut HashSet<u64>,
    version_id: u64,
    image_url: String,
    at_front: bool,
) {
    if let Some(pos) = queue.iter().position(|(id, _)| *id == version_id) {
        let _ = queue.remove(pos);
    } else {
        queued_ids.insert(version_id);
    }

    if at_front {
        queue.push_front((version_id, image_url));
    } else {
        queue.push_back((version_id, image_url));
    }
}

pub async fn spawn_worker(
    config: AppConfig,
) -> (mpsc::Sender<WorkerCommand>, mpsc::Receiver<AppMessage>) {
    let (tx_cmd, mut rx_cmd) = mpsc::channel::<WorkerCommand>(32);
    let (tx_msg, rx_msg) = mpsc::channel::<AppMessage>(32);
    let (cover_cmd_tx, mut cover_cmd_rx) = mpsc::channel::<CoverQueueCommand>(256);
    let (cover_done_tx, mut cover_done_rx) = mpsc::channel::<u64>(128);

    let mut downloader_config = config.clone();
    let cover_worker_config = downloader_config.clone();
    let search_cache_path = model_search_cache_path(&downloader_config);
    let image_search_cache_path = image_search_cache_root(&downloader_config);
    let search_cache_ttl_hours =
        Arc::new(Mutex::new(downloader_config.model_search_cache_ttl_hours));
    let image_search_cache_ttl_minutes =
        Arc::new(Mutex::new(downloader_config.image_search_cache_ttl_minutes));
    let image_cache_ttl_minutes = Arc::new(Mutex::new(downloader_config.image_cache_ttl_minutes));
    let model_cover_cache_path = Arc::new(Mutex::new(model_cover_cache_root(&downloader_config)));
    let image_bytes_cache_path = Arc::new(Mutex::new(image_bytes_cache_root(&downloader_config)));
    let search_cache = Arc::new(Mutex::new(if use_search_cache() {
        load_search_cache(
            search_cache_path.as_deref(),
            *search_cache_ttl_hours.lock().await,
        )
    } else {
        HashMap::new()
    }));
    let image_search_cache = Arc::new(Mutex::new(if use_search_cache() {
        load_image_search_cache(
            image_search_cache_path.as_deref(),
            *image_search_cache_ttl_minutes.lock().await,
        )
    } else {
        HashMap::new()
    }));
    let search_cache_path = Arc::new(Mutex::new(search_cache_path));
    let image_search_cache_path = Arc::new(Mutex::new(image_search_cache_path));
    let req_client = Client::builder().user_agent("civitai-cli").build().unwrap();

    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    tokio::spawn({
        let tx_msg = tx_msg.clone();
        let req_client = req_client.clone();
        let picker = picker.clone();
        let model_cover_cache_path = model_cover_cache_path.clone();
        let debug_config = cover_worker_config.clone();
        let api_client = {
            let builder = if let Some(api_key) = cover_worker_config.api_key.clone() {
                SdkClientBuilder::new().api_key(api_key)
            } else {
                SdkClientBuilder::new()
            };
            builder.build_api().unwrap()
        };
        async move {
            let mut queue: VecDeque<(u64, String)> = VecDeque::new();
            let mut queued_ids: HashSet<u64> = HashSet::new();
            let mut running_ids: HashSet<u64> = HashSet::new();
            let mut running_handles: HashMap<u64, tokio::task::JoinHandle<()>> = HashMap::new();
            let mut known_version_urls: HashMap<u64, String> = HashMap::new();
            let mut focus_version: Option<u64> = None;
            let max_in_flight = 3usize;

            let enqueue_or_bump_queue = |queue: &mut VecDeque<(u64, String)>,
                                         queued_ids: &mut HashSet<u64>,
                                         version_id: u64,
                                         image_url: String,
                                         at_front: bool| {
                upsert_job(queue, queued_ids, version_id, image_url, at_front);
            };

            loop {
                while running_handles.len() < max_in_flight {
                    let next_job = pop_next_job(&mut queue, focus_version);
                    let Some((version_id, image_url)) = next_job else {
                        break;
                    };

                    let _ = queued_ids.remove(&version_id);
                    running_ids.insert(version_id);

                    let tx_msg = tx_msg.clone();
                    let req_client = req_client.clone();
                    let picker = picker.clone();
                    let done_tx = cover_done_tx.clone();
                    let model_cover_cache_path = model_cover_cache_path.clone();
                    let use_cover_cache = use_search_cache();
                    let debug_config = debug_config.clone();

                    let handle = tokio::spawn(async move {
                        let cover_cache_root = model_cover_cache_path.lock().await.clone();
                        let result = load_model_cover_result(
                            version_id,
                            image_url,
                            req_client,
                            picker,
                            cover_cache_root,
                            debug_config,
                            use_cover_cache,
                        )
                        .await;
                        let _ = tx_msg.send(result).await;
                        let _ = done_tx.send(version_id).await;
                    });
                    running_handles.insert(version_id, handle);
                }

                if queue.is_empty() && running_handles.is_empty() {
                    let Some(cmd) = cover_cmd_rx.recv().await else {
                        break;
                    };
                    match cmd {
                        CoverQueueCommand::Enqueue(jobs) => {
                            for (version_id, image_url) in jobs {
                                known_version_urls.insert(version_id, image_url.clone());

                                if running_ids.contains(&version_id) {
                                    continue;
                                }

                                if queued_ids.contains(&version_id) {
                                    continue;
                                }

                                enqueue_or_bump_queue(
                                    &mut queue,
                                    &mut queued_ids,
                                    version_id,
                                    image_url,
                                    focus_version == Some(version_id),
                                );
                            }
                        }
                        CoverQueueCommand::Prefetch(jobs) => {
                            for (version_id, image_url) in jobs {
                                if running_ids.contains(&version_id) || queued_ids.contains(&version_id) {
                                    continue;
                                }

                                let resolved_url = if let Some(url) =
                                    image_url.or_else(|| known_version_urls.get(&version_id).cloned())
                                {
                                    Some(url)
                                } else {
                                    fetch_cover_urls_for_version(&api_client, version_id)
                                        .await
                                        .into_iter()
                                        .next()
                                };

                                if let Some(url) = resolved_url {
                                    known_version_urls.insert(version_id, url.clone());
                                    enqueue_or_bump_queue(
                                        &mut queue,
                                        &mut queued_ids,
                                        version_id,
                                        url,
                                        false,
                                    );
                                }
                            }
                        }
                        CoverQueueCommand::Prioritize(version_id, image_url) => {
                            focus_version = Some(version_id);
                            let mut resolved_urls =
                                fetch_cover_urls_for_version(&api_client, version_id).await;
                            if resolved_urls.is_empty() {
                                if let Some(url) =
                                    image_url.or_else(|| known_version_urls.get(&version_id).cloned())
                                {
                                    resolved_urls.push(url);
                                }
                            }

                            if let Some(first_url) = resolved_urls.first().cloned() {
                                known_version_urls.insert(version_id, first_url);
                            }

                            if !resolved_urls.is_empty() {
                                if let Some(handle) = running_handles.remove(&version_id) {
                                    handle.abort();
                                }
                                let _ = running_ids.remove(&version_id);
                                queued_ids.remove(&version_id);
                                queue.retain(|(queued_version_id, _)| *queued_version_id != version_id);

                                let tx_msg = tx_msg.clone();
                                let req_client = req_client.clone();
                                let picker = picker.clone();
                                let done_tx = cover_done_tx.clone();
                                let model_cover_cache_path = model_cover_cache_path.clone();
                                let use_cover_cache = use_search_cache();
                                let debug_config = debug_config.clone();

                                running_ids.insert(version_id);
                                let handle = tokio::spawn(async move {
                                    let cover_cache_root = model_cover_cache_path.lock().await.clone();
                                    let result = load_model_cover_results(
                                        version_id,
                                        resolved_urls,
                                        req_client,
                                        picker,
                                        cover_cache_root,
                                        debug_config,
                                        use_cover_cache,
                                    )
                                    .await;
                                    let _ = tx_msg.send(result).await;
                                    let _ = done_tx.send(version_id).await;
                                });
                                running_handles.insert(version_id, handle);
                            }

                            if running_ids.len() >= max_in_flight
                                && !running_ids.contains(&version_id)
                            {
                                let to_pause = running_ids
                                    .iter()
                                    .copied()
                                    .find(|id| Some(*id) != focus_version);
                                if let Some(pause_id) = to_pause {
                                    if let Some(handle) = running_handles.remove(&pause_id) {
                                        handle.abort();
                                    }
                                    let _ = running_ids.remove(&pause_id);

                                    if let Some(url) = known_version_urls.get(&pause_id).cloned() {
                                        enqueue_or_bump_queue(
                                            &mut queue,
                                            &mut queued_ids,
                                            pause_id,
                                            url,
                                            false,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }

                tokio::select! {
                    Some(cmd) = cover_cmd_rx.recv() => {
                        match cmd {
                            CoverQueueCommand::Enqueue(jobs) => {
                                for (version_id, image_url) in jobs {
                                    known_version_urls.insert(version_id, image_url.clone());

                                    if running_ids.contains(&version_id) {
                                        continue;
                                    }

                                    if queued_ids.contains(&version_id) {
                                        continue;
                                    }

                                    enqueue_or_bump_queue(
                                        &mut queue,
                                        &mut queued_ids,
                                        version_id,
                                        image_url,
                                        false,
                                    );
                                }
                            }
                            CoverQueueCommand::Prefetch(jobs) => {
                                for (version_id, image_url) in jobs {
                                    if running_ids.contains(&version_id) || queued_ids.contains(&version_id) {
                                        continue;
                                    }

                                    let resolved_url = if let Some(url) =
                                        image_url.or_else(|| known_version_urls.get(&version_id).cloned())
                                    {
                                        Some(url)
                                    } else {
                                        fetch_cover_urls_for_version(&api_client, version_id)
                                            .await
                                            .into_iter()
                                            .next()
                                    };

                                    if let Some(url) = resolved_url {
                                        known_version_urls.insert(version_id, url.clone());
                                        enqueue_or_bump_queue(
                                            &mut queue,
                                            &mut queued_ids,
                                            version_id,
                                            url,
                                            false,
                                        );
                                    }
                                }
                            }
                            CoverQueueCommand::Prioritize(version_id, image_url) => {
                                focus_version = Some(version_id);
                                let mut resolved_urls =
                                    fetch_cover_urls_for_version(&api_client, version_id).await;
                                if resolved_urls.is_empty() {
                                    if let Some(url) =
                                        image_url.or_else(|| known_version_urls.get(&version_id).cloned())
                                    {
                                        resolved_urls.push(url);
                                    }
                                }

                                if let Some(first_url) = resolved_urls.first().cloned() {
                                    known_version_urls.insert(version_id, first_url);
                                }

                                if !resolved_urls.is_empty() {
                                    if let Some(handle) = running_handles.remove(&version_id) {
                                        handle.abort();
                                    }
                                    let _ = running_ids.remove(&version_id);
                                    queued_ids.remove(&version_id);
                                    queue.retain(|(queued_version_id, _)| *queued_version_id != version_id);

                                    let tx_msg = tx_msg.clone();
                                    let req_client = req_client.clone();
                                    let picker = picker.clone();
                                    let done_tx = cover_done_tx.clone();
                                    let model_cover_cache_path = model_cover_cache_path.clone();
                                    let use_cover_cache = use_search_cache();
                                    let debug_config = debug_config.clone();

                                    running_ids.insert(version_id);
                                    let handle = tokio::spawn(async move {
                                        let cover_cache_root = model_cover_cache_path.lock().await.clone();
                                        let result = load_model_cover_results(
                                            version_id,
                                            resolved_urls,
                                            req_client,
                                            picker,
                                            cover_cache_root,
                                            debug_config,
                                            use_cover_cache,
                                        )
                                        .await;
                                        let _ = tx_msg.send(result).await;
                                        let _ = done_tx.send(version_id).await;
                                    });
                                    running_handles.insert(version_id, handle);
                                }

                                if running_ids.len() >= max_in_flight && !running_ids.contains(&version_id) {
                                    let to_pause = running_ids
                                        .iter()
                                        .copied()
                                        .find(|id| Some(*id) != focus_version);
                                    if let Some(pause_id) = to_pause {
                                        if let Some(handle) = running_handles.remove(&pause_id) {
                                            handle.abort();
                                        }
                                        let _ = running_ids.remove(&pause_id);

                                        if let Some(url) = known_version_urls.get(&pause_id).cloned() {
                                            enqueue_or_bump_queue(
                                                &mut queue,
                                                &mut queued_ids,
                                                pause_id,
                                                url,
                                                false,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(done_version_id) = cover_done_rx.recv() => {
                        let _ = running_ids.remove(&done_version_id);
                        let _ = running_handles.remove(&done_version_id);
                    }
                }
            }
        }
    });

    tokio::spawn(async move {
        let download_controls: Arc<Mutex<DownloadControlMap>> =
            Arc::new(Mutex::new(HashMap::new()));

        while let Some(cmd) = rx_cmd.recv().await {
            match cmd {
                WorkerCommand::FetchImages(image_opts, next_page_url) => {
                    let tx_msg_clone = tx_msg.clone();
                    let req_client = req_client.clone();
                    let picker = picker.clone();
                    let api_key = downloader_config.api_key.clone();
                    let debug_config = downloader_config.clone();
                    let image_search_cache = image_search_cache.clone();
                    let image_search_cache_path = image_search_cache_path.clone();
                    let image_bytes_cache_path = image_bytes_cache_path.clone();
                    let image_search_cache_ttl_minutes = image_search_cache_ttl_minutes.clone();
                    let image_cache_ttl_minutes = image_cache_ttl_minutes.clone();

                    tokio::spawn(async move {
                        debug_fetch_log(&debug_config, "FetchImages: started");
                        let client = match CivitaiClient::new(api_key) {
                            Ok(client) => client,
                            Err(e) => {
                                debug_fetch_log(
                                    &debug_config,
                                    &format!("FetchImages: image API client init failed: {}", e),
                                );
                                let _ = tx_msg_clone
                                    .send(AppMessage::StatusUpdate(format!(
                                        "Error initializing image API client: {}",
                                        e
                                    )))
                                    .await;
                                return;
                            }
                        };

                        let use_next_page = next_page_url.filter(|url| !url.trim().is_empty());
                        let is_append = use_next_page.is_some();
                        let requested_nsfw = image_opts.nsfw.clone();
                        let current_url =
                            use_next_page.unwrap_or_else(|| build_image_search_url(&image_opts));
                        let image_search_ttl_minutes = *image_search_cache_ttl_minutes.lock().await;
                        let image_cache_ttl_minutes = *image_cache_ttl_minutes.lock().await;
                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate(status_with_url(
                                if is_append {
                                    "Fetching image feed next page"
                                } else {
                                    "Fetching image feed"
                                },
                                &current_url,
                            )))
                            .await;
                        debug_fetch_log(
                            &debug_config,
                            &format!("FetchImages: request -> {}", current_url),
                        );

                        let cached_response = if use_search_cache() {
                            let cache_key = current_url.clone();
                            let cache_root = image_search_cache_path.lock().await.clone();
                            let mut cache = image_search_cache.lock().await;
                            let removed =
                                prune_image_search_cache(&mut cache, image_search_ttl_minutes);
                            if removed > 0 && cache_root.is_some() {
                                save_image_search_cache(
                                    cache_root.as_deref(),
                                    &cache,
                                    image_search_ttl_minutes,
                                );
                            }

                            let mut entry = cache.get(&cache_key).cloned();
                            if entry.is_none() {
                                if let Some(cache_root) = cache_root.as_deref() {
                                    if let Some(on_disk_entry) =
                                        load_cached_image_search_entry(cache_root, &cache_key)
                                    {
                                        if is_cache_valid_minutes(
                                            on_disk_entry.cached_at_unix_secs,
                                            image_search_ttl_minutes,
                                        ) {
                                            cache.insert(cache_key.clone(), on_disk_entry.clone());
                                            entry = Some(on_disk_entry);
                                        }
                                    }
                                }
                            }

                            match entry {
                                Some(entry)
                                    if is_cache_valid_minutes(
                                        entry.cached_at_unix_secs,
                                        image_search_ttl_minutes,
                                    ) =>
                                {
                                    let _ = tx_msg_clone
                                        .send(AppMessage::StatusUpdate(status_with_url(
                                            "Loaded cached image feed",
                                            &current_url,
                                        )))
                                        .await;
                                    Some(entry)
                                }
                                Some(_) => {
                                    cache.remove(&cache_key);
                                    if let Some(cache_root) = cache_root.as_deref() {
                                        let _ = std::fs::remove_file(cache_entry_path(
                                            cache_root, &cache_key,
                                        ));
                                        save_image_search_cache(
                                            Some(cache_root),
                                            &cache,
                                            image_search_ttl_minutes,
                                        );
                                    }
                                    None
                                }
                                None => None,
                            }
                        } else {
                            None
                        };

                        let fetch_result = if let Some(entry) = cached_response {
                            Ok(crate::api::ImageResponse {
                                items: entry.items,
                                metadata: entry.metadata,
                            })
                        } else {
                            client.get_images_by_url(current_url.clone()).await
                        };

                        let (visible_items, final_next_page) = match fetch_result {
                            Ok(res) => {
                                let filtered_items = filter_image_items_by_requested_nsfw(
                                    res.items,
                                    requested_nsfw.as_deref(),
                                );
                                let metadata = res.metadata.clone();
                                let next_page = metadata
                                    .as_ref()
                                    .and_then(|metadata| metadata.next_page.clone());
                                let filtered_count = filtered_items.len();
                                let visible_items = filtered_items
                                    .clone()
                                    .into_iter()
                                    .filter(|item| item.r#type.as_deref() != Some("video"))
                                    .collect::<Vec<_>>();
                                let skipped_videos =
                                    filtered_count.saturating_sub(visible_items.len());

                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "FetchImages: response -> visible_items={}, skipped_videos={}, next_page_present={}",
                                        visible_items.len(),
                                        skipped_videos,
                                        next_page.is_some()
                                    ),
                                );

                                if use_search_cache() {
                                    let cache_root = image_search_cache_path.lock().await.clone();
                                    if cache_root.is_some() && image_search_ttl_minutes > 0 {
                                        let mut cache = image_search_cache.lock().await;
                                        cache.insert(
                                            current_url.clone(),
                                            CachedImageSearchResult {
                                                cache_key: current_url.clone(),
                                                items: filtered_items,
                                                metadata,
                                                cached_at_unix_secs: now_unix_secs(),
                                            },
                                        );
                                        save_image_search_cache(
                                            cache_root.as_deref(),
                                            &cache,
                                            image_search_ttl_minutes,
                                        );
                                    }
                                }

                                (visible_items, next_page)
                            }
                            Err(e) => {
                                debug_fetch_log(
                                    &debug_config,
                                    &format!("FetchImages: get_images failed: {}", e),
                                );
                                let _ = tx_msg_clone
                                    .send(AppMessage::StatusUpdate(format!(
                                        "{}",
                                        error_status_with_url(
                                            "Error fetching images",
                                            &current_url,
                                            &e.to_string(),
                                        )
                                    )))
                                    .await;
                                return;
                            }
                        };

                        let _ = tx_msg_clone
                            .send(AppMessage::ImagesLoaded(
                                visible_items.clone(),
                                is_append,
                                final_next_page,
                            ))
                            .await;

                        let fetch_semaphore = Arc::new(Semaphore::new(3));
                        let mut handles = Vec::with_capacity(visible_items.len());
                        for item in visible_items {
                            debug_fetch_log(
                                &debug_config,
                                &format!("FetchImages: enqueue image id={}", item.id),
                            );
                            let image_bytes_cache_root =
                                image_bytes_cache_path.lock().await.clone();
                            handles.push(tokio::spawn(load_feed_image(
                                item.id,
                                item.url,
                                req_client.clone(),
                                picker.clone(),
                                tx_msg_clone.clone(),
                                fetch_semaphore.clone(),
                                debug_config.clone(),
                                image_bytes_cache_root,
                                image_cache_ttl_minutes,
                                use_search_cache(),
                            )));
                        }
                        for handle in handles {
                            let _ = handle.await;
                        }
                    });
                }
                WorkerCommand::SearchModels(
                    opts,
                    preferred_model_id,
                    preferred_version_id,
                    force_refresh,
                    append,
                    next_page_index,
                ) => {
                    let tx_msg_clone = tx_msg.clone();
                    let cover_cmd_tx = cover_cmd_tx.clone();
                    let opts = normalize_search_options_for_cache(opts);
                    let sdk_clone = {
                        let builder = if let Some(api_key) = downloader_config.api_key.clone() {
                            SdkClientBuilder::new().api_key(api_key)
                        } else {
                            SdkClientBuilder::new()
                        };
                        builder.build_web().unwrap()
                    };
                    let search_cache = search_cache.clone();
                    let search_cache_ttl_hours = search_cache_ttl_hours.clone();
                    let search_cache_path = search_cache_path.clone();
                    let debug_config = downloader_config.clone();
                    let use_cache = use_search_cache();

                    tokio::spawn(async move {
                        let query_label = opts
                            .query
                            .clone()
                            .unwrap_or_else(|| "<default>".to_string());
                        let effective_force_refresh = force_refresh || !use_cache;
                        let request_state = if let Some(page) = next_page_index {
                            let mut next = opts.clone();
                            next.page = Some(page);
                            next
                        } else {
                            opts.clone()
                        };
                        let request_url = build_search_url(&request_state);

                        debug_fetch_log(
                            &debug_config,
                            &format!(
                                "SearchModels: request limit={} query=\"{}\" sort={:?} type={:?} base={:?} force_refresh={} append={} cache_used={} next_page_url={:?}",
                                request_state.limit.unwrap_or(50),
                                query_label,
                                request_state.sort_by,
                                request_state.types,
                                request_state.base_models,
                                effective_force_refresh,
                                append,
                                use_cache,
                                next_page_index,
                            ),
                        );

                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate(format!(
                                "Loading cache lookup for \"{}\"...",
                                query_label
                            )))
                            .await;

                        let cache_key = cache_key_for_options(&opts);
                        let ttl_hours = *search_cache_ttl_hours.lock().await;
                        let cache_path = search_cache_path.lock().await.clone();
                        let mut cached_slice: Option<Vec<Model>> = None;
                        let mut cached_has_more = false;
                        let mut cached_next_page: Option<u32> = None;

                        if !effective_force_refresh && use_cache && next_page_index.is_none() {
                            let mut cache = search_cache.lock().await;
                            let removed = prune_search_cache(&mut cache, ttl_hours);
                            if removed > 0 && cache_path.is_some() {
                                save_search_cache(cache_path.as_deref(), &cache, ttl_hours);
                            }

                            let mut entry = cache.get(&cache_key).cloned();
                            if entry.is_none() {
                                if let Some(cache_root) = cache_path.as_deref() {
                                    let _ = tx_msg_clone
                                        .send(AppMessage::StatusUpdate(format!(
                                            "Checking on-disk cache for \"{}\"",
                                            query_label
                                        )))
                                        .await;
                                    if let Some(on_disk_entry) =
                                        load_cached_search_entry(cache_root, &cache_key)
                                    {
                                        if is_cache_valid(
                                            on_disk_entry.cached_at_unix_secs,
                                            ttl_hours,
                                        ) {
                                            cache.insert(cache_key.clone(), on_disk_entry.clone());
                                            entry = Some(on_disk_entry);
                                            debug_fetch_log(
                                                &debug_config,
                                                &format!(
                                                    "SearchModels: on-disk cache hit for \"{}\"",
                                                    query_label
                                                ),
                                            );
                                            let _ = tx_msg_clone
                                                .send(AppMessage::StatusUpdate(format!(
                                                    "Loaded on-disk cached results for \"{}\"",
                                                    query_label
                                                )))
                                                .await;
                                        } else {
                                            debug_fetch_log(
                                                &debug_config,
                                                &format!(
                                                    "SearchModels: on-disk cache expired for \"{}\"",
                                                    query_label
                                                ),
                                            );
                                            let _ = tx_msg_clone
                                                .send(AppMessage::StatusUpdate(format!(
                                                    "Cached file expired for \"{}\", refreshing",
                                                    query_label
                                                )))
                                                .await;
                                            remove_cached_search_entry(cache_root, &cache_key);
                                        }
                                    }
                                }
                            }

                            if let Some(entry) = entry {
                                if is_cache_valid(entry.cached_at_unix_secs, ttl_hours) {
                                    cached_next_page = entry.next_page;
                                    if !append {
                                        cached_slice = Some(entry.models.clone());
                                        cached_has_more = entry.has_more;
                                        debug_fetch_log(
                                            &debug_config,
                                            &format!(
                                                "SearchModels: in-memory cache full hit for query=\"{}\"",
                                                query_label
                                            ),
                                        );
                                        let _ = tx_msg_clone
                                            .send(AppMessage::StatusUpdate(format!(
                                                "Using in-memory cached results for \"{}\"",
                                                query_label
                                            )))
                                            .await;
                                    } else {
                                        debug_fetch_log(
                                            &debug_config,
                                            &format!(
                                                "SearchModels: append request bypasses in-memory slice cache for query=\"{}\"",
                                                query_label
                                            ),
                                        );
                                    }
                                } else {
                                    let _ = tx_msg_clone
                                        .send(AppMessage::StatusUpdate(format!(
                                            "Cached results expired for \"{}\", refreshing...",
                                            query_label
                                        )))
                                        .await;
                                    cache.remove(&cache_key);
                                    debug_fetch_log(
                                        &debug_config,
                                        &format!(
                                            "SearchModels: cached entry expired and removed for \"{}\"",
                                            query_label
                                        ),
                                    );
                                }
                            } else {
                                debug_fetch_log(
                                    &debug_config,
                                    &format!("SearchModels: no cache hit for \"{}\"", query_label),
                                );
                                let _ = tx_msg_clone
                                    .send(AppMessage::StatusUpdate(format!(
                                        "No cached results for \"{}\"",
                                        query_label
                                    )))
                                    .await;
                            }
                        } else {
                            let _ =
                                tx_msg_clone
                                    .send(AppMessage::StatusUpdate(format!(
                                        "{}",
                                        if use_cache {
                                            format!(
                                                "Bypassing cache for \"{}\" due to manual refresh",
                                                query_label
                                            )
                                        } else {
                                            format!(
                                                "Debug mode: cache disabled; fetching from network",
                                            )
                                        }
                                    )))
                                    .await;
                            debug_fetch_log(
                                &debug_config,
                                &format!(
                                    "SearchModels: {} for \"{}\"",
                                    if use_cache {
                                        "cache bypassed"
                                    } else {
                                        "cache disabled"
                                    },
                                    query_label
                                ),
                            );
                        }

                        if let Some(cached) = cached_slice {
                            debug_fetch_log(
                                &debug_config,
                                &format!(
                                    "SearchModels: emitting cached chunk append={} count={} has_more={} preferred_model={:?} preferred_version={:?}",
                                    append,
                                    cached.len(),
                                    cached_has_more,
                                    preferred_model_id,
                                    preferred_version_id
                                ),
                            );
                            let _ = tx_msg_clone
                                .send(AppMessage::ModelsSearchedChunk(
                                    cached.clone(),
                                    append,
                                    cached_has_more,
                                    cached_next_page,
                                ))
                                .await;

                            let mut jobs = Vec::new();
                            let mut preferred_url = None;

                            for model in &cached {
                                for version in model_versions(model) {
                                    if let Some(image_url) =
                                        version.images.first().map(|image| image.url.clone())
                                    {
                                        jobs.push((version.id, image_url.clone()));
                                        if Some(model.id) == preferred_model_id
                                            && Some(version.id) == preferred_version_id
                                        {
                                            preferred_url = Some(image_url);
                                        }
                                    }
                                }
                            }

                            let _ = cover_cmd_tx.send(CoverQueueCommand::Enqueue(jobs)).await;
                            if let Some(version_id) = preferred_version_id {
                                let _ = cover_cmd_tx
                                    .send(CoverQueueCommand::Prioritize(version_id, preferred_url))
                                    .await;
                            }

                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(format!(
                                    "Loaded {} cached models",
                                    cached.len()
                                )))
                                .await;
                            return;
                        }

                        if effective_force_refresh {
                            {
                                let mut cache = search_cache.lock().await;
                                cache.remove(&cache_key);
                                if let Some(cache_root) = cache_path.as_deref() {
                                    remove_cached_search_entry(cache_root, &cache_key);
                                }
                            }
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(format!(
                                    "Cache skipped, fetching models for \"{}\"",
                                    query_label
                                )))
                                .await;
                        }

                        if next_page_index.is_some() {
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(status_with_url(
                                    &format!("Fetching next models page for '{}'", query_label),
                                    &request_url,
                                )))
                                .await;
                            debug_fetch_log(
                                &debug_config,
                                &format!("SearchModels request url={}", request_url),
                            );
                        } else {
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(status_with_url(
                                    &format!("Fetching models matching '{}'", query_label),
                                    &request_url,
                                )))
                                .await;
                            debug_fetch_log(
                                &debug_config,
                                &format!("SearchModels request url={}", request_url),
                            );
                        }

                        let fetch_result = sdk_clone.search_models(&request_state).await;

                        match fetch_result {
                            Ok(res) => {
                                let has_more = has_more_from_response(
                                    res.limit,
                                    res.offset,
                                    res.estimated_total_hits,
                                );
                                let next_page =
                                    has_more.then_some(request_state.page.unwrap_or(0).saturating_add(1));
                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "SearchModels response query=\"{}\" count={} has_more={} next_page={} items={}",
                                        query_label,
                                        res.hits.len(),
                                        has_more,
                                        next_page
                                            .map(|value| value.to_string())
                                            .unwrap_or_else(|| "<none>".to_string()),
                                        res.estimated_total_hits
                                            .map(|value| value.to_string())
                                            .unwrap_or_else(|| "unknown".to_string()),
                                    ),
                                );
                                let models = res.hits.clone();
                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "SearchModels: emitting network chunk append={} count={} has_more={} next_page={:?} preferred_model={:?} preferred_version={:?}",
                                        append,
                                        models.len(),
                                        has_more,
                                        next_page,
                                        preferred_model_id,
                                        preferred_version_id
                                    ),
                                );
                                let _ = tx_msg_clone
                                    .send(AppMessage::ModelsSearchedChunk(
                                        models.clone(),
                                        append,
                                        has_more,
                                        next_page,
                                    ))
                                    .await;
                                let _ = tx_msg_clone
                                    .send(AppMessage::StatusUpdate(status_with_url(
                                        &format!(
                                            "Fetched {} models for \"{}\"",
                                            models.len(),
                                            query_label
                                        ),
                                        &request_url,
                                    )))
                                    .await;

                                let mut jobs = Vec::new();
                                let mut preferred_url = None;

                                for model in &models {
                                    for version in model_versions(model) {
                                        if let Some(image_url) =
                                            version.images.first().map(|image| image.url.clone())
                                        {
                                            jobs.push((version.id, image_url.clone()));
                                            if Some(model.id) == preferred_model_id
                                                && Some(version.id) == preferred_version_id
                                            {
                                                preferred_url = Some(image_url);
                                            }
                                        }
                                    }
                                }

                                let _ = cover_cmd_tx.send(CoverQueueCommand::Enqueue(jobs)).await;
                                if let Some(version_id) = preferred_version_id {
                                    let _ = cover_cmd_tx
                                        .send(CoverQueueCommand::Prioritize(
                                            version_id,
                                            preferred_url,
                                        ))
                                        .await;
                                }

                                if ttl_hours > 0 && use_cache {
                                    debug_fetch_log(
                                        &debug_config,
                                        &format!(
                                            "SearchModels: persist cache query=\"{}\" append={} count={}",
                                            query_label,
                                            append,
                                            models.len()
                                        ),
                                    );
                                    let mut cache = search_cache.lock().await;
                                    let cache_key = cache_key.clone();
                                    let entry = cache.entry(cache_key.clone()).or_insert(
                                        CachedSearchResult {
                                            cache_key: cache_key.clone(),
                                            models: Vec::new(),
                                            has_more,
                                            next_page,
                                            cached_at_unix_secs: now_unix_secs(),
                                        },
                                    );
                                    if append {
                                        let mut seen_ids =
                                            entry.models.iter().map(|model| model.id).collect::<HashSet<_>>();
                                        for model in models {
                                            if seen_ids.insert(model.id) {
                                                entry.models.push(model);
                                            }
                                        }
                                    } else {
                                        entry.models = models;
                                    }
                                    entry.has_more = has_more;
                                    entry.next_page = next_page;
                                    entry.cached_at_unix_secs = now_unix_secs();
                                    if let Some(cache_root) = cache_path.as_deref() {
                                        persist_search_cache_entry(cache_root, entry);
                                    }
                                }
                            }
                            Err(e) => {
                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "SearchModels: network fetch failed query=\"{}\", err={}",
                                        query_label, e
                                    ),
                                );
                                let _ = tx_msg_clone
                                    .send(AppMessage::StatusUpdate(error_status_with_url(
                                        "Search failed",
                                        &request_url,
                                        &e.to_string(),
                                    )))
                                    .await;
                            }
                        }
                    });
                }
                WorkerCommand::ClearSearchCache => {
                    if !use_search_cache() {
                        let _ = tx_msg
                            .send(AppMessage::StatusUpdate(
                                "Debug mode: search cache is disabled, nothing to clear.".into(),
                            ))
                            .await;
                        continue;
                    }
                    let cache_path = search_cache_path.lock().await.clone();
                    let mut cleared = {
                        let mut cache = search_cache.lock().await;
                        let count = cache.len();
                        cache.clear();
                        count
                    };

                    if let Some(cache_root) = cache_path.as_deref() {
                        cleared = cleared.saturating_add(clear_cached_search_cache(cache_root));
                    }
                    debug_fetch_log(
                        &downloader_config,
                        &format!("Search cache clear requested, removed={} entries", cleared),
                    );
                    let _ = tx_msg
                        .send(AppMessage::StatusUpdate(format!(
                            "Cleared {} cached search item(s)",
                            cleared
                        )))
                        .await;
                }
                WorkerCommand::PrioritizeModelCover(version_id, image_url) => {
                    let _ = cover_cmd_tx
                        .send(CoverQueueCommand::Prioritize(version_id, image_url))
                        .await;
                }
                WorkerCommand::PrefetchModelCovers(jobs) => {
                    let _ = cover_cmd_tx.send(CoverQueueCommand::Prefetch(jobs)).await;
                }
                WorkerCommand::UpdateConfig(new_cfg) => {
                    let new_key = new_cfg.api_key.clone();
                    match CivitaiClient::new(new_key) {
                        Ok(_) => {
                            downloader_config = new_cfg;
                            let ttl_hours = downloader_config.model_search_cache_ttl_hours;
                            let image_search_ttl_minutes_value =
                                downloader_config.image_search_cache_ttl_minutes;
                            let image_cache_ttl_minutes_value =
                                downloader_config.image_cache_ttl_minutes;
                            {
                                let mut ttl_hours_ref = search_cache_ttl_hours.lock().await;
                                *ttl_hours_ref = ttl_hours;
                            }
                            {
                                let mut ttl_ref = image_search_cache_ttl_minutes.lock().await;
                                *ttl_ref = image_search_ttl_minutes_value;
                            }
                            {
                                let mut ttl_ref = image_cache_ttl_minutes.lock().await;
                                *ttl_ref = image_cache_ttl_minutes_value;
                            }

                            let new_cache_path = model_search_cache_path(&downloader_config);
                            let new_image_search_cache_path =
                                image_search_cache_root(&downloader_config);
                            let new_cover_cache_path =
                                model_cover_cache_root(&downloader_config);
                            let new_image_bytes_cache_path =
                                image_bytes_cache_root(&downloader_config);
                            {
                                let mut cache_path = search_cache_path.lock().await;
                                let mut cache = search_cache.lock().await;
                                let mut image_cache_path = image_search_cache_path.lock().await;
                                let mut image_cache = image_search_cache.lock().await;
                                if *cache_path != new_cache_path {
                                    *cache_path = new_cache_path.clone();
                                    if use_search_cache() {
                                        *cache =
                                            load_search_cache(new_cache_path.as_deref(), ttl_hours);
                                    } else {
                                        cache.clear();
                                    }
                                } else if use_search_cache() {
                                    let _ = prune_search_cache(&mut cache, ttl_hours);
                                    save_search_cache(cache_path.as_deref(), &cache, ttl_hours);
                                } else {
                                    cache.clear();
                                }

                                if *image_cache_path != new_image_search_cache_path {
                                    *image_cache_path = new_image_search_cache_path.clone();
                                    if use_search_cache() {
                                        *image_cache = load_image_search_cache(
                                            new_image_search_cache_path.as_deref(),
                                            image_search_ttl_minutes_value,
                                        );
                                    } else {
                                        image_cache.clear();
                                    }
                                } else if use_search_cache() {
                                    let _ = prune_image_search_cache(
                                        &mut image_cache,
                                        image_search_ttl_minutes_value,
                                    );
                                    save_image_search_cache(
                                        image_cache_path.as_deref(),
                                        &image_cache,
                                        image_search_ttl_minutes_value,
                                    );
                                } else {
                                    image_cache.clear();
                                }

                                let mut cover_cache_path = model_cover_cache_path.lock().await;
                                *cover_cache_path = new_cover_cache_path;
                                let mut image_bytes_path = image_bytes_cache_path.lock().await;
                                *image_bytes_path = new_image_bytes_cache_path;
                            }

                            let _ = tx_msg
                                .send(AppMessage::StatusUpdate(
                                    "Configuration sync applied to worker".into(),
                                ))
                                .await;
                        }
                        Err(err) => {
                            let _ = tx_msg
                                .send(AppMessage::StatusUpdate(format!(
                                    "Failed to update worker API config: {}",
                                    err
                                )))
                                .await;
                        }
                    }
                }
                WorkerCommand::DownloadModelForImage(image_id) => {
                    let tx_msg_clone = tx_msg.clone();
                    tokio::spawn(async move {
                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate(format!(
                                "Inspecting image {} for linked model...",
                                image_id
                            )))
                            .await;
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate(
                                "Download finished (Placeholder)!".into(),
                            ))
                            .await;
                    });
                }
                WorkerCommand::DownloadModel(model_hit, version_id, file_index) => {
                    let tx_msg_clone = tx_msg.clone();
                    let download_client = {
                        let builder = if let Some(api_key) = downloader_config.api_key.clone() {
                            SdkClientBuilder::new().api_key(api_key)
                        } else {
                            SdkClientBuilder::new()
                        };
                        builder.build_download().unwrap()
                    };
                    let (control_tx, control_rx) = mpsc::channel(32);
                    {
                        let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                            download_controls.lock().await;
                        controls.insert(model_hit.id, control_tx.clone());
                    }
                    let control_map = download_controls.clone();
                    let config = downloader_config.clone();

                    tokio::spawn(async move {
                        let model_id = model_hit.id;
                        let model_url = build_model_url(&model_hit, version_id);
                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate(status_with_url(
                                &format!("Preparing download for {}", model_name(&model_hit)),
                                &model_url,
                            )))
                            .await;

                        if let Some(version) =
                            model_versions(&model_hit).into_iter().find(|item| item.id == version_id)
                        {
                            let selected_file = version
                                .files
                                .get(file_index)
                                .or_else(|| version.files.iter().find(|file| file.primary))
                                .or_else(|| version.files.first());
                            let filename = selected_file
                                .map(|file| build_download_file_name(&model_hit, &version, file))
                                .unwrap_or_else(|| model_hit.default_download_file_name());
                            let target_dir = resolve_download_target_dir(&config, &model_hit);
                            let target_path = target_dir.join(&filename);
                            let _estimated_size_bytes = selected_file
                                .and_then(estimated_file_size_bytes)
                                .unwrap_or(0);
                            let auth = config
                                .api_key
                                .clone()
                                .map(ModelDownloadAuth::QueryToken);
                            let download_url = selected_file
                                .and_then(|file| file.download_url.clone())
                                .unwrap_or_else(|| match auth.as_ref() {
                                    Some(ModelDownloadAuth::QueryToken(token)) => {
                                        download_client
                                            .build_model_download_url_with_token(version_id, token)
                                    }
                                    _ => download_client.build_model_download_url(version_id),
                                });
                            let spec = DownloadSpec::new(download_url, DownloadKind::Model)
                                .with_file_name(filename.clone());
                            let options = DownloadOptions {
                                destination: DownloadDestination::File(target_path.clone()),
                                overwrite: true,
                                resume: true,
                                create_parent_dirs: true,
                                progress_step_percent: 1.0,
                            };
                            let (progress_tx, progress_rx) = mpsc::channel(64);
                            tokio::spawn(forward_download_events(
                                progress_rx,
                                tx_msg_clone.clone(),
                                model_id,
                                version_id,
                                model_name(&model_hit),
                                filename.clone(),
                            ));
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(format!(
                                    "Starting download stream for {}",
                                    model_name(&model_hit)
                                )))
                                .await;
                            let result = download_client
                                .download(&spec, &options, Some(progress_tx), Some(control_rx))
                                .await;
                            match result {
                                Ok(_) => {
                                    let _ = tx_msg_clone
                                        .send(AppMessage::DownloadCompleted(model_id))
                                        .await;
                                }
                                Err(err) if err.to_string().contains("cancelled") => {
                                    let _ = tx_msg_clone
                                        .send(AppMessage::DownloadCancelled(model_id))
                                        .await;
                                }
                                Err(err) => {
                                    let _ = tx_msg_clone
                                        .send(AppMessage::DownloadFailed(model_id, err.to_string()))
                                        .await;
                                }
                            }
                        } else {
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(error_status_with_url(
                                    &format!("Failed to resolve version {} for model {}", version_id, model_id),
                                    &model_url,
                                    "selected version not found",
                                )))
                                .await;
                        }
                        {
                            let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                                control_map.lock().await;
                            controls.remove(&model_id);
                        }
                    });
                }
                WorkerCommand::ResumeDownloadModel(
                    model_id,
                    version_id,
                    resume_file_path,
                    resume_downloaded_bytes,
                    resume_total_bytes,
                ) => {
                    let tx_msg_clone = tx_msg.clone();
                    let download_client = {
                        let builder = if let Some(api_key) = downloader_config.api_key.clone() {
                            SdkClientBuilder::new().api_key(api_key)
                        } else {
                            SdkClientBuilder::new()
                        };
                        builder.build_download().unwrap()
                    };
                    let (control_tx, control_rx) = mpsc::channel(32);
                    {
                        let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                            download_controls.lock().await;
                        controls.insert(model_id, control_tx.clone());
                    }
                    let control_map = download_controls.clone();
                    let config = downloader_config.clone();

                    tokio::spawn(async move {
                        let model_url = format!(
                            "https://civitai.com/api/download/models/{}",
                            version_id
                        );
                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate(status_with_url(
                                &format!("Resuming download for model {}", model_id),
                                &model_url,
                            )))
                            .await;

                        let fallback_name = format!("civitai-model-v{version_id}");
                        let filename = resume_file_path
                            .as_ref()
                            .and_then(|path| path.file_name().map(|value| value.to_string_lossy().to_string()))
                            .filter(|value| !value.trim().is_empty())
                            .unwrap_or(fallback_name);
                        let target_path = resume_file_path.clone().unwrap_or_else(|| {
                            std::env::current_dir()
                                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                                .join(&filename)
                        });
                        let auth = config
                            .api_key
                            .clone()
                            .map(ModelDownloadAuth::QueryToken);
                        let download_url = match auth.as_ref() {
                            Some(ModelDownloadAuth::QueryToken(token)) => {
                                download_client.build_model_download_url_with_token(version_id, token)
                            }
                            _ => download_client.build_model_download_url(version_id),
                        };
                        let spec = DownloadSpec::new(download_url, DownloadKind::Model)
                            .with_file_name(filename.clone());
                        let options = DownloadOptions {
                            destination: DownloadDestination::File(target_path.clone()),
                            overwrite: true,
                            resume: true,
                            create_parent_dirs: true,
                            progress_step_percent: 1.0,
                        };
                        let (progress_tx, progress_rx) = mpsc::channel(64);
                        tokio::spawn(forward_download_events(
                            progress_rx,
                            tx_msg_clone.clone(),
                            model_id,
                            version_id,
                            format!("Model {}", model_id),
                            filename.clone(),
                        ));
                        let total_bytes = (resume_total_bytes > 0).then_some(resume_total_bytes);
                        let _ = tx_msg_clone
                            .send(AppMessage::DownloadStarted(
                                model_id,
                                filename.clone(),
                                version_id,
                                format!("Model {}", model_id),
                                total_bytes.unwrap_or(0),
                                Some(target_path.clone()),
                            ))
                            .await;
                        if resume_downloaded_bytes > 0 {
                            let percent = total_bytes
                                .filter(|value| *value > 0)
                                .map(|value| (resume_downloaded_bytes as f64 / value as f64) * 100.0)
                                .unwrap_or(0.0);
                            let _ = tx_msg_clone
                                .send(AppMessage::DownloadProgress(
                                    model_id,
                                    filename.clone(),
                                    percent,
                                    resume_downloaded_bytes,
                                    total_bytes.unwrap_or(0),
                                ))
                                .await;
                        }

                        let result = download_client
                            .download(&spec, &options, Some(progress_tx), Some(control_rx))
                            .await;
                        match result {
                            Ok(_) => {
                                let _ = tx_msg_clone
                                    .send(AppMessage::DownloadCompleted(model_id))
                                    .await;
                            }
                            Err(err) if err.to_string().contains("cancelled") => {
                                let _ = tx_msg_clone
                                    .send(AppMessage::DownloadCancelled(model_id))
                                    .await;
                            }
                            Err(err) => {
                                let _ = tx_msg_clone
                                    .send(AppMessage::DownloadFailed(model_id, err.to_string()))
                                    .await;
                            }
                        }

                        {
                            let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                                control_map.lock().await;
                            controls.remove(&model_id);
                        }
                    });
                }
                WorkerCommand::PauseDownload(model_id) => {
                    let control = {
                        let controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                            download_controls.lock().await;
                        controls.get(&model_id).cloned()
                    };
                    if let Some(control) = control {
                        let _ = control.try_send(DownloadControl::Pause);
                        let _ = tx_msg.send(AppMessage::DownloadPaused(model_id)).await;
                    }
                }
                WorkerCommand::ResumeDownload(model_id) => {
                    let control = {
                        let controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                            download_controls.lock().await;
                        controls.get(&model_id).cloned()
                    };
                    if let Some(control) = control {
                        let _ = control.try_send(DownloadControl::Resume);
                        let _ = tx_msg.send(AppMessage::DownloadResumed(model_id)).await;
                    }
                }
                WorkerCommand::CancelDownload(model_id) => {
                    let control = {
                        let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                            download_controls.lock().await;
                        controls.remove(&model_id)
                    };
                    if let Some(control) = control {
                        let _ = control.try_send(DownloadControl::Cancel);
                        let _ = tx_msg.send(AppMessage::DownloadCancelled(model_id)).await;
                    }
                }
                WorkerCommand::Quit => break,
            }
        }
    });

    (tx_cmd, rx_msg)
}

async fn fetch_image_bytes_with_debug(
    client: &Client,
    url: &str,
    debug_config: &AppConfig,
    context: &str,
) -> Result<bytes::Bytes> {
    debug_fetch_log(debug_config, &format!("{} request -> {}", context, url));
    let start = Instant::now();
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(err) => {
            debug_fetch_log(
                debug_config,
                &format!("{} request failed: {}", context, err),
            );
            return Err(anyhow::anyhow!(err));
        }
    };
    let elapsed_ms = start.elapsed().as_millis();
    let status = response.status();
    debug_fetch_log(
        debug_config,
        &format!(
            "{} response -> status={} elapsed_ms={}",
            context, status, elapsed_ms
        ),
    );
    if !status.is_success() {
        response
            .error_for_status_ref()
            .map_err(|err| anyhow::anyhow!(err))?;
    }
    let bytes = response.bytes().await?;
    debug_fetch_log(
        debug_config,
        &format!("{} response body size={} bytes", context, bytes.len()),
    );
    Ok(bytes)
}

async fn load_model_cover_result(
    version_id: u64,
    image_url: String,
    client: Client,
    picker: Picker,
    cover_cache_root: Option<PathBuf>,
    debug_config: AppConfig,
    use_cache: bool,
) -> AppMessage {
    if use_cache {
        if let Some(cache_root) = cover_cache_root.as_ref() {
            let cache_path = model_cover_cache_path(cache_root, version_id, &image_url);
            if let Some(bytes) = load_cached_model_cover(&cache_path) {
                if let Some(protocol) = decode_model_cover(&bytes, &picker) {
                    debug_fetch_log(
                        &debug_config,
                        &format!(
                            "Model cover loaded from cache: version_id={}, url={}",
                            version_id, image_url
                        ),
                    );
                    return AppMessage::ModelCoverDecoded(version_id, protocol);
                }

                let _ = std::fs::remove_file(cache_path);
            }
        }
    }

    match fetch_image_bytes_with_debug(&client, &image_url, &debug_config, "Model cover").await {
        Ok(bytes) => {
            debug_fetch_log(
                &debug_config,
                &format!(
                    "Model cover fetched: version_id={}, bytes={}",
                    version_id,
                    bytes.len()
                ),
            );
            if let Some(protocol) = decode_model_cover(&bytes, &picker) {
                if use_cache {
                    if let Some(cache_root) = cover_cache_root.as_ref() {
                        let cache_path = model_cover_cache_path(cache_root, version_id, &image_url);
                        persist_model_cover_cache(&cache_path, &bytes);
                    }
                }
                return AppMessage::ModelCoverDecoded(version_id, protocol);
            }
            debug_fetch_log(
                &debug_config,
                &format!(
                    "Model cover decode failed: version_id={}, url={}",
                    version_id, image_url
                ),
            );
        }
        Err(e) => {
            debug_fetch_log(
                &debug_config,
                &format!(
                    "Model cover fetch failed: version_id={}, url={}, err={}",
                    version_id, image_url, e
                ),
            );
        }
    }

    AppMessage::ModelCoverLoadFailed(version_id)
}

async fn load_model_cover_results(
    version_id: u64,
    image_urls: Vec<String>,
    client: Client,
    picker: Picker,
    cover_cache_root: Option<PathBuf>,
    debug_config: AppConfig,
    use_cache: bool,
) -> AppMessage {
    let mut protocols = Vec::new();

    for image_url in image_urls {
        if use_cache {
            if let Some(cache_root) = cover_cache_root.as_ref() {
                let cache_path = model_cover_cache_path(cache_root, version_id, &image_url);
                if let Some(bytes) = load_cached_model_cover(&cache_path) {
                    if let Some(protocol) = decode_model_cover(&bytes, &picker) {
                        protocols.push(protocol);
                        continue;
                    }
                    let _ = std::fs::remove_file(cache_path);
                }
            }
        }

        match fetch_image_bytes_with_debug(&client, &image_url, &debug_config, "Model cover").await {
            Ok(bytes) => {
                if let Some(protocol) = decode_model_cover(&bytes, &picker) {
                    if use_cache {
                        if let Some(cache_root) = cover_cache_root.as_ref() {
                            let cache_path = model_cover_cache_path(cache_root, version_id, &image_url);
                            persist_model_cover_cache(&cache_path, &bytes);
                        }
                    }
                    protocols.push(protocol);
                }
            }
            Err(e) => {
                debug_fetch_log(
                    &debug_config,
                    &format!(
                        "Model cover fetch failed: version_id={}, url={}, err={}",
                        version_id, image_url, e
                    ),
                );
            }
        }
    }

    if protocols.is_empty() {
        AppMessage::ModelCoverLoadFailed(version_id)
    } else {
        AppMessage::ModelCoversDecoded(version_id, protocols)
    }
}

async fn fetch_cover_urls_for_version(
    api_client: &civitai_cli::sdk::ApiClient,
    version_id: u64,
) -> Vec<String> {
    let response = api_client
        .search_images(&ApiImageSearchOptions {
            limit: Some(MODEL_VERSION_COVER_IMAGE_LIMIT as u32),
            sort: Some("Most Collected".to_string()),
            model_version_id: Some(version_id),
            ..Default::default()
        })
        .await
        .ok();

    let Some(response) = response else {
        return Vec::new();
    };

    response
        .items
        .into_iter()
        .filter_map(|item| (!item.url.trim().is_empty()).then_some(item.url))
        .take(MODEL_VERSION_COVER_IMAGE_LIMIT)
        .collect()
}

async fn load_feed_image(
    image_id: u64,
    image_url: String,
    client: Client,
    picker: Picker,
    tx_msg: mpsc::Sender<AppMessage>,
    semaphore: Arc<Semaphore>,
    debug_config: AppConfig,
    image_bytes_cache_root: Option<PathBuf>,
    ttl_minutes: u64,
    use_cache: bool,
) {
    if use_cache {
        if let Some(cache_root) = image_bytes_cache_root.as_ref() {
            let cache_path = image_bytes_cache_path(cache_root, image_id, &image_url);
            if let Some(bytes) = load_cached_feed_image(&cache_path, ttl_minutes) {
                debug_fetch_log(
                    &debug_config,
                    &format!(
                        "Image feed loaded from cache: id={}, url={}",
                        image_id, image_url
                    ),
                );
                if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                    let protocol = picker.new_resize_protocol(dyn_img);
                    let _ = tx_msg
                        .send(AppMessage::ImageDecoded(image_id, protocol))
                        .await;
                    return;
                }
                let _ = std::fs::remove_file(cache_path);
            }
        }
    }

    let _permit = match semaphore.acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => return,
    };

    match fetch_image_bytes_with_debug(&client, &image_url, &debug_config, "Image feed").await {
        Ok(bytes) => {
            debug_fetch_log(
                &debug_config,
                &format!("Image feed fetched: id={}, bytes={}", image_id, bytes.len()),
            );
            if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                if use_cache {
                    if let Some(cache_root) = image_bytes_cache_root.as_ref() {
                        let cache_path = image_bytes_cache_path(cache_root, image_id, &image_url);
                        persist_cached_feed_image(&cache_path, &bytes);
                    }
                }
                let protocol = picker.new_resize_protocol(dyn_img);
                let _ = tx_msg
                    .send(AppMessage::ImageDecoded(image_id, protocol))
                    .await;
            } else {
                debug_fetch_log(
                    &debug_config,
                    &format!("Image feed decode failed: id={}", image_id),
                );
            }
        }
        Err(e) => {
            debug_fetch_log(
                &debug_config,
                &format!(
                    "Image feed fetch failed: id={}, url={}, err={}",
                    image_id, image_url, e
                ),
            );
        }
    }
}
