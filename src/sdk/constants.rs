pub const IMAGES_SEARCH_INDEX: &str = "images_v6";
pub const MODELS_SEARCH_INDEX: &str = "models_v9";
pub const CIVITAI_IMAGE_SEARCH_MEILI_URL: &str = "https://search-new.civitai.com";
pub const CIVITAI_IMAGE_SEARCH_CLIENT_KEY: &str =
    "8c46eb2508e21db1e9828a97968d91ab1ca1caa5f70a00e88a2ba1e286603b61";
pub const CIVITAI_WEB_URL: &str = "https://civitai.com";
pub const CIVITAI_MEDIA_DELIVERY_URL: &str = "https://image.civitai.com";
pub const CIVITAI_MEDIA_DELIVERY_NAMESPACE: &str = "xG1nkqKTMzGDvpLrqFT7WA";
pub const CIVITAI_MODEL_DOWNLOAD_API_URL: &str = "https://civitai.com/api/download/models";

pub const DEFAULT_IMAGE_SORTS: [&str; 6] = [
    IMAGES_SEARCH_INDEX,
    "images_v6:stats.reactionCountAllTime:desc",
    "images_v6:stats.commentCountAllTime:desc",
    "images_v6:stats.collectedCountAllTime:desc",
    "images_v6:stats.tippedAmountCountAllTime:desc",
    "images_v6:createdAt:desc",
];

pub const DEFAULT_MODEL_SORTS: [&str; 8] = [
    MODELS_SEARCH_INDEX,
    "models_v9:metrics.thumbsUpCount:desc",
    "models_v9:metrics.downloadCount:desc",
    "models_v9:metrics.favoriteCount:desc",
    "models_v9:metrics.commentCount:desc",
    "models_v9:metrics.collectedCount:desc",
    "models_v9:metrics.tippedAmountCount:desc",
    "models_v9:createdAt:desc",
];
