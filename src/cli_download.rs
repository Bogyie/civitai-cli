use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use crate::config::AppConfig;
use crate::tui::model::{
    build_model_download_file_name, normalize_base_model_component, normalize_model_type_folder,
};
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

    let target_dir = resolve_model_target_dir(config, model, version);
    let target_path = target_dir.join(build_model_download_file_name(
        Some(model.name.as_str()),
        Some(version.name.as_str()),
        &file.name,
        &format!("{}-{}", model.name, version.id),
    ));

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

fn resolve_model_target_dir(config: &AppConfig, model: &Model, version: &ModelVersion) -> PathBuf {
    let target_dir = match config.comfyui_path.as_ref() {
        Some(base) => base
            .join("models")
            .join(normalize_model_type_folder(Some(model.r#type.as_str())))
            .join(normalize_base_model_component(&version.base_model)),
        None => std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("downloads")
            .join("models")
            .join(normalize_model_type_folder(Some(model.r#type.as_str())))
            .join(normalize_base_model_component(&version.base_model)),
    };
    if !target_dir.exists() {
        let _ = fs::create_dir_all(&target_dir);
    }
    target_dir
}
