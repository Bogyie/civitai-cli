use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;

fn as_string_from_serde<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    let value = match value {
        Some(value) => value,
        None => return Ok(None),
    };

    Ok(Some(match value {
        Value::String(s) => s,
        Value::Number(num) => num.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => return Ok(None),
        _ => value.to_string(),
    }))
}

fn as_u64_from_serde<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    let value = match value {
        Some(value) => value,
        None => return Ok(0),
    };

    Ok(match value {
        Value::Null => 0,
        Value::Bool(value) => {
            if value {
                1
            } else {
                0
            }
        }
        Value::Number(num) => num
            .as_u64()
            .or_else(|| num.as_i64().and_then(|v| u64::try_from(v).ok()))
            .unwrap_or(0),
        Value::String(value) => value.parse::<f64>().ok().map_or(0, |num| {
            if num.is_finite() && num >= 0.0 {
                num.round() as u64
            } else {
                0
            }
        }),
        _ => 0,
    })
}

fn as_f64_from_serde<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    let value = match value {
        Some(value) => value,
        None => return Ok(0.0),
    };

    Ok(match value {
        Value::Null => 0.0,
        Value::Bool(value) => {
            if value {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(num) => num.as_f64().unwrap_or(0.0),
        Value::String(value) => value.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    })
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: u64,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub r#type: String, // e.g., "LORA", "Checkpoint"
    #[serde(default)]
    pub nsfw: bool,
    #[serde(default)]
    pub tags: Vec<ModelTag>,
    pub stats: Option<ModelStats>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub creator: Option<ModelCreator>,
    #[serde(default)]
    pub allow_no_credit: Option<bool>,
    #[serde(default)]
    pub allow_commercial_use: Option<String>,
    #[serde(default)]
    pub allow_derivatives: Option<bool>,
    #[serde(default)]
    pub allow_different_license: Option<bool>,
    #[serde(default)]
    pub supports_generation: Option<bool>,
    #[serde(default)]
    pub poi: Option<bool>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub model_versions: Vec<ModelVersion>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelStats {
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub download_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub thumbs_up_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub thumbs_down_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub favorite_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub comment_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub comment_count_all: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub rating_count: u64,
    #[serde(default, deserialize_with = "as_f64_from_serde")]
    pub rating: f64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub comment_count_weekly: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum ModelTag {
    Name { name: String },
    NameOnly(String),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelCreator {
    pub username: String,
    pub image: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelVersion {
    pub id: u64,
    pub model_id: Option<u64>,
    pub name: String,
    #[serde(default)]
    pub base_model: String, // e.g., "SD 1.5", "SDXL"
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub early_access_time_frame: Option<u64>,
    #[serde(default)]
    pub trained_words: Option<Vec<String>>,
    #[serde(default)]
    pub description: Option<String>,
    pub stats: Option<VersionStats>,
    #[serde(default)]
    pub images: Vec<ModelImage>,
    #[serde(default)]
    pub files: Vec<ModelFile>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VersionStats {
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub download_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub favorite_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub comment_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub thumbs_up_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub thumbs_down_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub rating_count: u64,
    #[serde(default, deserialize_with = "as_f64_from_serde")]
    pub rating: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelImage {
    #[serde(default)]
    pub id: Option<u64>,
    pub url: String,
    #[serde(default)]
    pub nsfw: Option<NsfwValue>, // string ("None", "Soft", "Mature", "X") or bool
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub meta: Option<serde_json::Value>, // generation params
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum NsfwValue {
    Bool(bool),
    Text(String),
    Unknown,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub id: u64,
    pub name: String,
    #[serde(rename = "type")]
    pub file_type: Option<String>,
    #[serde(default)]
    pub primary: bool,
    #[serde(default, rename = "sizeKB", deserialize_with = "as_f64_from_serde")]
    pub size_kb: f64,
    pub metadata: Option<FileMetadata>,
    #[serde(default)]
    pub hashes: Option<HashMap<String, String>>,
    #[serde(default)]
    pub pickle_scan_result: Option<String>,
    #[serde(default)]
    pub pickle_scan_message: Option<String>,
    #[serde(default)]
    pub virus_scan_result: Option<String>,
    #[serde(default)]
    pub scanned_at: Option<String>,
    #[serde(default)]
    pub download_url: String,
}

impl Default for NsfwValue {
    fn default() -> Self {
        Self::Text("None".to_string())
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    pub format: Option<String>,
    pub size: Option<String>, // e.g., "pruned", "full"
    pub fp: Option<String>,   // e.g., "fp16", "fp32"
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub metadata: Option<PaginationMetadata>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMetadata {
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub total_items: Option<String>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub current_page: Option<String>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub page_size: Option<String>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub total_pages: Option<String>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub next_page: Option<String>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub prev_page: Option<String>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub next_cursor: Option<String>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub total: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImageItem {
    pub id: u64,
    pub url: String,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub hash: Option<String>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub r#type: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub nsfw: Option<bool>,
    #[serde(default)]
    pub nsfw_level: Option<String>,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub browsing_level: u64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub post_id: Option<u64>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub base_model: Option<String>,
    #[serde(default)]
    pub model_version_ids: Vec<u64>,
    #[serde(default)]
    pub stats: Option<ImageStats>,
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "as_string_from_serde")]
    pub username: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImageStats {
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub cry_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub laugh_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub like_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub dislike_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub heart_count: u64,
    #[serde(default, deserialize_with = "as_u64_from_serde")]
    pub comment_count: u64,
}

// Ensure alias since images endpoint has a specific response shape
pub type ImageResponse = PaginatedResponse<ImageItem>;
