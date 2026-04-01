use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use reqwest::{Client, IntoUrl, StatusCode};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use super::api::{
    ApiImageResponse, ApiImageSearchOptions, ApiModel, ApiModelSearchOptions, ApiModelVersion,
    ApiPaginatedResponse, build_api_images_search_url, build_api_model_url,
    build_api_model_version_by_hash_url, build_api_models_search_url,
};
use super::constants::{
    CIVITAI_IMAGE_SEARCH_CLIENT_KEY, CIVITAI_IMAGE_SEARCH_MEILI_URL,
    CIVITAI_MEDIA_DELIVERY_NAMESPACE, CIVITAI_MEDIA_DELIVERY_URL,
    CIVITAI_MODEL_DOWNLOAD_API_URL, CIVITAI_WEB_URL, IMAGES_SEARCH_INDEX, MODELS_SEARCH_INDEX,
};
use super::download::{
    DownloadControl, DownloadDestination, DownloadEvent, DownloadKind, DownloadOptions,
    DownloadResult, DownloadSpec, authorization_header_value, content_disposition_file_name,
    content_range_total, emit_event, ensure_parent_dir,
};
use super::image_search::{ImageSearchState, SearchImageHit, SearchImageResponse};
use super::model_search::{
    ModelDownloadAuth, ModelSearchState, SearchModelHit, SearchModelResponse,
    build_model_download_url_with_token_and_base,
};
use super::shared::{build_created_at_filters, push_equals_filters};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSdkConfig {
    pub api_base_url: String,
    pub api_key: Option<String>,
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

impl Default for SearchSdkConfig {
    fn default() -> Self {
        Self {
            api_base_url: CIVITAI_WEB_URL.to_string(),
            api_key: None,
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
    pub fn builder() -> SdkClientBuilder {
        SdkClientBuilder::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkClientBuilder {
    config: SearchSdkConfig,
}

impl Default for SdkClientBuilder {
    fn default() -> Self {
        Self {
            config: SearchSdkConfig::default(),
        }
    }
}

impl SdkClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn api_base_url(mut self, value: impl Into<String>) -> Self {
        self.config.api_base_url = value.into();
        self
    }

    pub fn api_key(mut self, value: impl Into<String>) -> Self {
        self.config.api_key = Some(value.into());
        self
    }

    pub fn clear_api_key(mut self) -> Self {
        self.config.api_key = None;
        self
    }

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

    pub fn build_config(self) -> SearchSdkConfig {
        self.config
    }

    pub fn build_web(self) -> Result<WebSearchClient> {
        WebSearchClient::from_config(self.config)
    }

    pub fn build_api(self) -> Result<ApiClient> {
        ApiClient::from_config(self.config)
    }

    pub fn build_download(self) -> Result<DownloadClient> {
        DownloadClient::from_config(self.config)
    }

    pub fn build_clients(self) -> Result<SdkClients> {
        SdkClients::from_config(self.config)
    }
}

#[derive(Clone)]
pub struct WebSearchClient {
    client: Client,
    config: SearchSdkConfig,
}

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    config: SearchSdkConfig,
}

#[derive(Clone)]
pub struct DownloadClient {
    client: Client,
    config: SearchSdkConfig,
}

#[derive(Clone)]
pub struct SdkClients {
    pub web: WebSearchClient,
    pub api: ApiClient,
    pub download: DownloadClient,
}

impl SdkClients {
    pub fn from_config(config: SearchSdkConfig) -> Result<Self> {
        let web = WebSearchClient::from_config(config.clone())?;
        let api = ApiClient::from_config(config.clone())?;
        let download = DownloadClient::from_config(config)?;
        Ok(Self {
            web,
            api,
            download,
        })
    }
}

impl WebSearchClient {
    pub fn new() -> Result<Self> {
        Self::from_config(SearchSdkConfig::default())
    }

    pub fn with_config(config: SearchSdkConfig) -> Result<Self> {
        Self::from_config(config)
    }

    pub fn from_config(config: SearchSdkConfig) -> Result<Self> {
        Ok(Self {
            client: build_http_client(&config)?,
            config,
        })
    }

    pub fn config(&self) -> &SearchSdkConfig {
        &self.config
    }

    pub async fn search_images_raw(&self, state: &ImageSearchState) -> Result<Value> {
        let url = format!(
            "{}/indexes/{}/search",
            self.config.meili_base_url.trim_end_matches('/'),
            self.config.images_index.trim_matches('/')
        );
        let payload = build_image_meili_payload(state);
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

    pub async fn search_images(&self, state: &ImageSearchState) -> Result<SearchImageResponse> {
        let value = self.search_images_raw(state).await?;
        serde_json::from_value(value).context("Failed to decode typed web image search response")
    }

    pub async fn search_models_raw(&self, state: &ModelSearchState) -> Result<Value> {
        let url = format!(
            "{}/indexes/{}/search",
            self.config.meili_base_url.trim_end_matches('/'),
            self.config.models_index.trim_matches('/')
        );
        let payload = build_model_meili_payload(state);
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

    pub async fn search_models(&self, state: &ModelSearchState) -> Result<SearchModelResponse> {
        let value = self.search_models_raw(state).await?;
        serde_json::from_value(value).context("Failed to decode typed web model search response")
    }
}

impl ApiClient {
    pub fn new() -> Result<Self> {
        Self::from_config(SearchSdkConfig::default())
    }

    pub fn with_config(config: SearchSdkConfig) -> Result<Self> {
        Self::from_config(config)
    }

    pub fn from_config(config: SearchSdkConfig) -> Result<Self> {
        Ok(Self {
            client: build_http_client(&config)?,
            config,
        })
    }

    pub fn config(&self) -> &SearchSdkConfig {
        &self.config
    }

    pub async fn get_model(&self, model_id: u64) -> Result<ApiModel> {
        let url = build_api_model_url(&self.config.api_base_url, model_id);
        self.fetch(url).await
    }

    pub async fn get_model_version_by_hash(&self, hash: &str) -> Result<ApiModelVersion> {
        let url = build_api_model_version_by_hash_url(&self.config.api_base_url, hash);
        self.fetch(url).await
    }

    pub async fn search_models(
        &self,
        opts: &ApiModelSearchOptions,
    ) -> Result<ApiPaginatedResponse<ApiModel>> {
        let url = build_api_models_search_url(&self.config.api_base_url, opts)?;
        self.fetch(url).await
    }

    pub async fn search_models_by_url(
        &self,
        url: impl IntoUrl,
    ) -> Result<ApiPaginatedResponse<ApiModel>> {
        self.fetch(url).await
    }

    pub async fn search_images(&self, opts: &ApiImageSearchOptions) -> Result<ApiImageResponse> {
        let url = build_api_images_search_url(&self.config.api_base_url, opts)?;
        self.fetch(url).await
    }

    pub async fn get_images_by_url(&self, url: impl IntoUrl) -> Result<ApiImageResponse> {
        self.fetch(url).await
    }

    async fn fetch<T: serde::de::DeserializeOwned, U: IntoUrl>(&self, url: U) -> Result<T> {
        let mut request = self.client.get(url);
        if let Some(api_key) = self
            .config
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            request = request.bearer_auth(api_key);
        }

        let response = request
            .send()
            .await
            .context("Failed to send Civitai API request")?
            .error_for_status()
            .context("Civitai API endpoint returned error")?;

        response
            .json::<T>()
            .await
            .context("Failed to decode Civitai API response")
    }
}

impl DownloadClient {
    pub fn new() -> Result<Self> {
        Self::from_config(SearchSdkConfig::default())
    }

    pub fn with_config(config: SearchSdkConfig) -> Result<Self> {
        Self::from_config(config)
    }

    pub fn from_config(config: SearchSdkConfig) -> Result<Self> {
        Ok(Self {
            client: build_http_client(&config)?,
            config,
        })
    }

    pub fn config(&self) -> &SearchSdkConfig {
        &self.config
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

    pub fn build_media_download_spec(&self, hit: &SearchImageHit) -> Option<DownloadSpec> {
        let url = self.original_media_url(hit)?;
        Some(
            DownloadSpec::new(url, hit.download_kind())
                .with_file_name(hit.default_download_file_name()),
        )
    }

    pub fn build_image_download_spec(&self, hit: &SearchImageHit) -> Option<DownloadSpec> {
        let spec = self.build_media_download_spec(hit)?;
        (spec.kind == DownloadKind::Image).then_some(spec)
    }

    pub fn build_video_download_spec(&self, hit: &SearchImageHit) -> Option<DownloadSpec> {
        let spec = self.build_media_download_spec(hit)?;
        (spec.kind == DownloadKind::Video).then_some(spec)
    }

    pub fn build_model_download_spec(
        &self,
        hit: &SearchModelHit,
        auth: Option<ModelDownloadAuth>,
    ) -> Option<DownloadSpec> {
        let url = match auth.as_ref() {
            Some(ModelDownloadAuth::QueryToken(token)) => {
                self.model_download_url_with_token(hit, token)?
            }
            _ => self.model_download_url(hit)?,
        };

        Some(DownloadSpec {
            url,
            kind: DownloadKind::Model,
            file_name: Some(hit.default_download_file_name()),
            auth: match auth {
                Some(ModelDownloadAuth::QueryToken(_)) => None,
                value => value,
            },
        })
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

    pub fn build_download_request(
        &self,
        url: &str,
        auth: Option<&ModelDownloadAuth>,
        range_start: Option<u64>,
    ) -> Result<reqwest::RequestBuilder> {
        let request_url = match auth {
            Some(ModelDownloadAuth::QueryToken(token)) => append_query_token(url, token)?,
            _ => url.to_string(),
        };

        let mut request = self.client.get(request_url);

        if let Some(ModelDownloadAuth::BearerToken(token)) = auth {
            let token = token.trim();
            if !token.is_empty() {
                request = request.header(
                    reqwest::header::AUTHORIZATION,
                    authorization_header_value(token)?,
                );
            }
        }

        if let Some(start) = range_start {
            request = request.header(reqwest::header::RANGE, format!("bytes={start}-"));
        }

        Ok(request)
    }

    pub async fn download(
        &self,
        spec: &DownloadSpec,
        options: &DownloadOptions,
        progress_tx: Option<mpsc::Sender<DownloadEvent>>,
        mut control_rx: Option<mpsc::Receiver<DownloadControl>>,
    ) -> Result<DownloadResult> {
        let provisional_path = match &options.destination {
            DownloadDestination::File(path) => path.clone(),
            DownloadDestination::Directory(path) => path.join(spec.suggested_file_name()),
        };

        if options.create_parent_dirs {
            ensure_parent_dir(&provisional_path).await?;
        }

        let initial_response = self
            .build_download_request(&spec.url, spec.auth.as_ref(), None)?
            .send()
            .await
            .context("Failed to send download request")?
            .error_for_status()
            .context("Download endpoint returned error")?;

        let headers = initial_response.headers().clone();
        let content_type = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        let actual_target_path = match &options.destination {
            DownloadDestination::File(_) => provisional_path,
            DownloadDestination::Directory(path) => {
                let file_name = content_disposition_file_name(&headers)
                    .or_else(|| {
                        provisional_path
                            .file_name()
                            .map(|value| value.to_string_lossy().to_string())
                    })
                    .unwrap_or_else(|| spec.suggested_file_name());
                path.join(file_name)
            }
        };

        if options.create_parent_dirs {
            ensure_parent_dir(&actual_target_path).await?;
        }

        let existing_size = if options.resume {
            tokio::fs::metadata(&actual_target_path)
                .await
                .map(|metadata| metadata.len())
                .unwrap_or(0)
        } else {
            0
        };
        let should_resume = options.resume
            && existing_size > 0
            && matches!(&options.destination, DownloadDestination::File(_));

        let response = if should_resume {
            self.build_download_request(&spec.url, spec.auth.as_ref(), Some(existing_size))?
                .send()
                .await
                .context("Failed to send resumable download request")?
                .error_for_status()
                .context("Download endpoint returned error")?
        } else {
            initial_response
        };

        let status = response.status();
        let headers = response.headers().clone();
        let total_bytes = if should_resume && status == StatusCode::PARTIAL_CONTENT {
            content_range_total(&headers).or_else(|| {
                response
                    .content_length()
                    .map(|length| length.saturating_add(existing_size))
            })
        } else {
            response.content_length()
        };
        let resumed = should_resume && status == StatusCode::PARTIAL_CONTENT;

        let mut downloaded_bytes = if resumed { existing_size } else { 0 };
        let mut file = if resumed {
            tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&actual_target_path)
                .await
                .with_context(|| format!("Failed to open {}", actual_target_path.display()))?
        } else {
            if !options.overwrite
                && tokio::fs::try_exists(&actual_target_path)
                    .await
                    .unwrap_or(false)
            {
                return Err(anyhow!(
                    "Refusing to overwrite {}",
                    actual_target_path.display()
                ));
            }
            tokio::fs::File::create(&actual_target_path)
                .await
                .with_context(|| format!("Failed to create {}", actual_target_path.display()))?
        };

        emit_event(
            &progress_tx,
            DownloadEvent::Started {
                path: actual_target_path.clone(),
                total_bytes,
                resumed,
            },
        )
        .await;

        let mut stream = response.bytes_stream();
        let mut paused = false;
        let mut last_percent = -1.0f64;

        loop {
            if paused {
                if let Some(control) = control_rx.as_mut() {
                    match control.recv().await {
                        Some(DownloadControl::Pause) => continue,
                        Some(DownloadControl::Resume) => {
                            paused = false;
                            emit_event(
                                &progress_tx,
                                DownloadEvent::Resumed {
                                    downloaded_bytes,
                                    total_bytes,
                                },
                            )
                            .await;
                        }
                        Some(DownloadControl::Cancel) => {
                            emit_event(
                                &progress_tx,
                                DownloadEvent::Cancelled {
                                    path: actual_target_path.clone(),
                                    downloaded_bytes,
                                    total_bytes,
                                },
                            )
                            .await;
                            return Err(anyhow!("download cancelled"));
                        }
                        None => return Err(anyhow!("control channel closed")),
                    }
                } else {
                    paused = false;
                }
            }

            match control_rx.as_mut() {
                None => match stream.next().await {
                    Some(chunk) => {
                        let chunk = chunk.context("Failed to stream response body")?;
                        file.write_all(&chunk).await?;
                        downloaded_bytes += chunk.len() as u64;
                        maybe_emit_progress(
                            &progress_tx,
                            downloaded_bytes,
                            total_bytes,
                            options.progress_step_percent,
                            &mut last_percent,
                        )
                        .await;
                    }
                    None => break,
                },
                Some(control) => {
                    tokio::select! {
                        chunk = stream.next() => {
                            match chunk {
                                Some(chunk) => {
                                    let chunk = chunk.context("Failed to stream response body")?;
                                    file.write_all(&chunk).await?;
                                    downloaded_bytes += chunk.len() as u64;
                                    maybe_emit_progress(
                                        &progress_tx,
                                        downloaded_bytes,
                                        total_bytes,
                                        options.progress_step_percent,
                                        &mut last_percent,
                                    ).await;
                                }
                                None => break,
                            }
                        }
                        Some(cmd) = control.recv() => {
                            match cmd {
                                DownloadControl::Pause => {
                                    paused = true;
                                    emit_event(
                                        &progress_tx,
                                        DownloadEvent::Paused {
                                            downloaded_bytes,
                                            total_bytes,
                                        },
                                    ).await;
                                }
                                DownloadControl::Resume => {}
                                DownloadControl::Cancel => {
                                    emit_event(
                                        &progress_tx,
                                        DownloadEvent::Cancelled {
                                            path: actual_target_path.clone(),
                                            downloaded_bytes,
                                            total_bytes,
                                        },
                                    ).await;
                                    return Err(anyhow!("download cancelled"));
                                }
                            }
                        }
                        else => return Err(anyhow!("control channel closed")),
                    }
                }
            }
        }

        emit_event(
            &progress_tx,
            DownloadEvent::Completed {
                path: actual_target_path.clone(),
                downloaded_bytes,
                total_bytes,
            },
        )
        .await;

        Ok(DownloadResult {
            path: actual_target_path,
            downloaded_bytes,
            total_bytes,
            resumed,
            content_type,
        })
    }
}

fn build_http_client(config: &SearchSdkConfig) -> Result<Client> {
    Client::builder()
        .user_agent(&config.user_agent)
        .build()
        .context("Failed to build HTTP client")
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

fn append_query_token(url: &str, token: &str) -> Result<String> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(url.to_string());
    }

    let mut parsed = reqwest::Url::parse(url).context("Failed to parse download URL")?;
    parsed.query_pairs_mut().append_pair("token", token);
    Ok(parsed.into())
}

async fn maybe_emit_progress(
    progress_tx: &Option<mpsc::Sender<DownloadEvent>>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    progress_step_percent: f64,
    last_percent: &mut f64,
) {
    let percent = total_bytes.and_then(|total| {
        if total == 0 {
            None
        } else {
            Some((downloaded_bytes as f64 / total as f64) * 100.0)
        }
    });

    let should_emit = match percent {
        Some(percent) => {
            progress_step_percent <= 0.0
                || *last_percent < 0.0
                || percent - *last_percent >= progress_step_percent
        }
        None => true,
    };

    if should_emit {
        if let Some(percent) = percent {
            *last_percent = percent;
        }
        emit_event(
            progress_tx,
            DownloadEvent::Progress {
                downloaded_bytes,
                total_bytes,
                percent,
            },
        )
        .await;
    }
}
