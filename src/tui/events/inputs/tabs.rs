use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::fs;
use std::io::ErrorKind;

use super::super::actions::{
    ensure_selected_image_loaded, refresh_visible_media, request_image_feed_if_needed,
    send_cover_prefetch, send_cover_priority,
};
use super::super::artifacts::{copy_to_clipboard, save_text_artifact};
use super::LoopControl;
use crate::tui::{
    app::{App, DownloadState, MainTab, WorkerCommand},
    image::comfy_workflow_json,
    runtime::{current_image_render_request, debug_fetch_log},
};

pub(super) fn handle_modifier_key(app: &mut App, key: KeyEvent) -> Option<LoopControl> {
    if matches!(app.active_tab, MainTab::Models | MainTab::SavedModels) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('d') => {
                    app.move_list_selection_by(10);
                    send_cover_priority(app);
                    send_cover_prefetch(app);
                    return Some(LoopControl::Continue);
                }
                KeyCode::Char('u') => {
                    app.move_list_selection_by(-10);
                    send_cover_priority(app);
                    send_cover_prefetch(app);
                    return Some(LoopControl::Continue);
                }
                _ => {}
            }
        }
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::Up => {
                    app.select_previous_file();
                    return Some(LoopControl::Continue);
                }
                KeyCode::Down => {
                    app.select_next_file();
                    return Some(LoopControl::Continue);
                }
                _ => {}
            }
        }
    } else if matches!(app.active_tab, MainTab::Images | MainTab::SavedImages)
        && key.modifiers.contains(KeyModifiers::SHIFT)
    {
        match key.code {
            KeyCode::Up => {
                app.select_previous_image_model();
                return Some(LoopControl::Continue);
            }
            KeyCode::Down => {
                app.select_next_image_model();
                return Some(LoopControl::Continue);
            }
            _ => {}
        }
    }

    None
}

