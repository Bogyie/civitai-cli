use anyhow::{Context, Result};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;

/// Constant used by Civitai image search pages.
pub const IMAGES_SEARCH_INDEX: &str = "images_v6";
pub const MODELS_SEARCH_INDEX: &str = "models_v9";
pub const CIVITAI_IMAGE_SEARCH_MEILI_URL: &str = "https://search-new.civitai.com";
pub const CIVITAI_IMAGE_SEARCH_CLIENT_KEY: &str =
    "8c46eb2508e21db1e9828a97968d91ab1ca1caa5f70a00e88a2ba1e286603b61";
pub const CIVITAI_WEB_URL: &str = "https://civitai.com";
pub const CIVITAI_MEDIA_DELIVERY_URL: &str = "https://image.civitai.com";
pub const CIVITAI_MEDIA_DELIVERY_NAMESPACE: &str = "xG1nkqKTMzGDvpLrqFT7WA";
pub const CIVITAI_MODEL_DOWNLOAD_API_URL: &str = "https://civitai.com/api/download/models";

const DEFAULT_SORTS: [&str; 6] = [
    IMAGES_SEARCH_INDEX,
    "images_v6:stats.reactionCountAllTime:desc",
    "images_v6:stats.commentCountAllTime:desc",
    "images_v6:stats.collectedCountAllTime:desc",
    "images_v6:stats.tippedAmountCountAllTime:desc",
    "images_v6:createdAt:desc",
];

const DEFAULT_MODEL_SORTS: [&str; 8] = [
    MODELS_SEARCH_INDEX,
    "models_v9:metrics.thumbsUpCount:desc",
    "models_v9:metrics.downloadCount:desc",
    "models_v9:metrics.favoriteCount:desc",
    "models_v9:metrics.commentCount:desc",
    "models_v9:metrics.collectedCount:desc",
    "models_v9:metrics.tippedAmountCount:desc",
    "models_v9:createdAt:desc",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSearchSortBy {
    /// Web default / `sortBy=images_v6`.
    Relevance,
    /// Web sort option: `images_v6:stats.reactionCountAllTime:desc`.
    MostReactions,
    /// Web sort option: `images_v6:stats.commentCountAllTime:desc`.
    MostDiscussed,
    /// Web sort option: `images_v6:stats.collectedCountAllTime:desc`.
    MostCollected,
    /// Web sort option: `images_v6:stats.tippedAmountCountAllTime:desc`.
    MostBuzz,
    /// Web sort option: `images_v6:createdAt:desc`.
    Newest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSearchSortBy {
    Relevance,
    HighestRated,
    MostDownloaded,
    MostLiked,
    MostDiscussed,
    MostCollected,
    MostBuzz,
    Newest,
}

impl Default for ModelSearchSortBy {
    fn default() -> Self {
        Self::Relevance
    }
}

impl ModelSearchSortBy {
    pub fn as_query_value(&self) -> &'static str {
        match self {
            Self::Relevance => DEFAULT_MODEL_SORTS[0],
            Self::HighestRated => DEFAULT_MODEL_SORTS[1],
            Self::MostDownloaded => DEFAULT_MODEL_SORTS[2],
            Self::MostLiked => DEFAULT_MODEL_SORTS[3],
            Self::MostDiscussed => DEFAULT_MODEL_SORTS[4],
            Self::MostCollected => DEFAULT_MODEL_SORTS[5],
            Self::MostBuzz => DEFAULT_MODEL_SORTS[6],
            Self::Newest => DEFAULT_MODEL_SORTS[7],
        }
    }

    pub fn from_query_value(value: &str) -> Self {
        if value == DEFAULT_MODEL_SORTS[0] {
            return Self::Relevance;
        }
        if value == DEFAULT_MODEL_SORTS[1] {
            return Self::HighestRated;
        }
        if value == DEFAULT_MODEL_SORTS[2] {
            return Self::MostDownloaded;
        }
        if value == DEFAULT_MODEL_SORTS[3] {
            return Self::MostLiked;
        }
        if value == DEFAULT_MODEL_SORTS[4] {
            return Self::MostDiscussed;
        }
        if value == DEFAULT_MODEL_SORTS[5] {
            return Self::MostCollected;
        }
        if value == DEFAULT_MODEL_SORTS[6] {
            return Self::MostBuzz;
        }
        if value == DEFAULT_MODEL_SORTS[7] {
            return Self::Newest;
        }
        Self::Relevance
    }

    pub fn to_meili_sort_value(&self) -> Option<&'static str> {
        match self {
            Self::Relevance => None,
            Self::HighestRated => Some("metrics.thumbsUpCount:desc"),
            Self::MostDownloaded => Some("metrics.downloadCount:desc"),
            Self::MostLiked => Some("metrics.favoriteCount:desc"),
            Self::MostDiscussed => Some("metrics.commentCount:desc"),
            Self::MostCollected => Some("metrics.collectedCount:desc"),
            Self::MostBuzz => Some("metrics.tippedAmountCount:desc"),
            Self::Newest => Some("createdAt:desc"),
        }
    }
}

