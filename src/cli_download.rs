use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::AppConfig;
use civitai_cli::sdk::{
    ApiClient, ApiModel as Model, ApiModelVersion as ModelVersion, DownloadClient,
    DownloadDestination, DownloadKind, DownloadOptions, DownloadSpec, ModelDownloadAuth,
};

pub async fn run_download_command(
    config: &AppConfig,
    api_client: &ApiClient,
    model_id: Option<u64>,
    model_hash: Option<&str>,
) -> Result<()> {
    let download_client = build_download_client(config)?;

    if let Some(model_id) = model_id {
        println!("Fetching model {} metadata...", model_id);
        let model = api_client.get_model(model_id).await?;

        if let Some(version) = model.model_versions.first() {
            let path = download_model_version(config, &download_client, &model, version).await?;
            println!("Successfully downloaded to {:?}", path);
        } else {
            println!("No downloadable versions found for model {}", model_id);
        }
    } else if let Some(model_hash) = model_hash {
        println!("Fetching version by hash {}...", model_hash);
        let version = api_client.get_model_version_by_hash(model_hash).await?;
        if let Some(parent_model_id) = version.model_id {
            let model = api_client.get_model(parent_model_id).await?;
            let path = download_model_version(config, &download_client, &model, &version).await?;
            println!("Successfully downloaded to {:?}", path);
        } else {
            println!("Could not resolve the parent model ID for this version.");
        }
    } else {
        println!("Please provide an --id or --hash to download.");
    }

    Ok(())
}

fn build_download_client(config: &AppConfig) -> Result<DownloadClient> {
    let builder = if let Some(api_key) = config
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        civitai_cli::sdk::SdkClientBuilder::new().api_key(api_key.to_string())
    } else {
        civitai_cli::sdk::SdkClientBuilder::new()
    };

    builder.build_download()
}

async fn download_model_version(
    config: &AppConfig,
    download_client: &DownloadClient,
    model: &Model,
    version: &ModelVersion,
) -> Result<PathBuf> {
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .context("No files found across this model version")?;

    let target_dir = resolve_comfy_path(config, model)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let target_path = target_dir.join(generate_smart_filename(version, &file.name));

    let auth = config
        .api_key
        .clone()
        .filter(|value| !value.trim().is_empty())
        .map(ModelDownloadAuth::QueryToken);
    let spec = DownloadSpec::new(file.download_url.clone(), DownloadKind::Model).with_file_name(
        target_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    );
    let spec = match auth {
        Some(auth) => spec.with_auth(auth),
        None => spec,
    };

    let options = DownloadOptions {
        destination: DownloadDestination::File(target_path.clone()),
        overwrite: true,
        resume: true,
        create_parent_dirs: true,
        progress_step_percent: 1.0,
    };

    let result = download_client
        .download(&spec, &options, None, None)
        .await?;
    Ok(result.path)
}

fn resolve_comfy_path(config: &AppConfig, model: &Model) -> Option<PathBuf> {
    let base = config.comfyui_path.as_ref()?;

    let sub_dir = match model.r#type.as_str() {
        "Checkpoint" => "models/checkpoints",
        "LORA" => "models/loras",
        "TextualInversion" => "models/embeddings",
        "Controlnet" => "models/controlnet",
        "VAE" => "models/vae",
        _ => "models/uncategorized",
    };

    let target_dir = base.join(sub_dir);
    if !target_dir.exists() {
        let _ = fs::create_dir_all(&target_dir);
    }

    Some(target_dir)
}

fn generate_smart_filename(version: &ModelVersion, original_filename: &str) -> String {
    let base_model_tag = format!("[{}]", version.base_model.replace(' ', ""));

    let path = Path::new(original_filename);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().unwrap_or_default().to_string_lossy();

    if ext.is_empty() {
        format!("{}_{}", base_model_tag, stem)
    } else {
        format!("{}_{}.{}", base_model_tag, stem, ext)
    }
}
