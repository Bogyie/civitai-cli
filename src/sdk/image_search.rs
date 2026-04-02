use anyhow::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize};
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

fn deserialize_normalized_value_opt<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    Ok(value.map(normalize_jsonish_value))
}

fn deserialize_normalized_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: for<'a> Deserialize<'a>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    let Some(value) = value.map(normalize_jsonish_value) else {
        return Ok(Vec::new());
    };
    serde_json::from_value::<Vec<T>>(value).map_err(serde::de::Error::custom)
}

fn deserialize_normalized_struct_opt<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: for<'a> Deserialize<'a>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    let Some(value) = value.map(normalize_jsonish_value) else {
        return Ok(None);
    };
    serde_json::from_value::<T>(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

fn normalize_jsonish_value(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, normalize_jsonish_value(value)))
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(normalize_jsonish_value).collect())
        }
        Value::String(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(map)) => {
                normalize_jsonish_value(Value::Object(map))
            }
            Ok(Value::Array(items)) => {
                normalize_jsonish_value(Value::Array(items))
            }
            _ => Value::String(raw),
        },
        other => other,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaUrlOptions {
    /// `original=true` returns the original asset; for videos this keeps the source mp4.
    pub original: Option<bool>,
    /// Video-specific variant selection. In live tests this gates transcoded mp4 delivery.
    pub transcode: Option<bool>,
    /// Resize target width for transformed variants.
    pub width: Option<u32>,
    /// Resize target height for transformed variants.
    pub height: Option<u32>,
    /// Quality hint used by transformed image/video variants.
    pub quality: Option<u8>,
    /// Prefers optimized delivery formats such as webp/transcoded mp4 when supported.
    pub optimized: Option<bool>,
    /// Disables animation in image-derived variants when the backend supports it.
    pub anim: Option<bool>,
}

impl MediaUrlOptions {
    pub fn original() -> Self {
        Self {
            original: Some(true),
            ..Self::default()
        }
    }

    pub fn default_variant() -> Self {
        Self {
            original: Some(false),
            ..Self::default()
        }
    }

    pub fn to_path_segment(&self) -> String {
        let mut parts = Vec::new();

        if let Some(original) = self.original {
            parts.push(format!("original={original}"));
        }
        if let Some(transcode) = self.transcode {
            parts.push(format!("transcode={transcode}"));
        }
        if let Some(width) = self.width {
            parts.push(format!("width={width}"));
        }
        if let Some(height) = self.height {
            parts.push(format!("height={height}"));
        }
        if let Some(quality) = self.quality {
            parts.push(format!("quality={quality}"));
        }
        if let Some(optimized) = self.optimized {
            parts.push(format!("optimized={optimized}"));
        }
        if let Some(anim) = self.anim {
            parts.push(format!("anim={anim}"));
        }

        // Default to the original asset so existing callers keep their prior behavior.
        if parts.is_empty() {
            "original=true".to_string()
        } else {
            parts.join(",")
        }
    }
}