impl Default for ImageSearchSortBy {
    fn default() -> Self {
        Self::Relevance
    }
}

impl ImageSearchSortBy {
    pub fn as_query_value(&self) -> &'static str {
        match self {
            Self::Relevance => DEFAULT_SORTS[0],
            Self::MostReactions => DEFAULT_SORTS[1],
            Self::MostDiscussed => DEFAULT_SORTS[2],
            Self::MostCollected => DEFAULT_SORTS[3],
            Self::MostBuzz => DEFAULT_SORTS[4],
            Self::Newest => DEFAULT_SORTS[5],
        }
    }

    pub fn from_query_value(value: &str) -> Self {
        if value == DEFAULT_SORTS[0] {
            return Self::Relevance;
        }
        if value == DEFAULT_SORTS[1] {
            return Self::MostReactions;
        }
        if value == DEFAULT_SORTS[2] {
            return Self::MostDiscussed;
        }
        if value == DEFAULT_SORTS[3] {
            return Self::MostCollected;
        }
        if value == DEFAULT_SORTS[4] {
            return Self::MostBuzz;
        }
        if value == DEFAULT_SORTS[5] {
            return Self::Newest;
        }
        Self::Relevance
    }

    pub fn to_meili_sort_value(&self) -> Option<&'static str> {
        match self {
            Self::Relevance => None,
            Self::MostReactions => Some("stats.reactionCountAllTime:desc"),
            Self::MostDiscussed => Some("stats.commentCountAllTime:desc"),
            Self::MostCollected => Some("stats.collectedCountAllTime:desc"),
            Self::MostBuzz => Some("stats.tippedAmountCountAllTime:desc"),
            Self::Newest => Some("createdAt:desc"),
        }
    }
}

