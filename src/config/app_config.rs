use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::path::PathBuf;

fn default_model_search_cache_ttl_hours() -> u64 {
    3
}

fn default_image_search_cache_ttl_minutes() -> u64 {
    15
}

fn default_image_detail_cache_ttl_minutes() -> u64 {
    60
}

fn default_image_cache_ttl_minutes() -> u64 {
    0
}

fn default_media_quality() -> MediaQualityPreference {
    MediaQualityPreference::Medium
}

fn default_debug_logging() -> bool {
    false
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MediaQualityPreference {
    Low,
    Medium,
    High,
    Original,
}

impl MediaQualityPreference {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Original => "Original",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Original,
            Self::Original => Self::Low,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Low => Self::Original,
            Self::Medium => Self::Low,
            Self::High => Self::Medium,
            Self::Original => Self::High,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub api_key: Option<String>,
    pub comfyui_path: Option<PathBuf>,
    pub bookmark_file_path: Option<PathBuf>,
    pub image_bookmark_file_path: Option<PathBuf>,
    pub model_cover_cache_path: Option<PathBuf>,
    pub model_search_cache_path: Option<PathBuf>,
    pub image_cache_path: Option<PathBuf>,
    pub download_history_file_path: Option<PathBuf>,
    pub interrupted_download_file_path: Option<PathBuf>,
    #[serde(default = "default_model_search_cache_ttl_hours")]
    pub model_search_cache_ttl_hours: u64,
    #[serde(default = "default_image_search_cache_ttl_minutes")]
    pub image_search_cache_ttl_minutes: u64,
    #[serde(default = "default_image_detail_cache_ttl_minutes")]
    pub image_detail_cache_ttl_minutes: u64,
    #[serde(default = "default_image_cache_ttl_minutes")]
    pub image_cache_ttl_minutes: u64,
    #[serde(default = "default_media_quality")]
    pub media_quality: MediaQualityPreference,
    #[serde(default = "default_debug_logging")]
    pub debug_logging: bool,
}

impl AppConfig {
    /// Loads the configuration from the standardized OS config path.
    pub fn load() -> Result<Self> {
        let config_file = Self::config_path().context("Unable to determine config directory")?;

        if !config_file.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_file)
            .with_context(|| format!("Failed to read config file at {:?}", config_file))?;

        let mut config: AppConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file at {:?}", config_file))?;

        if let Some(path) = config.comfyui_path.clone() {
            config.comfyui_path = Some(Self::normalize_comfyui_path(&path)?);
        }

        Ok(config)
    }

    /// Saves the configuration to disk.
    pub fn save(&self) -> Result<()> {
        let config_file = Self::config_path().context("Unable to determine config directory")?;
        let mut normalized = self.clone();
        if let Some(path) = normalized.comfyui_path.clone() {
            normalized.comfyui_path = Some(Self::normalize_comfyui_path(&path)?);
        }

        if let Some(parent) = config_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory at {:?}", parent))?;
        }

        let content =
            toml::to_string_pretty(&normalized).context("Failed to serialize config")?;
        fs::write(&config_file, content)
            .with_context(|| format!("Failed to write config file at {:?}", config_file))?;

        Ok(())
    }

    pub fn set_comfyui_path(&mut self, path: Option<impl AsRef<Path>>) -> Result<()> {
        self.comfyui_path = match path {
            Some(path) => Some(Self::normalize_comfyui_path(path.as_ref())?),
            None => None,
        };
        Ok(())
    }

    pub fn normalize_comfyui_path(path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        if !path.is_absolute() {
            anyhow::bail!("ComfyUI path must be an absolute path");
        }
        if !path.exists() {
            anyhow::bail!("ComfyUI path does not exist: {}", path.display());
        }
        if !path.is_dir() {
            anyhow::bail!("ComfyUI path is not a directory: {}", path.display());
        }

        path.canonicalize()
            .with_context(|| format!("Failed to resolve ComfyUI path {}", path.display()))
    }

    /// Determines the platform-specific configuration path.
    pub fn config_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "civitai", "civitai-cli")
            .map(|proj_dirs| proj_dirs.config_dir().join("config.toml"))
    }

    pub fn config_dir() -> Option<PathBuf> {
        ProjectDirs::from("com", "civitai", "civitai-cli")
            .map(|proj_dirs| proj_dirs.config_dir().to_path_buf())
    }

    pub fn bookmark_path() -> Option<PathBuf> {
        Self::config_dir().map(|config_dir| config_dir.join("bookmarks.json"))
    }

    pub fn image_bookmark_path() -> Option<PathBuf> {
        Self::config_dir().map(|config_dir| config_dir.join("image_bookmarks.json"))
    }

    pub fn image_tag_catalog_path(&self) -> Option<PathBuf> {
        Self::config_dir().map(|config_dir| config_dir.join("image_tags.json"))
    }

    pub fn model_cover_cache_path(&self) -> Option<PathBuf> {
        self.model_cover_cache_path
            .clone()
            .or_else(|| Self::config_dir().map(|config_dir| config_dir.join("model_cover_cache")))
    }

    pub fn search_cache_path(&self) -> Option<PathBuf> {
        self.model_search_cache_path
            .clone()
            .or_else(|| Self::config_dir().map(|config_dir| config_dir.join("model_search_cache")))
    }

    pub fn image_cache_path(&self) -> Option<PathBuf> {
        self.image_cache_path
            .clone()
            .or_else(|| Self::config_dir().map(|config_dir| config_dir.join("image_cache")))
    }

    pub fn download_history_path(&self) -> Option<PathBuf> {
        self.download_history_file_path.clone().or_else(|| {
            Self::config_dir().map(|config_dir| config_dir.join("download_history.json"))
        })
    }

    pub fn interrupted_download_path(&self) -> Option<PathBuf> {
        self.interrupted_download_file_path.clone().or_else(|| {
            Self::config_dir().map(|config_dir| config_dir.join("interrupted_downloads.json"))
        })
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            comfyui_path: None,
            bookmark_file_path: None,
            image_bookmark_file_path: None,
            model_cover_cache_path: None,
            model_search_cache_path: None,
            image_cache_path: None,
            download_history_file_path: None,
            interrupted_download_file_path: None,
            model_search_cache_ttl_hours: default_model_search_cache_ttl_hours(),
            image_search_cache_ttl_minutes: default_image_search_cache_ttl_minutes(),
            image_detail_cache_ttl_minutes: default_image_detail_cache_ttl_minutes(),
            image_cache_ttl_minutes: default_image_cache_ttl_minutes(),
            media_quality: default_media_quality(),
            debug_logging: default_debug_logging(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("civitai-cli-config-{unique}"));
        fs::create_dir_all(&dir).expect("create dir");
        dir
    }

    #[test]
    fn rejects_relative_comfyui_path() {
        let err = AppConfig::normalize_comfyui_path("relative/comfy").expect_err("relative path");
        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn rejects_missing_comfyui_path() {
        let path = temp_dir().join("missing");
        let err = AppConfig::normalize_comfyui_path(&path).expect_err("missing path");
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn normalizes_existing_comfyui_path_to_absolute() {
        let dir = temp_dir();
        let normalized = AppConfig::normalize_comfyui_path(&dir).expect("normalize path");
        assert!(normalized.is_absolute());
        assert_eq!(normalized, dir.canonicalize().expect("canonicalize"));
    }
}
