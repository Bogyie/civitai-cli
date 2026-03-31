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
