use anyhow::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::constants::{CIVITAI_MODEL_DOWNLOAD_API_URL, CIVITAI_WEB_URL};
use super::image_search::ImageHitUser;
use super::model_search_types::{
    ModelBaseModel, ModelCategory, ModelCheckpointType, ModelFileFormat, ModelSearchSortBy,
    ModelType,
};
use super::shared::{append_csv_pair, normalize_search_url, parse_query_map, split_multi_keys};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct ModelSearchState {
    pub query: Option<String>,
    pub sort_by: ModelSearchSortBy,
    pub base_models: Vec<ModelBaseModel>,
    pub types: Vec<ModelType>,
    pub checkpoint_types: Vec<ModelCheckpointType>,
    pub file_formats: Vec<ModelFileFormat>,
    pub categories: Vec<ModelCategory>,
    pub users: Vec<String>,
    pub tags: Vec<String>,
    pub created_at: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub extras: Vec<(String, String)>,
}

impl ModelSearchState {
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();

        pairs.push((
            "sortBy".to_string(),
            self.sort_by.to_query_value().into_owned(),
        ));

        if let Some(query) = self.query.as_ref().filter(|q| !q.is_empty()) {
            pairs.push(("query".to_string(), query.to_string()));
        }

        if let Some(value) = self.created_at.as_ref().filter(|s| !s.is_empty()) {
            pairs.push(("createdAt".to_string(), value.to_string()));
        }

        append_csv_pair(
            &mut pairs,
            "baseModel",
            &self
                .base_models
                .iter()
                .map(|value| value.as_query_value().to_string())
                .collect::<Vec<_>>(),
        );
        append_csv_pair(
            &mut pairs,
            "type",
            &self
                .types
                .iter()
                .map(|value| value.as_query_value().to_string())
                .collect::<Vec<_>>(),
        );
        append_csv_pair(
            &mut pairs,
            "checkpointType",
            &self
                .checkpoint_types
                .iter()
                .map(|value| value.as_query_value().to_string())
                .collect::<Vec<_>>(),
        );
        append_csv_pair(
            &mut pairs,
            "fileFormats",
            &self
                .file_formats
                .iter()
                .map(|value| value.as_query_value().to_string())
                .collect::<Vec<_>>(),
        );
        append_csv_pair(
            &mut pairs,
            "category",
            &self
                .categories
                .iter()
                .map(|value| value.as_query_value().to_string())
                .collect::<Vec<_>>(),
        );
        append_csv_pair(&mut pairs, "users", &self.users);
        append_csv_pair(&mut pairs, "tags", &self.tags);

        if let Some(page) = self.page {
            pairs.push(("page".to_string(), page.to_string()));
        }

        if let Some(limit) = self.limit {
            pairs.push(("limit".to_string(), limit.to_string()));
        }

        pairs.extend(self.extras.iter().map(|(k, v)| (k.clone(), v.clone())));
        pairs
    }

    pub fn to_web_url(&self, base_url: &str) -> Result<Url> {
        let pairs = self.to_query_pairs();
        let url = Url::parse_with_params(
            base_url,
            pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        )
        .context("Failed to build Civitai model search URL")?;
        Ok(url)
    }

    pub fn from_web_url(raw: &str) -> Result<Self> {
        let normalized = normalize_search_url(raw, "/search/models")?;
        let url = Url::parse(&normalized).context("Failed to parse model search URL")?;
        let mut query = Self::default();
        let map = parse_query_map(&url);

        if let Some(values) = map.get("query")
            && let Some(v) = values.first()
            && !v.is_empty()
        {
            query.query = Some(v.to_string());
        }

        if let Some(values) = map.get("sortBy")
            && let Some(v) = values.first()
        {
            query.sort_by = ModelSearchSortBy::from_query_value(v);
        }

        query.base_models = split_multi_keys(&map, &["baseModel", "baseModels"])
            .into_iter()
            .map(|value| ModelBaseModel::from_query_value(&value))
            .collect();
        query.types = split_multi_keys(&map, &["type", "types"])
            .into_iter()
            .map(|value| ModelType::from_query_value(&value))
            .collect();
        query.checkpoint_types = split_multi_keys(&map, &["checkpointType", "checkpointTypes"])
            .into_iter()
            .map(|value| ModelCheckpointType::from_query_value(&value))
            .collect();
        query.file_formats = split_multi_keys(&map, &["fileFormat", "fileFormats"])
            .into_iter()
            .map(|value| ModelFileFormat::from_query_value(&value))
            .collect();
        query.categories = split_multi_keys(&map, &["category", "categories"])
            .into_iter()
            .map(|value| ModelCategory::from_query_value(&value))
            .collect();
        query.users = split_multi_keys(&map, &["users"]);
        query.tags = split_multi_keys(&map, &["tags"]);

        if let Some(values) = map.get("createdAt")
            && let Some(v) = values.first()
            && !v.is_empty()
        {
            query.created_at = Some(v.to_string());
        }

        if let Some(values) = map.get("page")
            && let Some(v) = values.first()
            && let Ok(page) = v.parse::<u32>()
        {
            query.page = Some(page);
        }

        if let Some(values) = map.get("limit")
            && let Some(v) = values.first()
            && let Ok(limit) = v.parse::<u32>()
        {
            query.limit = Some(limit);
        }

        let mut extras = Vec::new();
        for (key, values) in map {
            if is_known_model_key(&key) {
                continue;
            }
            for value in values {
                extras.push((key.clone(), value));
            }
        }
        query.extras = extras;

        Ok(query)
    }
}

