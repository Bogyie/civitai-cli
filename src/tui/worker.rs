use anyhow::Result;
use image::imageops::FilterType;
use ratatui_image::picker::Picker;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex, Semaphore};

use crate::api::{CivitaiClient, Model};
use crate::api::types::PaginationMetadata;
use crate::config::AppConfig;
use crate::download::manager::DownloadControl;
use crate::download::DownloadManager;
use crate::tui::app::{AppMessage, WorkerCommand};

type DownloadControlMap = HashMap<u64, mpsc::Sender<DownloadControl>>;
type SearchCache = HashMap<String, CachedSearchResult>;

#[derive(Serialize, Deserialize, Clone)]
struct CachedSearchResult {
    cache_key: String,
    models: Vec<Model>,
    #[serde(default)]
    metadata: Option<PaginationMetadata>,
    cached_at_unix_secs: u64,
}

enum CoverQueueCommand {
    Enqueue(Vec<(u64, String)>),
    Prioritize(u64, Option<String>),
}

fn cache_key_for_options(opts: &crate::api::client::SearchOptions) -> String {
    let query = normalize_cache_segment(opts.query.as_str());
    let types = opts
        .types
        .as_deref()
        .map(normalize_cache_segment)
        .unwrap_or_else(|| "all".to_string());
    let tag = opts
        .tag
        .as_deref()
        .map(normalize_cache_segment)
        .unwrap_or_else(|| "all".to_string());
    let username = opts
        .username
        .as_deref()
        .map(normalize_cache_segment)
        .unwrap_or_else(|| "all".to_string());
    let sort = opts
        .sort
        .as_deref()
        .map(normalize_cache_segment)
        .unwrap_or_else(|| "all".to_string());
    let base_models = opts
        .base_models
        .as_deref()
        .map(normalize_cache_segment)
        .unwrap_or_else(|| "all".to_string());
    format!(
        "q={}|tag={}|user={}|t={}|sort={}|base={}",
        query,
        tag,
        username,
        types,
        sort,
        base_models
    )
}

