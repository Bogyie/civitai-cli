use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: u64,
    pub name: String,
    pub description: Option<String>,
    pub r#type: String, // e.g., "LORA", "Checkpoint"
    pub model_versions: Vec<ModelVersion>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelVersion {
    pub id: u64,
    pub model_id: u64,
    pub name: String,
    pub base_model: String, // e.g., "SD 1.5", "SDXL"
    pub files: Vec<ModelFile>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub id: u64,
    pub name: String,
    pub primary: bool,
    pub download_url: String,
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
