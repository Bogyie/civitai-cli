use crate::tui::worker::cache::{image_detail_cache_path, is_cache_valid_minutes, now_unix_secs};
use civitai_cli::sdk::SearchImageHit as ImageItem;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct CachedImageDetail {
    pub image: ImageItem,
    pub cached_at_unix_secs: u64,
}

pub(super) fn load_cached_image_detail(
    cache_root: &Path,
    image_id: u64,
) -> Option<CachedImageDetail> {
    let path = image_detail_cache_path(cache_root, image_id);
    let bytes = std::fs::read(path).ok()?;
    let config = bincode::config::standard();
    bincode::serde::decode_from_slice::<CachedImageDetail, _>(&bytes, config)
        .ok()
        .map(|(entry, _)| entry)
}

pub(super) fn persist_cached_image_detail(cache_root: &Path, image: &ImageItem) {
    let path = image_detail_cache_path(cache_root, image.id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let entry = CachedImageDetail {
        image: image.clone(),
        cached_at_unix_secs: now_unix_secs(),
    };
    let config = bincode::config::standard();
    if let Ok(encoded) = bincode::serde::encode_to_vec(&entry, config) {
        let _ = std::fs::write(path, encoded);
    }
}

pub(super) fn fill_image_detail_from_cache(mut image: ImageItem, cached: &ImageItem) -> ImageItem {
    if image.url.is_none() {
        image.url = cached.url.clone();
    }
    if image.width.is_none() {
        image.width = cached.width;
    }
    if image.height.is_none() {
        image.height = cached.height;
    }
    if image.r#type.is_none() {
        image.r#type = cached.r#type.clone();
    }
    if image.created_at.is_none() {
        image.created_at = cached.created_at.clone();
    }
    if image.prompt.is_none() {
        image.prompt = cached.prompt.clone();
    }
    if image.base_model.is_none() {
        image.base_model = cached.base_model.clone();
    }
    if image.hash.is_none() {
        image.hash = cached.hash.clone();
    }
    if image.hide_meta.is_none() {
        image.hide_meta = cached.hide_meta;
    }
    if image.user.is_none() {
        image.user = cached.user.clone();
    }
    if image.stats.is_none() {
        image.stats = cached.stats.clone();
    }
    if image.tag_names.is_empty() {
        image.tag_names = cached.tag_names.clone();
    }
    if image.model_version_ids.is_empty() {
        image.model_version_ids = cached.model_version_ids.clone();
    }
    if image.nsfw_level.is_none() {
        image.nsfw_level = cached.nsfw_level;
    }
    if image.browsing_level.is_none() {
        image.browsing_level = cached.browsing_level;
    }
    if image.sort_at.is_none() {
        image.sort_at = cached.sort_at.clone();
    }
    if image.sort_at_unix.is_none() {
        image.sort_at_unix = cached.sort_at_unix;
    }
    if let Some(cached_metadata) = cached.metadata.clone() {
        image.metadata = Some(match image.metadata.take() {
            Some(existing) => merge_json_objects(existing, cached_metadata),
            None => cached_metadata,
        });
    }
    if image.generation_process.is_none() {
        image.generation_process = cached.generation_process.clone();
    }
    if image.ai_nsfw_level.is_none() {
        image.ai_nsfw_level = cached.ai_nsfw_level;
    }
    if image.combined_nsfw_level.is_none() {
        image.combined_nsfw_level = cached.combined_nsfw_level;
    }
    if image.thumbnail_url.is_none() {
        image.thumbnail_url = cached.thumbnail_url.clone();
    }
    image
}

pub(super) fn merge_json_objects(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.remove(&key) {
                    Some(base_value) => {
                        base_map.insert(key, merge_json_objects(base_value, overlay_value));
                    }
                    None => {
                        base_map.insert(key, overlay_value);
                    }
                }
            }
            Value::Object(base_map)
        }
        (_, overlay) => overlay,
    }
}

pub(super) fn attach_generation_data(
    image: &mut ImageItem,
    generation: &civitai_cli::sdk::ImageGenerationData,
) {
    if image.generation_process.is_none() {
        image.generation_process = generation.process.clone();
    }

    let attachment = generation.as_metadata_attachment();
    image.metadata = Some(match image.metadata.take() {
        Some(existing) => merge_json_objects(existing, attachment),
        None => attachment,
    });
}

pub(super) async fn enrich_image_detail(
    sdk: &civitai_cli::sdk::WebSearchClient,
    cache_root: Option<&Path>,
    image: ImageItem,
    ttl_minutes: u64,
) -> ImageItem {
    let mut image = if let Some(cache_root) = cache_root {
        match load_cached_image_detail(cache_root, image.id) {
            Some(entry) if is_cache_valid_minutes(entry.cached_at_unix_secs, ttl_minutes) => {
                return fill_image_detail_from_cache(image, &entry.image);
            }
            Some(_) => {
                let _ = std::fs::remove_file(image_detail_cache_path(cache_root, image.id));
                image
            }
            None => image,
        }
    } else {
        image
    };

    if let Ok(generation) = sdk.get_generation_data(image.id).await {
        attach_generation_data(&mut image, &generation);
    }

    if let Some(cache_root) = cache_root {
        persist_cached_image_detail(cache_root, &image);
    }
    image
}
