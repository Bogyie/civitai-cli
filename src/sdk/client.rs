use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

use super::constants::{
    CIVITAI_IMAGE_SEARCH_CLIENT_KEY, CIVITAI_IMAGE_SEARCH_MEILI_URL,
    CIVITAI_MEDIA_DELIVERY_NAMESPACE, CIVITAI_MEDIA_DELIVERY_URL,
    CIVITAI_MODEL_DOWNLOAD_API_URL, CIVITAI_WEB_URL, IMAGES_SEARCH_INDEX, MODELS_SEARCH_INDEX,
};
use super::image_search::{ImageSearchState, SearchImageHit, SearchImageResponse};
use super::model_search::{
    build_model_download_url_with_token_and_base, ModelDownloadAuth, ModelSearchState,
    SearchModelHit, SearchModelResponse,
};
use super::shared::{build_created_at_filters, push_equals_filters};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSdkConfig {
    pub meili_base_url: String,
    pub meili_client_key: String,
    pub civitai_web_url: String,
    pub media_delivery_url: String,
    pub media_delivery_namespace: String,
    pub model_download_api_url: String,
    pub images_index: String,
    pub models_index: String,
    pub user_agent: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSdkConfigBuilder {
    config: SearchSdkConfig,
}

impl Default for SearchSdkConfig {
    fn default() -> Self {
        Self {
            meili_base_url: CIVITAI_IMAGE_SEARCH_MEILI_URL.to_string(),
            meili_client_key: CIVITAI_IMAGE_SEARCH_CLIENT_KEY.to_string(),
            civitai_web_url: CIVITAI_WEB_URL.to_string(),
            media_delivery_url: CIVITAI_MEDIA_DELIVERY_URL.to_string(),
            media_delivery_namespace: CIVITAI_MEDIA_DELIVERY_NAMESPACE.to_string(),
            model_download_api_url: CIVITAI_MODEL_DOWNLOAD_API_URL.to_string(),
            images_index: IMAGES_SEARCH_INDEX.to_string(),
            models_index: MODELS_SEARCH_INDEX.to_string(),
            user_agent: "civitai-search-sdk/0.1".to_string(),
        }
    }
}

impl SearchSdkConfig {
    pub fn builder() -> SearchSdkConfigBuilder {
        SearchSdkConfigBuilder {
            config: Self::default(),
        }
    }
}

impl Default for SearchSdkConfigBuilder {
    fn default() -> Self {
        SearchSdkConfig::builder()
    }
}

impl SearchSdkConfigBuilder {
    pub fn meili_base_url(mut self, value: impl Into<String>) -> Self {
        self.config.meili_base_url = value.into();
        self
    }

    pub fn meili_client_key(mut self, value: impl Into<String>) -> Self {
        self.config.meili_client_key = value.into();
        self
    }

    pub fn civitai_web_url(mut self, value: impl Into<String>) -> Self {
        self.config.civitai_web_url = value.into();
        self
    }

    pub fn media_delivery_url(mut self, value: impl Into<String>) -> Self {
        self.config.media_delivery_url = value.into();
        self
    }

    pub fn media_delivery_namespace(mut self, value: impl Into<String>) -> Self {
        self.config.media_delivery_namespace = value.into();
        self
    }

    pub fn model_download_api_url(mut self, value: impl Into<String>) -> Self {
        self.config.model_download_api_url = value.into();
        self
    }

    pub fn images_index(mut self, value: impl Into<String>) -> Self {
        self.config.images_index = value.into();
        self
    }

    pub fn models_index(mut self, value: impl Into<String>) -> Self {
        self.config.models_index = value.into();
        self
    }

    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.config.user_agent = value.into();
        self
    }

    pub fn build(self) -> SearchSdkConfig {
        self.config
    }
}

#[derive(Clone)]
pub struct SearchSdkClient {
    client: Client,
    config: SearchSdkConfig,
}

impl SearchSdkClient {
    pub fn new() -> Result<Self> {
        Self::with_config(SearchSdkConfig::default())
    }

    pub fn with_config(config: SearchSdkConfig) -> Result<Self> {
        let client = Client::builder()
            .user_agent(&config.user_agent)
            .build()
            .context("Failed to build HTTP client")?;
        Ok(Self { client, config })
    }

