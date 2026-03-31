use anyhow::Result;
use image::{imageops::FilterType, DynamicImage};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use reqwest::Client;
use tokio::sync::mpsc;

use crate::api::CivitaiClient;
use crate::config::AppConfig;
use crate::download::DownloadManager;
use crate::tui::app::{AppMessage, WorkerCommand};

pub async fn spawn_worker(
    config: AppConfig,
) -> (mpsc::Sender<WorkerCommand>, mpsc::Receiver<AppMessage>) {
    let (tx_cmd, mut rx_cmd) = mpsc::channel::<WorkerCommand>(32);
    let (tx_msg, rx_msg) = mpsc::channel::<AppMessage>(32);

    let civitai = CivitaiClient::new(config.api_key.clone()).unwrap();
    let downloader_config = config.clone();
    let req_client = Client::builder().user_agent("civitai-cli").build().unwrap();

    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::from_fontsize((8, 16)));

    // Background orchestrator loop
    tokio::spawn(async move {
        // This vector holds our image state inside the worker to spawn tasks
        let mut image_cache = Vec::new();

        while let Some(cmd) = rx_cmd.recv().await {
            match cmd {
                WorkerCommand::FetchImages => {
                    let _ = tx_msg.try_send(AppMessage::StatusUpdate("Fetching feed...".into()));
                    match civitai.get_images(50, 1).await {
                        Ok(res) => {
                            image_cache = res.items.clone();
                            let _ = tx_msg.try_send(AppMessage::ImagesLoaded(res.items.clone()));

                            // Spawn downscaling tasks for all fetched images
                            for item in res.items {
                                let tx_msg_clone = tx_msg.clone();
                                let req_client_clone = req_client.clone();
                                let mut picker_clone = picker.clone();

                                tokio::spawn(async move {
                                    if let Ok(bytes) = fetch_image_bytes(&req_client_clone, &item.url).await {
                                        if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                                            // Downscale huge 4K images instantly
                                            let resized = dyn_img.resize(600, 600, FilterType::Triangle);
                                            let protocol = picker_clone.new_resize_protocol(resized);
                                            let _ = tx_msg_clone.try_send(AppMessage::ImageDecoded(item.id, protocol));
                                        }
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            let _ = tx_msg.try_send(AppMessage::StatusUpdate(format!("Error fetching images: {}", e)));
                        }
                    }
                }
                WorkerCommand::SearchModels(opts) => {
                    let tx_msg_clone = tx_msg.clone();
                    let civitai_clone = CivitaiClient::new(downloader_config.api_key.clone()).unwrap();
                    let req_client_clone = req_client.clone();
                    let mut picker_clone = picker.clone();

                    tokio::spawn(async move {
                        let _ = tx_msg_clone.try_send(AppMessage::StatusUpdate(format!("Fetching models matching '{}'...", opts.query)));
                        match civitai_clone.search_models(opts).await {
                            Ok(res) => {
                                let _ = tx_msg_clone.try_send(AppMessage::ModelsSearched(res.items.clone()));
                                
                                for model in res.items {
                                    if let Some(version) = model.model_versions.first() {
                                        if let Some(image) = version.images.first() {
                                            if let Ok(bytes) = fetch_image_bytes(&req_client_clone, &image.url).await {
                                                if let Ok(img) = image::load_from_memory(&bytes) {
                                                    let resized = img.resize(600, 600, image::imageops::FilterType::Triangle);
                                                    let protocol = picker_clone.new_resize_protocol(resized.into());
                                                    let _ = tx_msg_clone.try_send(AppMessage::ModelCoverDecoded(model.id, protocol));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx_msg_clone.try_send(AppMessage::StatusUpdate(format!("Search failed: {}", e)));
                            }
                        }
                    });
                }
                WorkerCommand::DownloadModelForImage(image_id) => {
                    // Start downloading logic in background... (mocked for now, need model lookup)
                    let tx_msg_clone = tx_msg.clone();
                    let downloader_config_clone = downloader_config.clone();
                    let civitai_clone = CivitaiClient::new(downloader_config.api_key.clone()).unwrap();
                    
                    tokio::spawn(async move {
                        let _ = tx_msg_clone.try_send(AppMessage::StatusUpdate(format!("Inspecting image {} for linked model...", image_id)));
                        
                        // NOTE: Civitai's /api/v1/images returns image metadata. However, strictly identifying the origin model version
                        // purely from the `get_images` endpoint is notoriously tricky because `meta.model` usually only gives the string name (e.g., "SDXL 1.0").
                        // There's no direct "model_version_id" exposed in the standard public Images response item! Just the 'hash' embedded randomly for different resources.
                        // For MVP, we pretend logic. This would usually query image details API logic.
                        
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        let _ = tx_msg_clone.try_send(AppMessage::StatusUpdate("Download finished (Placeholder)!".into()));
                    });
                }
                WorkerCommand::DownloadModel(model_id, version_id) => {
                    let tx_msg_clone = tx_msg.clone();
                    let cv_clone = CivitaiClient::new(downloader_config.api_key.clone()).unwrap();
                    let dl_clone = crate::download::manager::DownloadManager::new(downloader_config.clone().into()).unwrap();
                    
                    tokio::spawn(async move {
                        let _ = tx_msg_clone.try_send(AppMessage::StatusUpdate(format!("Fetching Model {} metadata for download...", model_id)));
                        if let Ok(model) = cv_clone.get_model(model_id).await {
                            if let Some(version) = model.model_versions.iter().find(|v| v.id == version_id) {
                                let _ = tx_msg_clone.try_send(AppMessage::StatusUpdate(format!("Starting download stream for {}", model_id)));
                                let res = dl_clone.download_version(&model, version, Some(tx_msg_clone.clone())).await;
                                if let Err(e) = res {
                                    let _ = tx_msg_clone.try_send(AppMessage::StatusUpdate(format!("Download {} failed: {:?}", model_id, e)));
                                }
                            }
                        } else {
                            let _ = tx_msg_clone.try_send(AppMessage::StatusUpdate(format!("Failed to retrieve Model {} metadata", model_id)));
                        }
                    });
                }
                WorkerCommand::Quit => break,
            }
        }
    });

    (tx_cmd, rx_msg)
}

async fn fetch_image_bytes(client: &Client, url: &str) -> Result<bytes::Bytes> {
    let res = client.get(url).send().await?.error_for_status()?;
    Ok(res.bytes().await?)
}
