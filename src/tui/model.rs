use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::config::AppConfig;
use civitai_cli::sdk::{
    CIVITAI_MEDIA_DELIVERY_NAMESPACE, CIVITAI_MEDIA_DELIVERY_URL, SearchModelCategory,
    SearchModelFile, SearchModelHit, SearchModelImage, SearchModelMetrics, SearchModelTag,
    SearchModelVersion,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedModelMetrics {
    pub download_count: u64,
    pub thumbs_up_count: u64,
    pub favorite_count: u64,
    pub comment_count: u64,
    pub collected_count: u64,
    pub tipped_amount_count: u64,
    pub rating_count: u64,
    pub rating: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedModelFile {
    pub id: Option<u64>,
    pub name: String,
    pub file_type: Option<String>,
    pub size_kb: Option<f64>,
    pub format: Option<String>,
    pub fp: Option<String>,
    pub primary: bool,
    pub download_url: Option<String>,
    pub pickle_scan_result: Option<String>,
    pub virus_scan_result: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedModelImage {
    pub id: Option<u64>,
    pub model_version_id: Option<u64>,
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub nsfw: Option<String>,
    pub meta: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedModelVersion {
    pub id: u64,
    pub name: String,
    pub base_model: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub early_access_time_frame: Option<u64>,
    pub description: Option<String>,
    pub stats: ParsedModelMetrics,
    pub files: Vec<ParsedModelFile>,
    pub images: Vec<ParsedModelImage>,
}

pub fn model_name(hit: &SearchModelHit) -> String {
    hit.name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("Model {}", hit.id))
}

pub fn creator_name(hit: &SearchModelHit) -> Option<String> {
    hit.user
        .as_ref()
        .and_then(|user| user.username.clone())
        .filter(|value| !value.trim().is_empty())
}

pub fn category_name(hit: &SearchModelHit) -> Option<String> {
    hit.category
        .as_ref()
        .and_then(SearchModelCategory::name)
        .map(str::to_string)
}

pub fn tag_names(hit: &SearchModelHit) -> Vec<String> {
    hit.tags
        .iter()
        .filter_map(SearchModelTag::name)
        .map(str::to_string)
        .collect()
}

pub fn model_metrics(hit: &SearchModelHit) -> ParsedModelMetrics {
    parse_metrics(hit.metrics.as_ref())
}

pub fn model_versions(hit: &SearchModelHit) -> Vec<ParsedModelVersion> {
    let mut versions = Vec::new();
    let top_level_metrics = model_metrics(hit);

    if let Some(value) = hit.version.as_ref()
        && let Some(version) = parse_version(value)
    {
        push_or_merge_version(&mut versions, version);
    }

    for item in &hit.versions {
        if let Some(version) = parse_version(item) {
            push_or_merge_version(&mut versions, version);
        }
    }

    if versions.is_empty()
        && let Some(version_id) = hit.primary_model_version_id()
    {
        versions.push(ParsedModelVersion {
            id: version_id,
            name: format!("Version {version_id}"),
            base_model: hit.version.as_ref().and_then(|value| value.base_model.clone()),
            ..ParsedModelVersion::default()
        });
    }

    let top_level_images = parse_images_from_array(&hit.images);
    if !top_level_images.is_empty() {
        for version in &mut versions {
            if version.images.is_empty() {
                let matching = top_level_images
                    .iter()
                    .filter(|image| image.model_version_id == Some(version.id))
                    .cloned()
                    .collect::<Vec<_>>();
                if !matching.is_empty() {
                    version.images = matching;
                }
            }
        }

        if let Some(first_version) = versions.first_mut()
            && first_version.images.is_empty()
        {
            first_version.images = top_level_images.clone();
        }
    }

    for version in &mut versions {
        merge_metrics(&mut version.stats, &top_level_metrics);
    }

    versions
}

pub fn selected_version(hit: &SearchModelHit, index: usize) -> Option<ParsedModelVersion> {
    let versions = model_versions(hit);
    if versions.is_empty() {
        return None;
    }
    let safe_index = index.min(versions.len().saturating_sub(1));
    versions.get(safe_index).cloned()
}

pub fn default_base_model(hit: &SearchModelHit) -> Option<String> {
    selected_version(hit, 0)
        .and_then(|version| version.base_model)
        .or_else(|| hit.version.as_ref().and_then(|value| value.base_model.clone()))
}

pub fn build_model_url(hit: &SearchModelHit, version_id: Option<u64>) -> String {
    match version_id {
        Some(version_id) => format!(
            "https://civitai.com/models/{}?modelVersionId={version_id}",
            hit.id
        ),
        None => hit.model_page_url(),
    }
}

pub fn resolve_model_download_target_dir(
    config: &AppConfig,
    model_type: Option<&str>,
    base_model: Option<&str>,
) -> PathBuf {
    let target_dir = match config.comfyui_path.as_ref() {
        Some(base) => base
            .join("models")
            .join(normalize_model_type_folder(model_type))
            .join(normalize_base_model_component(base_model.unwrap_or("unknown-base"))),
        None => std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("downloads")
            .join("models")
            .join(normalize_model_type_folder(model_type))
            .join(normalize_base_model_component(base_model.unwrap_or("unknown-base"))),
    };

    if !target_dir.exists() {
        let _ = std::fs::create_dir_all(&target_dir);
    }

    target_dir
}

pub fn resolve_image_download_target_dir(config: &AppConfig) -> PathBuf {
    let target_dir = match config.comfyui_path.as_ref() {
        Some(base) => base.join("input").join("civitai-cli"),
        None => std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("downloads")
            .join("images"),
    };

    if !target_dir.exists() {
        let _ = std::fs::create_dir_all(&target_dir);
    }

    target_dir
}

pub fn build_download_file_name(
    hit: &SearchModelHit,
    version: &ParsedModelVersion,
    file: &ParsedModelFile,
) -> String {
    build_model_download_file_name(
        hit.name.as_deref(),
        Some(version.name.as_str()),
        &file.name,
        &hit.default_download_file_name(),
    )
}

pub fn build_model_download_file_name(
    model_name: Option<&str>,
    version_name: Option<&str>,
    original_file_name: &str,
    fallback: &str,
) -> String {
    let original = original_file_name.trim();
    let path = Path::new(original);
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    let model_part = normalize_path_component(model_name.unwrap_or("unknown-model"));
    let version_part = normalize_path_component(version_name.unwrap_or("unknown-version"));
    let safe_stem = if original.is_empty() {
        sanitize_path_component(fallback)
    } else {
        sanitize_path_component(&format!("{model_part}_{version_part}"))
    };
    if ext.is_empty() {
        safe_stem
    } else {
        format!("{}.{}", safe_stem, sanitize_path_component(&ext))
    }
}

pub fn normalize_model_type_folder(model_type: Option<&str>) -> String {
    match model_type.unwrap_or_default().to_ascii_lowercase().as_str() {
        "checkpoint" => "checkpoints".to_string(),
        "lora" => "loras".to_string(),
        "textualinversion" => "embeddings".to_string(),
        "controlnet" => "controlnet".to_string(),
        "vae" => "vae".to_string(),
        other if !other.is_empty() => sanitize_path_component(other),
        _ => "uncategorized".to_string(),
    }
}

pub fn normalize_path_component(value: &str) -> String {
    let normalized = sanitize_path_component(value);
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized
    }
}

pub fn normalize_base_model_component(value: &str) -> String {
    normalize_path_component(value).to_ascii_lowercase()
}

pub fn sanitize_path_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect::<String>();
    sanitized.trim().trim_matches('.').to_string()
}

fn push_or_merge_version(versions: &mut Vec<ParsedModelVersion>, incoming: ParsedModelVersion) {
    if let Some(existing) = versions
        .iter_mut()
        .find(|version| version.id == incoming.id)
    {
        if existing.name.is_empty() && !incoming.name.is_empty() {
            existing.name = incoming.name;
        }
        if existing.base_model.is_none() {
            existing.base_model = incoming.base_model;
        }
        if existing.created_at.is_none() {
            existing.created_at = incoming.created_at;
        }
        if existing.updated_at.is_none() {
            existing.updated_at = incoming.updated_at;
        }
        if existing.description.is_none() {
            existing.description = incoming.description;
        }
        if existing.early_access_time_frame.is_none() {
            existing.early_access_time_frame = incoming.early_access_time_frame;
        }
        if existing.images.is_empty() {
            existing.images = incoming.images;
        }
        if existing.files.is_empty() {
            existing.files = incoming.files;
        }
        merge_metrics(&mut existing.stats, &incoming.stats);
    } else {
        versions.push(incoming);
    }
}


fn parse_version(value: &SearchModelVersion) -> Option<ParsedModelVersion> {
    let id = value.id;
    Some(ParsedModelVersion {
        id,
        name: value
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("Version {id}")),
        base_model: value.base_model.clone(),
        created_at: value.created_at.clone(),
        updated_at: value.updated_at.clone(),
        early_access_time_frame: value.early_access_time_frame,
        description: value.description.clone(),
        stats: parse_metrics(value.stats.as_ref()),
        files: parse_files(&value.files),
        images: parse_images(&value.images),
    })
}

