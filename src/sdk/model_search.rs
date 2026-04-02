use anyhow::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::constants::{CIVITAI_MODEL_DOWNLOAD_API_URL, CIVITAI_WEB_URL};
use super::api_types::{
    Model as ApiModel, ModelCreator as ApiModelCreator, ModelFile as ApiModelFile,
    ModelImage as ApiModelImage, ModelStats as ApiModelStats, ModelTag as ApiModelTag,
    ModelVersion as ApiModelVersion, NsfwValue as ApiNsfwValue, VersionStats as ApiVersionStats,
};
use super::image_search::ImageHitUser;
use super::model_search_types::{
    ModelBaseModel, ModelCategory, ModelCheckpointType, ModelFileFormat, ModelSearchSortBy,
    ModelType,
};
use super::serde_utils::{
    deserialize_boolish, deserialize_f64ish, deserialize_option_f64ish,
    deserialize_option_u64ish, deserialize_stringish_opt, deserialize_u64ish,
    normalize_optional_string,
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
    pub tags: Vec<SearchModelTag>,
    #[serde(default)]
    pub category: Option<SearchModelCategory>,
    #[serde(default)]
    pub permissions: Option<Value>,
    #[serde(default)]
    pub metrics: Option<SearchModelMetrics>,
    #[serde(default)]
    pub rank: Option<Value>,
    #[serde(default)]
    pub user: Option<ImageHitUser>,
    #[serde(default)]
    pub version: Option<SearchModelVersion>,
    #[serde(default)]
    pub versions: Vec<SearchModelVersion>,
    #[serde(default)]
    pub images: Vec<SearchModelImage>,
    #[serde(default)]
    pub can_generate: Option<bool>,
    #[serde(default)]
    pub nsfw: Option<bool>,
    #[serde(default)]
    pub nsfw_level: Option<Vec<u64>>,
    #[serde(flatten, default)]
    pub extras: Value,
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
            .map(|version| version.id)
            .or_else(|| self.versions.first().map(|version| version.id))
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SearchModelTag {
    Name { name: String },
    NameOnly(String),
}

impl SearchModelTag {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Name { name } | Self::NameOnly(name) if !name.trim().is_empty() => Some(name),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SearchModelCategory {
    Name { name: String },
    NameOnly(String),
}

impl SearchModelCategory {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Name { name } | Self::NameOnly(name) if !name.trim().is_empty() => Some(name),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchModelMetrics {
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub download_count: u64,
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub thumbs_up_count: u64,
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub favorite_count: u64,
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub comment_count: u64,
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub collected_count: u64,
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub tipped_amount_count: u64,
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub rating_count: u64,
    #[serde(default, deserialize_with = "deserialize_f64ish")]
    pub rating: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchModelVersion {
    pub id: u64,
    pub name: Option<String>,
    pub base_model: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub early_access_time_frame: Option<u64>,
    pub description: Option<String>,
    pub stats: Option<SearchModelMetrics>,
    pub files: Vec<SearchModelFile>,
    pub images: Vec<SearchModelImage>,
}

impl<'de> Deserialize<'de> for SearchModelVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawSearchModelVersion {
            #[serde(deserialize_with = "deserialize_u64ish")]
            id: u64,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            base_model: Option<String>,
            #[serde(default)]
            created_at: Option<String>,
            #[serde(default)]
            updated_at: Option<String>,
            #[serde(default, deserialize_with = "deserialize_option_u64ish")]
            early_access_time_frame: Option<u64>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default, alias = "metrics")]
            stats: Option<SearchModelMetrics>,
            #[serde(default, alias = "downloadableFiles", alias = "modelFiles")]
            files: Vec<SearchModelFile>,
            #[serde(default)]
            images: Vec<SearchModelImage>,
        }

        let raw = RawSearchModelVersion::deserialize(deserializer)?;
        Ok(Self {
            id: raw.id,
            name: raw.name,
            base_model: normalize_optional_string(raw.base_model),
            created_at: normalize_optional_string(raw.created_at),
            updated_at: normalize_optional_string(raw.updated_at),
            early_access_time_frame: raw.early_access_time_frame,
            description: normalize_optional_string(raw.description),
            stats: raw.stats,
            files: raw.files,
            images: raw.images,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchModelFile {
    pub id: Option<u64>,
    pub name: Option<String>,
    pub file_type: Option<String>,
    pub size_kb: Option<f64>,
    pub metadata: Option<SearchModelFileMetadata>,
    pub primary: bool,
    pub download_url: Option<String>,
    pub pickle_scan_result: Option<String>,
    pub virus_scan_result: Option<String>,
}

impl<'de> Deserialize<'de> for SearchModelFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawSearchModelFile {
            #[serde(default, deserialize_with = "deserialize_option_u64ish")]
            id: Option<u64>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default, rename = "type")]
            file_type: Option<String>,
            #[serde(
                default,
                rename = "sizeKB",
                alias = "sizeKb",
                alias = "size_kb",
                deserialize_with = "deserialize_option_f64ish"
            )]
            size_kb: Option<f64>,
            #[serde(
                default,
                rename = "sizeMB",
                alias = "sizeMb",
                alias = "size_mb",
                deserialize_with = "deserialize_option_f64ish"
            )]
            size_mb: Option<f64>,
            #[serde(
                default,
                rename = "sizeB",
                alias = "sizeBytes",
                alias = "size_bytes",
                deserialize_with = "deserialize_option_f64ish"
            )]
            size_b: Option<f64>,
            #[serde(default)]
            metadata: Option<SearchModelFileMetadata>,
            #[serde(default, deserialize_with = "deserialize_boolish")]
            primary: bool,
            #[serde(default)]
            download_url: Option<String>,
            #[serde(default)]
            pickle_scan_result: Option<String>,
            #[serde(default)]
            virus_scan_result: Option<String>,
        }

        let raw = RawSearchModelFile::deserialize(deserializer)?;
        let size_kb = raw
            .size_kb
            .or_else(|| raw.size_mb.map(|value| value * 1024.0))
            .or_else(|| raw.size_b.map(|value| value / 1024.0));
        Ok(Self {
            id: raw.id,
            name: normalize_optional_string(raw.name),
            file_type: normalize_optional_string(raw.file_type),
            size_kb,
            metadata: raw.metadata,
            primary: raw.primary,
            download_url: normalize_optional_string(raw.download_url),
            pickle_scan_result: normalize_optional_string(raw.pickle_scan_result),
            virus_scan_result: normalize_optional_string(raw.virus_scan_result),
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchModelFileMetadata {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub fp: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchModelImage {
    #[serde(default, deserialize_with = "deserialize_option_u64ish")]
    pub id: Option<u64>,
    pub url: String,
    #[serde(default, deserialize_with = "deserialize_option_stringish")]
    pub nsfw: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_option_u64ish")]
    pub model_version_id: Option<u64>,
    #[serde(default)]
    pub meta: Option<Value>,
}

fn deserialize_option_stringish<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_stringish_opt(deserializer)
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

impl From<ApiModel> for SearchModelHit {
    fn from(value: ApiModel) -> Self {
        let version = value.model_versions.first().cloned().map(Into::into);
        let versions: Vec<SearchModelVersion> =
            value.model_versions.into_iter().map(Into::into).collect();
        let file_formats = versions
            .iter()
            .flat_map(|version| version.files.iter())
            .filter_map(|file| {
                file.metadata
                    .as_ref()
                    .and_then(|metadata| metadata.format.clone())
                    .or_else(|| file.file_type.clone())
            })
            .collect::<Vec<_>>();
        let images = versions
            .iter()
            .flat_map(|version| version.images.iter().cloned())
            .collect::<Vec<_>>();

        Self {
            id: value.id,
            name: Some(value.name),
            r#type: Some(value.r#type),
            created_at: None,
            last_version_at: value.updated_at.clone(),
            last_version_at_unix: None,
            checkpoint_type: None,
            availability: None,
            file_formats,
            hashes: Vec::new(),
            tags: value.tags.into_iter().map(Into::into).collect(),
            category: None,
            permissions: None,
            metrics: value.stats.map(Into::into),
            rank: None,
            user: value.creator.map(Into::into),
            version,
            versions,
            images,
            can_generate: value.supports_generation,
            nsfw: Some(value.nsfw),
            nsfw_level: None,
            extras: serde_json::json!({
                "description": value.description,
            }),
        }
    }
}

impl From<ApiModelTag> for SearchModelTag {
    fn from(value: ApiModelTag) -> Self {
        match value {
            ApiModelTag::Name { name } => Self::Name { name },
            ApiModelTag::NameOnly(name) => Self::NameOnly(name),
        }
    }
}

impl From<ApiModelCreator> for ImageHitUser {
    fn from(value: ApiModelCreator) -> Self {
        Self {
            username: Some(value.username),
        }
    }
}

impl From<ApiModelStats> for SearchModelMetrics {
    fn from(value: ApiModelStats) -> Self {
        Self {
            download_count: value.download_count,
            thumbs_up_count: value.thumbs_up_count,
            favorite_count: value.favorite_count,
            comment_count: value.comment_count,
            collected_count: 0,
            tipped_amount_count: 0,
            rating_count: value.rating_count,
            rating: value.rating,
        }
    }
}

impl From<ApiVersionStats> for SearchModelMetrics {
    fn from(value: ApiVersionStats) -> Self {
        Self {
            download_count: value.download_count,
            thumbs_up_count: value.thumbs_up_count,
            favorite_count: value.favorite_count,
            comment_count: value.comment_count,
            collected_count: 0,
            tipped_amount_count: 0,
            rating_count: value.rating_count,
            rating: value.rating,
        }
    }
}

impl From<ApiModelVersion> for SearchModelVersion {
    fn from(value: ApiModelVersion) -> Self {
        Self {
            id: value.id,
            name: Some(value.name),
            base_model: normalize_optional_string(Some(value.base_model)),
            created_at: value.created_at,
            updated_at: value.updated_at,
            early_access_time_frame: value.early_access_time_frame,
            description: value.description,
            stats: value.stats.map(Into::into),
            files: value.files.into_iter().map(Into::into).collect(),
            images: value.images.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ApiModelFile> for SearchModelFile {
    fn from(value: ApiModelFile) -> Self {
        Self {
            id: Some(value.id),
            name: normalize_optional_string(Some(value.name)),
            file_type: value.file_type,
            size_kb: (value.size_kb > 0.0).then_some(value.size_kb),
            metadata: value.metadata.map(Into::into),
            primary: value.primary,
            download_url: normalize_optional_string(Some(value.download_url)),
            pickle_scan_result: value.pickle_scan_result,
            virus_scan_result: value.virus_scan_result,
        }
    }
}

impl From<super::api_types::FileMetadata> for SearchModelFileMetadata {
    fn from(value: super::api_types::FileMetadata) -> Self {
        Self {
            format: value.format,
            size: value.size,
            fp: value.fp,
        }
    }
}

impl From<ApiModelImage> for SearchModelImage {
    fn from(value: ApiModelImage) -> Self {
        Self {
            id: value.id,
            url: value.url,
            nsfw: value.nsfw.map(|value| match value {
                ApiNsfwValue::Bool(value) => value.to_string(),
                ApiNsfwValue::Text(value) => value,
                ApiNsfwValue::Unknown => "unknown".to_string(),
            }),
            width: value.width,
            height: value.height,
            model_version_id: None,
            meta: value.meta,
        }
    }
}
