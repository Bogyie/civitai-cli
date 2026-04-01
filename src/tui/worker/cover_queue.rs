use crate::config::AppConfig;
use crate::tui::app::{AppMessage, MediaRenderRequest, VersionCoverJob};
use crate::tui::runtime::render_request_key;
use crate::tui::worker::cache::{model_cover_cache_root, use_search_cache};
use crate::tui::worker::media::{
    ModelCoverLoadContext, fetch_cover_urls_for_version, load_model_cover_result,
    load_model_cover_results, rewrite_cover_url_for_display,
};
use ratatui_image::picker::Picker;
use reqwest::Client;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

pub(super) enum CoverQueueCommand {
    Enqueue(Vec<(u64, String)>),
    Prioritize(u64, Option<String>, Option<(u32, u32)>, MediaRenderRequest),
    Prefetch(Vec<VersionCoverJob>, MediaRenderRequest),
}

fn pop_next_job(
    queue: &mut VecDeque<(u64, String)>,
    focus_version: Option<u64>,
) -> Option<(u64, String)> {
    if let Some(focus_version) = focus_version
        && let Some(pos) = queue.iter().position(|(id, _)| *id == focus_version)
    {
        return queue.remove(pos);
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

pub(super) fn spawn_cover_queue(
    tx_msg: mpsc::Sender<AppMessage>,
    req_client: Client,
    picker: Picker,
    config: AppConfig,
    mut cover_cmd_rx: mpsc::Receiver<CoverQueueCommand>,
    cover_done_tx: mpsc::Sender<u64>,
    mut cover_done_rx: mpsc::Receiver<u64>,
) {
    let model_cover_cache_path = Arc::new(Mutex::new(model_cover_cache_root(&config)));
    tokio::spawn({
        let tx_msg = tx_msg.clone();
        let req_client = req_client.clone();
        let picker = picker.clone();
        let model_cover_cache_path = model_cover_cache_path.clone();
        let debug_config = config.clone();
        let api_client = {
            let builder = if let Some(api_key) = config.api_key.clone() {
                civitai_cli::sdk::SdkClientBuilder::new().api_key(api_key)
            } else {
                civitai_cli::sdk::SdkClientBuilder::new()
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
                    let request_key = render_request_key(
                        MediaRenderRequest {
                            width: 960,
                            height: 720,
                        },
                        debug_config.media_quality,
                    );

                    let handle = tokio::spawn(async move {
                        let cover_cache_root = model_cover_cache_path.lock().await.clone();
                        let result = load_model_cover_result(
                            version_id,
                            image_url,
                            ModelCoverLoadContext {
                                request_key,
                                client: req_client,
                                picker,
                                cover_cache_root,
                                debug_config,
                                use_cache: use_cover_cache,
                            },
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
                    handle_cover_command(
                        cmd,
                        &api_client,
                        &tx_msg,
                        &req_client,
                        &picker,
                        &model_cover_cache_path,
                        &debug_config,
                        &cover_done_tx,
                        &mut queue,
                        &mut queued_ids,
                        &mut running_ids,
                        &mut running_handles,
                        &mut known_version_urls,
                        &mut focus_version,
                        max_in_flight,
                        &enqueue_or_bump_queue,
                    )
                    .await;
                    continue;
                }

                tokio::select! {
                    Some(cmd) = cover_cmd_rx.recv() => {
                        handle_cover_command(
                            cmd,
                            &api_client,
                            &tx_msg,
                            &req_client,
                            &picker,
                            &model_cover_cache_path,
                            &debug_config,
                            &cover_done_tx,
                            &mut queue,
                            &mut queued_ids,
                            &mut running_ids,
                            &mut running_handles,
                            &mut known_version_urls,
                            &mut focus_version,
                            max_in_flight,
                            &enqueue_or_bump_queue,
                        ).await;
                    }
                    Some(done_version_id) = cover_done_rx.recv() => {
                        let _ = running_ids.remove(&done_version_id);
                        let _ = running_handles.remove(&done_version_id);
                    }
                }
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
async fn handle_cover_command<F>(
    cmd: CoverQueueCommand,
    api_client: &civitai_cli::sdk::ApiClient,
    tx_msg: &mpsc::Sender<AppMessage>,
    req_client: &Client,
    picker: &Picker,
    model_cover_cache_path: &Arc<Mutex<Option<std::path::PathBuf>>>,
    debug_config: &AppConfig,
    cover_done_tx: &mpsc::Sender<u64>,
    queue: &mut VecDeque<(u64, String)>,
    queued_ids: &mut HashSet<u64>,
    running_ids: &mut HashSet<u64>,
    running_handles: &mut HashMap<u64, tokio::task::JoinHandle<()>>,
    known_version_urls: &mut HashMap<u64, String>,
    focus_version: &mut Option<u64>,
    max_in_flight: usize,
    enqueue_or_bump_queue: &F,
) where
    F: Fn(&mut VecDeque<(u64, String)>, &mut HashSet<u64>, u64, String, bool),
{
    match cmd {
        CoverQueueCommand::Enqueue(jobs) => {
            for (version_id, image_url) in jobs {
                known_version_urls.insert(version_id, image_url.clone());
                if running_ids.contains(&version_id) || queued_ids.contains(&version_id) {
                    continue;
                }
                enqueue_or_bump_queue(
                    queue,
                    queued_ids,
                    version_id,
                    image_url,
                    *focus_version == Some(version_id),
                );
            }
        }
        CoverQueueCommand::Prefetch(jobs, render_request) => {
            for (version_id, image_url, source_dims) in jobs {
                if running_ids.contains(&version_id) || queued_ids.contains(&version_id) {
                    continue;
                }
                let resolved_url = if let Some(url) =
                    image_url.or_else(|| known_version_urls.get(&version_id).cloned())
                {
                    rewrite_cover_url_for_display(
                        &url,
                        source_dims,
                        render_request,
                        debug_config.media_quality,
                    )
                } else {
                    fetch_cover_urls_for_version(api_client, version_id)
                        .await
                        .into_iter()
                        .next()
                        .and_then(|url| {
                            rewrite_cover_url_for_display(
                                &url,
                                source_dims,
                                render_request,
                                debug_config.media_quality,
                            )
                        })
                };

                if let Some(url) = resolved_url {
                    known_version_urls.insert(version_id, url.clone());
                    enqueue_or_bump_queue(queue, queued_ids, version_id, url, false);
                }
            }
        }
        CoverQueueCommand::Prioritize(version_id, image_url, source_dims, render_request) => {
            *focus_version = Some(version_id);
            let mut resolved_urls = fetch_cover_urls_for_version(api_client, version_id).await;
            resolved_urls = resolved_urls
                .into_iter()
                .filter_map(|url| {
                    rewrite_cover_url_for_display(
                        &url,
                        source_dims,
                        render_request,
                        debug_config.media_quality,
                    )
                })
                .collect();
            if resolved_urls.is_empty()
                && let Some(url) =
                    image_url.or_else(|| known_version_urls.get(&version_id).cloned())
                && let Some(transformed) = rewrite_cover_url_for_display(
                    &url,
                    source_dims,
                    render_request,
                    debug_config.media_quality,
                )
            {
                resolved_urls.push(transformed);
            }

            if let Some(first_url) = resolved_urls.first().cloned() {
                known_version_urls.insert(version_id, first_url);
            }

            if resolved_urls.is_empty() {
                let _ = tx_msg
                    .send(AppMessage::ModelCoverLoadFailed(version_id))
                    .await;
                return;
            }

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
            let request_key = render_request_key(render_request, debug_config.media_quality);

            running_ids.insert(version_id);
            let handle = tokio::spawn(async move {
                let cover_cache_root = model_cover_cache_path.lock().await.clone();
                let result = load_model_cover_results(
                    version_id,
                    resolved_urls,
                    ModelCoverLoadContext {
                        request_key,
                        client: req_client,
                        picker,
                        cover_cache_root,
                        debug_config,
                        use_cache: use_cover_cache,
                    },
                )
                .await;
                let _ = tx_msg.send(result).await;
                let _ = done_tx.send(version_id).await;
            });
            running_handles.insert(version_id, handle);

            if running_ids.len() >= max_in_flight && !running_ids.contains(&version_id) {
                let to_pause = running_ids
                    .iter()
                    .copied()
                    .find(|id| Some(*id) != *focus_version);
                if let Some(pause_id) = to_pause {
                    if let Some(handle) = running_handles.remove(&pause_id) {
                        handle.abort();
                    }
                    let _ = running_ids.remove(&pause_id);

                    if let Some(url) = known_version_urls.get(&pause_id).cloned() {
                        enqueue_or_bump_queue(queue, queued_ids, pause_id, url, false);
                    }
                }
            }
        }
    }
}
