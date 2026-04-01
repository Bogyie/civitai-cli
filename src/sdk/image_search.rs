use anyhow::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::constants::{
    CIVITAI_MEDIA_DELIVERY_NAMESPACE, CIVITAI_MEDIA_DELIVERY_URL, CIVITAI_WEB_URL,
};
use super::image_search_types::{
    ImageAspectRatio, ImageBaseModel, ImageMediaType, ImageSearchSortBy, ImageTechnique, ImageTool,
};
use super::shared::{
    append_csv_pair, normalize_search_url, parse_query_map, split_multi, split_multi_keys,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct ImageSearchState {
    pub query: Option<String>,
    pub sort_by: ImageSearchSortBy,
    pub media_types: Vec<ImageMediaType>,
    pub tags: Vec<String>,
    pub tools: Vec<ImageTool>,
    pub techniques: Vec<ImageTechnique>,
    pub users: Vec<String>,
    pub base_models: Vec<ImageBaseModel>,
    pub aspect_ratios: Vec<ImageAspectRatio>,
    pub created_at: Option<String>,
    pub image_id: Option<u64>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub extras: Vec<(String, String)>,
}

impl ImageSearchState {
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

        if let Some(image_id) = self.image_id {
            pairs.push(("imageId".to_string(), image_id.to_string()));
        }

        append_csv_pair(
            &mut pairs,
            "type",
            &self
                .media_types
                .iter()
                .map(|value| value.as_query_value().to_string())
                .collect::<Vec<_>>(),
        );
        append_csv_pair(&mut pairs, "tags", &self.tags);
        append_csv_pair(
            &mut pairs,
            "tools",
            &self
                .tools
                .iter()
                .map(|value| value.as_query_value().to_string())
                .collect::<Vec<_>>(),
        );
        append_csv_pair(
            &mut pairs,
            "techniques",
            &self
                .techniques
                .iter()
                .map(|value| value.as_query_value().to_string())
                .collect::<Vec<_>>(),
        );
        append_csv_pair(&mut pairs, "users", &self.users);
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
            "aspectRatio",
            &self
                .aspect_ratios
                .iter()
                .map(|value| value.as_query_value().to_string())
                .collect::<Vec<_>>(),
        );

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
        .context("Failed to build Civitai image search URL")?;
        Ok(url)
    }

    pub fn from_web_url(raw: &str) -> Result<Self> {
        let normalized = normalize_search_url(raw, "/search/images")?;
        let url = Url::parse(&normalized).context("Failed to parse search URL")?;
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
            query.sort_by = ImageSearchSortBy::from_query_value(v);
        }

        query.media_types = split_multi_keys(&map, &["type", "types"])
            .into_iter()
            .map(|value| ImageMediaType::from_query_value(&value))
            .collect();
        query.tags = split_multi(map.get("tags"));
        query.tools = split_multi(map.get("tools"))
            .into_iter()
            .map(|value| ImageTool::from_query_value(&value))
            .collect();
        query.techniques = split_multi(map.get("techniques"))
            .into_iter()
            .map(|value| ImageTechnique::from_query_value(&value))
            .collect();
        query.users = split_multi(map.get("users"));
        query.base_models = split_multi(map.get("baseModel"))
            .into_iter()
            .map(|value| ImageBaseModel::from_query_value(&value))
            .collect();
        query.aspect_ratios = split_multi(map.get("aspectRatio"))
            .into_iter()
            .map(|value| ImageAspectRatio::from_query_value(&value))
            .collect();

        if let Some(values) = map.get("createdAt")
            && let Some(v) = values.first()
            && !v.is_empty()
        {
            query.created_at = Some(v.to_string());
        }

        if let Some(values) = map.get("imageId")
            && let Some(v) = values.first()
            && let Ok(image_id) = v.parse::<u64>()
        {
            query.image_id = Some(image_id);
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
            if is_known_image_key(&key) {
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

fn is_known_image_key(key: &str) -> bool {
    matches!(
        key,
        "query"
            | "sortBy"
            | "type"
            | "types"
            | "tags"
            | "tools"
            | "techniques"
            | "users"
            | "baseModel"
            | "aspectRatio"
            | "createdAt"
            | "imageId"
            | "page"
            | "limit"
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchImageHit {
    pub id: u64,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub base_model: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub hide_meta: Option<bool>,
    #[serde(default)]
    pub user: Option<ImageHitUser>,
    #[serde(default)]
    pub stats: Option<Value>,
    #[serde(default)]
    pub tag_names: Vec<Option<String>>,
    #[serde(default)]
    pub model_version_ids: Vec<u64>,
    #[serde(default)]
    pub nsfw_level: Option<u64>,
    #[serde(default)]
    pub browsing_level: Option<u64>,
    #[serde(default)]
    pub sort_at: Option<String>,
    #[serde(default)]
    pub sort_at_unix: Option<i64>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub generation_process: Option<String>,
    #[serde(default)]
    pub ai_nsfw_level: Option<u64>,
    #[serde(default)]
    pub combined_nsfw_level: Option<u64>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

impl SearchImageHit {
    pub fn has_public_metadata(&self) -> bool {
        !self.hide_meta.unwrap_or(false) && self.prompt.is_some()
    }

    pub fn media_token(&self) -> Option<&str> {
        self.url.as_deref().filter(|value| !value.trim().is_empty())
    }

    pub fn original_media_url(&self) -> Option<String> {
        self.media_url_with_base_and_namespace(
            CIVITAI_MEDIA_DELIVERY_URL,
            CIVITAI_MEDIA_DELIVERY_NAMESPACE,
        )
    }

    pub fn image_page_url(&self) -> String {
        self.image_page_url_with_base(CIVITAI_WEB_URL)
    }

    pub fn image_page_url_with_base(&self, base_url: &str) -> String {
        let base_url = base_url.trim_end_matches('/');
        format!("{base_url}/images/{}", self.id)
    }

    pub fn media_url_with_namespace(&self, namespace: &str) -> Option<String> {
        self.media_url_with_base_and_namespace(CIVITAI_MEDIA_DELIVERY_URL, namespace)
    }

    pub fn media_url_with_base_and_namespace(
        &self,
        base_url: &str,
        namespace: &str,
    ) -> Option<String> {
        let token = self.media_token()?;
        let base_url = base_url.trim_end_matches('/');
        let namespace = namespace.trim().trim_matches('/');
        if namespace.is_empty() {
            return None;
        }

        Some(format!("{base_url}/{namespace}/{token}/original=true"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageHitUser {
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchImageResponse {
    #[serde(default)]
    pub hits: Vec<SearchImageHit>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default, alias = "nbPages")]
    pub total_pages: Option<u32>,
    #[serde(default, alias = "nbHitsPerPage")]
    pub hits_per_page: Option<u32>,
    #[serde(default, alias = "nbHits")]
    pub total_hits: Option<u64>,
    #[serde(default, alias = "estimatedTotalHits")]
    pub estimated_total_hits: Option<u64>,
    #[serde(default, alias = "processingTimeMs")]
    pub processing_time_ms: Option<u32>,
    #[serde(default, alias = "limit")]
    pub limit: Option<u32>,
    #[serde(default, alias = "offset")]
    pub offset: Option<u32>,
    #[serde(flatten, default)]
    pub extras: Value,
}