fn parse_metrics(value: Option<&SearchModelMetrics>) -> ParsedModelMetrics {
    let Some(value) = value else {
        return ParsedModelMetrics::default();
    };

    ParsedModelMetrics {
        download_count: value.download_count,
        thumbs_up_count: value.thumbs_up_count,
        favorite_count: value.favorite_count,
        comment_count: value.comment_count,
        collected_count: value.collected_count,
        tipped_amount_count: value.tipped_amount_count,
        rating_count: value.rating_count,
        rating: value.rating,
    }
}

fn merge_metrics(target: &mut ParsedModelMetrics, incoming: &ParsedModelMetrics) {
    if target.download_count == 0 {
        target.download_count = incoming.download_count;
    }
    if target.thumbs_up_count == 0 {
        target.thumbs_up_count = incoming.thumbs_up_count;
    }
    if target.favorite_count == 0 {
        target.favorite_count = incoming.favorite_count;
    }
    if target.comment_count == 0 {
        target.comment_count = incoming.comment_count;
    }
    if target.collected_count == 0 {
        target.collected_count = incoming.collected_count;
    }
    if target.tipped_amount_count == 0 {
        target.tipped_amount_count = incoming.tipped_amount_count;
    }
    if target.rating_count == 0 {
        target.rating_count = incoming.rating_count;
    }
    if target.rating == 0.0 {
        target.rating = incoming.rating;
    }
}

