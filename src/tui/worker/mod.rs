mod cache;
mod cover_queue;
mod downloads;
mod images;
mod media;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures_util::stream::{self, StreamExt};
use ratatui_image::picker::Picker;
use reqwest::Client;
use tokio::sync::{Mutex, Semaphore, mpsc};

use self::cache::{
    CachedImageSearchResult, CachedSearchResult, build_image_search_url, build_search_url,
    cache_entry_path, cache_key_for_options, clear_cached_search_cache, has_more_from_response,
    image_bytes_cache_root, image_detail_cache_root, image_search_cache_root,
    image_search_cache_ttl_minutes_for, is_cache_valid, is_cache_valid_minutes,
    load_cached_image_search_entry, load_cached_search_entry, load_image_search_cache,
    load_search_cache, model_cover_cache_root, model_search_cache_path,
    normalize_image_search_options_for_cache, normalize_search_options_for_cache, now_unix_secs,
    persist_search_cache_entry, prune_image_search_cache, prune_search_cache,
    remove_cached_search_entry, save_image_search_cache, save_search_cache, use_search_cache,
};
use self::cover_queue::{CoverQueueCommand, spawn_cover_queue};
use self::downloads::{DownloadControlMap, estimated_file_size_bytes, forward_download_events};
use self::images::enrich_image_detail;
use self::media::{
    FeedImageLoadContext, build_image_display_url, decode_model_cover, load_feed_image,
};
use crate::config::AppConfig;
use crate::tui::app::types::DownloadKey;
use crate::tui::app::{AppMessage, MediaRenderRequest, WorkerCommand};
use crate::tui::model::{
    build_download_file_name, model_name, model_versions, resolve_image_download_target_dir,
    resolve_model_download_target_dir,
};
use crate::tui::runtime::{debug_fetch_log, render_request_key};
use crate::tui::status::StatusEvent;
use civitai_cli::sdk::{
    DownloadControl, DownloadDestination, DownloadKind, DownloadOptions, DownloadSpec,
    ModelDownloadAuth, SdkClientBuilder, SearchImageHit, SearchModelHit as Model,
};

fn image_has_excluded_tags(item: &SearchImageHit, excluded_tags: &[String]) -> bool {
    if excluded_tags.is_empty() || item.tag_names.is_empty() {
        return false;
    }

    let item_tags = item
        .tag_names
        .iter()
        .filter_map(|value| value.as_deref())
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();

    excluded_tags
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .any(|value| item_tags.contains(&value))
}

fn build_model_url(model: &Model, version_id: u64) -> String {
    build_model_page_url(model, Some(version_id))
}

fn build_model_page_url(model: &Model, version_id: Option<u64>) -> String {
    crate::tui::model::build_model_url(model, version_id)
}

fn status_message(event: impl Into<StatusEvent>) -> AppMessage {
    AppMessage::StatusUpdate(event.into())
}

fn status_with_url(action: &str, url: &str) -> StatusEvent {
    StatusEvent::info_detail(action, format!("URL: {url}"))
}

fn error_status_with_url(action: &str, url: &str, err: &str) -> StatusEvent {
    StatusEvent::error_detail(action, format!("URL: {url}\nError: {err}"))
}

fn model_decode_warning_events(extras: &serde_json::Value, request_url: &str) -> Vec<StatusEvent> {
    extras
        .get("decodeWarnings")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|warning| {
            let id = warning.get("id")?.as_str().unwrap_or("<unknown>");
            let error = warning.get("error")?.as_str().unwrap_or("unknown decode error");
            let raw_preview = warning
                .get("rawPreview")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing raw preview>");
            Some(StatusEvent::debug(format!(
                "Model hit decode skipped: id={id}\nURL: {request_url}\nError: {error}\nRaw: {raw_preview}"
            )))
        })
        .collect()
}

fn collect_model_cover_jobs(
    models: &[Model],
    preferred_model_id: Option<u64>,
    preferred_version_id: Option<u64>,
) -> (Vec<(u64, String)>, Option<String>) {
    let mut jobs = Vec::new();
    let mut preferred_url = None;

    for model in models {
        for version in model_versions(model) {
            if let Some(image_url) = version.images.first().map(|image| image.url.clone()) {
                jobs.push((version.id, image_url.clone()));
                if Some(model.id) == preferred_model_id && Some(version.id) == preferred_version_id
                {
                    preferred_url = Some(image_url);
                }
            }
        }
    }

    (jobs, preferred_url)
}

