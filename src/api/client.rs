use anyhow::{Context, Result};
use reqwest::{Client, IntoUrl};

use super::types::{ImageResponse, Model, ModelVersion};

#[derive(Clone, Default, Debug)]
pub struct ImageSearchOptions {
    pub limit: u32,
    pub nsfw: Option<String>,
    pub sort: Option<String>,
    pub period: Option<String>,
    pub model_version_id: Option<u64>,
    pub tags: Option<u64>,
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
