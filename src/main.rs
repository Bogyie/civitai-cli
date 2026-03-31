mod api;
mod cli;
mod config;
mod download;
mod tui;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

use cli::{Cli, Commands};
use config::AppConfig;
use api::CivitaiClient;
use download::DownloadManager;

#[tokio::main]
async fn main() -> Result<()> {
    let mut app_config = AppConfig::load().unwrap_or_default();
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Config { api_key, comfyui_path }) => {
            if let Some(key) = api_key {
                app_config.api_key = Some(key.clone());
                println!("API key updated.");
            }
            if let Some(path) = comfyui_path {
                app_config.comfyui_path = Some(PathBuf::from(path));
                println!("ComfyUI path updated to {:?}", path);
            }
            app_config.save().context("Failed to save config")?;
        }
        Some(Commands::Download { id, hash }) => {
            let client = CivitaiClient::new(app_config.api_key.clone())?;
            let manager = DownloadManager::new(app_config)?;

            if let Some(model_id) = id {
                println!("Fetching model {} metadata...", model_id);
                let model = client.get_model(*model_id).await?;
                
                // For simplicity, download the latest or primary version
                if let Some(version) = model.model_versions.first() {
                    let path = manager.download_version(&model, version).await?;
                    println!("Successfully downloaded to {:?}", path);
                } else {
                    println!("No downloadable versions found for model {}", model_id);
                }
            } else if let Some(model_hash) = hash {
                println!("Fetching version by hash {}...", model_hash);
                let version = client.get_model_version_by_hash(model_hash).await?;
                let model = client.get_model(version.model_id).await?;
                let path = manager.download_version(&model, &version).await?;
                println!("Successfully downloaded to {:?}", path);
            } else {
                println!("Please provide an --id or --hash to download.");
            }
        }
        Some(Commands::Ui) | None => {
            // Run Interactive TUI
            tui::run_tui().await?;
        }
    }

    Ok(())
}
