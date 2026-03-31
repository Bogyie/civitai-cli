use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub api_key: Option<String>,
    pub comfyui_path: Option<PathBuf>,
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
}
