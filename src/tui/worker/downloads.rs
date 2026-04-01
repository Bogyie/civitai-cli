use crate::tui::app::AppMessage;
use crate::tui::model::ParsedModelFile;
use civitai_cli::sdk::{DownloadControl, DownloadEvent};
use std::collections::HashMap;
use tokio::sync::mpsc;

pub(super) type DownloadControlMap = HashMap<u64, mpsc::Sender<DownloadControl>>;

pub(super) fn estimated_file_size_bytes(file: &ParsedModelFile) -> Option<u64> {
    file.size_kb.and_then(|size_kb| {
        if size_kb.is_finite() && size_kb > 0.0 {
            Some((size_kb * 1024.0).round() as u64)
        } else {
            None
        }
    })
}

pub(super) async fn forward_download_events(
    mut progress_rx: mpsc::Receiver<DownloadEvent>,
    tx_msg: mpsc::Sender<AppMessage>,
    model_id: u64,
    version_id: u64,
    model_name: String,
    filename: String,
) {
    while let Some(event) = progress_rx.recv().await {
        match event {
            DownloadEvent::Started {
                path, total_bytes, ..
            } => {
                let _ = tx_msg
                    .send(AppMessage::DownloadStarted(
                        model_id,
                        filename.clone(),
                        version_id,
                        model_name.clone(),
                        total_bytes.unwrap_or(0),
                        Some(path),
                    ))
                    .await;
            }
            DownloadEvent::Progress {
                downloaded_bytes,
                total_bytes,
                percent,
            } => {
                let _ = tx_msg
                    .send(AppMessage::DownloadProgress(
                        model_id,
                        filename.clone(),
                        percent.unwrap_or(0.0),
                        downloaded_bytes,
                        total_bytes.unwrap_or(0),
                    ))
                    .await;
            }
            DownloadEvent::Paused {
                downloaded_bytes,
                total_bytes,
            } => {
                let percent = total_bytes
                    .filter(|value| *value > 0)
                    .map(|value| (downloaded_bytes as f64 / value as f64) * 100.0)
                    .unwrap_or(0.0);
                let _ = tx_msg
                    .send(AppMessage::DownloadProgress(
                        model_id,
                        filename.clone(),
                        percent,
                        downloaded_bytes,
                        total_bytes.unwrap_or(0),
                    ))
                    .await;
                let _ = tx_msg.send(AppMessage::DownloadPaused(model_id)).await;
            }
            DownloadEvent::Resumed {
                downloaded_bytes,
                total_bytes,
            } => {
                let percent = total_bytes
                    .filter(|value| *value > 0)
                    .map(|value| (downloaded_bytes as f64 / value as f64) * 100.0)
                    .unwrap_or(0.0);
                let _ = tx_msg
                    .send(AppMessage::DownloadProgress(
                        model_id,
                        filename.clone(),
                        percent,
                        downloaded_bytes,
                        total_bytes.unwrap_or(0),
                    ))
                    .await;
                let _ = tx_msg.send(AppMessage::DownloadResumed(model_id)).await;
            }
            DownloadEvent::Completed {
                downloaded_bytes,
                total_bytes,
                ..
            }
            | DownloadEvent::Cancelled {
                downloaded_bytes,
                total_bytes,
                ..
            } => {
                let percent = total_bytes
                    .filter(|value| *value > 0)
                    .map(|value| (downloaded_bytes as f64 / value as f64) * 100.0)
                    .unwrap_or(0.0);
                let _ = tx_msg
                    .send(AppMessage::DownloadProgress(
                        model_id,
                        filename.clone(),
                        percent,
                        downloaded_bytes,
                        total_bytes.unwrap_or(0),
                    ))
                    .await;
            }
        }
    }
}
