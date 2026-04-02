use crate::config::{AppConfig, MediaQualityPreference};
use crate::tui::app::{AppMessage, MediaRenderRequest};
use crate::tui::image::image_media_url;
use crate::tui::runtime::debug_fetch_log;
use crate::tui::worker::cache::{
    image_bytes_cache_path, is_cache_valid_minutes_or_persistent, model_cover_cache_path,
    now_unix_secs,
};
use anyhow::Result;
use bytes::Bytes;
use civitai_cli::sdk::{
    ApiImageSearchOptions, MediaUrlOptions, SearchImageHit as ImageItem,
    media_url_from_raw_with_options,
};
use ratatui::layout::Rect;
use ratatui_image::Resize;
use ratatui_image::picker::Picker;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Semaphore, mpsc};

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct CachedBinaryBlob {
    pub cached_at_unix_secs: u64,
    pub bytes: Vec<u8>,
}

pub(super) struct ModelCoverLoadContext {
    pub request_key: String,
    pub client: Client,
    pub picker: Picker,
    pub cover_cache_root: Option<PathBuf>,
    pub debug_config: AppConfig,
    pub use_cache: bool,
}

pub(super) struct FeedImageLoadContext {
    pub request_key: String,
    pub client: Client,
    pub picker: Picker,
    pub tx_msg: mpsc::Sender<AppMessage>,
    pub semaphore: Arc<Semaphore>,
    pub debug_config: AppConfig,
    pub image_bytes_cache_root: Option<PathBuf>,
    pub ttl_minutes: u64,
    pub use_cache: bool,
    pub render_area: Rect,
}

const MODEL_VERSION_COVER_IMAGE_LIMIT: usize = 1;

fn media_quality_scale(preference: MediaQualityPreference) -> f32 {
    match preference {
        MediaQualityPreference::Low => 0.7,
        MediaQualityPreference::Medium => 0.95,
        MediaQualityPreference::High => 1.25,
        MediaQualityPreference::Original => 1.0,
    }
}

fn media_quality_value(preference: MediaQualityPreference) -> Option<u8> {
    match preference {
        MediaQualityPreference::Low => Some(55),
        MediaQualityPreference::Medium => Some(72),
        MediaQualityPreference::High => Some(85),
        MediaQualityPreference::Original => None,
    }
}

fn resolve_dynamic_media_target(
    request: MediaRenderRequest,
    source_dims: Option<(u32, u32)>,
    preference: MediaQualityPreference,
) -> (u32, u32, Option<u8>) {
    let scale = media_quality_scale(preference);
    let mut target_width = ((request.width as f32) * scale).round().max(1.0) as u32;
    let mut target_height = ((request.height as f32) * scale).round().max(1.0) as u32;
    let mut quality = media_quality_value(preference);

    if let Some((source_width, source_height)) = source_dims {
        target_width = target_width.min(source_width.max(1));
        target_height = target_height.min(source_height.max(1));

        let width_ratio = source_width as f32 / target_width.max(1) as f32;
        let height_ratio = source_height as f32 / target_height.max(1) as f32;
        let downscale_ratio = width_ratio.max(height_ratio);

        if let Some(base_quality) = quality {
            let adjusted = if downscale_ratio >= 3.5 {
                base_quality.saturating_sub(14)
            } else if downscale_ratio >= 2.4 {
                base_quality.saturating_sub(10)
            } else if downscale_ratio >= 1.6 {
                base_quality.saturating_sub(5)
            } else if downscale_ratio <= 1.05 {
                base_quality.saturating_add(4).min(95)
            } else {
                base_quality
            };
            quality = Some(adjusted);
        }
    }

    (target_width.max(1), target_height.max(1), quality)
}

fn build_media_options_for_display(
    request: MediaRenderRequest,
    source_dims: Option<(u32, u32)>,
    preference: MediaQualityPreference,
    is_video: bool,
) -> MediaUrlOptions {
    if preference == MediaQualityPreference::Original {
        return MediaUrlOptions {
            original: Some(true),
            ..Default::default()
        };
    }

    let (width, height, quality) = resolve_dynamic_media_target(request, source_dims, preference);

    if is_video {
        MediaUrlOptions {
            original: Some(false),
            width: Some(width),
            height: Some(height),
            quality,
            optimized: Some(true),
            anim: Some(false),
            ..Default::default()
        }
    } else {
        MediaUrlOptions {
            width: Some(width),
            height: Some(height),
            quality,
            optimized: Some(true),
            anim: Some(false),
            ..Default::default()
        }
    }
}

