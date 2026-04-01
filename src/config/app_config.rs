use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
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

        let config: AppConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file at {:?}", config_file))?;

        Ok(config)
    }

    /// Saves the configuration to disk.
    pub fn save(&self) -> Result<()> {
        let config_file = Self::config_path().context("Unable to determine config directory")?;

        if let Some(parent) = config_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory at {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&config_file, content)
            .with_context(|| format!("Failed to write config file at {:?}", config_file))?;

        Ok(())
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
        }
    }
}