pub(super) fn handle_tab_key(
    app: &mut App,
    key: KeyEvent,
    is_ctrl_c_exit: bool,
) -> Option<LoopControl> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc if !is_ctrl_c_exit => {
            if app.has_active_download() {
                app.begin_exit_confirm_modal();
            } else if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::Quit);
                return Some(LoopControl::Break);
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('c') if is_ctrl_c_exit => {
            if app.has_active_download() {
                app.begin_exit_confirm_modal();
            } else if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::Quit);
                return Some(LoopControl::Break);
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Enter => {
            handle_enter(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Left | KeyCode::Char('h') => {
            handle_left(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            handle_right(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('b') => {
            handle_bookmark_toggle(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('j') => {
            handle_next_selection(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('k') => {
            handle_previous_selection(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Down => {
            handle_down(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Up => {
            handle_up(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('[') => {
            if matches!(app.active_tab, MainTab::Models | MainTab::SavedModels) {
                app.select_previous_version();
                send_cover_priority(app);
                send_cover_prefetch(app);
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char(']') => {
            if matches!(app.active_tab, MainTab::Models | MainTab::SavedModels) {
                app.select_next_version();
                send_cover_priority(app);
                send_cover_prefetch(app);
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('J') => {
            match app.active_tab {
                MainTab::Models | MainTab::SavedModels => app.select_next_file(),
                MainTab::Downloads => app.select_next_history(),
                MainTab::Images | MainTab::SavedImages | MainTab::Settings => {}
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('K') => {
            match app.active_tab {
                MainTab::Models | MainTab::SavedModels => app.select_previous_file(),
                MainTab::Downloads => app.select_previous_history(),
                MainTab::Images | MainTab::SavedImages | MainTab::Settings => {}
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('d') => {
            handle_download_or_delete(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('D') => {
            handle_delete_with_file(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('p') => {
            handle_pause_or_download(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('c') => {
            handle_c_action(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('m') => {
            if matches!(app.active_tab, MainTab::Images | MainTab::SavedImages) {
                app.show_image_prompt_modal = true;
                app.image_prompt_scroll = 0;
                app.status = "Prompt viewer opened".into();
            } else {
                app.show_status_modal = true;
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('T') => {
            match app.active_tab {
                MainTab::Models | MainTab::SavedModels => {
                    app.open_search_template_modal(crate::tui::app::SearchTemplateKind::Model);
                }
                MainTab::Images | MainTab::SavedImages => {
                    app.open_search_template_modal(crate::tui::app::SearchTemplateKind::Image);
                }
                MainTab::Downloads | MainTab::Settings => {}
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('M') => {
            app.begin_status_history_modal();
            app.status = "Status history opened".into();
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('a') => {
            if matches!(app.active_tab, MainTab::Images | MainTab::SavedImages) {
                app.image_advanced_visible = !app.image_advanced_visible;
                app.status = if app.image_advanced_visible {
                    "Advanced image metadata enabled".into()
                } else {
                    "Advanced image metadata hidden".into()
                };
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('o') => {
            if matches!(app.active_tab, MainTab::Images | MainTab::SavedImages)
                && let Some(image) = app.selected_image_in_active_view()
            {
                match copy_to_clipboard(&image.image_page_url()) {
                    Ok(()) => app.status = format!("Copied image page URL for {}", image.id),
                    Err(err) => {
                        app.last_error = Some(err.to_string());
                        app.show_status_modal = true;
                        app.status = "Failed to copy image page URL".into();
                    }
                }
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('w') => {
            copy_image_workflow(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('W') => {
            save_image_workflow(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('v') => {
            if matches!(app.active_tab, MainTab::Models | MainTab::SavedModels) {
                app.show_model_details = !app.show_model_details;
                if app.show_model_details {
                    app.request_selected_model_detail_sidebar();
                }
                app.status = if app.show_model_details {
                    "Model details panel enabled".into()
                } else {
                    "Model details panel disabled".into()
                };
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('r') => {
            handle_refresh_or_resume(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('/') => {
            handle_quick_search(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('f') => {
            handle_filter_builder(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('g') => {
            handle_jump_first(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('G') => {
            handle_jump_last(app);
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('?') => {
            app.show_help_modal = true;
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('e') => {
            if app.active_tab == MainTab::SavedModels {
                app.begin_bookmark_export_prompt();
            }
            return Some(LoopControl::Continue);
        }
        KeyCode::Char('i') => {
            if app.active_tab == MainTab::SavedModels {
                app.begin_bookmark_import_prompt();
            }
            return Some(LoopControl::Continue);
        }
        _ => {}
    }

    None
}

fn handle_enter(app: &mut App) {
    match app.active_tab {
        MainTab::Images | MainTab::SavedImages => {
            if let Some(selected_model) = app.selected_image_used_model()
                && selected_model.navigable
            {
                if let Some(model_id) = selected_model.model_id {
                    app.begin_image_model_detail_modal_loading();
                    if let Some(tx) = &app.tx {
                        let _ = tx.try_send(WorkerCommand::FetchModelDetail(
                            model_id,
                            selected_model.version_id,
                            selected_model
                                .query_name
                                .clone()
                                .unwrap_or_else(|| selected_model.label.clone()),
                        ));
                    }
                    app.status = format!("Opening model details: {}", selected_model.label);
                } else {
                    app.status = "Selected model item has no model id.".to_string();
                }
            }
        }
        MainTab::Models | MainTab::SavedModels => {
            if let Some((_, version_id)) = app.selected_model_version() {
                app.image_search_form
                    .set_linked_model_version(Some(version_id));
                app.active_tab = MainTab::Images;
                app.images.clear();
                app.image_cache.clear();
                app.image_bytes_cache.clear();
                app.selected_index = 0;
                app.image_feed_loaded = false;
                app.image_feed_loading = false;
                app.image_feed_next_page = None;
                app.image_feed_has_more = true;
                if let Some(tx) = &app.tx {
                    let _ = tx.try_send(WorkerCommand::FetchImages(
                        app.image_search_form.build_options(),
                        None,
                        current_image_render_request(),
                    ));
                    app.image_feed_loading = true;
                    app.status = format!("Opening images for model version {version_id}...");
                }
            } else {
                app.status = "Selected model has no model version id to open images.".into();
            }
        }
        MainTab::Settings => {
            if app.settings_form.focused_field == 9 {
                app.config.media_quality = app.config.media_quality.next();
                refresh_visible_media(app);
                if let Err(e) = app.config.save() {
                    app.last_error = Some(format!("Failed to save config: {}", e));
                    app.show_status_modal = true;
                } else {
                    app.last_error = None;
                    if let Some(tx) = &app.tx {
                        let _ = tx.try_send(WorkerCommand::UpdateConfig(app.config.clone()));
                    }
                }
                return;
            }
            if app.settings_form.focused_field == 10 {
                app.config.debug_logging = !app.config.debug_logging;
                if let Err(e) = app.config.save() {
                    app.last_error = Some(format!("Failed to save config: {}", e));
                    app.show_status_modal = true;
                } else if let Some(tx) = &app.tx {
                    let _ = tx.try_send(WorkerCommand::UpdateConfig(app.config.clone()));
                }
                return;
            }
            if app.settings_form.focused_field == 12 {
                app.image_cache.clear();
                app.image_bytes_cache.clear();
                app.image_request_keys.clear();
                app.model_version_image_cache.clear();
                app.model_version_image_bytes_cache.clear();
                app.model_version_image_request_keys.clear();
                app.model_version_image_failed.clear();
                if let Some(tx) = &app.tx {
                    let _ = tx.try_send(WorkerCommand::ClearAllCaches);
                }
                app.status = "Clearing cache storage...".into();
                return;
            }
            app.settings_form.editing = true;
            app.settings_form.input_buffer = match app.settings_form.focused_field {
                0 => app.config.api_key.clone().unwrap_or_default(),
                1 => app
                    .config
                    .comfyui_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
                2 => app
                    .config
                    .bookmark_file_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default(),
                3 => app
                    .config
                    .model_search_cache_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default(),
                5 => app
                    .config
                    .image_cache_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default(),
                6 => app.config.image_search_cache_ttl_minutes.to_string(),
                7 => app.config.image_detail_cache_ttl_minutes.to_string(),
                8 => app.config.image_cache_ttl_minutes.to_string(),
                11 => app
                    .config
                    .download_history_file_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default(),
                9 | 10 | 12 => String::new(),
                _ => app.config.model_search_cache_ttl_hours.to_string(),
            };
        }
        MainTab::Downloads => {}
    }
}

fn handle_left(app: &mut App) {
    if app.active_tab == MainTab::Settings
        && !app.settings_form.editing
        && app.settings_form.focused_field == 9
    {
        app.config.media_quality = app.config.media_quality.previous();
        refresh_visible_media(app);
        if let Err(e) = app.config.save() {
            app.last_error = Some(format!("Failed to save config: {}", e));
            app.show_status_modal = true;
        } else if let Some(tx) = &app.tx {
            let _ = tx.try_send(WorkerCommand::UpdateConfig(app.config.clone()));
        }
        return;
    }

    match app.active_tab {
        MainTab::Models | MainTab::SavedModels => {
            app.select_previous_version();
            send_cover_priority(app);
            send_cover_prefetch(app);
        }
        _ => {}
    }
}

fn handle_right(app: &mut App) {
    if app.active_tab == MainTab::Settings
        && !app.settings_form.editing
        && app.settings_form.focused_field == 9
    {
        app.config.media_quality = app.config.media_quality.next();
        refresh_visible_media(app);
        if let Err(e) = app.config.save() {
            app.last_error = Some(format!("Failed to save config: {}", e));
            app.show_status_modal = true;
        } else if let Some(tx) = &app.tx {
            let _ = tx.try_send(WorkerCommand::UpdateConfig(app.config.clone()));
        }
        return;
    }

    match app.active_tab {
        MainTab::Models | MainTab::SavedModels => {
            app.select_next_version();
            send_cover_priority(app);
            send_cover_prefetch(app);
        }
        _ => {}
    }
}

fn handle_bookmark_toggle(app: &mut App) {
    match app.active_tab {
        MainTab::Models => {
            if let Some(model) = app.selected_model_in_active_view().cloned() {
                app.toggle_bookmark_for_selected_model(&model);
            }
        }
        MainTab::SavedModels => app.request_bookmark_remove_selected(),
        MainTab::Images | MainTab::SavedImages => {
            if let Some(image) = app.selected_image_in_active_view().cloned() {
                app.toggle_bookmark_for_selected_image(&image);
            }
        }
        _ => {}
    }
}

fn handle_next_selection(app: &mut App) {
    if app.active_tab == MainTab::Settings {
        if app.settings_form.focused_field < 12 {
            app.settings_form.focused_field += 1;
        }
    } else if app.active_tab == MainTab::Downloads {
        if app.active_downloads.is_empty() {
            app.select_next_history();
        } else {
            app.select_next_download();
        }
    } else {
        app.select_next();
        if app.active_tab == MainTab::Models && app.can_request_more_models() {
            let prefetch_threshold = 30usize;
            let load_more = app.model_list_state.selected().is_some_and(|selected| {
                let trigger_idx = app.models.len().saturating_sub(prefetch_threshold);
                selected >= trigger_idx
            });
            if load_more && let Some((opts, next_page)) = app.next_model_search_options_if_needed()
            {
                debug_fetch_log(
                    &app.config,
                    &format!(
                        "UI: request more models append=true query=\"{}\" next_page={}",
                        opts.query.clone().unwrap_or_default(),
                        next_page.is_some()
                    ),
                );
                if let Some(tx) = &app.tx {
                    let _ = tx.try_send(WorkerCommand::SearchModels(
                        opts, None, None, false, true, next_page,
                    ));
                    app.status = "Loading more results...".to_string();
                }
            }
        }
        if app.active_tab == MainTab::Images
            && app.can_request_more_images(5)
            && let Some(next_page) = app.next_image_feed_page()
        {
            request_image_feed_if_needed(app, Some(next_page));
        }
        ensure_selected_image_loaded(app);
        if matches!(app.active_tab, MainTab::Models | MainTab::SavedModels) {
            send_cover_priority(app);
            send_cover_prefetch(app);
        }
    }
}

fn handle_previous_selection(app: &mut App) {
    if app.active_tab == MainTab::Settings {
        if app.settings_form.focused_field > 0 {
            app.settings_form.focused_field -= 1;
        }
    } else if app.active_tab == MainTab::Downloads {
        if app.active_downloads.is_empty() {
            app.select_previous_history();
        } else {
            app.select_previous_download();
        }
    } else {
        app.select_previous();
        ensure_selected_image_loaded(app);
        if matches!(app.active_tab, MainTab::Models | MainTab::SavedModels) {
            send_cover_priority(app);
            send_cover_prefetch(app);
        }
    }
}

fn handle_down(app: &mut App) {
    match app.active_tab {
        MainTab::Settings => {
            if app.settings_form.focused_field < 12 {
                app.settings_form.focused_field += 1;
            }
        }
        MainTab::Downloads => {
            if app.active_downloads.is_empty() {
                app.select_next_history();
            } else {
                app.select_next_download();
            }
        }
        MainTab::Models | MainTab::SavedModels | MainTab::Images | MainTab::SavedImages => {
            handle_next_selection(app);
        }
    }
}

fn handle_up(app: &mut App) {
    match app.active_tab {
        MainTab::Settings => {
            if app.settings_form.focused_field > 0 {
                app.settings_form.focused_field -= 1;
            }
        }
        MainTab::Downloads => {
            if app.active_downloads.is_empty() {
                app.select_previous_history();
            } else {
                app.select_previous_download();
            }
        }
        MainTab::Models | MainTab::SavedModels | MainTab::Images | MainTab::SavedImages => {
            handle_previous_selection(app);
        }
    }
}

fn handle_download_or_delete(app: &mut App) {
    if app.active_tab == MainTab::Downloads {
        if let Some(removed) = app.remove_selected_history() {
            let active_key = app.active_download_key_for_history_entry(&removed);
            let was_active = active_key.is_some();
            if let Some(download_key) = active_key
                && let Some(tx) = &app.tx
            {
                let _ = tx.try_send(WorkerCommand::CancelDownload(download_key));
            }
            app.status = if was_active {
                format!(
                    "Deleted history for {} and cancelled active download",
                    removed.model_name
                )
            } else {
                format!("Deleted history for {}", removed.model_name)
            };
        } else {
            app.status = "No download history selected".into();
        }
    } else {
        app.request_download();
    }
}

fn handle_delete_with_file(app: &mut App) {
    if app.active_tab == MainTab::Downloads {
        if let Some(removed) = app.remove_selected_history() {
            let active_key = app.active_download_key_for_history_entry(&removed);
            let was_active = active_key.is_some();
            if let Some(download_key) = active_key
                && let Some(tx) = &app.tx
            {
                let _ = tx.try_send(WorkerCommand::CancelDownload(download_key));
            }

            match removed.file_path {
                Some(path) => match fs::remove_file(&path) {
                    Ok(()) => {
                        app.last_error = None;
                        app.status = format!("Deleted history and file for {}", removed.model_name);
                    }
                    Err(err) => {
                        app.last_error = Some(err.to_string());
                        app.show_status_modal = true;
                        app.status = if err.kind() == ErrorKind::NotFound {
                            format!("No file found for {}", removed.model_name)
                        } else {
                            format!("Failed to delete file for {}: {}", removed.model_name, err)
                        };
                    }
                },
                None => {
                    app.last_error = None;
                    app.status = format!("No file path recorded for {}", removed.model_name);
                }
            }

            if was_active {
                app.status = format!("{} (and cancelled active download)", app.status);
            }
        } else {
            app.status = "No download history selected".into();
        }
    } else {
        app.request_download();
    }
}

fn handle_pause_or_download(app: &mut App) {
    if app.active_tab == MainTab::Downloads {
        if let Some(download_key) = app.selected_download_key()
            && let Some(tracker) = app.active_downloads.get(&download_key)
        {
            if tracker.state == DownloadState::Running {
                if let Some(tx) = &app.tx {
                    let _ = tx.try_send(WorkerCommand::PauseDownload(download_key));
                }
            } else if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::ResumeDownload(download_key));
            }
        }
    } else {
        app.request_download();
    }
}

fn handle_c_action(app: &mut App) {
    if app.active_tab == MainTab::Models {
        if let Some(tx) = &app.tx {
            let _ = tx.try_send(WorkerCommand::ClearSearchCache);
            app.status = "Clearing cached search results...".into();
        }
    } else if matches!(app.active_tab, MainTab::Images | MainTab::SavedImages)
        && let Some(image) = app.selected_image_in_active_view()
    {
        if let Some(json) = comfy_workflow_json(image) {
            match copy_to_clipboard(&json) {
                Ok(()) => app.status = format!("Copied Comfy workflow for image {}", image.id),
                Err(err) => {
                    app.last_error = Some(err.to_string());
                    app.show_status_modal = true;
                    app.status = "Failed to copy workflow".into();
                }
            }
        } else {
            app.status = "No Comfy workflow metadata for current image".into();
        }
    } else if app.active_tab == MainTab::Downloads
        && let Some(download_key) = app.selected_download_key()
        && let Some(tx) = &app.tx
    {
        let _ = tx.try_send(WorkerCommand::CancelDownload(download_key));
    }
}

fn copy_image_workflow(app: &mut App) {
    if matches!(app.active_tab, MainTab::Images | MainTab::SavedImages)
        && let Some(image) = app.selected_image_in_active_view()
    {
        if let Some(json) = comfy_workflow_json(image) {
            match copy_to_clipboard(&json) {
                Ok(()) => app.status = format!("Copied Comfy workflow for image {}", image.id),
                Err(err) => {
                    app.last_error = Some(err.to_string());
                    app.show_status_modal = true;
                    app.status = "Failed to copy workflow".into();
                }
            }
        } else {
            app.status = "No Comfy workflow metadata for current image".into();
        }
    }
}

fn save_image_workflow(app: &mut App) {
    if matches!(app.active_tab, MainTab::Images | MainTab::SavedImages)
        && let Some(image) = app.selected_image_in_active_view()
    {
        if let Some(json) = comfy_workflow_json(image) {
            match save_text_artifact("comfy-workflow", "json", &json) {
                Ok(path) => app.status = format!("Saved workflow to {}", path.display()),
                Err(err) => {
                    app.last_error = Some(err.to_string());
                    app.show_status_modal = true;
                    app.status = "Failed to save workflow".into();
                }
            }
        } else {
            app.status = "No Comfy workflow metadata for current image".into();
        }
    }
}

fn handle_refresh_or_resume(app: &mut App) {
    if app.active_tab == MainTab::Models {
        if let Some(tx) = &app.tx {
            let selected_model_id = app.selected_model_version().map(|(model_id, _)| model_id);
            let selected_version_id = app
                .selected_model_version()
                .map(|(_, version_id)| version_id);
            debug_fetch_log(
                &app.config,
                &format!(
                    "UI: refresh search query=\"{}\" force=true append=false",
                    app.search_form.query
                ),
            );
            let _ = tx.try_send(WorkerCommand::SearchModels(
                app.search_form.build_options(),
                selected_model_id,
                selected_version_id,
                true,
                false,
                None,
            ));
            app.model_search_has_more = true;
            app.model_search_loading_more = false;
            app.status = format!("Refreshing search cache for '{}'", app.search_form.query);
        }
    } else if app.active_tab == MainTab::Downloads {
        if let Some(entry) = app.selected_download_history_entry().cloned() {
            if entry.total_bytes > 0 && entry.downloaded_bytes >= entry.total_bytes {
                app.status = "Selected item already complete.".into();
                return;
            }
            if app.active_download_key_for_history_entry(&entry).is_some() {
                app.status = "Download already active for selected model.".into();
                return;
            }
            if entry.file_path.is_none() {
                app.status = "Selected history has no file path.".into();
                return;
            }
            let has_file = entry.file_path.as_deref().is_some_and(|path| path.exists());
            if !has_file {
                app.last_error = Some("Missing partial file".to_string());
                app.show_status_modal = true;
                app.status = "Cannot resume: partial file not found.".into();
                return;
            }
            if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::ResumeDownloadModel(
                    entry.model_id,
                    entry.version_id,
                    entry.file_path.clone(),
                    entry.downloaded_bytes,
                    entry.total_bytes,
                ));
                app.remove_history_for_session(entry.model_id, entry.version_id, &entry.filename);
                app.status = format!("Resuming {} (v{})...", entry.model_name, entry.version_id);
            }
        } else {
            app.status = "No download history selected".into();
        }
    }
}

fn handle_quick_search(app: &mut App) {
    match app.active_tab {
        MainTab::Models => {
            app.mode = crate::tui::app::AppMode::SearchForm;
            app.search_form.begin_quick_search();
            app.status = "Quick search. Type query, Enter apply, Esc cancel.".into();
        }
        MainTab::Images => {
            app.mode = crate::tui::app::AppMode::SearchImages;
            app.image_search_form.begin_quick_search();
            app.status = "Quick image search. Type query, Enter apply, Esc cancel.".into();
        }
        MainTab::SavedImages => app.begin_image_bookmark_search(),
        MainTab::SavedModels => {
            app.begin_bookmark_search();
            app.bookmark_search_form_draft.begin_quick_search();
        }
        _ => {}
    }
}

fn handle_filter_builder(app: &mut App) {
    match app.active_tab {
        MainTab::Models => {
            app.mode = crate::tui::app::AppMode::SearchForm;
            app.search_form.begin_builder();
            app.status =
                "Search builder. Up/Down sections, Left/Right options, Space toggle, Enter apply."
                    .into();
        }
        MainTab::Images => {
            app.mode = crate::tui::app::AppMode::SearchImages;
            app.image_search_form.begin_builder();
            app.status =
                "Image filters. Up/Down sections, Left/Right options, Space toggle, Enter apply."
                    .into();
        }
        MainTab::SavedModels => {
            app.begin_bookmark_search();
            app.bookmark_search_form_draft.begin_builder();
            app.status =
                "Bookmark filters. Up/Down sections, Left/Right options, Space toggle, Enter apply."
                    .into();
        }
        _ => {}
    }
}

fn handle_jump_first(app: &mut App) {
    match app.active_tab {
        MainTab::Models | MainTab::SavedModels => {
            app.select_list_first();
            send_cover_priority(app);
            send_cover_prefetch(app);
        }
        MainTab::Images => app.selected_index = 0,
        MainTab::SavedImages => app.selected_image_bookmark_index = 0,
        _ => {}
    }
}

fn handle_jump_last(app: &mut App) {
    match app.active_tab {
        MainTab::Models | MainTab::SavedModels => {
            app.select_list_last();
            send_cover_priority(app);
            send_cover_prefetch(app);
        }
        MainTab::Images => {
            if !app.images.is_empty() {
                app.selected_index = app.images.len().saturating_sub(1);
            }
        }
        MainTab::SavedImages => {
            let visible = app.visible_image_bookmarks();
            if !visible.is_empty() {
                app.selected_image_bookmark_index = visible.len().saturating_sub(1);
            }
        }
        _ => {}
    }
}
