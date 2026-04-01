mod cli;
mod config;
mod download;
mod tui;

use anyhow::{Context, Result};
use civitai_cli::sdk::{ApiClient, SdkClientBuilder};
use clap::Parser;
use std::path::PathBuf;

use cli::{Cli, Commands};
use config::AppConfig;
use download::DownloadManager;

fn build_api_client(api_key: Option<&str>) -> Result<ApiClient> {
    let builder = if let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) {
        SdkClientBuilder::new().api_key(api_key.to_string())
    } else {
        SdkClientBuilder::new()
    };

    builder.build_api()
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut app_config = AppConfig::load().unwrap_or_default();
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Config {
            api_key,
            comfyui_path,
        }) => {
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
            let client = build_api_client(app_config.api_key.as_deref())?;
            let manager = DownloadManager::new(app_config)?;

            if let Some(model_id) = id {
                println!("Fetching model {} metadata...", model_id);
                let model = client.get_model(*model_id).await?;

                // For simplicity, download the latest or primary version
                if let Some(version) = model.model_versions.first() {
                    let path = manager.download_version(&model, version, None).await?;
                    println!("Successfully downloaded to {:?}", path);
                } else {
                    println!("No downloadable versions found for model {}", model_id);
                }
            } else if let Some(model_hash) = hash {
                println!("Fetching version by hash {}...", model_hash);
                let version = client.get_model_version_by_hash(model_hash).await?;
                if let Some(mid) = version.model_id {
                    let model = client.get_model(mid).await?;
                    let path = manager.download_version(&model, &version, None).await?;
                    println!("Successfully downloaded to {:?}", path);
                } else {
                    println!("Could not resolve the parent model ID for this version.");
                }
            } else {
                println!("Please provide an --id or --hash to download.");
            }
        }
        Some(Commands::Ui) | None => {
            // Run Interactive TUI
            tui::run_tui(app_config.clone()).await?;
        }
    }

    Ok(())
}
