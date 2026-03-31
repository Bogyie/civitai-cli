use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::api::{Model, ModelVersion};
use crate::config::AppConfig;

pub struct DownloadManager {
    client: Client,
    config: AppConfig,
}

#[derive(Clone, Copy, Debug)]
pub enum DownloadControl {
    Pause,
    Resume,
    Cancel,
}

impl DownloadManager {
    pub fn new(config: AppConfig) -> Result<Self> {
        let client = Client::builder()
            .user_agent("civitai-cli/0.1")
            .build()?;
        Ok(Self { client, config })
    }

    pub fn resolve_comfy_path(&self, model: &Model) -> Option<PathBuf> {
        let base = self.config.comfyui_path.as_ref()?;
        
        // Follow standard ComfyUI directory structure
        let sub_dir = match model.r#type.as_str() {
            "Checkpoint" => "models/checkpoints",
            "LORA" => "models/loras",
            "TextualInversion" => "models/embeddings",
            "Controlnet" => "models/controlnet",
            "VAE" => "models/vae",
            _ => "models/uncategorized",
        };

        let target_dir = base.join(sub_dir);
        // Create it if it doesn't exist
        if !target_dir.exists() {
            let _ = fs::create_dir_all(&target_dir);
        }

        Some(target_dir)
    }

    pub fn generate_smart_filename(&self, _model: &Model, version: &ModelVersion, original_filename: &str) -> String {
        // e.g. [SDXL]_my_lora.safetensors
        let base_model_tag = format!("[{}]", version.base_model.replace(" ", ""));
        
        let path = Path::new(original_filename);
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let ext = path.extension().unwrap_or_default().to_string_lossy();

        if ext.is_empty() {
            format!("{}_{}", base_model_tag, stem)
        } else {
            format!("{}_{}.{}", base_model_tag, stem, ext)
        }
    }

    pub async fn download_version(
        &self, 
        model: &Model, 
        version: &ModelVersion,
        tx: Option<tokio::sync::mpsc::Sender<crate::tui::app::AppMessage>>,
    ) -> Result<PathBuf> {
        let file = version.files.iter().find(|f| f.primary).or_else(|| version.files.first())
            .context("No files found across this model version")?;

        let url = &file.download_url;
        let original_filename = &file.name;

        let smart_filename = self.generate_smart_filename(model, version, original_filename);
        
        let dest_dir = self.resolve_comfy_path(model).unwrap_or_else(|| {
            // fallback to current directory
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        });

        let target_path = dest_dir.join(&smart_filename);

        let mut req = self.client.get(url);
        if let Some(token) = &self.config.api_key {
            req = req.bearer_auth(token);
        }

        let res = req.send().await?.error_for_status()?;
        let total_size = res.content_length().unwrap_or(0) as f64;

        // Stream to file
        let mut file_obj = tokio::fs::File::create(&target_path).await?;
        let mut stream = res.bytes_stream();
        let mut downloaded: f64 = 0.0;
        let mut last_percent = -1.0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file_obj.write_all(&chunk).await?;
            downloaded += chunk.len() as f64;

            if let Some(ref chan) = tx {
                if total_size > 0.0 {
                    let percent = (downloaded / total_size) * 100.0;
                    // Only send update if progressed more than 1% to prevent channel flooding
                    if percent - last_percent >= 1.0 {
                        last_percent = percent;
                        let _ = chan.try_send(crate::tui::app::AppMessage::DownloadProgress(
                            model.id,
                            smart_filename.clone(),
                            percent,
                            downloaded.round() as u64,
                            total_size.round() as u64,
                        ));
                    }
                }
            }
        }

        if let Some(ref chan) = tx {
            let _ = chan.try_send(crate::tui::app::AppMessage::DownloadProgress(
                model.id,
                smart_filename.clone(),
                100.0,
                downloaded.round() as u64,
                total_size.round() as u64,
            ));
        }

        Ok(target_path)
    }

    pub async fn download_version_with_control(
        &self,
        model: &Model,
        version: &ModelVersion,
        tx: Option<tokio::sync::mpsc::Sender<crate::tui::app::AppMessage>>,
        mut control: Option<mpsc::Receiver<DownloadControl>>,
    ) -> Result<PathBuf> {
        let file = version.files.iter().find(|f| f.primary).or_else(|| version.files.first())
            .context("No files found across this model version")?;

        let url = &file.download_url;
        let original_filename = &file.name;
        let smart_filename = self.generate_smart_filename(model, version, original_filename);
        let dest_dir = self.resolve_comfy_path(model).unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        });
        let target_path = dest_dir.join(&smart_filename);

        let mut req = self.client.get(url);
        if let Some(token) = &self.config.api_key {
            req = req.bearer_auth(token);
        }

        let res = req.send().await?.error_for_status()?;
        let total_size = res.content_length().unwrap_or(0) as f64;
        let mut stream = res.bytes_stream();
        let mut file_obj = tokio::fs::File::create(&target_path).await?;

        let mut downloaded: f64 = 0.0;
        let mut last_percent = -1.0f64;
        let mut paused = false;
        let send_progress = |percent: f64, downloaded: f64, total_size: f64| {
            if let Some(ref chan) = tx {
                if percent > 100.0 {
                    return;
                }
                let _ = chan.try_send(crate::tui::app::AppMessage::DownloadProgress(
                    model.id,
                    smart_filename.clone(),
                    percent,
                    downloaded.round() as u64,
                    total_size.round() as u64,
                ));
            }
        };

        loop {
            if paused {
                if let Some(control_rx) = control.as_mut() {
                    loop {
                        match control_rx.recv().await {
                            Some(DownloadControl::Pause) => continue,
                            Some(DownloadControl::Resume) => {
                                paused = false;
                                break;
                            }
                            Some(DownloadControl::Cancel) => return Err(anyhow::anyhow!("download cancelled")),
                            None => return Err(anyhow::anyhow!("control channel closed")),
                        }
                    }
                } else {
                    paused = false;
                }
            }

            match control.as_mut() {
                None => {
                    match stream.next().await {
                        Some(chunk_result) => {
                            let chunk = chunk_result?;
                            file_obj.write_all(&chunk).await?;
                            downloaded += chunk.len() as f64;
                            if total_size > 0.0 {
                                let percent = (downloaded / total_size) * 100.0;
                                if percent - last_percent >= 1.0 {
                                    last_percent = percent;
                                    send_progress(percent, downloaded, total_size);
                                }
                            }
                        }
                        None => break,
                    }
                }
                Some(control_rx) => {
                    tokio::select! {
                        chunk = stream.next() => {
                            if let Some(chunk_result) = chunk {
                                let chunk = chunk_result?;
                                file_obj.write_all(&chunk).await?;
                                downloaded += chunk.len() as f64;
                                if total_size > 0.0 {
                                    let percent = (downloaded / total_size) * 100.0;
                                    if percent - last_percent >= 1.0 {
                                        last_percent = percent;
                                        send_progress(percent, downloaded, total_size);
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                        Some(cmd) = control_rx.recv() => {
                            match cmd {
                                DownloadControl::Pause => paused = true,
                                DownloadControl::Resume => {}
                                DownloadControl::Cancel => return Err(anyhow::anyhow!("download cancelled")),
                            }
                        }
                        else => {
                            return Err(anyhow::anyhow!("control channel closed"));
                        }
                    }
                }
            }
        }

        if let Some(ref chan) = tx {
            let _ = chan.try_send(crate::tui::app::AppMessage::DownloadProgress(
                model.id,
                smart_filename.clone(),
                100.0,
                downloaded.round() as u64,
                total_size.round() as u64,
            ));
        }

        Ok(target_path)
    }
}
