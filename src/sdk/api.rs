use anyhow::{Context, Result};
use reqwest::Url;

pub use super::api_types::{
    FileMetadata, ImageItem as ApiImageItem, ImageResponse as ApiImageResponse,
    ImageStats as ApiImageStats, Model as ApiModel, ModelCreator as ApiModelCreator,
    ModelFile as ApiModelFile, ModelImage as ApiModelImage, ModelStats as ApiModelStats,
    ModelTag as ApiModelTag, ModelVersion as ApiModelVersion, NsfwValue as ApiNsfwValue,
    PaginatedResponse as ApiPaginatedResponse, PaginationMetadata as ApiPaginationMetadata,
    VersionStats as ApiVersionStats,
};

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct ApiModelSearchOptions {
    pub query: Option<String>,
    pub limit: Option<u32>,
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

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct ApiImageSearchOptions {
    pub limit: Option<u32>,
    pub nsfw: Option<String>,
    pub sort: Option<String>,
    pub period: Option<String>,
    pub model_version_id: Option<u64>,
    pub tags: Option<u64>,
}

pub fn build_api_model_url(base_url: &str, model_id: u64) -> String {
    format!("{}/models/{model_id}", api_v1_base(base_url))
}

pub fn build_api_model_version_by_hash_url(base_url: &str, hash: &str) -> String {
    format!("{}/model-versions/by-hash/{}", api_v1_base(base_url), hash)
}

pub fn build_api_models_search_url(base_url: &str, opts: &ApiModelSearchOptions) -> Result<Url> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    pairs.push(("limit".to_string(), opts.limit.unwrap_or(20).to_string()));

    push_optional_string(&mut pairs, "query", opts.query.as_deref());
    push_optional_string(&mut pairs, "tag", opts.tag.as_deref());
    push_optional_string(&mut pairs, "username", opts.username.as_deref());
    push_optional_string(
        &mut pairs,
        "sort",
        opts.sort.as_deref().filter(|v| *v != "All"),
    );
    push_optional_string(
        &mut pairs,
        "types",
        opts.types.as_deref().filter(|v| *v != "All"),
    );
    push_optional_string(&mut pairs, "period", opts.period.as_deref());

    if let Some(value) = opts.rating {
        pairs.push(("rating".to_string(), value.to_string()));
    }
    if let Some(value) = opts.favorites {
        pairs.push(("favorites".to_string(), value.to_string()));
    }
    if let Some(value) = opts.hidden {
        pairs.push(("hidden".to_string(), value.to_string()));
    }
    if let Some(value) = opts.primary_file_only {
        pairs.push(("primaryFileOnly".to_string(), value.to_string()));
    }
    if let Some(value) = opts.allow_no_credit {
        pairs.push(("allowNoCredit".to_string(), value.to_string()));
    }
    if let Some(value) = opts.allow_derivatives {
        pairs.push(("allowDerivatives".to_string(), value.to_string()));
    }
    if let Some(value) = opts.allow_different_licenses {
        pairs.push(("allowDifferentLicenses".to_string(), value.to_string()));
    }
    push_optional_string(
        &mut pairs,
        "allowCommercialUse",
        opts.allow_commercial_use.as_deref(),
    );
    if let Some(value) = opts.nsfw {
        pairs.push(("nsfw".to_string(), value.to_string()));
    }
    if let Some(value) = opts.supports_generation {
        pairs.push(("supportsGeneration".to_string(), value.to_string()));
    }
    if let Some(ids) = opts.ids.as_ref().filter(|ids| !ids.is_empty()) {
        let joined = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        pairs.push(("ids".to_string(), joined));
    }
    push_optional_string(
        &mut pairs,
        "baseModels",
        opts.base_models.as_deref().filter(|v| *v != "All"),
    );

    Url::parse_with_params(&format!("{}/models", api_v1_base(base_url)), pairs)
        .context("Failed to build Civitai models API URL")
}

pub fn build_api_images_search_url(base_url: &str, opts: &ApiImageSearchOptions) -> Result<Url> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    pairs.push(("limit".to_string(), opts.limit.unwrap_or(20).to_string()));
    push_optional_string(&mut pairs, "nsfw", opts.nsfw.as_deref());
    push_optional_string(&mut pairs, "sort", opts.sort.as_deref());
    push_optional_string(&mut pairs, "period", opts.period.as_deref());
    if let Some(value) = opts.model_version_id {
        pairs.push(("modelVersionId".to_string(), value.to_string()));
    }
    if let Some(value) = opts.tags {
        pairs.push(("tags".to_string(), value.to_string()));
    }

    Url::parse_with_params(&format!("{}/images", api_v1_base(base_url)), pairs)
        .context("Failed to build Civitai images API URL")
}

fn api_v1_base(base_url: &str) -> String {
    format!("{}/api/v1", base_url.trim_end_matches('/'))
}

fn push_optional_string(pairs: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        pairs.push((key.to_string(), value.to_string()));
    }
}
