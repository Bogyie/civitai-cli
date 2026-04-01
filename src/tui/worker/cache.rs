use crate::config::AppConfig;
use civitai_cli::sdk::{
    ImageSearchSortBy, ImageSearchState, ModelSearchState, SearchImageHit as ImageItem,
    SearchModelHit as Model,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) type SearchCache = HashMap<String, CachedSearchResult>;
pub(super) type ImageSearchCache = HashMap<String, CachedImageSearchResult>;

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct CachedSearchResult {
    pub cache_key: String,
    pub models: Vec<Model>,
    pub has_more: bool,
    pub next_page: Option<u32>,
    pub cached_at_unix_secs: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct CachedImageSearchResult {
    pub cache_key: String,
    pub items: Vec<ImageItem>,
    pub cached_at_unix_secs: u64,
}

pub(super) fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs()
}

pub(super) fn is_cache_valid(cached_at_unix_secs: u64, ttl_hours: u64) -> bool {
    if ttl_hours == 0 {
        return false;
    }

    let ttl_secs = ttl_hours.saturating_mul(3600);
    now_unix_secs().saturating_sub(cached_at_unix_secs) < ttl_secs
}

pub(super) fn is_cache_valid_minutes(cached_at_unix_secs: u64, ttl_minutes: u64) -> bool {
    if ttl_minutes == 0 {
        return false;
    }

    let ttl_secs = ttl_minutes.saturating_mul(60);
    now_unix_secs().saturating_sub(cached_at_unix_secs) < ttl_secs
}

pub(super) fn is_cache_valid_minutes_or_persistent(
    cached_at_unix_secs: u64,
    ttl_minutes: u64,
) -> bool {
    if ttl_minutes == 0 {
        return true;
    }

    let ttl_secs = ttl_minutes.saturating_mul(60);
    now_unix_secs().saturating_sub(cached_at_unix_secs) < ttl_secs
}

pub(super) fn model_search_cache_path(config: &AppConfig) -> Option<PathBuf> {
    config.search_cache_path()
}

pub(super) fn model_cover_cache_root(config: &AppConfig) -> Option<PathBuf> {
    config.model_cover_cache_path()
}

pub(super) fn image_search_cache_root(config: &AppConfig) -> Option<PathBuf> {
    config
        .search_cache_path()
        .map(|root| root.join("image_search"))
}

pub(super) fn image_bytes_cache_root(config: &AppConfig) -> Option<PathBuf> {
    config.image_cache_path().map(|root| root.join("bytes"))
}

pub(super) fn image_detail_cache_root(config: &AppConfig) -> Option<PathBuf> {
    config.image_cache_path().map(|root| root.join("details"))
}

pub(super) fn use_search_cache() -> bool {
    !cfg!(debug_assertions)
}

pub(super) fn normalize_cache_segment(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(super) fn normalize_search_options_for_cache(mut opts: ModelSearchState) -> ModelSearchState {
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

pub(super) fn normalize_image_search_options_for_cache(
    mut opts: ImageSearchState,
) -> ImageSearchState {
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

pub(super) fn cache_key_for_options(opts: &ModelSearchState) -> String {
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
        opts.limit
            .map(|value| value.to_string())
            .unwrap_or_default()
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

pub(super) fn build_search_url(opts: &ModelSearchState) -> String {
    opts.to_web_url("https://civitai.com/search/models")
        .map(|url| url.to_string())
        .unwrap_or_else(|_| "https://civitai.com/search/models".to_string())
}

pub(super) fn build_image_search_url(opts: &ImageSearchState) -> String {
    opts.to_web_url("https://civitai.com/search/images")
        .map(|url| url.to_string())
        .unwrap_or_else(|_| "https://civitai.com/search/images".to_string())
}

pub(super) fn cache_entry_path(cache_root: &Path, cache_key: &str) -> PathBuf {
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

pub(super) fn load_search_cache(path: Option<&Path>, ttl_hours: u64) -> SearchCache {
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

pub(super) fn save_search_cache(path: Option<&Path>, cache: &SearchCache, ttl_hours: u64) {
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

pub(super) fn persist_search_cache_entry(cache_root: &Path, entry: &CachedSearchResult) {
    let _ = std::fs::create_dir_all(cache_root);
    let data_path = cache_entry_path(cache_root, &entry.cache_key);
    let config = bincode::config::standard();
    if let Ok(bytes) = bincode::serde::encode_to_vec(entry, config) {
        let _ = std::fs::write(data_path, bytes);
    }
}

pub(super) fn prune_search_cache(cache: &mut SearchCache, ttl_hours: u64) -> usize {
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

pub(super) fn clear_cached_search_cache(cache_root: &Path) -> usize {
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

pub(super) fn load_cached_search_entry(
    cache_root: &Path,
    cache_key: &str,
) -> Option<CachedSearchResult> {
    let cache_path = cache_entry_path(cache_root, cache_key);
    let bytes = std::fs::read(cache_path).ok()?;
    let config = bincode::config::standard();
    bincode::serde::decode_from_slice::<CachedSearchResult, _>(&bytes, config)
        .ok()
        .map(|(entry, _)| entry)
}

pub(super) fn has_more_from_response(
    limit: Option<u32>,
    offset: Option<u32>,
    total: Option<u64>,
) -> bool {
    let limit = limit.unwrap_or(50) as u64;
    let offset = offset.unwrap_or(0) as u64;
    total.is_some_and(|total| offset.saturating_add(limit) < total)
}

pub(super) fn remove_cached_search_entry(cache_root: &Path, cache_key: &str) {
    let cache_path = cache_entry_path(cache_root, cache_key);
    let _ = std::fs::remove_file(cache_path);
}

pub(super) fn load_image_search_cache(path: Option<&Path>, ttl_minutes: u64) -> ImageSearchCache {
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

pub(super) fn save_image_search_cache(
    path: Option<&Path>,
    cache: &ImageSearchCache,
    ttl_minutes: u64,
) {
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

pub(super) fn prune_image_search_cache(cache: &mut ImageSearchCache, ttl_minutes: u64) -> usize {
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

pub(super) fn load_cached_image_search_entry(
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

pub(super) fn model_cover_cache_path(
    cache_root: &Path,
    version_id: u64,
    image_url: &str,
) -> PathBuf {
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

pub(super) fn image_bytes_cache_path(cache_root: &Path, image_id: u64, image_url: &str) -> PathBuf {
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

pub(super) fn image_detail_cache_path(cache_root: &Path, image_id: u64) -> PathBuf {
    cache_root.join(format!("img{image_id}.bin"))
}

pub(super) fn image_search_cache_ttl_minutes_for(
    state: &ImageSearchState,
    configured_ttl_minutes: u64,
) -> u64 {
    if matches!(state.sort_by, ImageSearchSortBy::Newest) {
        0
    } else {
        configured_ttl_minutes
    }
}