fn build_cover_media_options(
    request: MediaRenderRequest,
    source_dims: Option<(u32, u32)>,
    preference: MediaQualityPreference,
) -> MediaUrlOptions {
    if preference == MediaQualityPreference::Original {
        return MediaUrlOptions {
            original: Some(true),
            ..Default::default()
        };
    }

    let (width, height, quality) = resolve_dynamic_media_target(request, source_dims, preference);
    MediaUrlOptions {
        width: Some(width),
        height: Some(height),
        quality,
        optimized: Some(true),
        anim: Some(false),
        ..Default::default()
    }
}

pub(super) fn build_image_display_url(
    item: &ImageItem,
    request: MediaRenderRequest,
    preference: MediaQualityPreference,
) -> Option<String> {
    let source_dims = item.width.zip(item.height);
    if item.r#type.as_deref() == Some("video") {
        item.thumbnail_url
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                item.media_url_with_options(&build_media_options_for_display(
                    request,
                    source_dims,
                    preference,
                    true,
                ))
            })
    } else if item.media_token().is_some() {
        item.media_url_with_options(&build_media_options_for_display(
            request,
            source_dims,
            preference,
            false,
        ))
    } else {
        image_media_url(item)
    }
}

pub(super) fn rewrite_cover_url_for_display(
    raw_url: &str,
    source_dims: Option<(u32, u32)>,
    request: MediaRenderRequest,
    preference: MediaQualityPreference,
) -> Option<String> {
    media_url_from_raw_with_options(
        raw_url,
        &build_cover_media_options(request, source_dims, preference),
    )
    .or_else(|| Some(raw_url.to_string()))
}

pub(super) fn load_cached_model_cover(path: &std::path::Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        let _ = std::fs::remove_file(path);
        None
    } else {
        Some(bytes)
    }
}

pub(super) fn decode_model_cover(
    bytes: &[u8],
    picker: &Picker,
) -> Option<ratatui_image::protocol::StatefulProtocol> {
    let img = image::load_from_memory(bytes).ok()?;
    Some(picker.new_resize_protocol(img))
}

pub(super) fn persist_model_cover_cache(path: &std::path::Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, bytes);
}