#[derive(Clone)]
pub struct SearchSdkClient {
    client: Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelDownloadAuth {
    QueryToken(String),
    BearerToken(String),
}

impl SearchSdkClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("civitai-search-sdk/0.1")
            .build()
            .context("Failed to build HTTP client")?;
        Ok(Self { client })
    }

    fn escape_filter_value(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn push_equals_filters(filters: &mut Vec<String>, field: &str, values: &[String]) {
        for value in values.iter().filter(|value| !value.trim().is_empty()) {
            filters.push(format!(
                "{field} = \"{}\"",
                Self::escape_filter_value(value.trim())
            ));
        }
    }

    fn build_created_at_filters(raw: &str) -> Vec<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let Some((start, end)) = trimmed.split_once('-').or_else(|| trimmed.split_once(':')) else {
            return Vec::new();
        };

        let mut filters = Vec::new();

        if let Ok(start_unix) = start.trim().parse::<i64>() {
            let normalized = if start_unix < 10_000_000_000 {
                start_unix * 1000
            } else {
                start_unix
            };
            filters.push(format!("createdAtUnix >= {normalized}"));
        }

        if let Ok(end_unix) = end.trim().parse::<i64>() {
            let normalized = if end_unix < 10_000_000_000 {
                end_unix * 1000
            } else {
                end_unix
            };
            filters.push(format!("createdAtUnix <= {normalized}"));
        }

        filters
    }

    fn build_meili_payload(state: &ImageSearchState) -> Value {
        let limit = state.limit.unwrap_or(50);
        let page_index = state.page.unwrap_or(0);
        let offset = page_index.saturating_mul(limit);

        let mut filters = Vec::new();
        Self::push_equals_filters(&mut filters, "tagNames", &state.tags);
        Self::push_equals_filters(&mut filters, "toolNames", &state.tools);
        Self::push_equals_filters(&mut filters, "techniqueNames", &state.techniques);
        Self::push_equals_filters(&mut filters, "user.username", &state.users);
        Self::push_equals_filters(&mut filters, "baseModel", &state.base_models);
        Self::push_equals_filters(&mut filters, "aspectRatio", &state.aspect_ratios);

        if let Some(created_at) = state.created_at.as_ref() {
            filters.extend(Self::build_created_at_filters(created_at));
        }

        if let Some(image_id) = state.image_id {
            filters.push(format!("id = {image_id}"));
        }

        let mut payload = json!({
            "q": state.query.clone().unwrap_or_default(),
            "limit": limit,
            "offset": offset,
            "attributesToHighlight": [],
        });

        if !filters.is_empty() {
            payload["filter"] = json!(filters);
        }

        if let Some(sort) = state.sort_by.to_meili_sort_value() {
            payload["sort"] = json!([sort]);
        }

        payload
    }

    fn build_model_meili_payload(state: &ModelSearchState) -> Value {
        let limit = state.limit.unwrap_or(50);
        let page_index = state.page.unwrap_or(0);
        let offset = page_index.saturating_mul(limit);

        let mut filters = Vec::new();
        Self::push_equals_filters(&mut filters, "version.baseModel", &state.base_models);
        Self::push_equals_filters(&mut filters, "type", &state.types);
        Self::push_equals_filters(&mut filters, "checkpointType", &state.checkpoint_types);
        Self::push_equals_filters(&mut filters, "fileFormats", &state.file_formats);
        Self::push_equals_filters(&mut filters, "category.name", &state.categories);
        Self::push_equals_filters(&mut filters, "user.username", &state.users);
        Self::push_equals_filters(&mut filters, "tags.name", &state.tags);

        if let Some(created_at) = state.created_at.as_ref() {
            let range_filters = Self::build_created_at_filters(created_at)
                .into_iter()
                .map(|filter| filter.replace("createdAtUnix", "lastVersionAtUnix"));
            filters.extend(range_filters);
        }

        let mut payload = json!({
            "q": state.query.clone().unwrap_or_default(),
            "limit": limit,
            "offset": offset,
            "attributesToHighlight": [],
        });

        if !filters.is_empty() {
            payload["filter"] = json!(filters);
        }

        if let Some(sort) = state.sort_by.to_meili_sort_value() {
            payload["sort"] = json!([sort]);
        }

        payload
    }

    pub async fn search_images_web_raw(&self, state: &ImageSearchState) -> Result<Value> {
        let url = format!("{CIVITAI_IMAGE_SEARCH_MEILI_URL}/indexes/{IMAGES_SEARCH_INDEX}/search");
        let payload = Self::build_meili_payload(state);
        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {CIVITAI_IMAGE_SEARCH_CLIENT_KEY}"))
            .json(&payload)
            .send()
            .await
            .context("Failed to send Civitai web image search request")?
            .error_for_status()
            .context("Civitai web image search endpoint returned error")?;

        response
            .json::<Value>()
            .await
            .context("Failed to decode Civitai web image search response")
    }

    pub async fn search_images_web(&self, state: &ImageSearchState) -> Result<SearchImageResponse> {
        let value = self.search_images_web_raw(state).await?;
        serde_json::from_value(value).context("Failed to decode typed web search response")
    }

    pub async fn search_images_raw(&self, state: &ImageSearchState) -> Result<Value> {
        self.search_images_web_raw(state).await
    }

    pub async fn search_models_web_raw(&self, state: &ModelSearchState) -> Result<Value> {
        let url = format!("{CIVITAI_IMAGE_SEARCH_MEILI_URL}/indexes/{MODELS_SEARCH_INDEX}/search");
        let payload = Self::build_model_meili_payload(state);
        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {CIVITAI_IMAGE_SEARCH_CLIENT_KEY}"))
            .json(&payload)
            .send()
            .await
            .context("Failed to send Civitai web model search request")?
            .error_for_status()
            .context("Civitai web model search endpoint returned error")?;

        response
            .json::<Value>()
            .await
            .context("Failed to decode Civitai web model search response")
    }

    pub async fn search_models_web(&self, state: &ModelSearchState) -> Result<SearchModelResponse> {
        let value = self.search_models_web_raw(state).await?;
        serde_json::from_value(value).context("Failed to decode typed web model search response")
    }

    pub async fn search_models_raw(&self, state: &ModelSearchState) -> Result<Value> {
        self.search_models_web_raw(state).await
    }

    pub fn build_model_download_request(
        &self,
        version_id: u64,
        auth: Option<&ModelDownloadAuth>,
    ) -> reqwest::RequestBuilder {
        let request_url = match auth {
            Some(ModelDownloadAuth::QueryToken(token)) => {
                build_model_download_url_with_token(version_id, token)
            }
            _ => build_model_download_url(version_id),
        };
        let mut request = self.client.get(request_url);

        if let Some(auth) = auth {
            match auth {
                ModelDownloadAuth::QueryToken(_) => {}
                ModelDownloadAuth::BearerToken(token) => {
                    let token = token.trim();
                    if !token.is_empty() {
                        request = request.bearer_auth(token);
                    }
                }
            }
        }

        request
    }
}

