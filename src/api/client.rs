use anyhow::{Context, Result};
use reqwest::{Client, IntoUrl};

use super::types::{Model, ModelVersion, ImageResponse, PaginatedResponse};

#[derive(Clone, Default, Debug)]
pub struct SearchOptions {
    pub query: String,
    pub limit: u32,
    pub tag: Option<String>,
    pub username: Option<String>,
    pub sort: Option<String>,
    pub types: Option<String>,
    pub period: Option<String>,
    pub rating: Option<u32>,
    pub favorites: Option<bool>,
    pub hidden: Option<bool>,
    pub primary_file_only: Option<bool>,
    pub allow_no_credit: Option<bool>,
    pub allow_derivatives: Option<bool>,
    pub allow_different_licenses: Option<bool>,
    pub allow_commercial_use: Option<String>,
    pub nsfw: Option<bool>,
    pub supports_generation: Option<bool>,
    pub ids: Option<Vec<u64>>,
    pub base_models: Option<String>,
}

pub struct CivitaiClient {
    client: Client,
    api_key: Option<String>,
}

impl CivitaiClient {
    pub fn new(api_key: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .user_agent("civitai-cli/0.1")
            .build()
            .context("Failed to build HTTP client")?;
        
        Ok(Self { client, api_key })
    }

    pub async fn get_model(&self, model_id: u64) -> Result<Model> {
        let url = format!("https://civitai.com/api/v1/models/{}", model_id);
        self.fetch(&url).await
    }

    pub async fn get_model_version_by_hash(&self, hash: &str) -> Result<ModelVersion> {
        let url = format!("https://civitai.com/api/v1/model-versions/by-hash/{}", hash);
        self.fetch(&url).await
    }

    pub async fn search_models(&self, opts: SearchOptions) -> Result<PaginatedResponse<Model>> {
        let mut url = format!("https://civitai.com/api/v1/models?limit={}", opts.limit);

        if !opts.query.is_empty() {
            url.push_str(&format!("&query={}", opts.query.replace(" ", "%20")));
        }
        if let Some(tag) = &opts.tag {
            if !tag.is_empty() {
                url.push_str(&format!("&tag={}", tag.replace(" ", "%20")));
            }
        }
        if let Some(username) = &opts.username {
            if !username.is_empty() {
                url.push_str(&format!("&username={}", username.replace(" ", "%20")));
            }
        }
        if let Some(s) = &opts.sort {
            if s != "All" { url.push_str(&format!("&sort={}", s.replace(" ", "%20"))); }
        }
        if let Some(t) = &opts.types {
            if t != "All" { url.push_str(&format!("&types={}", t.replace(" ", "%20"))); }
        }
        if let Some(period) = &opts.period {
            if !period.is_empty() {
                url.push_str(&format!("&period={}", period.replace(" ", "%20")));
            }
        }
        if let Some(rating) = &opts.rating {
            url.push_str(&format!("&rating={}", rating));
        }
        if let Some(favorites) = opts.favorites {
            url.push_str(&format!("&favorites={}", favorites));
        }
        if let Some(hidden) = opts.hidden {
            url.push_str(&format!("&hidden={}", hidden));
        }
        if let Some(primary_file_only) = opts.primary_file_only {
            url.push_str(&format!("&primaryFileOnly={}", primary_file_only));
        }
        if let Some(allow_no_credit) = opts.allow_no_credit {
            url.push_str(&format!("&allowNoCredit={}", allow_no_credit));
        }
        if let Some(allow_derivatives) = opts.allow_derivatives {
            url.push_str(&format!("&allowDerivatives={}", allow_derivatives));
        }
        if let Some(allow_different_licenses) = opts.allow_different_licenses {
            url.push_str(&format!(
                "&allowDifferentLicenses={}",
                allow_different_licenses
            ));
        }
        if let Some(allow_commercial_use) = &opts.allow_commercial_use {
            if !allow_commercial_use.is_empty() {
                url.push_str(&format!(
                    "&allowCommercialUse={}",
                    allow_commercial_use.replace(" ", "%20")
                ));
            }
        }
        if let Some(nsfw) = opts.nsfw {
            url.push_str(&format!("&nsfw={}", nsfw));
        }
        if let Some(supports_generation) = opts.supports_generation {
            url.push_str(&format!("&supportsGeneration={}", supports_generation));
        }
        if let Some(ids) = &opts.ids {
            if !ids.is_empty() {
                let joined = ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                url.push_str(&format!("&ids={}", joined));
            }
        }
        if let Some(b) = &opts.base_models {
            if b != "All" { url.push_str(&format!("&baseModels={}", b.replace(" ", "%20"))); }
        }

        self.fetch(&url).await
    }

    pub async fn search_models_by_url(&self, url: String) -> Result<PaginatedResponse<Model>> {
        self.fetch(&url).await
    }

    pub async fn get_images(&self, limit: u32) -> Result<ImageResponse> {
        let url = format!("https://civitai.com/api/v1/images?limit={}", limit);
        self.fetch(&url).await
    }

    pub async fn get_images_by_url(&self, url: String) -> Result<ImageResponse> {
        self.fetch(&url).await
    }

    async fn fetch<T: serde::de::DeserializeOwned, U: IntoUrl>(&self, url: U) -> Result<T> {
        let mut req = self.client.get(url);
        
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let res = req.send().await?.error_for_status()?;
        let data = res.json::<T>().await?;
        Ok(data)
    }
}
