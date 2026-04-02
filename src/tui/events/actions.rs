use crate::tui::app::{App, MainTab, WorkerCommand};
use crate::tui::runtime::{
    current_image_render_request, current_model_cover_render_request, render_request_key,
};

pub(super) fn reload_selected_image(app: &mut App) {
    if !matches!(app.active_tab, MainTab::Images | MainTab::SavedImages) {
        return;
    }
    if let Some(image) = app.selected_image_in_active_view().cloned()
        && let Some(tx) = &app.tx
    {
        let request = current_image_render_request();
        let request_key = render_request_key(request, app.config.media_quality);
        if app.has_cached_image_request(image.id, &request_key) {
            if let Some(bytes) = app.image_bytes_cache.get(&image.id).cloned() {
                let _ = tx.try_send(WorkerCommand::RebuildImageProtocol(image.id, bytes));
            }
        } else {
            app.image_cache.remove(&image.id);
            app.image_request_keys.remove(&image.id);
            let _ = tx.try_send(WorkerCommand::LoadImage(image, request));
        }
    }
}

pub(super) fn ensure_selected_image_loaded(app: &mut App) {
    if !matches!(app.active_tab, MainTab::Images | MainTab::SavedImages) {
        return;
    }
    if let Some(image) = app.selected_image_in_active_view().cloned() {
        let request = current_image_render_request();
        let request_key = render_request_key(request, app.config.media_quality);
        if !app.has_cached_image_request(image.id, &request_key)
            && let Some(tx) = &app.tx
        {
            let _ = tx.try_send(WorkerCommand::LoadImage(image, request));
        }
    }
}

pub(super) fn send_cover_priority(app: &mut App) {
    if let Some((_, version_id, cover_url, source_dims)) =
        app.selected_model_version_with_cover_url()
    {
        let request = current_model_cover_render_request();
        let request_key = render_request_key(request, app.config.media_quality);
        if app.has_cached_model_cover_request(version_id, &request_key) {
            return;
        }
        if let Some(tx) = &app.tx {
            let _ = tx.try_send(WorkerCommand::PrioritizeModelCover(
                version_id,
                cover_url,
                source_dims,
                request,
            ));
        }
    }
}

pub(super) fn reload_selected_model_cover(app: &mut App) {
    if !matches!(app.active_tab, MainTab::Models | MainTab::SavedModels) {
        return;
    }
    let Some((_, version_id)) = app.selected_model_version() else {
        return;
    };
    if let Some(tx) = &app.tx {
        let request = current_model_cover_render_request();
        let request_key = render_request_key(request, app.config.media_quality);
        if app.has_cached_model_cover_request(version_id, &request_key) {
            if let Some(bytes) = app
                .model_version_image_bytes_cache
                .get(&version_id)
                .and_then(|entries| entries.first())
                .cloned()
            {
                let _ = tx.try_send(WorkerCommand::RebuildModelCover(version_id, bytes));
            }
        } else {
            app.model_version_image_cache.remove(&version_id);
            app.model_version_image_request_keys.remove(&version_id);
            let selected_cover = app.selected_model_version_with_cover_url();
            let cover_url = selected_cover
                .as_ref()
                .and_then(|(_, _, cover_url, _)| cover_url.clone());
            let source_dims = selected_cover.and_then(|(_, _, _, source_dims)| source_dims);
            let _ = tx.try_send(WorkerCommand::PrioritizeModelCover(
                version_id,
                cover_url,
                source_dims,
                request,
            ));
        }
    }
}

pub(super) fn send_cover_prefetch(app: &mut App) {
    let neighbors = app.selected_model_neighbor_cover_urls(2);
    if neighbors.is_empty() {
        return;
    }
    if let Some(tx) = &app.tx {
        let _ = tx.try_send(WorkerCommand::PrefetchModelCovers(
            neighbors,
            current_model_cover_render_request(),
        ));
    }
}

pub(super) fn send_image_model_detail_cover_priority(app: &mut App) {
    if !app.show_image_model_detail_modal {
        return;
    }
    let Some((version_id, cover_url, source_dims)) = app.image_model_detail_selected_cover() else {
        return;
    };
    let request = current_model_cover_render_request();
    let request_key = render_request_key(request, app.config.media_quality);
    if app.has_cached_model_cover_request(version_id, &request_key) {
        return;
    }
    if let Some(tx) = &app.tx {
        let _ = tx.try_send(WorkerCommand::PrioritizeModelCover(
            version_id,
            cover_url,
            source_dims,
            request,
        ));
    }
}

pub(super) fn send_image_model_detail_cover_prefetch(app: &mut App) {
    if !app.show_image_model_detail_modal {
        return;
    }
    let request = current_model_cover_render_request();
    let jobs = app.image_model_detail_neighbor_cover_urls(2);
    if jobs.is_empty() {
        return;
    }
    if let Some(tx) = &app.tx {
        let _ = tx.try_send(WorkerCommand::PrefetchModelCovers(jobs, request));
    }
}

pub(super) fn refresh_visible_media(app: &mut App) {
    app.image_cache.clear();
    app.image_request_keys.clear();
    app.model_version_image_cache.clear();
    app.model_version_image_request_keys.clear();
    app.model_version_image_failed.clear();
    reload_selected_image(app);
    reload_selected_model_cover(app);
    send_cover_prefetch(app);
}

pub(super) fn request_image_feed_if_needed(app: &mut App, next_page: Option<u32>) {
    match &next_page {
        None => {
            if app.image_feed_loaded {
                return;
            }
        }
        Some(requested_next_page) => {
            if app.image_feed_loading
                || app.image_feed_next_page.is_none()
                || app.image_feed_next_page.as_ref() != Some(requested_next_page)
            {
                return;
            }
        }
    };

    if app.image_feed_loading {
        return;
    }

    if let Some(tx) = &app.tx
        && tx
            .try_send(WorkerCommand::FetchImages(
                app.image_search_form.build_options(),
                next_page,
                current_image_render_request(),
            ))
            .is_ok()
    {
        app.image_feed_loading = true;
        if app.image_feed_loaded {
            app.set_status("Loading more images...");
        } else {
            app.set_status("Fetching image feed...");
        }
    }
}
