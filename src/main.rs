mod cli;
mod cli_download;
mod config;
mod tui;

use anyhow::{Context, Result};
use civitai_cli::sdk::{ApiClient, SdkClientBuilder};
use clap::Parser;

use cli::{Cli, Commands};
use cli_download::run_download_command;
use config::AppConfig;

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
                app_config
                    .set_comfyui_path(Some(path))
                    .context("Invalid ComfyUI path")?;
                println!(
                    "ComfyUI path updated to {}",
                    app_config
                        .comfyui_path
                        .as_ref()
                        .map(|value| value.display().to_string())
                        .unwrap_or_default()
                );
            }
            app_config.save().context("Failed to save config")?;
        }
        Some(Commands::Download { id, hash }) => {
            let client = build_api_client(app_config.api_key.as_deref())?;
            run_download_command(&app_config, &client, *id, hash.as_deref()).await?;
        }
        Some(Commands::Ui) | None => {
            // Run Interactive TUI
            tui::run_tui(app_config.clone()).await?;
        }
    }

    Ok(())
}
