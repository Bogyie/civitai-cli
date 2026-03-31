use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: u64,
    pub name: String,
    pub description: Option<String>,
    pub r#type: String, // e.g., "LORA", "Checkpoint"
    #[serde(default)]
    pub nsfw: bool,
    pub stats: Option<ModelStats>,
    #[serde(default)]
    pub model_versions: Vec<ModelVersion>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelStats {
    #[serde(default)]
    pub download_count: u64,
    #[serde(default)]
    pub favorite_count: u64,
    #[serde(default)]
    pub comment_count: u64,
    #[serde(default)]
    pub rating_count: u64,
    #[serde(default)]
    pub rating: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelVersion {
    pub id: u64,
    pub model_id: Option<u64>,
    pub name: String,
    pub base_model: String, // e.g., "SD 1.5", "SDXL"
    pub stats: Option<VersionStats>,
    #[serde(default)]
    pub images: Vec<ModelImage>,
    #[serde(default)]
    pub files: Vec<ModelFile>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VersionStats {
    #[serde(default)]
    pub download_count: u64,
    #[serde(default)]
    pub rating_count: u64,
    #[serde(default)]
    pub rating: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelImage {
    pub url: String,
    pub nsfw: Option<String>, // sometimes string in API ("None", "Soft") or bool. Let's make it Option<serde_json::Value> for safety if we don't use it yet
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default, rename = "sizeKB")]
    pub size_kb: f64,
    pub metadata: Option<FileMetadata>,
    pub download_url: String,
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
    pub next_page: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImageItem {
    pub id: u64,
    pub url: String,
    pub hash: String,
    pub meta: Option<serde_json::Value>,
}

// Ensure alias since images endpoint has a specific response shape
pub type ImageResponse = PaginatedResponse<ImageItem>;