fn is_known_model_key(key: &str) -> bool {
    matches!(
        key,
        "query"
            | "sortBy"
            | "baseModel"
            | "baseModels"
            | "type"
            | "types"
            | "checkpointType"
            | "checkpointTypes"
            | "fileFormat"
            | "fileFormats"
            | "category"
            | "categories"
            | "users"
            | "tags"
            | "createdAt"
            | "page"
            | "limit"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelDownloadAuth {
    QueryToken(String),
    BearerToken(String),
}

pub fn build_model_download_url(version_id: u64) -> String {
    build_model_download_url_with_base(CIVITAI_MODEL_DOWNLOAD_API_URL, version_id)
}

pub fn build_model_download_url_with_token(version_id: u64, token: &str) -> String {
    build_model_download_url_with_token_and_base(CIVITAI_MODEL_DOWNLOAD_API_URL, version_id, token)
}

pub fn build_model_download_url_with_base(base_url: &str, version_id: u64) -> String {
    let base_url = base_url.trim_end_matches('/');
    format!("{base_url}/{version_id}")
}

pub fn build_model_download_url_with_token_and_base(
    base_url: &str,
    version_id: u64,
    token: &str,
) -> String {
    let token = token.trim();
    if token.is_empty() {
        return build_model_download_url_with_base(base_url, version_id);
    }

    let mut url = Url::parse(&build_model_download_url_with_base(base_url, version_id))
        .expect("Civitai model download URL should always be valid");
    url.query_pairs_mut().append_pair("token", token);
    url.into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchModelHit {
    pub id: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub last_version_at: Option<String>,
    #[serde(default)]
    pub last_version_at_unix: Option<i64>,
    #[serde(default)]
    pub checkpoint_type: Option<String>,
    #[serde(default)]
    pub availability: Option<String>,
    #[serde(default)]
    pub file_formats: Vec<String>,
    #[serde(default)]
    pub hashes: Vec<String>,
    #[serde(default)]
    pub tags: Option<Vec<Value>>,
    #[serde(default)]
    pub category: Option<Value>,
    #[serde(default)]
    pub permissions: Option<Value>,
    #[serde(default)]
    pub metrics: Option<Value>,
    #[serde(default)]
    pub rank: Option<Value>,
    #[serde(default)]
    pub user: Option<ImageHitUser>,
    #[serde(default)]
    pub version: Option<Value>,
    #[serde(default)]
    pub versions: Option<Vec<Value>>,
    #[serde(default)]
    pub images: Option<Vec<Value>>,
    #[serde(default)]
    pub can_generate: Option<bool>,
    #[serde(default)]
    pub nsfw: Option<bool>,
    #[serde(default)]
    pub nsfw_level: Option<Vec<u64>>,
}

impl SearchModelHit {
    pub fn model_page_url(&self) -> String {
        self.model_page_url_with_base(CIVITAI_WEB_URL)
    }

    pub fn model_page_url_with_base(&self, base_url: &str) -> String {
        let base_url = base_url.trim_end_matches('/');
        format!("{base_url}/models/{}", self.id)
    }

    pub fn primary_model_version_id(&self) -> Option<u64> {
        self.version
            .as_ref()
            .and_then(Self::extract_version_id)
            .or_else(|| {
                self.versions
                    .as_ref()
                    .and_then(|versions| versions.iter().find_map(Self::extract_version_id))
            })
    }

    pub fn model_download_url(&self) -> Option<String> {
        self.model_download_url_with_base(CIVITAI_MODEL_DOWNLOAD_API_URL)
    }

    pub fn model_download_url_with_base(&self, base_url: &str) -> Option<String> {
        self.primary_model_version_id()
            .map(|version_id| build_model_download_url_with_base(base_url, version_id))
    }

    pub fn model_download_url_with_token(&self, token: &str) -> Option<String> {
        self.model_download_url_with_token_and_base(CIVITAI_MODEL_DOWNLOAD_API_URL, token)
    }

    pub fn model_download_url_with_token_and_base(
        &self,
        base_url: &str,
        token: &str,
    ) -> Option<String> {
        self.primary_model_version_id().map(|version_id| {
            build_model_download_url_with_token_and_base(base_url, version_id, token)
        })
    }

    fn extract_version_id(value: &Value) -> Option<u64> {
        value.get("id").and_then(Value::as_u64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchModelResponse {
    #[serde(default)]
    pub hits: Vec<SearchModelHit>,
    #[serde(default, alias = "estimatedTotalHits")]
    pub estimated_total_hits: Option<u64>,
    #[serde(default, alias = "processingTimeMs")]
    pub processing_time_ms: Option<u32>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(flatten, default)]
    pub extras: Value,
}