fn parse_files(items: &[SearchModelFile]) -> Vec<ParsedModelFile> {
    items
        .iter()
        .map(|item| ParsedModelFile {
            id: item.id,
            name: item
                .name
                .clone()
                .unwrap_or_else(|| "Unnamed file".to_string()),
            file_type: item.file_type.clone(),
            size_kb: item.size_kb,
            format: item.metadata.as_ref().and_then(|metadata| metadata.format.clone()),
            fp: item.metadata.as_ref().and_then(|metadata| metadata.fp.clone()),
            primary: item.primary,
            download_url: item.download_url.clone(),
            pickle_scan_result: item.pickle_scan_result.clone(),
            virus_scan_result: item.virus_scan_result.clone(),
        })
        .collect()
}

fn parse_images(items: &[SearchModelImage]) -> Vec<ParsedModelImage> {
    items
        .iter()
        .filter_map(|item| {
            let url = normalize_model_image_url(&item.url)?;
            Some(ParsedModelImage {
                id: item.id,
                model_version_id: item.model_version_id,
                url,
                width: item.width,
                height: item.height,
                nsfw: item.nsfw.clone(),
                meta: item.meta.clone(),
            })
        })
        .collect()
}

fn parse_images_from_array(items: &[SearchModelImage]) -> Vec<ParsedModelImage> {
    parse_images(items)
}

fn normalize_model_image_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    if raw.starts_with("http://") || raw.starts_with("https://") {
        return Some(raw.to_string());
    }

    Some(format!(
        "{}/{}/{}/original=true",
        CIVITAI_MEDIA_DELIVERY_URL.trim_end_matches('/'),
        CIVITAI_MEDIA_DELIVERY_NAMESPACE.trim_matches('/'),
        raw
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[test]
    fn lowercases_base_model_folder_for_model_downloads() {
        let root = std::env::temp_dir().join("civitai-cli-model-path-test");
        let config = AppConfig {
            comfyui_path: Some(root.clone()),
            ..AppConfig::default()
        };

        let path = resolve_model_download_target_dir(&config, Some("LORA"), Some("Flux.1 Dev"));

        assert_eq!(path, root.join("models").join("loras").join("flux.1 dev"));
    }

    #[test]
    fn builds_model_file_name_from_model_and_version_names() {
        let name = build_model_download_file_name(
            Some("My Model"),
            Some("v1.0 release"),
            "weights.safetensors",
            "fallback.bin",
        );

        assert_eq!(name, "My Model_v1.0 release.safetensors");
    }

    #[test]
    fn leaves_files_empty_until_detail_payload_arrives() {
        let hit: SearchModelHit = serde_json::from_value(serde_json::json!({
            "id": 42,
            "fileFormats": ["SafeTensor"],
            "version": {
                "id": 7,
                "name": "v1"
            }
        }))
        .expect("valid search model hit");

        let versions = model_versions(&hit);

        assert_eq!(versions.len(), 1);
        assert!(versions[0].files.is_empty());
    }
}
