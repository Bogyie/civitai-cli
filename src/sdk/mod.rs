mod api;
mod client;
mod constants;
mod download;
mod image_search;
mod model_search;
mod shared;

#[allow(unused_imports)]
pub use api::{
    build_api_images_search_url, build_api_model_url, build_api_model_version_by_hash_url,
    build_api_models_search_url, ApiImageItem, ApiImageResponse, ApiImageSearchOptions,
    ApiImageStats, ApiModel, ApiModelCreator, ApiModelFile, ApiModelImage,
    ApiModelSearchOptions, ApiModelStats, ApiModelTag, ApiModelVersion, ApiNsfwValue,
    ApiPaginatedResponse, ApiPaginationMetadata, ApiVersionStats, FileMetadata,
};
#[allow(unused_imports)]
pub use client::{
    ApiClient, DownloadClient, SdkClientBuilder, SdkClients, SearchSdkConfig, WebSearchClient,
};
#[allow(unused_imports)]
pub use constants::{
    CIVITAI_IMAGE_SEARCH_CLIENT_KEY, CIVITAI_IMAGE_SEARCH_MEILI_URL,
    CIVITAI_MEDIA_DELIVERY_NAMESPACE, CIVITAI_MEDIA_DELIVERY_URL,
    CIVITAI_MODEL_DOWNLOAD_API_URL, CIVITAI_WEB_URL, IMAGES_SEARCH_INDEX, MODELS_SEARCH_INDEX,
};
#[allow(unused_imports)]
pub use download::{
    DownloadControl, DownloadDestination, DownloadEvent, DownloadKind, DownloadOptions,
    DownloadResult, DownloadSpec,
};
#[allow(unused_imports)]
pub use image_search::{
    ImageHitUser, ImageSearchSortBy, ImageSearchState, SearchImageHit, SearchImageResponse,
};
#[allow(unused_imports)]
pub use model_search::{
    build_model_download_url, build_model_download_url_with_base,
    build_model_download_url_with_token, build_model_download_url_with_token_and_base,
    ModelDownloadAuth, ModelSearchSortBy, ModelSearchState, SearchModelHit,
    SearchModelResponse,
};