pub async fn spawn_worker(
    config: AppConfig,
) -> (mpsc::Sender<WorkerCommand>, mpsc::Receiver<AppMessage>) {
    let (tx_cmd, mut rx_cmd) = mpsc::channel::<WorkerCommand>(32);
    let (tx_msg, rx_msg) = mpsc::channel::<AppMessage>(32);
    let (cover_cmd_tx, cover_cmd_rx) = mpsc::channel::<CoverQueueCommand>(256);
    let (cover_done_tx, cover_done_rx) = mpsc::channel::<u64>(128);

    let mut downloader_config = config.clone();
    let cover_worker_config = downloader_config.clone();
    let search_cache_path = model_search_cache_path(&downloader_config);
    let image_search_cache_path = image_search_cache_root(&downloader_config);
    let search_cache_ttl_hours =
        Arc::new(Mutex::new(downloader_config.model_search_cache_ttl_hours));
    let image_search_cache_ttl_minutes =
        Arc::new(Mutex::new(downloader_config.image_search_cache_ttl_minutes));
    let image_detail_cache_ttl_minutes =
        Arc::new(Mutex::new(downloader_config.image_detail_cache_ttl_minutes));
    let image_cache_ttl_minutes = Arc::new(Mutex::new(downloader_config.image_cache_ttl_minutes));
    let model_cover_cache_path = Arc::new(Mutex::new(model_cover_cache_root(&downloader_config)));
    let image_bytes_cache_path = Arc::new(Mutex::new(image_bytes_cache_root(&downloader_config)));
    let image_detail_cache_path = Arc::new(Mutex::new(image_detail_cache_root(&downloader_config)));
    let search_cache = Arc::new(Mutex::new(if use_search_cache() {
        load_search_cache(
            search_cache_path.as_deref(),
            *search_cache_ttl_hours.lock().await,
        )
    } else {
        HashMap::new()
    }));
    let image_search_cache = Arc::new(Mutex::new(load_image_search_cache(
        image_search_cache_path.as_deref(),
        *image_search_cache_ttl_minutes.lock().await,
    )));
    let search_cache_path = Arc::new(Mutex::new(search_cache_path));
    let image_search_cache_path = Arc::new(Mutex::new(image_search_cache_path));
    let req_client = Client::builder().user_agent("civitai-cli").build().unwrap();

    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    spawn_cover_queue(
        tx_msg.clone(),
        req_client.clone(),
        picker.clone(),
        cover_worker_config.clone(),
        cover_cmd_rx,
        cover_done_tx,
        cover_done_rx,
    );

    tokio::spawn(async move {
        let download_controls: Arc<Mutex<DownloadControlMap>> =
            Arc::new(Mutex::new(HashMap::new()));

        while let Some(cmd) = rx_cmd.recv().await {
            match cmd {
                WorkerCommand::FetchImages(image_opts, next_page_url, render_request) => {
                    let tx_msg_clone = tx_msg.clone();
                    let req_client = req_client.clone();
                    let picker = picker.clone();
                    let debug_config = downloader_config.clone();
                    let image_search_cache = image_search_cache.clone();
                    let image_search_cache_path = image_search_cache_path.clone();
                    let image_bytes_cache_path = image_bytes_cache_path.clone();
                    let image_detail_cache_path = image_detail_cache_path.clone();
                    let image_search_cache_ttl_minutes = image_search_cache_ttl_minutes.clone();
                    let image_detail_cache_ttl_minutes = image_detail_cache_ttl_minutes.clone();
                    let image_cache_ttl_minutes = image_cache_ttl_minutes.clone();
                    let opts = normalize_image_search_options_for_cache(image_opts);
                    let sdk_clone = {
                        let builder = if let Some(api_key) = downloader_config.api_key.clone() {
                            SdkClientBuilder::new().api_key(api_key)
                        } else {
                            SdkClientBuilder::new()
                        };
                        builder.build_web().unwrap()
                    };

                    tokio::spawn(async move {
                        debug_fetch_log(&debug_config, "FetchImages: started");
                        let use_next_page = next_page_url;
                        let is_append = use_next_page.is_some();
                        let request_state = if let Some(page) = use_next_page {
                            let mut next = opts.clone();
                            next.page = Some(page);
                            next
                        } else {
                            opts.clone()
                        };
                        let current_url = build_image_search_url(&request_state);
                        let configured_image_search_ttl_minutes =
                            *image_search_cache_ttl_minutes.lock().await;
                        let image_detail_ttl_minutes = *image_detail_cache_ttl_minutes.lock().await;
                        let image_search_ttl_minutes = image_search_cache_ttl_minutes_for(
                            &request_state,
                            configured_image_search_ttl_minutes,
                        );
                        let image_cache_ttl_minutes = *image_cache_ttl_minutes.lock().await;
                        let media_quality = debug_config.media_quality;
                        let request_key = render_request_key(render_request, media_quality);
                        let use_image_search_cache = image_search_ttl_minutes > 0;
                        let _ = tx_msg_clone
                            .send(status_message(status_with_url(
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

                        let cached_response = if use_image_search_cache {
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
                            if entry.is_none()
                                && let Some(cache_root) = cache_root.as_deref()
                                && let Some(on_disk_entry) =
                                    load_cached_image_search_entry(cache_root, &cache_key)
                                && is_cache_valid_minutes(
                                    on_disk_entry.cached_at_unix_secs,
                                    image_search_ttl_minutes,
                                )
                            {
                                cache.insert(cache_key.clone(), on_disk_entry.clone());
                                entry = Some(on_disk_entry);
                            }

                            match entry {
                                Some(entry)
                                    if is_cache_valid_minutes(
                                        entry.cached_at_unix_secs,
                                        image_search_ttl_minutes,
                                    ) =>
                                {
                                    let _ = tx_msg_clone
                                        .send(status_message(status_with_url(
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
                            Ok((entry.items, entry.next_page, entry.total_hits))
                        } else {
                            sdk_clone
                                .search_images(&request_state)
                                .await
                                .map(|response| {
                                    let total_hits =
                                        response.total_hits.or(response.estimated_total_hits);
                                    let has_more = has_more_from_response(
                                        response.limit.or(request_state.limit),
                                        response.offset,
                                        total_hits,
                                    );
                                    let next_page = has_more.then_some(
                                        request_state.page.unwrap_or(1).saturating_add(1),
                                    );
                                    (response.hits, next_page, total_hits)
                                })
                        };

                        let (visible_items, final_next_page, total_hits) = match fetch_result {
                            Ok((items, next_page, total_hits)) => {
                                let total_items = items.len();
                                let mut visible_items = items;
                                visible_items
                                    .retain(|item| item.r#type.as_deref() != Some("video"));
                                if !request_state.excluded_tags.is_empty() {
                                    visible_items.retain(|item| {
                                        !image_has_excluded_tags(item, &request_state.excluded_tags)
                                    });
                                }
                                let skipped_videos =
                                    total_items.saturating_sub(visible_items.len());

                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "FetchImages: response -> visible_items={}, skipped_videos={}, next_page_present={}",
                                        visible_items.len(),
                                        skipped_videos,
                                        next_page.is_some()
                                    ),
                                );

                                if use_image_search_cache {
                                    let cache_root = image_search_cache_path.lock().await.clone();
                                    if cache_root.is_some() && image_search_ttl_minutes > 0 {
                                        let mut cache = image_search_cache.lock().await;
                                        cache.insert(
                                            current_url.clone(),
                                            CachedImageSearchResult {
                                                cache_key: current_url.clone(),
                                                items: visible_items.clone(),
                                                next_page,
                                                total_hits,
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

                                (visible_items, next_page, total_hits)
                            }
                            Err(e) => {
                                debug_fetch_log(
                                    &debug_config,
                                    &format!("FetchImages: get_images failed: {}", e),
                                );
                                let _ = tx_msg_clone
                                    .send(status_message(error_status_with_url(
                                        "Error fetching images",
                                        &current_url,
                                        &e.to_string(),
                                    )))
                                    .await;
                                return;
                            }
                        };

                        let image_jobs = visible_items
                            .iter()
                            .filter_map(|item| {
                                build_image_display_url(item, render_request, media_quality)
                                    .map(|image_url| (item.id, image_url))
                            })
                            .collect::<Vec<_>>();

                        let _ = tx_msg_clone
                            .send(AppMessage::ImagesLoaded(
                                visible_items.clone(),
                                is_append,
                                final_next_page,
                                total_hits,
                            ))
                            .await;

                        let detail_cache_root = image_detail_cache_path.lock().await.clone();
                        let enriched_items = stream::iter(visible_items.into_iter().map(|item| {
                            let sdk_clone = sdk_clone.clone();
                            let detail_cache_root = detail_cache_root.clone();
                            async move {
                                enrich_image_detail(
                                    &sdk_clone,
                                    detail_cache_root.as_deref(),
                                    item,
                                    image_detail_ttl_minutes,
                                )
                                .await
                            }
                        }))
                        .buffered(6)
                        .collect::<Vec<_>>()
                        .await;

                        for enriched in enriched_items {
                            let _ = tx_msg_clone
                                .send(AppMessage::ImageDetailEnriched(enriched))
                                .await;
                        }

                        let fetch_semaphore = Arc::new(Semaphore::new(3));
                        let mut handles = Vec::with_capacity(image_jobs.len());
                        for (image_id, image_url) in image_jobs {
                            debug_fetch_log(
                                &debug_config,
                                &format!("FetchImages: enqueue image id={}", image_id),
                            );
                            let image_bytes_cache_root =
                                image_bytes_cache_path.lock().await.clone();
                            let use_binary_cache = image_bytes_cache_root.is_some();
                            handles.push(tokio::spawn(load_feed_image(
                                image_id,
                                image_url,
                                FeedImageLoadContext {
                                    request_key: request_key.clone(),
                                    client: req_client.clone(),
                                    picker: picker.clone(),
                                    tx_msg: tx_msg_clone.clone(),
                                    semaphore: fetch_semaphore.clone(),
                                    debug_config: debug_config.clone(),
                                    image_bytes_cache_root,
                                    ttl_minutes: image_cache_ttl_minutes,
                                    use_cache: use_binary_cache,
                                },
                            )));
                        }
                        for handle in handles {
                            let _ = handle.await;
                        }
                    });
                }
                WorkerCommand::LoadImage(item, render_request) => {
                    let tx_msg_clone = tx_msg.clone();
                    let req_client = req_client.clone();
                    let picker = picker.clone();
                    let debug_config = downloader_config.clone();
                    let image_bytes_cache_root = image_bytes_cache_path.lock().await.clone();
                    let image_cache_ttl_minutes = *image_cache_ttl_minutes.lock().await;
                    let use_cache = image_bytes_cache_root.is_some();
                    tokio::spawn(async move {
                        if let Some(image_url) = build_image_display_url(
                            &item,
                            render_request,
                            debug_config.media_quality,
                        ) {
                            let request_key =
                                render_request_key(render_request, debug_config.media_quality);
                            load_feed_image(
                                item.id,
                                image_url,
                                FeedImageLoadContext {
                                    request_key,
                                    client: req_client,
                                    picker,
                                    tx_msg: tx_msg_clone,
                                    semaphore: Arc::new(Semaphore::new(1)),
                                    debug_config,
                                    image_bytes_cache_root,
                                    ttl_minutes: image_cache_ttl_minutes,
                                    use_cache,
                                },
                            )
                            .await;
                        }
                    });
                }
                WorkerCommand::RebuildImageProtocol(image_id, bytes) => {
                    if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                        let protocol = picker.new_resize_protocol(dyn_img);
                        let _ = tx_msg
                            .send(AppMessage::ImageDecoded(
                                image_id,
                                protocol,
                                bytes,
                                String::new(),
                            ))
                            .await;
                    }
                }
                WorkerCommand::RebuildModelCover(version_id, bytes) => {
                    if let Some(protocol) = decode_model_cover(&bytes, &picker) {
                        let _ = tx_msg
                            .send(AppMessage::ModelCoverDecoded(
                                version_id,
                                protocol,
                                bytes,
                                String::new(),
                            ))
                            .await;
                    }
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
                            .send(status_message(format!(
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
                            if entry.is_none()
                                && let Some(cache_root) = cache_path.as_deref()
                            {
                                let _ = tx_msg_clone
                                    .send(status_message(format!(
                                        "Checking on-disk cache for \"{}\"",
                                        query_label
                                    )))
                                    .await;
                                if let Some(on_disk_entry) =
                                    load_cached_search_entry(cache_root, &cache_key)
                                {
                                    if is_cache_valid(on_disk_entry.cached_at_unix_secs, ttl_hours)
                                    {
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
                                            .send(status_message(format!(
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
                                            .send(status_message(format!(
                                                "Cached file expired for \"{}\", refreshing",
                                                query_label
                                            )))
                                            .await;
                                        remove_cached_search_entry(cache_root, &cache_key);
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
                                            .send(status_message(format!(
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
                                        .send(status_message(format!(
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
                                    .send(status_message(format!(
                                        "No cached results for \"{}\"",
                                        query_label
                                    )))
                                    .await;
                            }
                        } else {
                            let _ = tx_msg_clone
                                .send(status_message(
                                    (if use_cache {
                                        format!(
                                            "Bypassing cache for \"{}\" due to manual refresh",
                                            query_label
                                        )
                                    } else {
                                        "Debug mode: cache disabled; fetching from network"
                                            .to_string()
                                    })
                                    .to_string(),
                                ))
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
                            let cached_len = cached.len();
                            debug_fetch_log(
                                &debug_config,
                                &format!(
                                    "SearchModels: emitting cached chunk append={} count={} has_more={} preferred_model={:?} preferred_version={:?}",
                                    append,
                                    cached_len,
                                    cached_has_more,
                                    preferred_model_id,
                                    preferred_version_id
                                ),
                            );
                            let (jobs, preferred_url) = collect_model_cover_jobs(
                                &cached,
                                preferred_model_id,
                                preferred_version_id,
                            );
                            let _ = tx_msg_clone
                                .send(AppMessage::ModelsSearchedChunk(
                                    cached,
                                    append,
                                    cached_has_more,
                                    cached_next_page,
                                ))
                                .await;

                            let _ = cover_cmd_tx.send(CoverQueueCommand::Enqueue(jobs)).await;
                            if let Some(version_id) = preferred_version_id {
                                let _ = cover_cmd_tx
                                    .send(CoverQueueCommand::Prioritize(
                                        version_id,
                                        preferred_url,
                                        None,
                                        MediaRenderRequest {
                                            width: 960,
                                            height: 720,
                                        },
                                    ))
                                    .await;
                            }

                            let _ = tx_msg_clone
                                .send(status_message(format!(
                                    "Loaded {} cached models",
                                    cached_len
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
                                .send(status_message(format!(
                                    "Cache skipped, fetching models for \"{}\"",
                                    query_label
                                )))
                                .await;
                        }

                        if next_page_index.is_some() {
                            let _ = tx_msg_clone
                                .send(status_message(status_with_url(
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
                                .send(status_message(status_with_url(
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
                                let warning_events =
                                    model_decode_warning_events(&res.extras, &request_url);
                                let has_more = has_more_from_response(
                                    res.limit,
                                    res.offset,
                                    res.estimated_total_hits,
                                );
                                let next_page = has_more
                                    .then_some(request_state.page.unwrap_or(1).saturating_add(1));
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
                                let mut models = res.hits;
                                let model_count = models.len();
                                debug_fetch_log(
                                    &debug_config,
                                    &format!(
                                        "SearchModels: emitting network chunk append={} count={} has_more={} next_page={:?} preferred_model={:?} preferred_version={:?}",
                                        append,
                                        model_count,
                                        has_more,
                                        next_page,
                                        preferred_model_id,
                                        preferred_version_id
                                    ),
                                );
                                let (jobs, preferred_url) = collect_model_cover_jobs(
                                    &models,
                                    preferred_model_id,
                                    preferred_version_id,
                                );
                                if ttl_hours > 0 && use_cache {
                                    let _ = tx_msg_clone
                                        .send(AppMessage::ModelsSearchedChunk(
                                            models.clone(),
                                            append,
                                            has_more,
                                            next_page,
                                        ))
                                        .await;
                                } else {
                                    let app_models = std::mem::take(&mut models);
                                    let _ = tx_msg_clone
                                        .send(AppMessage::ModelsSearchedChunk(
                                            app_models, append, has_more, next_page,
                                        ))
                                        .await;
                                }
                                let _ = tx_msg_clone
                                    .send(status_message(status_with_url(
                                        &format!(
                                            "Fetched {} models for \"{}\"",
                                            model_count, query_label
                                        ),
                                        &request_url,
                                    )))
                                    .await;
                                for event in warning_events {
                                    let _ = tx_msg_clone.send(status_message(event)).await;
                                }

                                let _ = cover_cmd_tx.send(CoverQueueCommand::Enqueue(jobs)).await;
                                if let Some(version_id) = preferred_version_id {
                                    let _ = cover_cmd_tx
                                        .send(CoverQueueCommand::Prioritize(
                                            version_id,
                                            preferred_url,
                                            None,
                                            MediaRenderRequest {
                                                width: 960,
                                                height: 720,
                                            },
                                        ))
                                        .await;
                                }

                                if ttl_hours > 0 && use_cache {
                                    debug_fetch_log(
                                        &debug_config,
                                        &format!(
                                            "SearchModels: persist cache query=\"{}\" append={} count={}",
                                            query_label, append, model_count
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
                                        let mut seen_ids = entry
                                            .models
                                            .iter()
                                            .map(|model| model.id)
                                            .collect::<HashSet<_>>();
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
                                    .send(status_message(error_status_with_url(
                                        "Search failed",
                                        &request_url,
                                        &e.to_string(),
                                    )))
                                    .await;
                            }
                        }
                    });
                }
                WorkerCommand::FetchModelDetail(model_id, preferred_version_id, model_query) => {
                    let tx_msg_clone = tx_msg.clone();
                    let sdk_clone = {
                        let builder = if let Some(api_key) = downloader_config.api_key.clone() {
                            SdkClientBuilder::new().api_key(api_key)
                        } else {
                            SdkClientBuilder::new()
                        };
                        builder.build_api().unwrap()
                    };

                    tokio::spawn(async move {
                        let _ = tx_msg_clone
                            .send(status_message(format!(
                                "Loading model details for {}...",
                                model_id
                            )))
                            .await;

                        match sdk_clone.get_model(model_id).await {
                            Ok(model) => {
                                let message = if preferred_version_id.is_some() {
                                    AppMessage::ModelDetailLoaded(
                                        Box::new(model.into()),
                                        preferred_version_id,
                                    )
                                } else {
                                    AppMessage::ModelSidebarDetailLoaded(Box::new(model.into()))
                                };
                                let _ = tx_msg_clone.send(message).await;
                            }
                            Err(err) => {
                                let _ = tx_msg_clone
                                    .send(status_message(format!(
                                        "Error loading model details for {}: {}",
                                        model_query, err
                                    )))
                                    .await;
                            }
                        }
                    });
                }
                WorkerCommand::ClearSearchCache => {
                    if !use_search_cache() {
                        let _ = tx_msg
                            .send(status_message(StatusEvent::debug(
                                "Debug mode: search cache is disabled, nothing to clear.",
                            )))
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
                        .send(status_message(format!(
                            "Cleared {} cached search item(s)",
                            cleared
                        )))
                        .await;
                }
                WorkerCommand::ClearAllCaches => {
                    let mut removed_targets = 0usize;

                    {
                        let mut cache = search_cache.lock().await;
                        if !cache.is_empty() {
                            cache.clear();
                            removed_targets += 1;
                        }
                    }
                    {
                        let mut cache = image_search_cache.lock().await;
                        if !cache.is_empty() {
                            cache.clear();
                            removed_targets += 1;
                        }
                    }

                    for root in [
                        search_cache_path.lock().await.clone(),
                        image_search_cache_path.lock().await.clone(),
                        model_cover_cache_path.lock().await.clone(),
                        image_bytes_cache_path.lock().await.clone(),
                        image_detail_cache_path.lock().await.clone(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let existed = root.exists();
                        let removed_dir = std::fs::remove_dir_all(&root).is_ok();
                        if existed || removed_dir {
                            removed_targets += 1;
                        }
                    }

                    debug_fetch_log(
                        &downloader_config,
                        &format!("Clear all caches requested, removed_targets={removed_targets}"),
                    );
                    let _ = tx_msg
                        .send(status_message(format!(
                            "Cleared cache storage ({removed_targets} target(s))"
                        )))
                        .await;
                }
                WorkerCommand::PrioritizeModelCover(
                    version_id,
                    image_url,
                    source_dims,
                    render_request,
                ) => {
                    let _ = cover_cmd_tx
                        .send(CoverQueueCommand::Prioritize(
                            version_id,
                            image_url,
                            source_dims,
                            render_request,
                        ))
                        .await;
                }
                WorkerCommand::PrefetchModelCovers(jobs, render_request) => {
                    let _ = cover_cmd_tx
                        .send(CoverQueueCommand::Prefetch(jobs, render_request))
                        .await;
                }
                WorkerCommand::UpdateConfig(new_cfg) => {
                    let builder = if let Some(api_key) = new_cfg
                        .api_key
                        .clone()
                        .filter(|value| !value.trim().is_empty())
                    {
                        SdkClientBuilder::new().api_key(api_key)
                    } else {
                        SdkClientBuilder::new()
                    };
                    match builder.build_api() {
                        Ok(_) => {
                            downloader_config = new_cfg;
                            let ttl_hours = downloader_config.model_search_cache_ttl_hours;
                            let image_search_ttl_minutes_value =
                                downloader_config.image_search_cache_ttl_minutes;
                            let image_detail_ttl_minutes_value =
                                downloader_config.image_detail_cache_ttl_minutes;
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
                                let mut ttl_ref = image_detail_cache_ttl_minutes.lock().await;
                                *ttl_ref = image_detail_ttl_minutes_value;
                            }
                            {
                                let mut ttl_ref = image_cache_ttl_minutes.lock().await;
                                *ttl_ref = image_cache_ttl_minutes_value;
                            }

                            let new_cache_path = model_search_cache_path(&downloader_config);
                            let new_image_search_cache_path =
                                image_search_cache_root(&downloader_config);
                            let new_cover_cache_path = model_cover_cache_root(&downloader_config);
                            let new_image_bytes_cache_path =
                                image_bytes_cache_root(&downloader_config);
                            let new_image_detail_cache_path =
                                image_detail_cache_root(&downloader_config);
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
                                    *image_cache = load_image_search_cache(
                                        new_image_search_cache_path.as_deref(),
                                        image_search_ttl_minutes_value,
                                    );
                                } else {
                                    let _ = prune_image_search_cache(
                                        &mut image_cache,
                                        image_search_ttl_minutes_value,
                                    );
                                    save_image_search_cache(
                                        image_cache_path.as_deref(),
                                        &image_cache,
                                        image_search_ttl_minutes_value,
                                    );
                                }

                                let mut cover_cache_path = model_cover_cache_path.lock().await;
                                *cover_cache_path = new_cover_cache_path;
                                let mut image_bytes_path = image_bytes_cache_path.lock().await;
                                *image_bytes_path = new_image_bytes_cache_path;
                                let mut image_detail_path = image_detail_cache_path.lock().await;
                                *image_detail_path = new_image_detail_cache_path;
                            }

                            let _ = tx_msg
                                .send(status_message(StatusEvent::debug(
                                    "Configuration sync applied to worker",
                                )))
                                .await;
                        }
                        Err(err) => {
                            let _ = tx_msg
                                .send(status_message(format!(
                                    "Failed to update worker API config: {}",
                                    err
                                )))
                                .await;
                        }
                    }
                }
                WorkerCommand::DownloadImage(image_hit) => {
                    let tx_msg_clone = tx_msg.clone();
                    let config = downloader_config.clone();
                    let download_client = {
                        let builder = if let Some(api_key) = downloader_config.api_key.clone() {
                            SdkClientBuilder::new().api_key(api_key)
                        } else {
                            SdkClientBuilder::new()
                        };
                        builder.build_download().unwrap()
                    };
                    tokio::spawn(async move {
                        let Some(spec) = download_client.build_media_download_spec(&image_hit)
                        else {
                            let _ = tx_msg_clone
                                .send(status_message(format!(
                                    "No downloadable media found for image {}",
                                    image_hit.id
                                )))
                                .await;
                            return;
                        };

                        let downloads_root = resolve_image_download_target_dir(&config);
                        let _ = tokio::fs::create_dir_all(&downloads_root).await;
                        let file_name = spec.suggested_file_name();
                        let options = DownloadOptions {
                            destination: DownloadDestination::File(downloads_root.join(&file_name)),
                            create_parent_dirs: true,
                            ..DownloadOptions::default()
                        };
                        let _ = tx_msg_clone
                            .send(status_message(format!(
                                "Downloading image {}...",
                                image_hit.id
                            )))
                            .await;

                        match download_client.download(&spec, &options, None, None).await {
                            Ok(result) => {
                                let _ = tx_msg_clone
                                    .send(status_message(format!(
                                        "Downloaded image {} to {}",
                                        image_hit.id,
                                        result.path.display()
                                    )))
                                    .await;
                            }
                            Err(err) => {
                                let _ = tx_msg_clone
                                    .send(status_message(format!(
                                        "Failed to download image {}: {}",
                                        image_hit.id, err
                                    )))
                                    .await;
                            }
                        }
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
                    let control_map = download_controls.clone();
                    let config = downloader_config.clone();

                    tokio::spawn(async move {
                        let model_id = model_hit.id;
                        let model_url = build_model_url(&model_hit, version_id);
                        let _ = tx_msg_clone
                            .send(status_message(status_with_url(
                                &format!("Preparing download for {}", model_name(&model_hit)),
                                &model_url,
                            )))
                            .await;

                        if let Some(version) = model_versions(&model_hit)
                            .into_iter()
                            .find(|item| item.id == version_id)
                        {
                            let selected_file = version
                                .files
                                .get(file_index)
                                .or_else(|| version.files.iter().find(|file| file.primary))
                                .or_else(|| version.files.first());
                            let filename = selected_file
                                .map(|file| build_download_file_name(&model_hit, &version, file))
                                .unwrap_or_else(|| model_hit.default_download_file_name());
                            let target_dir = resolve_model_download_target_dir(
                                &config,
                                model_hit.r#type.as_deref(),
                                version.base_model.as_deref(),
                            );
                            let target_path = target_dir.join(&filename);
                            let _estimated_size_bytes = selected_file
                                .and_then(estimated_file_size_bytes)
                                .unwrap_or(0);
                            let auth = config.api_key.clone().map(ModelDownloadAuth::QueryToken);
                            let download_url = selected_file
                                .and_then(|file| file.download_url.clone())
                                .unwrap_or_else(|| match auth.as_ref() {
                                    Some(ModelDownloadAuth::QueryToken(token)) => download_client
                                        .build_model_download_url_with_token(version_id, token),
                                    _ => download_client.build_model_download_url(version_id),
                                });
                            let spec = DownloadSpec::new(download_url, DownloadKind::Model)
                                .with_file_name(filename.clone());
                            let download_key =
                                DownloadKey::new(model_id, version_id, filename.clone());
                            {
                                let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                                    control_map.lock().await;
                                controls.insert(download_key.clone(), control_tx.clone());
                            }
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
                                download_key.clone(),
                                model_name(&model_hit),
                            ));
                            let _ = tx_msg_clone
                                .send(status_message(format!(
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
                                        .send(AppMessage::DownloadCompleted(download_key.clone()))
                                        .await;
                                }
                                Err(err) if err.to_string().contains("cancelled") => {
                                    let _ = tx_msg_clone
                                        .send(AppMessage::DownloadCancelled(download_key.clone()))
                                        .await;
                                }
                                Err(err) => {
                                    let _ = tx_msg_clone
                                        .send(AppMessage::DownloadFailed(
                                            download_key.clone(),
                                            err.to_string(),
                                        ))
                                        .await;
                                }
                            }
                        } else {
                            let _ = tx_msg_clone
                                .send(status_message(error_status_with_url(
                                    &format!(
                                        "Failed to resolve version {} for model {}",
                                        version_id, model_id
                                    ),
                                    &model_url,
                                    "selected version not found",
                                )))
                                .await;
                        }
                        {
                            let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                                control_map.lock().await;
                            controls.retain(|key, _| {
                                !(key.model_id == model_id && key.version_id == version_id)
                            });
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
                    let control_map = download_controls.clone();
                    let config = downloader_config.clone();

                    tokio::spawn(async move {
                        let model_url =
                            format!("https://civitai.com/api/download/models/{}", version_id);
                        let _ = tx_msg_clone
                            .send(status_message(status_with_url(
                                &format!("Resuming download for model {}", model_id),
                                &model_url,
                            )))
                            .await;

                        let fallback_name = format!("civitai-model-v{version_id}");
                        let filename = resume_file_path
                            .as_ref()
                            .and_then(|path| {
                                path.file_name()
                                    .map(|value| value.to_string_lossy().to_string())
                            })
                            .filter(|value| !value.trim().is_empty())
                            .unwrap_or(fallback_name);
                        let target_path = resume_file_path.clone().unwrap_or_else(|| {
                            std::env::current_dir()
                                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                                .join(&filename)
                        });
                        let auth = config.api_key.clone().map(ModelDownloadAuth::QueryToken);
                        let download_url = match auth.as_ref() {
                            Some(ModelDownloadAuth::QueryToken(token)) => download_client
                                .build_model_download_url_with_token(version_id, token),
                            _ => download_client.build_model_download_url(version_id),
                        };
                        let spec = DownloadSpec::new(download_url, DownloadKind::Model)
                            .with_file_name(filename.clone());
                        let download_key = DownloadKey::new(model_id, version_id, filename.clone());
                        {
                            let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                                control_map.lock().await;
                            controls.insert(download_key.clone(), control_tx.clone());
                        }
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
                            download_key.clone(),
                            format!("Model {}", model_id),
                        ));
                        let total_bytes = (resume_total_bytes > 0).then_some(resume_total_bytes);
                        let _ = tx_msg_clone
                            .send(AppMessage::DownloadStarted(
                                download_key.clone(),
                                format!("Model {}", model_id),
                                total_bytes.unwrap_or(0),
                                Some(target_path.clone()),
                            ))
                            .await;
                        if resume_downloaded_bytes > 0 {
                            let percent = total_bytes
                                .filter(|value| *value > 0)
                                .map(|value| {
                                    (resume_downloaded_bytes as f64 / value as f64) * 100.0
                                })
                                .unwrap_or(0.0);
                            let _ = tx_msg_clone
                                .send(AppMessage::DownloadProgress(
                                    download_key.clone(),
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
                                    .send(AppMessage::DownloadCompleted(download_key.clone()))
                                    .await;
                            }
                            Err(err) if err.to_string().contains("cancelled") => {
                                let _ = tx_msg_clone
                                    .send(AppMessage::DownloadCancelled(download_key.clone()))
                                    .await;
                            }
                            Err(err) => {
                                let _ = tx_msg_clone
                                    .send(AppMessage::DownloadFailed(
                                        download_key.clone(),
                                        err.to_string(),
                                    ))
                                    .await;
                            }
                        }

                        {
                            let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                                control_map.lock().await;
                            controls.remove(&download_key);
                        }
                    });
                }
                WorkerCommand::PauseDownload(download_key) => {
                    let control = {
                        let controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                            download_controls.lock().await;
                        controls.get(&download_key).cloned()
                    };
                    if let Some(control) = control {
                        let _ = control.try_send(DownloadControl::Pause);
                        let _ = tx_msg.send(AppMessage::DownloadPaused(download_key)).await;
                    }
                }
                WorkerCommand::ResumeDownload(download_key) => {
                    let control = {
                        let controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                            download_controls.lock().await;
                        controls.get(&download_key).cloned()
                    };
                    if let Some(control) = control {
                        let _ = control.try_send(DownloadControl::Resume);
                        let _ = tx_msg.send(AppMessage::DownloadResumed(download_key)).await;
                    }
                }
                WorkerCommand::CancelDownload(download_key) => {
                    let control = {
                        let mut controls: tokio::sync::MutexGuard<'_, DownloadControlMap> =
                            download_controls.lock().await;
                        controls.remove(&download_key)
                    };
                    if let Some(control) = control {
                        let _ = control.try_send(DownloadControl::Cancel);
                        let _ = tx_msg
                            .send(AppMessage::DownloadCancelled(download_key))
                            .await;
                    }
                }
                WorkerCommand::Quit => break,
            }
        }
    });

    (tx_cmd, rx_msg)
}