fn normalize_cache_segment(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_search_options_for_cache(mut opts: crate::api::client::SearchOptions) -> crate::api::client::SearchOptions {
    opts.query = opts.query.trim().to_string();
    let normalize = |value: Option<String>| {
        value
            .map(|raw| raw.trim().to_string())
            .filter(|raw| !raw.is_empty())
    };

    opts.types = normalize(opts.types);
    opts.tag = normalize(opts.tag);
    opts.username = normalize(opts.username);
    opts.sort = normalize(opts.sort);
    opts.period = normalize(opts.period);
    opts.allow_commercial_use = normalize(opts.allow_commercial_use);
    opts.base_models = normalize(opts.base_models);
    opts
}

fn build_search_url(opts: &crate::api::client::SearchOptions) -> String {
    let mut url = format!("https://civitai.com/api/v1/models?limit={}", opts.limit);

    if let Some(page) = opts.page {
        if page > 0 {
            url.push_str(&format!("&page={}", page));
        }
    }

    if !opts.query.is_empty() {
        url.push_str(&format!("&query={}", opts.query.replace(' ', "%20")));
    }

    if let Some(tag) = &opts.tag {
        if !tag.is_empty() {
            url.push_str(&format!("&tag={}", tag.replace(' ', "%20")));
        }
    }

    if let Some(username) = &opts.username {
        if !username.is_empty() {
            url.push_str(&format!("&username={}", username.replace(' ', "%20")));
        }
    }

    if let Some(sort) = &opts.sort {
        if sort != "All" {
            url.push_str(&format!("&sort={}", sort.replace(' ', "%20")));
        }
    }

    if let Some(types) = &opts.types {
        if types != "All" {
            url.push_str(&format!("&types={}", types.replace(' ', "%20")));
        }
    }

    if let Some(period) = &opts.period {
        if !period.is_empty() {
            url.push_str(&format!("&period={}", period.replace(' ', "%20")));
        }
    }

    if let Some(allow_commercial_use) = &opts.allow_commercial_use {
        if !allow_commercial_use.is_empty() {
            url.push_str(&format!(
                "&allowCommercialUse={}",
                allow_commercial_use.replace(' ', "%20")
            ));
        }
    }

    if let Some(base_models) = &opts.base_models {
        if base_models != "All" {
            url.push_str(&format!("&baseModels={}", base_models.replace(' ', "%20")));
        }
    }

    if let Some(rating) = opts.rating {
        url.push_str(&format!("&rating={}", rating));
    }

    if let Some(favorites) = opts.favorites {
        url.push_str(&format!("&favorites={}", favorites));
    }

    if let Some(hidden) = opts.hidden {
        url.push_str(&format!("&hidden={}", hidden));
    }

    if let Some(primary_file_only) = opts.primary_file_only {
        url.push_str(&format!("&primaryFileOnly={}", primary_file_only));
    }

    if let Some(allow_no_credit) = opts.allow_no_credit {
        url.push_str(&format!("&allowNoCredit={}", allow_no_credit));
    }

    if let Some(allow_derivatives) = opts.allow_derivatives {
        url.push_str(&format!("&allowDerivatives={}", allow_derivatives));
    }

    if let Some(allow_different_licenses) = opts.allow_different_licenses {
        url.push_str(&format!("&allowDifferentLicenses={}", allow_different_licenses));
    }

    if let Some(nsfw) = opts.nsfw {
        url.push_str(&format!("&nsfw={}", nsfw));
    }

    if let Some(supports_generation) = opts.supports_generation {
        url.push_str(&format!("&supportsGeneration={}", supports_generation));
    }

    if let Some(ids) = &opts.ids {
        if !ids.is_empty() {
            let joined = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
            url.push_str(&format!("&ids={}", joined));
        }
    }

    url
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

fn model_search_cache_path(config: &AppConfig) -> Option<PathBuf> {
    config.search_cache_path()
}

fn model_cover_cache_root(config: &AppConfig) -> Option<PathBuf> {
    config.model_cover_cache_path()
}

fn debug_fetch_log_path(config: &AppConfig) -> Option<PathBuf> {
    AppConfig::config_dir().or_else(|| config.search_cache_path()).map(|dir| {
        dir.join("fetch_debug.log")
    })
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
    let key = cache_key.chars().map(|c| {
        if c.is_ascii_alphanumeric() || c == '=' || c == '_' || c == '.' || c == '-' {
            c
        } else {
            '_'
        }
    }).collect::<String>();
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in cache_key.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    cache_root.join(format!("{:x}_{}.bin", hash, key.chars().take(32).collect::<String>()))
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
    let resized = img.resize(600, 600, FilterType::Triangle);
    Some(picker.new_resize_protocol(resized))
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

fn parse_metadata_u64(value: Option<&str>) -> Option<u64> {
    value.and_then(|raw| raw.trim().parse::<u64>().ok())
}

fn has_more_from_metadata(metadata: Option<&PaginationMetadata>, current_page: u32) -> bool {
    let Some(metadata) = metadata else {
        return false;
    };

    if metadata
        .next_page
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }

    if let Some(total_pages) = parse_metadata_u64(metadata.total_pages.as_deref()) {
        return (current_page as u64) < total_pages;
    }

    let Some(total_items) = parse_metadata_u64(metadata.total_items.as_deref()) else {
        return false;
    };
    let Some(page_size) = parse_metadata_u64(metadata.page_size.as_deref()).filter(|size| *size > 0) else {
        return false;
    };
    let estimated_pages = (total_items + page_size - 1) / page_size;
    (current_page as u64) < estimated_pages
}

fn remove_cached_search_entry(cache_root: &Path, cache_key: &str) {
    let cache_path = cache_entry_path(cache_root, cache_key);
    let _ = std::fs::remove_file(cache_path);
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
    let search_cache_ttl_hours = Arc::new(Mutex::new(downloader_config.model_search_cache_ttl_hours));
    let model_cover_cache_path = Arc::new(Mutex::new(model_cover_cache_root(&downloader_config)));
    let search_cache = Arc::new(Mutex::new(if use_search_cache() {
        load_search_cache(search_cache_path.as_deref(), *search_cache_ttl_hours.lock().await)
    } else {
        HashMap::new()
    }));
    let search_cache_path = Arc::new(Mutex::new(search_cache_path));
    let req_client = Client::builder().user_agent("civitai-cli").build().unwrap();

    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    tokio::spawn({
                    let tx_msg = tx_msg.clone();
                    let req_client = req_client.clone();
                    let picker = picker.clone();
                    let model_cover_cache_path = model_cover_cache_path.clone();
                    let debug_config = cover_worker_config.clone();
                    async move {
            let mut queue: VecDeque<(u64, String)> = VecDeque::new();
            let mut queued_ids: HashSet<u64> = HashSet::new();
            let mut running_ids: HashSet<u64> = HashSet::new();
            let mut running_handles: HashMap<u64, tokio::task::JoinHandle<()>> = HashMap::new();
            let mut known_version_urls: HashMap<u64, String> = HashMap::new();
            let mut focus_version: Option<u64> = None;
            let max_in_flight = 3usize;

            let enqueue_or_bump_queue =
                |queue: &mut VecDeque<(u64, String)>,
                 queued_ids: &mut HashSet<u64>,
                 version_id: u64,
                 image_url: String,
                 at_front: bool| {
                    upsert_job(queue, queued_ids, version_id, image_url, at_front);
                };

            loop {
                while running_handles.len() < max_in_flight {
                    let next_job = pop_next_job(&mut queue, focus_version);
                    let Some((version_id, image_url)) = next_job else { break };

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
                    let Some(cmd) = cover_cmd_rx.recv().await else { break };
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
                        CoverQueueCommand::Prioritize(version_id, image_url) => {
                            focus_version = Some(version_id);

                            if let Some(url) =
                                image_url.or_else(|| known_version_urls.get(&version_id).cloned())
                            {
                                known_version_urls.insert(version_id, url.clone());
                                if !running_ids.contains(&version_id) && !queued_ids.contains(&version_id) {
                                    enqueue_or_bump_queue(&mut queue, &mut queued_ids, version_id, url, true);
                                }
                            }

                            if running_ids.len() >= max_in_flight && !running_ids.contains(&version_id) {
                                let to_pause = running_ids.iter().copied().find(|id| Some(*id) != focus_version);
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
                            CoverQueueCommand::Prioritize(version_id, image_url) => {
                                focus_version = Some(version_id);

                                if let Some(url) = image_url.or_else(|| known_version_urls.get(&version_id).cloned()) {
                                    known_version_urls.insert(version_id, url.clone());
                                    if !running_ids.contains(&version_id) && !queued_ids.contains(&version_id) {
                                        enqueue_or_bump_queue(
                                            &mut queue,
                                            &mut queued_ids,
                                            version_id,
                                            url,
                                            true,
                                        );
                                    }
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
                WorkerCommand::FetchImages => {
                    let tx_msg_clone = tx_msg.clone();
                    let req_client = req_client.clone();
                    let picker = picker.clone();
                    let api_key = downloader_config.api_key.clone();
                    let debug_config = downloader_config.clone();

                    tokio::spawn(async move {
                        debug_fetch_log(&debug_config, "FetchImages: started");
                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate("Fetching feed...".into()))
                            .await;

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

                        let image_feed_url =
                            "https://civitai.com/api/v1/images?limit=50&page=1".to_string();
                        debug_fetch_log(
                            &debug_config,
                            &format!("FetchImages: request -> {}", image_feed_url),
                        );
                        match client.get_images(50, 1).await {
                            Ok(res) => {
                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "FetchImages: response -> {} items, has_metadata={}",
                                        res.items.len(),
                                        res.metadata.is_some()
                                    ),
                                );
                                let _ = tx_msg_clone
                                    .send(AppMessage::ImagesLoaded(res.items.clone()))
                                    .await;

                                let fetch_semaphore = Arc::new(Semaphore::new(3));
                                let mut handles = Vec::with_capacity(res.items.len());
                                for item in res.items {
                                    debug_fetch_log(
                                        &debug_config,
                                        &format!("FetchImages: enqueue image id={}", item.id),
                                    );
                                    handles.push(tokio::spawn(load_feed_image(
                                        item.id,
                                        item.url,
                                        req_client.clone(),
                                        picker.clone(),
                                        tx_msg_clone.clone(),
                                        fetch_semaphore.clone(),
                                        debug_config.clone(),
                                    )));
                                }
                                for handle in handles {
                                    let _ = handle.await;
                                }
                            }
                            Err(e) => {
                                debug_fetch_log(
                                    &debug_config,
                                    &format!("FetchImages: get_images failed: {}", e),
                                );
                                let _ = tx_msg_clone
                                    .send(AppMessage::StatusUpdate(format!(
                                        "Error fetching images: {}",
                                        e
                                    )))
                                    .await;
                            }
                        }
                    });
                }
                WorkerCommand::SearchModels(
                    opts,
                    preferred_model_id,
                    preferred_version_id,
                    force_refresh,
                    append,
                    next_page_url,
                ) => {
                    let tx_msg_clone = tx_msg.clone();
                    let cover_cmd_tx = cover_cmd_tx.clone();
                    let opts = normalize_search_options_for_cache(opts);
                    let civitai_clone = CivitaiClient::new(downloader_config.api_key.clone()).unwrap();
                    let search_cache = search_cache.clone();
                    let search_cache_ttl_hours = search_cache_ttl_hours.clone();
                    let search_cache_path = search_cache_path.clone();
                    let debug_config = downloader_config.clone();
                    let use_cache = use_search_cache();

                    tokio::spawn(async move {
                        let query_label = if opts.query.is_empty() {
                            "<default>".to_string()
                        } else {
                            opts.query.clone()
                        };
                        let next_page_url = next_page_url.filter(|url| !url.trim().is_empty());
                        let use_next_page_url = next_page_url.is_some();
                        let effective_force_refresh = force_refresh || !use_cache;
                        let page = opts.page.unwrap_or(1).max(1);

                        debug_fetch_log(
                            &debug_config,
                            &format!(
                                "SearchModels: request page={} limit={} query=\"{}\" sort={:?} type={:?} base={:?} force_refresh={} append={} cache_used={} next_page_url={:?}",
                                page,
                                opts.limit,
                                query_label,
                                opts.sort,
                                opts.types,
                                opts.base_models,
                                effective_force_refresh,
                                append,
                                use_cache,
                                next_page_url,
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
                        let limit = opts.limit.max(1);
                        let start_idx = (page.saturating_sub(1).saturating_mul(limit)) as usize;
                        let end_idx = start_idx.saturating_add(limit as usize);
                        let mut cached_slice: Option<Vec<Model>> = None;
                        let mut cached_has_more = false;
                        let mut cached_next_page: Option<String> = None;

                        if !effective_force_refresh && use_cache && !use_next_page_url {
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
                                    if let Some(on_disk_entry) = load_cached_search_entry(cache_root, &cache_key) {
                                        if is_cache_valid(on_disk_entry.cached_at_unix_secs, ttl_hours) {
                                            cache.insert(cache_key.clone(), on_disk_entry.clone());
                                            save_search_cache(Some(cache_root), &cache, ttl_hours);
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
                                    cached_next_page = entry.metadata.as_ref().and_then(|metadata| metadata.next_page.clone());
                                    if append {
                                        if start_idx < entry.models.len() {
                                            let take_end = end_idx.min(entry.models.len());
                                            let has_more = has_more_from_metadata(
                                                entry.metadata.as_ref(),
                                                page,
                                            );
                                            cached_slice = Some(entry.models[start_idx..take_end].to_vec());
                                            cached_has_more = take_end < entry.models.len() || has_more;
                                            debug_fetch_log(
                                                &debug_config,
                                                &format!(
                                                    "SearchModels: in-memory cache page hit query=\"{}\", start={}, end={}, total={}",
                                                    query_label,
                                                    start_idx,
                                                    take_end,
                                                    entry.models.len()
                                                ),
                                            );
                                            let _ = tx_msg_clone
                                                .send(AppMessage::StatusUpdate(format!(
                                                    "Using cached page for \"{}\"",
                                                    query_label
                                                )))
                                                .await;
                                    } else {
                                        cached_slice = None;
                                        cached_has_more = has_more_from_metadata(
                                            entry.metadata.as_ref(),
                                            page,
                                        );
                                        debug_fetch_log(
                                            &debug_config,
                                            &format!(
                                                "SearchModels: in-memory cache page missing range for query=\"{}\"",
                                                query_label
                                            ),
                                        );
                                    }
                                        let cache_status = if cached_has_more {
                                            format!(
                                                "Reached cached page boundary for \"{}\"",
                                                query_label
                                            )
                                        } else {
                                            format!(
                                                "Cached page not complete for \"{}\", refreshing from network",
                                                query_label
                                            )
                                        };
                                        let _ = tx_msg_clone
                                            .send(AppMessage::StatusUpdate(cache_status))
                                            .await;
                                    } else {
                                        cached_slice = Some(entry.models.clone());
                                        cached_has_more = has_more_from_metadata(entry.metadata.as_ref(), page);
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
                                    &format!(
                                        "SearchModels: no cache hit for \"{}\"",
                                        query_label
                                    ),
                                );
                                let _ = tx_msg_clone
                                    .send(AppMessage::StatusUpdate(format!(
                                        "No cached results for \"{}\"",
                                        query_label
                                    )))
                                    .await;
                            }
                            } else {
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(format!(
                                    "{}",
                                    if use_cache {
                                        format!("Bypassing cache for \"{}\" due to manual refresh", query_label)
                                    } else {
                                        format!("Debug mode: cache disabled; fetching from network",)
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

                            let mut jobs = Vec::with_capacity(cached.len());
                            let mut preferred_url = None;

                            for model in &cached {
                                for version in &model.model_versions {
                                    if let Some(image) = version.images.first() {
                                        jobs.push((version.id, image.url.clone()));
                                        if Some(model.id) == preferred_model_id
                                            && Some(version.id) == preferred_version_id
                                        {
                                            preferred_url = Some(image.url.clone());
                                        }
                                    } else {
                                        let _ = tx_msg_clone
                                            .send(AppMessage::ModelCoverLoadFailed(version.id))
                                            .await;
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
                                save_search_cache(cache_path.as_deref(), &cache, ttl_hours);
                            }
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(format!(
                                    "Cache skipped, fetching models for \"{}\"",
                                    query_label
                                )))
                                .await;
                        }

                        if let Some(url) = next_page_url.as_deref() {
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(format!(
                                    "Fetching next models page for '{}'",
                                    query_label
                                )))
                                .await;
                            debug_fetch_log(
                                &debug_config,
                                &format!("SearchModels request url={}", url),
                            );
                        } else {
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(format!(
                                    "Fetching models matching '{}' (page {})...",
                                    opts.query,
                                    page
                                )))
                                .await;
                            let mut query_opts = opts.clone();
                            query_opts.page = None;
                            debug_fetch_log(
                                &debug_config,
                                &format!("SearchModels request url={}", build_search_url(&query_opts)),
                            );
                        }

                        let fetch_result = if let Some(url) = next_page_url {
                            civitai_clone.search_models_by_url(url).await
                        } else {
                            let mut request_opts = opts.clone();
                            request_opts.page = None;
                            civitai_clone.search_models(request_opts).await
                        };

                        match fetch_result {
                            Ok(res) => {
                                let next_page = res.metadata.as_ref().and_then(|metadata| metadata.next_page.clone());
                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "SearchModels response query=\"{}\" page={} count={} has_more={} next_page={} items={}",
                                        query_label,
                                        page,
                                        res.items.len(),
                                        has_more_from_metadata(res.metadata.as_ref(), page),
                                        next_page.as_deref().unwrap_or("<none>"),
                                        res
                                            .metadata
                                            .as_ref()
                                            .map(|metadata| metadata.total_items.clone().unwrap_or_else(|| "unknown".to_string()))
                                            .unwrap_or_else(|| "unknown".to_string()),
                                    ),
                                );
                                let has_more = has_more_from_metadata(res.metadata.as_ref(), page);
                                let models = res.items.clone();
                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "SearchModels: emitting network chunk append={} count={} has_more={} next_page={:?} page={} preferred_model={:?} preferred_version={:?}",
                                        append,
                                        models.len(),
                                        has_more,
                                        next_page,
                                        page,
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
                                    .send(AppMessage::StatusUpdate(format!(
                                        "Fetched {} models for \"{}\" (page {})",
                                        models.len(),
                                        query_label
                                            ,
                                        page
                                    )))
                                    .await;

                                let mut jobs = Vec::with_capacity(models.len());
                                let mut preferred_url = None;

                                for model in &models {
                                    for version in &model.model_versions {
                                        if let Some(image) = version.images.first() {
                                            jobs.push((version.id, image.url.clone()));
                                            if Some(model.id) == preferred_model_id
                                                && Some(version.id) == preferred_version_id
                                            {
                                                preferred_url = Some(image.url.clone());
                                            }
                                        } else {
                                            let _ = tx_msg_clone
                                                .send(AppMessage::ModelCoverLoadFailed(version.id))
                                                .await;
                                        }
                                    }
                                }

                                let _ = cover_cmd_tx.send(CoverQueueCommand::Enqueue(jobs)).await;
                                if let Some(version_id) = preferred_version_id {
                                    let _ = cover_cmd_tx
                                        .send(CoverQueueCommand::Prioritize(version_id, preferred_url))
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
                                    let entry = cache.entry(cache_key.clone()).or_insert(CachedSearchResult {
                                        cache_key: cache_key.clone(),
                                        models: Vec::new(),
                                        metadata: None,
                                        cached_at_unix_secs: now_unix_secs(),
                                    });
                                    if append {
                                        entry.models.extend(models);
                                    } else {
                                        entry.models = models;
                                    }
                                    entry.metadata = res.metadata;
                                    entry.cached_at_unix_secs = now_unix_secs();
                                    save_search_cache(cache_path.as_deref(), &cache, ttl_hours);
                                }
                            }
                            Err(e) => {
                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "SearchModels: network fetch failed query=\"{}\", page={}, err={}",
                                        query_label,
                                        page,
                                        e
                                    ),
                                );
                                let _ = tx_msg_clone
                                    .send(AppMessage::StatusUpdate(format!(
                                        "Search failed: {}",
                                        e
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
                WorkerCommand::UpdateConfig(new_cfg) => {
                    let new_key = new_cfg.api_key.clone();
                    match CivitaiClient::new(new_key) {
                        Ok(_) => {
                            match DownloadManager::new(new_cfg.clone()) {
                                Ok(_) => {
                                    downloader_config = new_cfg;
                                    let ttl_hours = downloader_config.model_search_cache_ttl_hours;
                                    {
                                        let mut ttl_hours_ref = search_cache_ttl_hours.lock().await;
                                        *ttl_hours_ref = ttl_hours;
                                    }

                                    let new_cache_path = model_search_cache_path(&downloader_config);
                                    let new_cover_cache_path = model_cover_cache_root(&downloader_config);
                                    {
                                        let mut cache_path = search_cache_path.lock().await;
                                        let mut cache = search_cache.lock().await;
                                        if *cache_path != new_cache_path {
                                            *cache_path = new_cache_path.clone();
                                            if use_search_cache() {
                                                *cache =
                                                    load_search_cache(new_cache_path.as_deref(), ttl_hours);
                                            } else {
                                                cache.clear();
                                            }
                                        } else {
                                            if use_search_cache() {
                                                let _ = prune_search_cache(&mut cache, ttl_hours);
                                                save_search_cache(cache_path.as_deref(), &cache, ttl_hours);
                                            } else {
                                                cache.clear();
                                            }
                                        }

                                        let mut cover_cache_path = model_cover_cache_path.lock().await;
                                        *cover_cache_path = new_cover_cache_path;
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
                                            "Failed to update worker download config: {}",
                                            err
                                        )))
                                        .await;
                                }
                            }
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
                            .send(AppMessage::StatusUpdate("Download finished (Placeholder)!".into()))
                            .await;
                    });
                }
                WorkerCommand::DownloadModel(model_id, version_id) => {
                    let tx_msg_clone = tx_msg.clone();
                    let cv_clone = CivitaiClient::new(downloader_config.api_key.clone()).unwrap();
                    let dl_clone = DownloadManager::new(downloader_config.clone()).unwrap();
                    let (control_tx, control_rx) = mpsc::channel(32);
                    {
                        let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> = download_controls.lock().await;
                        controls.insert(model_id, control_tx.clone());
                    }
                    let control_map = download_controls.clone();

                    tokio::spawn(async move {
                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate(format!(
                                "Fetching Model {} metadata for download...",
                                model_id
                            )))
                            .await;

                        if let Ok(model) = cv_clone.get_model(model_id).await {
                            if let Some(version) =
                                model.model_versions.iter().find(|v| v.id == version_id)
                            {
                                let primary_file = version.files.iter().find(|f| f.primary).or_else(|| version.files.first());
                                let filename = dl_clone.generate_smart_filename(
                                    &model,
                                    version,
                                    &primary_file.map(|f| f.name.as_str()).unwrap_or_default(),
                                );
                                let target_dir = dl_clone
                                    .resolve_comfy_path(&model)
                                    .unwrap_or_else(|| {
                                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                                    });
                                let target_path = target_dir.join(&filename);
                                let estimated_size_bytes = primary_file
                                    .and_then(|file| {
                                        if file.size_kb.is_finite() && file.size_kb > 0.0 {
                                            Some((file.size_kb * 1024.0).round() as u64)
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or(0);
                                let _ = tx_msg_clone
                                    .send(AppMessage::DownloadStarted(
                                        model_id,
                                        filename.clone(),
                                        version_id,
                                        model.name.clone(),
                                        estimated_size_bytes,
                                        Some(target_path),
                                    ))
                                    .await;
                                let _ = tx_msg_clone
                                    .send(AppMessage::StatusUpdate(format!(
                                        "Starting download stream for {}",
                                        model.name
                                    )))
                                    .await;

                                let _ = tx_msg_clone
                                    .send(AppMessage::DownloadProgress(
                                        model_id,
                                        filename.clone(),
                                        0.0,
                                        0,
                                        estimated_size_bytes,
                                    ))
                                    .await;

                                    let result = dl_clone
                                        .download_version_with_control(
                                            &model,
                                            version,
                                            None,
                                            Some(tx_msg_clone.clone()),
                                            Some(control_rx),
                                            None,
                                            Some(estimated_size_bytes),
                                        )
                                        .await;
                                match result {
                                    Ok(_) => {
                                        let _ = tx_msg_clone
                                            .send(AppMessage::DownloadCompleted(model_id))
                                            .await;
                                    }
                                    Err(err) => {
                                        if let Some(reason) = err.downcast_ref::<std::io::Error>() {
                                            let _ = tx_msg_clone
                                                .send(AppMessage::DownloadFailed(model_id, format!(
                                                    "{} ({})",
                                                    reason,
                                                    reason.kind()
                                                )))
                                                .await;
                                        } else if err.to_string().contains("cancelled") {
                                            let _ = tx_msg_clone
                                                .send(AppMessage::DownloadCancelled(model_id))
                                                .await;
                                        } else {
                                            let _ = tx_msg_clone
                                                .send(AppMessage::DownloadFailed(model_id, err.to_string()))
                                                .await;
                                        }
                                    }
                                }
                            }
                        } else {
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(format!(
                                    "Failed to retrieve Model {} metadata",
                                    model_id
                            )))
                            .await;
                        }
                        {
                            let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> = control_map.lock().await;
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
                    let cv_clone = CivitaiClient::new(downloader_config.api_key.clone()).unwrap();
                    let dl_clone = DownloadManager::new(downloader_config.clone()).unwrap();
                    let (control_tx, control_rx) = mpsc::channel(32);
                    {
                        let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                            download_controls.lock().await;
                        controls.insert(model_id, control_tx.clone());
                    }
                    let control_map = download_controls.clone();

                    tokio::spawn(async move {
                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate(format!(
                                "Resuming download for model {}...",
                                model_id
                            )))
                            .await;

                        if let Ok(model) = cv_clone.get_model(model_id).await {
                            if let Some(version) =
                                model.model_versions.iter().find(|v| v.id == version_id)
                            {
                                let primary_file = version.files.iter().find(|f| f.primary).or_else(|| version.files.first());
                                let filename = dl_clone.generate_smart_filename(
                                    &model,
                                    version,
                                    &primary_file.map(|f| f.name.as_str()).unwrap_or_default(),
                                );
                                let target_path = resume_file_path.clone().unwrap_or_else(|| {
                                    let target_dir = dl_clone
                                        .resolve_comfy_path(&model)
                                        .unwrap_or_else(|| {
                                            std::env::current_dir().unwrap_or_else(|_| {
                                                std::path::PathBuf::from(".")
                                            })
                                        });
                                    target_dir.join(&filename)
                                });
                                let estimated_size_bytes = primary_file
                                    .and_then(|file| {
                                        if file.size_kb.is_finite() && file.size_kb > 0.0 {
                                            Some((file.size_kb * 1024.0).round() as u64)
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or(0);
                                let total_bytes = if resume_total_bytes > 0 {
                                    resume_total_bytes
                                } else {
                                    estimated_size_bytes
                                };
                                let _ = tx_msg_clone
                                    .send(AppMessage::DownloadStarted(
                                        model_id,
                                        filename.clone(),
                                        version_id,
                                        model.name.clone(),
                                        total_bytes,
                                        Some(target_path.clone()),
                                    ))
                                    .await;
                                let initial_downloaded_bytes = resume_downloaded_bytes
                                    .min(if total_bytes > 0 { total_bytes } else { resume_downloaded_bytes });
                                let _ = tx_msg_clone
                                    .send(AppMessage::DownloadProgress(
                                        model_id,
                                        filename.clone(),
                                        if total_bytes > 0 {
                                            (initial_downloaded_bytes as f64 / total_bytes as f64) * 100.0
                                        } else {
                                            0.0
                                        },
                                        initial_downloaded_bytes,
                                        total_bytes,
                                    ))
                                    .await;

                                let result = dl_clone
                                    .download_version_with_control(
                                        &model,
                                        version,
                                        Some(target_path.clone()),
                                        Some(tx_msg_clone.clone()),
                                        Some(control_rx),
                                        if resume_downloaded_bytes > 0 {
                                            Some(resume_downloaded_bytes)
                                        } else {
                                            None
                                        },
                                        if total_bytes > 0 {
                                            Some(total_bytes)
                                        } else {
                                            None
                                        },
                                    )
                                    .await;
                                match result {
                                    Ok(_) => {
                                        let _ = tx_msg_clone
                                            .send(AppMessage::DownloadCompleted(model_id))
                                            .await;
                                    }
                                    Err(err) => {
                                        if let Some(reason) = err.downcast_ref::<std::io::Error>() {
                                            let _ = tx_msg_clone
                                                .send(AppMessage::DownloadFailed(
                                                    model_id,
                                                    format!("{} ({})", reason, reason.kind()),
                                                ))
                                                .await;
                                        } else if err.to_string().contains("cancelled") {
                                            let _ = tx_msg_clone
                                                .send(AppMessage::DownloadCancelled(model_id))
                                                .await;
                                        } else {
                                            let _ = tx_msg_clone
                                                .send(AppMessage::DownloadFailed(
                                                    model_id,
                                                    err.to_string(),
                                                ))
                                                .await;
                                        }
                                    }
                                }
                            }
                        } else {
                            let _ = tx_msg_clone
                                .send(AppMessage::StatusUpdate(format!(
                                    "Failed to retrieve Model {} metadata",
                                    model_id
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
                WorkerCommand::PauseDownload(model_id) => {
                    let control = {
                        let controls: tokio::sync::MutexGuard<'_, DownloadControlMap> = download_controls.lock().await;
                        controls.get(&model_id).cloned()
                    };
                    if let Some(control) = control {
                        let _ = control.try_send(DownloadControl::Pause);
                        let _ = tx_msg
                            .send(AppMessage::DownloadPaused(model_id))
                            .await;
                    }
                }
                WorkerCommand::ResumeDownload(model_id) => {
                    let control = {
                        let controls: tokio::sync::MutexGuard<'_, DownloadControlMap> = download_controls.lock().await;
                        controls.get(&model_id).cloned()
                    };
                    if let Some(control) = control {
                        let _ = control.try_send(DownloadControl::Resume);
                        let _ = tx_msg
                            .send(AppMessage::DownloadResumed(model_id))
                            .await;
                    }
                }
                WorkerCommand::CancelDownload(model_id) => {
                    let control = {
                        let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> = download_controls.lock().await;
                        controls.remove(&model_id)
                    };
                    if let Some(control) = control {
                        let _ = control.try_send(DownloadControl::Cancel);
                        let _ = tx_msg
                            .send(AppMessage::DownloadCancelled(model_id))
                            .await;
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
    debug_fetch_log(
        debug_config,
        &format!("{} request -> {}", context, url),
    );
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

    match fetch_image_bytes_with_debug(
        &client,
        &image_url,
        &debug_config,
        "Model cover",
    )
    .await
    {
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

async fn load_feed_image(
    image_id: u64,
    image_url: String,
    client: Client,
    picker: Picker,
    tx_msg: mpsc::Sender<AppMessage>,
    semaphore: Arc<Semaphore>,
    debug_config: AppConfig,
) {
    let _permit = match semaphore.acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => return,
    };

    match fetch_image_bytes_with_debug(
        &client,
        &image_url,
        &debug_config,
        "Image feed",
    )
    .await
    {
        Ok(bytes) => {
            debug_fetch_log(
                &debug_config,
                &format!("Image feed fetched: id={}, bytes={}", image_id, bytes.len()),
            );
            if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                let resized = dyn_img.resize(600, 600, FilterType::Triangle);
                let protocol = picker.new_resize_protocol(resized);
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