pub(super) fn load_cached_feed_image(path: &std::path::Path, ttl_minutes: u64) -> Option<Vec<u8>> {
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

pub(super) fn persist_cached_feed_image(path: &std::path::Path, bytes: &[u8]) {
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

pub(super) async fn fetch_image_bytes_with_debug(
    client: &Client,
    url: &str,
    debug_config: &AppConfig,
    context: &str,
) -> Result<Bytes> {
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

pub(super) async fn load_model_cover_result(
    version_id: u64,
    image_url: String,
    context: ModelCoverLoadContext,
) -> AppMessage {
    if context.use_cache
        && let Some(cache_root) = context.cover_cache_root.as_ref()
    {
        let cache_path = model_cover_cache_path(cache_root, version_id, &image_url);
        if let Some(bytes) = load_cached_model_cover(&cache_path) {
            if let Some(protocol) = decode_model_cover(&bytes, &context.picker) {
                debug_fetch_log(
                    &context.debug_config,
                    &format!(
                        "Model cover loaded from cache: version_id={}, url={}",
                        version_id, image_url
                    ),
                );
                return AppMessage::ModelCoverDecoded(
                    version_id,
                    protocol,
                    bytes,
                    context.request_key.clone(),
                );
            }

            let _ = std::fs::remove_file(cache_path);
        }
    }

    match fetch_image_bytes_with_debug(
        &context.client,
        &image_url,
        &context.debug_config,
        "Model cover",
    )
    .await
    {
        Ok(bytes) => {
            debug_fetch_log(
                &context.debug_config,
                &format!(
                    "Model cover fetched: version_id={}, bytes={}",
                    version_id,
                    bytes.len()
                ),
            );
            if let Some(protocol) = decode_model_cover(&bytes, &context.picker) {
                if context.use_cache
                    && let Some(cache_root) = context.cover_cache_root.as_ref()
                {
                    let cache_path = model_cover_cache_path(cache_root, version_id, &image_url);
                    persist_model_cover_cache(&cache_path, &bytes);
                }
                return AppMessage::ModelCoverDecoded(
                    version_id,
                    protocol,
                    bytes.to_vec(),
                    context.request_key.clone(),
                );
            }
            debug_fetch_log(
                &context.debug_config,
                &format!(
                    "Model cover decode failed: version_id={}, url={}",
                    version_id, image_url
                ),
            );
        }
        Err(e) => {
            debug_fetch_log(
                &context.debug_config,
                &format!(
                    "Model cover fetch failed: version_id={}, url={}, err={}",
                    version_id, image_url, e
                ),
            );
        }
    }

    AppMessage::ModelCoverLoadFailed(version_id)
}

pub(super) async fn load_model_cover_results(
    version_id: u64,
    image_urls: Vec<String>,
    context: ModelCoverLoadContext,
) -> AppMessage {
    let mut protocols = Vec::new();

    for image_url in image_urls {
        if context.use_cache
            && let Some(cache_root) = context.cover_cache_root.as_ref()
        {
            let cache_path = model_cover_cache_path(cache_root, version_id, &image_url);
            if let Some(bytes) = load_cached_model_cover(&cache_path) {
                if let Some(protocol) = decode_model_cover(&bytes, &context.picker) {
                    protocols.push((protocol, bytes));
                    continue;
                }
                let _ = std::fs::remove_file(cache_path);
            }
        }

        match fetch_image_bytes_with_debug(
            &context.client,
            &image_url,
            &context.debug_config,
            "Model cover",
        )
        .await
        {
            Ok(bytes) => {
                if let Some(protocol) = decode_model_cover(&bytes, &context.picker) {
                    if context.use_cache
                        && let Some(cache_root) = context.cover_cache_root.as_ref()
                    {
                        let cache_path = model_cover_cache_path(cache_root, version_id, &image_url);
                        persist_model_cover_cache(&cache_path, &bytes);
                    }
                    protocols.push((protocol, bytes.to_vec()));
                }
            }
            Err(e) => {
                debug_fetch_log(
                    &context.debug_config,
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
        AppMessage::ModelCoversDecoded(version_id, protocols, context.request_key)
    }
}

pub(super) async fn fetch_cover_urls_for_version(
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

pub(super) async fn load_feed_image(
    image_id: u64,
    image_url: String,
    context: FeedImageLoadContext,
) {
    if context.use_cache
        && let Some(cache_root) = context.image_bytes_cache_root.as_ref()
    {
        let cache_path = image_bytes_cache_path(cache_root, image_id, &image_url);
        if let Some(bytes) = load_cached_feed_image(&cache_path, context.ttl_minutes) {
            debug_fetch_log(
                &context.debug_config,
                &format!(
                    "Image feed loaded from cache: id={}, url={}",
                    image_id, image_url
                ),
            );
            if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                if let Ok(protocol) =
                    context
                        .picker
                        .new_protocol(dyn_img, context.render_area, Resize::Scale(None))
                {
                    let _ = context
                        .tx_msg
                        .send(AppMessage::ImageDecoded(
                            image_id,
                            protocol,
                            bytes,
                            context.request_key.clone(),
                        ))
                        .await;
                    return;
                }
            }
            let _ = std::fs::remove_file(cache_path);
        }
    }

    let _permit = match context.semaphore.acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => return,
    };

    match fetch_image_bytes_with_debug(
        &context.client,
        &image_url,
        &context.debug_config,
        "Image feed",
    )
    .await
    {
        Ok(bytes) => {
            debug_fetch_log(
                &context.debug_config,
                &format!("Image feed fetched: id={}, bytes={}", image_id, bytes.len()),
            );
            if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                if context.use_cache
                    && let Some(cache_root) = context.image_bytes_cache_root.as_ref()
                {
                    let cache_path = image_bytes_cache_path(cache_root, image_id, &image_url);
                    persist_cached_feed_image(&cache_path, &bytes);
                }
                if let Ok(protocol) =
                    context
                        .picker
                        .new_protocol(dyn_img, context.render_area, Resize::Scale(None))
                {
                    let _ = context
                        .tx_msg
                        .send(AppMessage::ImageDecoded(
                            image_id,
                            protocol,
                            bytes.to_vec(),
                            context.request_key.clone(),
                        ))
                        .await;
                } else {
                    debug_fetch_log(
                        &context.debug_config,
                        &format!("Image feed protocol encode failed: id={}", image_id),
                    );
                }
            } else {
                debug_fetch_log(
                    &context.debug_config,
                    &format!("Image feed decode failed: id={}", image_id),
                );
            }
        }
        Err(e) => {
            debug_fetch_log(
                &context.debug_config,
                &format!(
                    "Image feed fetch failed: id={}, url={}, err={}",
                    image_id, image_url, e
                ),
            );
        }
    }
}
