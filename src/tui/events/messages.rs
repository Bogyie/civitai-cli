use crate::tui::app::{
    App, AppMessage, DownloadHistoryStatus, DownloadState, NewDownloadHistoryEntry,
};
use crate::tui::runtime::debug_fetch_log;

use super::actions::{
    request_image_feed_if_needed, send_cover_prefetch, send_cover_priority,
    send_image_model_detail_cover_prefetch, send_image_model_detail_cover_priority,
};

pub(super) fn handle_app_message(app: &mut App, msg: AppMessage) {
    match msg {
        AppMessage::ImagesLoaded(new_images, append, next_page) => {
            app.merge_image_tag_catalog_from_hits(&new_images);
            let loaded_count = new_images.len();
            if append {
                let before = app.images.len();
                app.append_image_feed_results(new_images, next_page);
                if app.active_tab == crate::tui::app::MainTab::Images {
                    app.set_status(format!(
                        "Loaded {} more images (total {})",
                        app.images.len().saturating_sub(before),
                        app.images.len()
                    ));
                }
            } else {
                app.set_image_feed_results(new_images, next_page);
                if app.active_tab == crate::tui::app::MainTab::Images {
                    app.set_status(format!("Loaded {} images", app.images.len()));
                }
            }
            if app.status.is_empty() && app.active_tab == crate::tui::app::MainTab::Images {
                app.set_status(format!("Loaded {} images", app.images.len()));
            }
            if loaded_count == 0 {
                if let Some(next_page) = app.next_image_feed_page() {
                    request_image_feed_if_needed(app, Some(next_page));
                }
            } else if app.can_request_more_images(5)
                && let Some(next_page) = app.next_image_feed_page()
            {
                request_image_feed_if_needed(app, Some(next_page));
            }
        }
        AppMessage::ImageDecoded(id, protocol, bytes, request_key) => {
            app.image_cache.insert(id, protocol);
            app.image_bytes_cache.insert(id, bytes);
            if !request_key.is_empty() {
                app.image_request_keys.insert(id, request_key);
            }
        }
        AppMessage::ImageDetailEnriched(image) => {
            app.merge_image_tag_catalog_from_hits(std::slice::from_ref(&image));
            app.update_image_detail(&image);
        }
        AppMessage::ModelCoverDecoded(version_id, protocol, bytes, request_key) => {
            app.model_version_image_cache
                .insert(version_id, vec![protocol]);
            app.model_version_image_bytes_cache
                .insert(version_id, vec![bytes]);
            if !request_key.is_empty() {
                app.model_version_image_request_keys
                    .insert(version_id, request_key);
            }
            app.model_version_image_failed.remove(&version_id);
        }
        AppMessage::ModelCoversDecoded(version_id, protocols, request_key) => {
            let (protocols, bytes): (Vec<_>, Vec<_>) = protocols.into_iter().unzip();
            app.model_version_image_cache.insert(version_id, protocols);
            app.model_version_image_bytes_cache
                .insert(version_id, bytes);
            if !request_key.is_empty() {
                app.model_version_image_request_keys
                    .insert(version_id, request_key);
            }
            app.model_version_image_failed.remove(&version_id);
        }
        AppMessage::ModelCoverLoadFailed(version_id) => {
            app.model_version_image_failed.insert(version_id);
        }
        AppMessage::ModelsSearchedChunk(results, append, has_more, next_page) => {
            let before_count = app.models.len();
            debug_fetch_log(
                &app.config,
                &format!(
                    "UI: received ModelsSearchedChunk append={} incoming={} before={} has_more={} next_page={}",
                    append,
                    results.len(),
                    before_count,
                    has_more,
                    next_page.is_some(),
                ),
            );
            if append {
                let appended_len = results.len();
                app.append_models_results(results, has_more, next_page);
                debug_fetch_log(
                    &app.config,
                    &format!(
                        "UI: append done before={} after={} has_more={}",
                        before_count,
                        app.models.len(),
                        app.model_search_has_more
                    ),
                );
                app.set_status(format!(
                    "Loaded {} more models (total {})",
                    appended_len,
                    app.models.len()
                ));
            } else {
                app.set_models_results(results, has_more, next_page);
                debug_fetch_log(
                    &app.config,
                    &format!(
                        "UI: set models done count={} has_more={}",
                        app.models.len(),
                        app.model_search_has_more
                    ),
                );
                send_cover_priority(app);
                send_cover_prefetch(app);
                app.set_status(format!("Found {} models", app.models.len()));
            }
        }
        AppMessage::ModelDetailLoaded(model, version_id) => {
            app.open_image_model_detail_modal(*model, version_id);
            send_image_model_detail_cover_priority(app);
            send_image_model_detail_cover_prefetch(app);
            if let Some(model) = app.image_model_detail_model.as_ref() {
                app.set_status(format!(
                    "Loaded model details: {}",
                    crate::tui::model::model_name(model)
                ));
            } else {
                app.set_status("Loaded model details");
            }
        }
        AppMessage::ModelSidebarDetailLoaded(model) => {
            app.apply_sidebar_model_detail(*model);
            if let Some(model) = app.selected_model_in_active_view() {
                app.set_status(format!(
                    "Loaded model details: {}",
                    crate::tui::model::model_name(model)
                ));
            } else {
                app.set_status("Loaded model details");
            }
        }
        AppMessage::StatusUpdate(status) => {
            let is_image_fetch_error = status.summary.contains("Error fetching images");
            app.apply_status(status);
            if is_image_fetch_error {
                app.image_feed_loading = false;
            }
        }
        AppMessage::DownloadProgress(download_key, progress, downloaded_bytes, total_bytes) => {
            if let Some(existing) = app.active_downloads.get_mut(&download_key) {
                existing.progress = progress;
                existing.downloaded_bytes = downloaded_bytes;
                existing.total_bytes = total_bytes;
            }
        }
        AppMessage::DownloadStarted(download_key, model_name, total_bytes, file_path) => {
            if !app.active_download_order.contains(&download_key) {
                app.active_download_order.push(download_key.clone());
            }

            app.active_downloads.insert(
                download_key.clone(),
                crate::tui::app::DownloadTracker {
                    filename: download_key.filename.clone(),
                    progress: 0.0,
                    downloaded_bytes: 0,
                    total_bytes,
                    file_path,
                    model_name,
                    version_id: download_key.version_id,
                    state: DownloadState::Running,
                },
            );
            app.set_status(format!(
                "Download started for model {} ({})",
                download_key.model_id, download_key.version_id
            ));
        }
        AppMessage::DownloadPaused(download_key) => {
            if let Some(tracker) = app.active_downloads.get_mut(&download_key) {
                tracker.state = DownloadState::Paused;
                let filename = tracker.filename.clone();
                let _ = tracker;
                app.set_status(format!("Download paused: {}", filename));
            }
        }
        AppMessage::DownloadResumed(download_key) => {
            if let Some(tracker) = app.active_downloads.get_mut(&download_key) {
                tracker.state = DownloadState::Running;
                let filename = tracker.filename.clone();
                let _ = tracker;
                app.set_status(format!("Download resumed: {}", filename));
            }
        }
        AppMessage::DownloadCompleted(download_key) => {
            if let Some(tracker) = app.active_downloads.remove(&download_key) {
                app.push_download_history(NewDownloadHistoryEntry {
                    model_id: download_key.model_id,
                    version_id: tracker.version_id,
                    filename: tracker.filename,
                    model_name: tracker.model_name,
                    file_path: tracker.file_path,
                    downloaded_bytes: tracker.downloaded_bytes,
                    total_bytes: tracker.total_bytes,
                    status: DownloadHistoryStatus::Completed,
                    progress: tracker.progress,
                });
            }
            app.active_download_order.retain(|key| *key != download_key);
            app.clamp_selected_download_index();
            app.clamp_selected_history_index();
            app.set_status(format!("Download complete: {}", download_key.filename));
        }
        AppMessage::DownloadFailed(download_key, reason) => {
            if let Some(tracker) = app.active_downloads.remove(&download_key) {
                app.push_download_history(NewDownloadHistoryEntry {
                    model_id: download_key.model_id,
                    version_id: tracker.version_id,
                    filename: tracker.filename,
                    model_name: tracker.model_name,
                    file_path: tracker.file_path,
                    downloaded_bytes: tracker.downloaded_bytes,
                    total_bytes: tracker.total_bytes,
                    status: DownloadHistoryStatus::Failed(reason.clone()),
                    progress: tracker.progress,
                });
            }
            app.active_download_order.retain(|key| *key != download_key);
            app.clamp_selected_download_index();
            app.clamp_selected_history_index();
            app.set_error(format!("Download failed: {}", reason), reason);
        }
        AppMessage::DownloadCancelled(download_key) => {
            if let Some(tracker) = app.active_downloads.remove(&download_key) {
                app.push_download_history(NewDownloadHistoryEntry {
                    model_id: download_key.model_id,
                    version_id: tracker.version_id,
                    filename: tracker.filename,
                    model_name: tracker.model_name,
                    file_path: tracker.file_path,
                    downloaded_bytes: tracker.downloaded_bytes,
                    total_bytes: tracker.total_bytes,
                    status: DownloadHistoryStatus::Cancelled,
                    progress: tracker.progress,
                });
            }
            app.active_download_order.retain(|key| *key != download_key);
            app.clamp_selected_download_index();
            app.clamp_selected_history_index();
            app.set_status(format!("Download cancelled: {}", download_key.filename));
        }
    }
}