    pub fn config(&self) -> &SearchSdkConfig {
        &self.config
    }

    fn build_image_meili_payload(state: &ImageSearchState) -> Value {
        let limit = state.limit.unwrap_or(50);
        let page_index = state.page.unwrap_or(0);
        let offset = page_index.saturating_mul(limit);

        let mut filters = Vec::new();
        push_equals_filters(&mut filters, "tagNames", &state.tags);
        push_equals_filters(&mut filters, "toolNames", &state.tools);
        push_equals_filters(&mut filters, "techniqueNames", &state.techniques);
        push_equals_filters(&mut filters, "user.username", &state.users);
        push_equals_filters(&mut filters, "baseModel", &state.base_models);
        push_equals_filters(&mut filters, "aspectRatio", &state.aspect_ratios);

        if let Some(created_at) = state.created_at.as_ref() {
            filters.extend(build_created_at_filters(created_at, "createdAtUnix"));
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
        push_equals_filters(&mut filters, "version.baseModel", &state.base_models);
        push_equals_filters(&mut filters, "type", &state.types);
        push_equals_filters(&mut filters, "checkpointType", &state.checkpoint_types);
        push_equals_filters(&mut filters, "fileFormats", &state.file_formats);
        push_equals_filters(&mut filters, "category.name", &state.categories);
        push_equals_filters(&mut filters, "user.username", &state.users);
        push_equals_filters(&mut filters, "tags.name", &state.tags);

        if let Some(created_at) = state.created_at.as_ref() {
            filters.extend(build_created_at_filters(created_at, "lastVersionAtUnix"));
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
        let url = format!(
            "{}/indexes/{}/search",
            self.config.meili_base_url.trim_end_matches('/'),
            self.config.images_index.trim_matches('/')
        );
        let payload = Self::build_image_meili_payload(state);
        let response = self
            .client
            .post(url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.meili_client_key),
            )
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
        let url = format!(
            "{}/indexes/{}/search",
            self.config.meili_base_url.trim_end_matches('/'),
            self.config.models_index.trim_matches('/')
        );
        let payload = Self::build_model_meili_payload(state);
        let response = self
            .client
            .post(url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.meili_client_key),
            )
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
                self.build_model_download_url_with_token(version_id, token)
            }
            _ => self.build_model_download_url(version_id),
        };
        let mut request = self.client.get(request_url);

        if let Some(ModelDownloadAuth::BearerToken(token)) = auth {
            let token = token.trim();
            if !token.is_empty() {
                request = request.bearer_auth(token);
            }
        }

        request
    }

    pub fn image_page_url(&self, hit: &SearchImageHit) -> String {
        hit.image_page_url_with_base(&self.config.civitai_web_url)
    }

    pub fn original_media_url(&self, hit: &SearchImageHit) -> Option<String> {
        hit.media_url_with_base_and_namespace(
            &self.config.media_delivery_url,
            &self.config.media_delivery_namespace,
        )
    }

    pub fn media_url_with_namespace(
        &self,
        hit: &SearchImageHit,
        namespace: &str,
    ) -> Option<String> {
        hit.media_url_with_base_and_namespace(&self.config.media_delivery_url, namespace)
    }

    pub fn model_page_url(&self, hit: &SearchModelHit) -> String {
        hit.model_page_url_with_base(&self.config.civitai_web_url)
    }

    pub fn build_model_download_url(&self, version_id: u64) -> String {
        let base = self.config.model_download_api_url.trim_end_matches('/');
        format!("{base}/{version_id}")
    }

    pub fn build_model_download_url_with_token(&self, version_id: u64, token: &str) -> String {
        build_model_download_url_with_token_and_base(
            &self.config.model_download_api_url,
            version_id,
            token,
        )
    }

    pub fn model_download_url(&self, hit: &SearchModelHit) -> Option<String> {
        hit.model_download_url_with_base(&self.config.model_download_api_url)
    }

    pub fn model_download_url_with_token(
        &self,
        hit: &SearchModelHit,
        token: &str,
    ) -> Option<String> {
        hit.model_download_url_with_token_and_base(&self.config.model_download_api_url, token)
    }
}