pub fn build_model_download_url(version_id: u64) -> String {
    format!("{CIVITAI_MODEL_DOWNLOAD_API_URL}/{version_id}")
}

pub fn build_model_download_url_with_token(version_id: u64, token: &str) -> String {
    let token = token.trim();
    if token.is_empty() {
        return build_model_download_url(version_id);
    }

    let mut url = Url::parse(&build_model_download_url(version_id))
        .expect("Civitai model download URL should always be valid");
    url.query_pairs_mut().append_pair("token", token);
    url.into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct ImageSearchState {
    /// Equivalent to instantsearch `query` field.
    pub query: Option<String>,
    /// `sortBy` on web page.
    pub sort_by: ImageSearchSortBy,
    /// URL param `tags` (mapped to tagNames refinement).
    pub tags: Vec<String>,
    /// URL param `tools` (mapped to toolNames refinement).
    pub tools: Vec<String>,
    /// URL param `techniques` (mapped to techniqueNames refinement).
    pub techniques: Vec<String>,
    /// URL param `users` (mapped to user.username refinement).
    pub users: Vec<String>,
    /// URL param `baseModel`.
    pub base_models: Vec<String>,
    /// URL param `aspectRatio`.
    pub aspect_ratios: Vec<String>,
    /// URL param `createdAt`.
    pub created_at: Option<String>,
    /// URL param `imageId`.
    pub image_id: Option<u64>,
    /// URL param `page`.
    pub page: Option<u32>,
    /// URL param `limit`.
    pub limit: Option<u32>,
    /// Unknown query params that we keep for forward compatibility.
    pub extras: Vec<(String, String)>,
}

impl ImageSearchState {
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();

        pairs.push(("sortBy".to_string(), self.sort_by.as_query_value().to_string()));

        if let Some(query) = self.query.as_ref().filter(|q| !q.is_empty()) {
            pairs.push(("query".to_string(), query.to_string()));
        }

        if let Some(value) = self.created_at.as_ref().filter(|s| !s.is_empty()) {
            pairs.push(("createdAt".to_string(), value.to_string()));
        }

        if let Some(image_id) = self.image_id {
            pairs.push(("imageId".to_string(), image_id.to_string()));
        }

        Self::append_csv_pair(&mut pairs, "tags", &self.tags);
        Self::append_csv_pair(&mut pairs, "tools", &self.tools);
        Self::append_csv_pair(&mut pairs, "techniques", &self.techniques);
        Self::append_csv_pair(&mut pairs, "users", &self.users);
        Self::append_csv_pair(&mut pairs, "baseModel", &self.base_models);
        Self::append_csv_pair(&mut pairs, "aspectRatio", &self.aspect_ratios);

        if let Some(page) = self.page {
            pairs.push(("page".to_string(), page.to_string()));
        }

        if let Some(limit) = self.limit {
            pairs.push(("limit".to_string(), limit.to_string()));
        }

        pairs.extend(self.extras.iter().map(|(k, v)| (k.clone(), v.clone())));
        pairs
    }

    fn append_csv_pair(acc: &mut Vec<(String, String)>, key: &str, values: &[String]) {
        if values.is_empty() {
            return;
        }
        acc.push((key.to_string(), values.join(",")));
    }

    pub fn to_web_url(&self, base_url: &str) -> Result<Url> {
        let pairs = self.to_query_pairs();
        let url = Url::parse_with_params(base_url, pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .context("Failed to build Civitai image search URL")?;
        Ok(url)
    }

    pub fn from_web_url(raw: &str) -> Result<Self> {
        let normalized = Self::normalize_search_url(raw)?;
        let url = Url::parse(&normalized).context("Failed to parse search URL")?;
        let mut query = Self::default();

        let mut map: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
        for (key, value) in url.query_pairs() {
            map.entry(key.to_string())
                .or_default()
                .push(value.to_string());
        }

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

        query.tags = Self::split_multi(map.get("tags"));
        query.tools = Self::split_multi(map.get("tools"));
        query.techniques = Self::split_multi(map.get("techniques"));
        query.users = Self::split_multi(map.get("users"));
        query.base_models = Self::split_multi(map.get("baseModel"));
        query.aspect_ratios = Self::split_multi(map.get("aspectRatio"));

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
            if Self::is_known_key(&key) {
                continue;
            }
            for value in values {
                extras.push((key.clone(), value));
            }
        }
        query.extras = extras;

        Ok(query)
    }

    fn split_multi(values: Option<&Vec<String>>) -> Vec<String> {
        values
            .into_iter()
            .flat_map(|items| items.iter())
            .flat_map(|item| item.split(','))
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn is_known_key(key: &str) -> bool {
        matches!(
            key,
            "query"
                | "sortBy"
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

    fn normalize_search_url(raw: &str) -> Result<String> {
        if raw.contains("://") {
            return Ok(raw.to_string());
        }

        if raw.starts_with('?') {
            return Ok(format!("https://example.local/search/images{raw}"));
        }

        if raw.starts_with('/') {
            return Ok(format!("https://example.local{raw}"));
        }

        Ok(format!("https://example.local/search/images?{raw}"))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct ModelSearchState {
    pub query: Option<String>,
    pub sort_by: ModelSearchSortBy,
    pub base_models: Vec<String>,
    pub types: Vec<String>,
    pub checkpoint_types: Vec<String>,
    pub file_formats: Vec<String>,
    pub categories: Vec<String>,
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

        pairs.push(("sortBy".to_string(), self.sort_by.as_query_value().to_string()));

        if let Some(query) = self.query.as_ref().filter(|q| !q.is_empty()) {
            pairs.push(("query".to_string(), query.to_string()));
        }

        if let Some(value) = self.created_at.as_ref().filter(|s| !s.is_empty()) {
            pairs.push(("createdAt".to_string(), value.to_string()));
        }

        ImageSearchState::append_csv_pair(&mut pairs, "baseModel", &self.base_models);
        ImageSearchState::append_csv_pair(&mut pairs, "type", &self.types);
        ImageSearchState::append_csv_pair(&mut pairs, "checkpointType", &self.checkpoint_types);
        ImageSearchState::append_csv_pair(&mut pairs, "fileFormats", &self.file_formats);
        ImageSearchState::append_csv_pair(&mut pairs, "category", &self.categories);
        ImageSearchState::append_csv_pair(&mut pairs, "users", &self.users);
        ImageSearchState::append_csv_pair(&mut pairs, "tags", &self.tags);

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
        let url = Url::parse_with_params(base_url, pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .context("Failed to build Civitai model search URL")?;
        Ok(url)
    }

    pub fn from_web_url(raw: &str) -> Result<Self> {
        let normalized = ImageSearchState::normalize_search_url(raw)?;
        let url = Url::parse(&normalized).context("Failed to parse model search URL")?;
        let mut query = Self::default();

        let mut map: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
        for (key, value) in url.query_pairs() {
            map.entry(key.to_string())
                .or_default()
                .push(value.to_string());
        }

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

        query.base_models = Self::split_multi_keys(&map, &["baseModel", "baseModels"]);
        query.types = Self::split_multi_keys(&map, &["type", "types"]);
        query.checkpoint_types = Self::split_multi_keys(&map, &["checkpointType", "checkpointTypes"]);
        query.file_formats = Self::split_multi_keys(&map, &["fileFormat", "fileFormats"]);
        query.categories = Self::split_multi_keys(&map, &["category", "categories"]);
        query.users = Self::split_multi_keys(&map, &["users"]);
        query.tags = Self::split_multi_keys(&map, &["tags"]);

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
            if Self::is_known_key(&key) {
                continue;
            }
            for value in values {
                extras.push((key.clone(), value));
            }
        }
        query.extras = extras;

        Ok(query)
    }

    fn split_multi_keys(
        map: &std::collections::BTreeMap<String, Vec<String>>,
        keys: &[&str],
    ) -> Vec<String> {
        keys.iter()
            .filter_map(|key| map.get(*key))
            .flat_map(|items| items.iter())
            .flat_map(|item| item.split(','))
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn is_known_key(key: &str) -> bool {
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
    pub tag_names: Vec<String>,
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
        self.media_url_with_namespace(CIVITAI_MEDIA_DELIVERY_NAMESPACE)
    }

    pub fn image_page_url(&self) -> String {
        format!("{CIVITAI_WEB_URL}/images/{}", self.id)
    }

    pub fn media_url_with_namespace(&self, namespace: &str) -> Option<String> {
        let token = self.media_token()?;
        let namespace = namespace.trim().trim_matches('/');
        if namespace.is_empty() {
            return None;
        }

        Some(format!(
            "{CIVITAI_MEDIA_DELIVERY_URL}/{namespace}/{token}/original=true"
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
        format!("{CIVITAI_WEB_URL}/models/{}", self.id)
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
        self.primary_model_version_id().map(build_model_download_url)
    }

    pub fn model_download_url_with_token(&self, token: &str) -> Option<String> {
        self.primary_model_version_id()
            .map(|version_id| build_model_download_url_with_token(version_id, token))
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
