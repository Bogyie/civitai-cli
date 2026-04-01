use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::config::AppConfig;
use civitai_cli::sdk::{
    CIVITAI_MEDIA_DELIVERY_NAMESPACE, CIVITAI_MEDIA_DELIVERY_URL, SearchModelHit,
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
    value_name(hit.category.as_ref())
}

pub fn tag_names(hit: &SearchModelHit) -> Vec<String> {
    hit.tags
        .as_ref()
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| value_name(Some(tag)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn model_metrics(hit: &SearchModelHit) -> ParsedModelMetrics {
    parse_metrics(hit.metrics.as_ref())
}

pub fn model_versions(hit: &SearchModelHit) -> Vec<ParsedModelVersion> {
    let mut versions = Vec::new();
    let top_level_metrics = model_metrics(hit);

    if let Some(value) = hit.version.as_ref() {
        if let Some(version) = parse_version(value) {
            push_or_merge_version(&mut versions, version);
        }
    }

    if let Some(items) = hit.versions.as_ref() {
        for item in items {
            if let Some(version) = parse_version(item) {
                push_or_merge_version(&mut versions, version);
            }
        }
    }

    if versions.is_empty() {
        if let Some(version_id) = hit.primary_model_version_id() {
            versions.push(ParsedModelVersion {
                id: version_id,
                name: format!("Version {version_id}"),
                base_model: hit
                    .version
                    .as_ref()
                    .and_then(|value| value_string(value.get("baseModel"))),
                ..ParsedModelVersion::default()
            });
        }
    }

    let top_level_images = parse_images_from_array(hit.images.as_ref());
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

        if let Some(first_version) = versions.first_mut() {
            if first_version.images.is_empty() {
                first_version.images = top_level_images.clone();
            }
        }
    }

    for version in &mut versions {
        merge_metrics(&mut version.stats, &top_level_metrics);
        if version.files.is_empty() {
            version.files = synthetic_files(hit);
        }
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
        .or_else(|| {
            hit.version
                .as_ref()
                .and_then(|value| value_string(value.get("baseModel")))
        })
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

pub fn resolve_download_target_dir(config: &AppConfig, hit: &SearchModelHit) -> PathBuf {
    let model_type = hit.r#type.as_deref().unwrap_or_default();
    let target_dir = match config.comfyui_path.as_ref() {
        Some(base) => {
            let sub_dir = match model_type {
                "Checkpoint" => "models/checkpoints",
                "LORA" => "models/loras",
                "TextualInversion" => "models/embeddings",
                "Controlnet" => "models/controlnet",
                "VAE" => "models/vae",
                _ => "models/uncategorized",
            };
            base.join(sub_dir)
        }
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
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
    let original = file.name.trim().to_string();
    if original.is_empty() {
        return hit.default_download_file_name();
    }

    let base_model = version
        .base_model
        .as_deref()
        .map(|value| value.replace(' ', ""))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Model".to_string());
    let base_model_tag = format!("[{base_model}]");

    let path = Path::new(&original);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    if ext.is_empty() {
        format!("{base_model_tag}_{stem}")
    } else {
        format!("{base_model_tag}_{stem}.{ext}")
    }
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

fn parse_version(value: &Value) -> Option<ParsedModelVersion> {
    let id = value_u64(value.get("id"))?;
    Some(ParsedModelVersion {
        id,
        name: value_string(value.get("name")).unwrap_or_else(|| format!("Version {id}")),
        base_model: value_string(value.get("baseModel")),
        created_at: value_string(value.get("createdAt")),
        updated_at: value_string(value.get("updatedAt")),
        early_access_time_frame: value_u64(value.get("earlyAccessTimeFrame")),
        description: value_string(value.get("description")),
        stats: parse_metrics(value.get("stats").or_else(|| value.get("metrics"))),
        files: parse_files(
            value
                .get("files")
                .or_else(|| value.get("downloadableFiles"))
                .or_else(|| value.get("modelFiles")),
        ),
        images: parse_images(value.get("images")),
    })
}

fn parse_metrics(value: Option<&Value>) -> ParsedModelMetrics {
    let Some(value) = value else {
        return ParsedModelMetrics::default();
    };

    ParsedModelMetrics {
        download_count: value_u64(value.get("downloadCount")).unwrap_or(0),
        thumbs_up_count: value_u64(value.get("thumbsUpCount")).unwrap_or(0),
        favorite_count: value_u64(value.get("favoriteCount")).unwrap_or(0),
        comment_count: value_u64(value.get("commentCount")).unwrap_or(0),
        collected_count: value_u64(value.get("collectedCount")).unwrap_or(0),
        tipped_amount_count: value_u64(value.get("tippedAmountCount")).unwrap_or(0),
        rating_count: value_u64(value.get("ratingCount")).unwrap_or(0),
        rating: value_f64(value.get("rating")).unwrap_or(0.0),
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

fn parse_files(value: Option<&Value>) -> Vec<ParsedModelFile> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| ParsedModelFile {
                    id: value_u64(item.get("id")),
                    name: value_string(item.get("name"))
                        .unwrap_or_else(|| "Unnamed file".to_string()),
                    file_type: value_string(item.get("type")),
                    size_kb: value_f64(item.get("sizeKB")),
                    format: item
                        .get("metadata")
                        .and_then(|metadata| value_string(metadata.get("format"))),
                    fp: item
                        .get("metadata")
                        .and_then(|metadata| value_string(metadata.get("fp"))),
                    primary: value_bool(item.get("primary")).unwrap_or(false),
                    download_url: value_string(item.get("downloadUrl")),
                    pickle_scan_result: value_string(item.get("pickleScanResult")),
                    virus_scan_result: value_string(item.get("virusScanResult")),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn synthetic_files(hit: &SearchModelHit) -> Vec<ParsedModelFile> {
    if hit.file_formats.is_empty() {
        return Vec::new();
    }

    hit.file_formats
        .iter()
        .enumerate()
        .map(|(idx, format)| ParsedModelFile {
            id: None,
            name: if idx == 0 {
                format!("Primary {format}")
            } else {
                format!("{format} file")
            },
            file_type: hit.r#type.clone(),
            size_kb: None,
            format: Some(format.clone()),
            fp: None,
            primary: idx == 0,
            download_url: None,
            pickle_scan_result: None,
            virus_scan_result: None,
        })
        .collect()
}

fn parse_images(value: Option<&Value>) -> Vec<ParsedModelImage> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = normalize_model_image_url(&value_string(item.get("url"))?)?;
                    Some(ParsedModelImage {
                        id: value_u64(item.get("id")),
                        model_version_id: value_u64(item.get("modelVersionId")),
                        url,
                        width: value_u64(item.get("width"))
                            .and_then(|value| u32::try_from(value).ok()),
                        height: value_u64(item.get("height"))
                            .and_then(|value| u32::try_from(value).ok()),
                        nsfw: value_string(item.get("nsfw")),
                        meta: item.get("meta").cloned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_images_from_array(value: Option<&Vec<Value>>) -> Vec<ParsedModelImage> {
    value
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = normalize_model_image_url(&value_string(item.get("url"))?)?;
                    Some(ParsedModelImage {
                        id: value_u64(item.get("id")),
                        model_version_id: value_u64(item.get("modelVersionId")),
                        url,
                        width: value_u64(item.get("width"))
                            .and_then(|value| u32::try_from(value).ok()),
                        height: value_u64(item.get("height"))
                            .and_then(|value| u32::try_from(value).ok()),
                        nsfw: value_string(item.get("nsfw")),
                        meta: item.get("meta").cloned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
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

fn value_name(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(text) = value_string(Some(value)) {
        return Some(text);
    }
    value_string(value.get("name"))
}

fn value_string(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_bool(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    match value {
        Value::Bool(value) => Some(*value),
        Value::Number(number) => Some(number.as_u64().unwrap_or(0) != 0),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn value_u64(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    match value {
        Value::Number(number) => number
            .as_u64()
            .or_else(|| number.as_i64().and_then(|value| u64::try_from(value).ok())),
        Value::String(text) => text.trim().parse::<u64>().ok(),
        Value::Bool(value) => Some(if *value { 1 } else { 0 }),
        _ => None,
    }
}

fn value_f64(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        _ => None,
    }
}