pub fn media_url_from_raw_with_options(raw_url: &str, options: &MediaUrlOptions) -> Option<String> {
    let url = Url::parse(raw_url).ok()?;
    let mut segments = url
        .path_segments()
        .map(|parts| parts.map(str::to_string).collect::<Vec<_>>())?;
    if segments.len() < 3 {
        return None;
    }

    let namespace = segments.remove(0);
    let token = segments.remove(0);
    let mut rebuilt = format!(
        "{}/{}/{}/{}",
        url.origin().ascii_serialization().trim_end_matches('/'),
        namespace,
        token,
        options.to_path_segment()
    );

    if !segments.is_empty() {
        rebuilt.push('/');
        rebuilt.push_str(&segments.join("/"));
    }

    Some(rebuilt)
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
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
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
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
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
        self.media_url_with_options_and_base_and_namespace(
            CIVITAI_MEDIA_DELIVERY_URL,
            CIVITAI_MEDIA_DELIVERY_NAMESPACE,
            &MediaUrlOptions::original(),
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
        self.media_url_with_options_and_base_and_namespace(
            CIVITAI_MEDIA_DELIVERY_URL,
            namespace,
            &MediaUrlOptions::original(),
        )
    }

    pub fn media_url_with_base_and_namespace(
        &self,
        base_url: &str,
        namespace: &str,
    ) -> Option<String> {
        self.media_url_with_options_and_base_and_namespace(
            base_url,
            namespace,
            &MediaUrlOptions::original(),
        )
    }

    pub fn media_url_with_options(&self, options: &MediaUrlOptions) -> Option<String> {
        self.media_url_with_options_and_base_and_namespace(
            CIVITAI_MEDIA_DELIVERY_URL,
            CIVITAI_MEDIA_DELIVERY_NAMESPACE,
            options,
        )
    }

    pub fn media_url_with_options_and_namespace(
        &self,
        namespace: &str,
        options: &MediaUrlOptions,
    ) -> Option<String> {
        self.media_url_with_options_and_base_and_namespace(
            CIVITAI_MEDIA_DELIVERY_URL,
            namespace,
            options,
        )
    }

    pub fn media_url_with_options_and_base_and_namespace(
        &self,
        base_url: &str,
        namespace: &str,
        options: &MediaUrlOptions,
    ) -> Option<String> {
        let token = self.media_token()?;
        let base_url = base_url.trim_end_matches('/');
        let namespace = namespace.trim().trim_matches('/');
        if namespace.is_empty() {
            return None;
        }

        // Civitai media variants are encoded as a comma-separated path segment.
        Some(format!(
            "{base_url}/{namespace}/{token}/{}",
            options.to_path_segment()
        ))
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenerationData {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub on_site: Option<bool>,
    #[serde(default)]
    pub process: Option<String>,
    #[serde(default, deserialize_with = "deserialize_normalized_struct_opt")]
    pub meta: Option<ImageGenerationMeta>,
    #[serde(default, deserialize_with = "deserialize_normalized_vec")]
    pub resources: Vec<ImageGenerationResource>,
    #[serde(default)]
    pub tools: Vec<ImageGenerationTool>,
    #[serde(default)]
    pub techniques: Vec<ImageGenerationTechnique>,
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
    pub external: Option<Value>,
    #[serde(default)]
    pub can_remix: Option<bool>,
    #[serde(default)]
    pub remix_of_id: Option<u64>,
    #[serde(flatten, default)]
    pub extras: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenerationMeta {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub negative_prompt: Option<String>,
    #[serde(default)]
    pub cfg_scale: Option<f64>,
    #[serde(default)]
    pub steps: Option<u64>,
    #[serde(default)]
    pub sampler: Option<String>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub scheduler: Option<String>,
    #[serde(default)]
    pub denoise: Option<f64>,
    #[serde(default, rename = "Model")]
    pub model: Option<String>,
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
    pub models: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
    pub upscalers: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
    pub width: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
    pub height: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_normalized_struct_opt")]
    pub comfy: Option<ImageGenerationComfy>,
    #[serde(flatten, default)]
    pub extras: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenerationComfy {
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
    pub prompt: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
    pub workflow: Option<Value>,
    #[serde(flatten, default)]
    pub extras: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenerationResource {
    #[serde(default)]
    pub image_id: Option<u64>,
    #[serde(default)]
    pub model_version_id: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_normalized_value_opt")]
    pub strength: Option<Value>,
    #[serde(default)]
    pub model_id: Option<u64>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub model_type: Option<String>,
    #[serde(default)]
    pub version_id: Option<u64>,
    #[serde(default)]
    pub version_name: Option<String>,
    #[serde(default)]
    pub base_model: Option<String>,
    #[serde(flatten, default)]
    pub extras: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenerationTool {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(flatten, default)]
    pub extras: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenerationTechnique {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(flatten, default)]
    pub extras: Value,
}

impl ImageGenerationData {
    pub fn as_metadata_attachment(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert(
            "_generationData".to_string(),
            serde_json::to_value(self).unwrap_or(Value::Null),
        );

        if let Some(process) = self.process.as_ref() {
            object.insert("process".to_string(), Value::String(process.clone()));
        }
        if let Some(meta) = self.meta.as_ref() {
            let meta_value = serde_json::to_value(meta).unwrap_or(Value::Null);
            if let Some(meta_object) = meta_value.as_object() {
                for (key, value) in meta_object {
                    object.insert(key.clone(), value.clone());
                }
            } else {
                object.insert("meta".to_string(), meta_value);
            }
        }
        if !self.resources.is_empty() {
            object.insert(
                "resources".to_string(),
                serde_json::to_value(&self.resources).unwrap_or(Value::Array(Vec::new())),
            );
        }
        if !self.tools.is_empty() {
            object.insert(
                "tools".to_string(),
                serde_json::to_value(&self.tools).unwrap_or(Value::Array(Vec::new())),
            );
        }
        if !self.techniques.is_empty() {
            object.insert(
                "techniques".to_string(),
                serde_json::to_value(&self.techniques).unwrap_or(Value::Array(Vec::new())),
            );
        }
        if let Some(external) = self.external.as_ref() {
            object.insert("external".to_string(), external.clone());
        }
        if let Some(can_remix) = self.can_remix {
            object.insert("canRemix".to_string(), Value::Bool(can_remix));
        }
        if let Some(remix_of_id) = self.remix_of_id {
            object.insert("remixOfId".to_string(), Value::Number(remix_of_id.into()));
        }

        Value::Object(object)
    }
}
