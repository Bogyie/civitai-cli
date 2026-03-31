use anyhow::Result;
use image::imageops::FilterType;
use ratatui_image::picker::Picker;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex, Semaphore};

use crate::api::{CivitaiClient, Model};
use crate::config::AppConfig;
use crate::download::manager::DownloadControl;
use crate::download::DownloadManager;
use crate::tui::app::{AppMessage, WorkerCommand};

type DownloadControlMap = HashMap<u64, mpsc::Sender<DownloadControl>>;
type SearchCache = HashMap<String, CachedSearchResult>;

#[derive(Serialize, Deserialize, Clone)]
struct CachedSearchResult {
    models: Vec<Model>,
    cached_at_unix_secs: u64,
}

enum CoverQueueCommand {
    Enqueue(Vec<(u64, String)>),
    Prioritize(u64, Option<String>),
}

fn cache_key_for_options(opts: &crate::api::client::SearchOptions) -> String {
    let sort = opts.sort.as_deref().unwrap_or("All");
    let types = opts.types.as_deref().unwrap_or("All");
    let base_models = opts.base_models.as_deref().unwrap_or("All");
    format!(
        "q={}|t={}|sort={}|base={}",
        opts.query.trim().to_ascii_lowercase(),
        types.trim().to_ascii_lowercase(),
        sort.trim().to_ascii_lowercase(),
        base_models.trim().to_ascii_lowercase()
    )
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

fn load_search_cache(path: Option<&Path>, ttl_hours: u64) -> SearchCache {
    let Some(path) = path else {
        return HashMap::new();
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let mut cache = serde_json::from_str::<SearchCache>(&content).unwrap_or_else(|_| HashMap::new());
    let _ = prune_search_cache(&mut cache, ttl_hours);
    cache
}

fn save_search_cache(path: Option<&Path>, cache: &SearchCache, ttl_hours: u64) {
    let Some(path) = path else {
        return;
    };

    let mut normalized_cache = cache.to_owned();
    let _ = prune_search_cache(&mut normalized_cache, ttl_hours);

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(&normalized_cache) {
        let _ = std::fs::write(path, json);
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

    let mut civitai = CivitaiClient::new(config.api_key.clone()).unwrap();
    let mut downloader_config = config.clone();
    let search_cache_path = model_search_cache_path(&downloader_config);
    let search_cache_ttl_hours = Arc::new(Mutex::new(downloader_config.model_search_cache_ttl_hours.max(1)));
    let search_cache = Arc::new(Mutex::new(load_search_cache(
        search_cache_path.as_deref(),
        *search_cache_ttl_hours.lock().await,
    )));
    let search_cache_path = Arc::new(Mutex::new(search_cache_path));
    let req_client = Client::builder().user_agent("civitai-cli").build().unwrap();

    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    tokio::spawn({
        let tx_msg = tx_msg.clone();
        let req_client = req_client.clone();
        let picker = picker.clone();
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

                    let handle = tokio::spawn(async move {
                        let result = load_model_cover_result(
                            version_id,
                            image_url,
                            req_client,
                            picker,
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
                    let _ = tx_msg
                        .send(AppMessage::StatusUpdate("Fetching feed...".into()))
                        .await;
                    match civitai.get_images(50, 1).await {
                        Ok(res) => {
                            let _ = tx_msg
                                .send(AppMessage::ImagesLoaded(res.items.clone()))
                                .await;

                            let fetch_semaphore = Arc::new(Semaphore::new(3));
                            let mut handles = Vec::with_capacity(res.items.len());
                            for item in res.items {
                                handles.push(tokio::spawn(load_feed_image(
                                    item.id,
                                    item.url,
                                    req_client.clone(),
                                    picker.clone(),
                                    tx_msg.clone(),
                                    fetch_semaphore.clone(),
                                )));
                            }
                            for handle in handles {
                                let _ = handle.await;
                            }
                        }
                        Err(e) => {
                            let _ = tx_msg
                                .send(AppMessage::StatusUpdate(format!(
                                    "Error fetching images: {}",
                                    e
                                )))
                                .await;
                        }
                    }
                }
                WorkerCommand::SearchModels(
                    opts,
                    preferred_model_id,
                    preferred_version_id,
                    force_refresh,
                ) => {
                    let tx_msg_clone = tx_msg.clone();
                    let cover_cmd_tx = cover_cmd_tx.clone();
                    let civitai_clone = CivitaiClient::new(downloader_config.api_key.clone()).unwrap();
                    let search_cache = search_cache.clone();
                    let search_cache_ttl_hours = search_cache_ttl_hours.clone();
                    let search_cache_path = search_cache_path.clone();

                    tokio::spawn(async move {
                        let cache_key = cache_key_for_options(&opts);
                        let ttl_hours = *search_cache_ttl_hours.lock().await;
                        let cache_path = search_cache_path.lock().await.clone();
                        {
                            let mut cache = search_cache.lock().await;
                            let removed = prune_search_cache(&mut cache, ttl_hours);
                            if removed > 0 {
                                save_search_cache(cache_path.as_deref(), &cache, ttl_hours);
                            }
                        }

                        if !force_refresh {
                            let cached = {
                                let cache = search_cache.lock().await;
                                if let Some(entry) = cache.get(&cache_key) {
                                    if is_cache_valid(entry.cached_at_unix_secs, ttl_hours) {
                                        Some(entry.models.clone())
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            };

                            if let Some(cached) = cached {
                                let _ = tx_msg_clone
                                    .send(AppMessage::ModelsSearched(cached.clone()))
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
                        }

                        if force_refresh {
                            {
                                let mut cache = search_cache.lock().await;
                                cache.remove(&cache_key);
                                save_search_cache(cache_path.as_deref(), &cache, ttl_hours);
                            }
                        }

                        let _ = tx_msg_clone
                            .send(AppMessage::StatusUpdate(format!(
                                "Fetching models matching '{}'...",
                                opts.query
                            )))
                            .await;

                        match civitai_clone.search_models(opts).await {
                            Ok(res) => {
                                let _ = tx_msg_clone
                                    .send(AppMessage::ModelsSearched(res.items.clone()))
                                    .await;

                                let mut jobs = Vec::with_capacity(res.items.len());
                                let mut preferred_url = None;

                                for model in &res.items {
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
                                        .send(CoverQueueCommand::Prioritize(
                                            version_id,
                                            preferred_url,
                                        ))
                                        .await;
                                }

                                if ttl_hours > 0 {
                                    let mut cache = search_cache.lock().await;
                                    let cache_path = search_cache_path.lock().await.clone();
                                    cache.insert(
                                        cache_key,
                                        CachedSearchResult {
                                            models: res.items,
                                            cached_at_unix_secs: now_unix_secs(),
                                        },
                                    );
                                    save_search_cache(cache_path.as_deref(), &cache, ttl_hours);
                                }
                            }
                            Err(e) => {
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
                    let cache_path = search_cache_path.lock().await.clone();
                    let ttl_hours = *search_cache_ttl_hours.lock().await;
                    let cleared = {
                        let mut cache = search_cache.lock().await;
                        let count = cache.len();
                        cache.clear();
                        count
                    };

                    save_search_cache(cache_path.as_deref(), &HashMap::new(), ttl_hours);
                    let _ = tx_msg
                        .send(AppMessage::StatusUpdate(format!("Cleared {} cached search item(s)", cleared)))
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
                        Ok(updated_client) => {
                            civitai = updated_client;
                            match DownloadManager::new(new_cfg.clone()) {
                                Ok(_) => {
                                    downloader_config = new_cfg;
                                    let ttl_hours = downloader_config.model_search_cache_ttl_hours.max(1);
                                    {
                                        let mut ttl_hours_ref = search_cache_ttl_hours.lock().await;
                                        *ttl_hours_ref = ttl_hours;
                                    }

                                    let new_cache_path = model_search_cache_path(&downloader_config);
                                    {
                                        let mut cache_path = search_cache_path.lock().await;
                                        let mut cache = search_cache.lock().await;
                                        if *cache_path != new_cache_path {
                                            *cache_path = new_cache_path.clone();
                                            *cache = load_search_cache(new_cache_path.as_deref(), ttl_hours);
                                        } else {
                                            let _ = prune_search_cache(&mut cache, ttl_hours);
                                            save_search_cache(cache_path.as_deref(), &cache, ttl_hours);
                                        }
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
                                        Some(tx_msg_clone.clone()),
                                        Some(control_rx),
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

async fn fetch_image_bytes(client: &Client, url: &str) -> Result<bytes::Bytes> {
    let res = client.get(url).send().await?.error_for_status()?;
    Ok(res.bytes().await?)
}

async fn load_model_cover_result(
    version_id: u64,
    image_url: String,
    client: Client,
    picker: Picker,
) -> AppMessage {
    if let Ok(bytes) = fetch_image_bytes(&client, &image_url).await {
        if let Ok(img) = image::load_from_memory(&bytes) {
            let resized = img.resize(600, 600, FilterType::Triangle);
            let protocol = picker.new_resize_protocol(resized.into());
            return AppMessage::ModelCoverDecoded(version_id, protocol);
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
) {
    let _permit = match semaphore.acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => return,
    };

    if let Ok(bytes) = fetch_image_bytes(&client, &image_url).await {
        if let Ok(dyn_img) = image::load_from_memory(&bytes) {
            let resized = dyn_img.resize(600, 600, FilterType::Triangle);
            let protocol = picker.new_resize_protocol(resized);
            let _ = tx_msg.send(AppMessage::ImageDecoded(image_id, protocol)).await;
        }
    }
}
