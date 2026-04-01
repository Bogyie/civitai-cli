use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::fs::{self, OpenOptions, create_dir_all};
use std::io::Stdout;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::tui::app::{
    App, AppMessage, AppMode, DownloadHistoryStatus, DownloadState, MainTab, SearchFormMode,
    SearchFormSection, WorkerCommand,
};
use crate::tui::ui;

fn debug_fetch_log_path(config: &crate::config::AppConfig) -> Option<PathBuf> {
    crate::config::AppConfig::config_dir()
        .or_else(|| config.search_cache_path())
        .map(|dir| dir.join("fetch_debug.log"))
}

fn debug_fetch_log_to_file(path: &std::path::Path, message: &str) {
    if !cfg!(debug_assertions) {
        return;
    }

    if let Some(parent) = path.parent() {
        let _ = create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|dur| dur.as_secs())
            .unwrap_or_default();
        let _ = writeln!(file, "[{}] {}", ts, message);
    }
}

fn debug_fetch_log(config: &crate::config::AppConfig, message: &str) {
    if let Some(path) = debug_fetch_log_path(config) {
        debug_fetch_log_to_file(&path, message);
    }
}

pub async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    mut rx: mpsc::Receiver<AppMessage>,
) -> Result<()> {
    let send_cover_priority = |app: &mut App| {
        if let Some((_, version_id, cover_url)) = app.selected_model_version_with_cover_url() {
            if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::PrioritizeModelCover(version_id, cover_url));
            }
        }
    };

    let send_cover_prefetch = |app: &mut App| {
        let neighbors = app.selected_model_neighbor_cover_urls(2);
        if neighbors.is_empty() {
            return;
        }
        if let Some(tx) = &app.tx {
            let _ = tx.try_send(WorkerCommand::PrefetchModelCovers(neighbors));
        }
    };

    let request_image_feed_if_needed = |app: &mut App, next_page: Option<String>| {
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

        if let Some(tx) = &app.tx {
            match tx.try_send(WorkerCommand::FetchImages(
                app.image_search_form.build_options(),
                next_page,
            )) {
                Ok(_) => {
                    app.image_feed_loading = true;
                    app.status = if app.image_feed_loaded {
                        "Loading more images...".to_string()
                    } else {
                        "Fetching image feed...".to_string()
                    };
                }
                Err(_) => {}
            }
        }
    };

    loop {
        let poll_timeout_ms = match app.mode {
            AppMode::SearchForm
            | AppMode::SearchImages
            | AppMode::SearchBookmarks
            | AppMode::SearchImageBookmarks
            | AppMode::BookmarkPathPrompt => 200,
            _ => 50,
        };

        terminal.draw(|f| ui::draw(f, app))?;

        // Wait for either terminal input or worker message update
        tokio::select! {
             // Polling keypresses
                 event_res = tokio::task::spawn_blocking(move || {
                    event::poll(std::time::Duration::from_millis(poll_timeout_ms))
                 }) => {
                 if let Ok(Ok(true)) = event_res {
                     if let Ok(Event::Key(key)) = event::read() {
                        let is_ctrl_c_exit = matches!(key.code, KeyCode::Char('c'))
                            && key.modifiers.contains(KeyModifiers::CONTROL);
                        if app.mode == AppMode::SearchForm {
                            match key.code {
                                KeyCode::Esc => {
                                    app.mode = AppMode::Browsing;
                                }
                                KeyCode::Enter => {
                                    app.mode = AppMode::Browsing;
                                    let selected_model_id = app.selected_model_version().map(|(model_id, _)| model_id);
                                    let selected_version_id = app.selected_model_version().map(|(_, version_id)| version_id);
                                    let search_options = app.search_form.build_options();
                                    debug_fetch_log(
                                        &app.config,
                                        &format!(
                                            "UI: search submit query=\"{}\" limit={} append=false force_refresh=false",
                                            app.search_form.query,
                                            search_options.limit.unwrap_or(50)
                                        ),
                                    );
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::SearchModels(
                                            search_options,
                                            selected_model_id,
                                            selected_version_id,
                                            false,
                                            false,
                                            None,
                                        ));
                                        app.model_search_has_more = true;
                                        app.model_search_loading_more = false;
                                        app.status = format!("Searching for models: '{}'...", app.search_form.query);
                                    }
                                }
                                KeyCode::Tab => {
                                    if app.search_form.mode == SearchFormMode::Builder {
                                        app.search_form.focused_section = match app.search_form.focused_section {
                                            SearchFormSection::Query => SearchFormSection::Sort,
                                            SearchFormSection::Sort => SearchFormSection::Period,
                                            SearchFormSection::Period => SearchFormSection::Type,
                                            SearchFormSection::Type => SearchFormSection::BaseModel,
                                            SearchFormSection::BaseModel => SearchFormSection::Query,
                                        };
                                    }
                                }
                                KeyCode::Up => {
                                    if app.search_form.mode == SearchFormMode::Builder {
                                        app.search_form.focused_section = match app.search_form.focused_section {
                                            SearchFormSection::Query => SearchFormSection::BaseModel,
                                            SearchFormSection::Sort => SearchFormSection::Query,
                                            SearchFormSection::Period => SearchFormSection::Sort,
                                            SearchFormSection::Type => SearchFormSection::Period,
                                            SearchFormSection::BaseModel => SearchFormSection::Type,
                                        };
                                    }
                                }
                                KeyCode::Down => {
                                    if app.search_form.mode == SearchFormMode::Builder {
                                        app.search_form.focused_section = match app.search_form.focused_section {
                                            SearchFormSection::Query => SearchFormSection::Sort,
                                            SearchFormSection::Sort => SearchFormSection::Period,
                                            SearchFormSection::Period => SearchFormSection::Type,
                                            SearchFormSection::Type => SearchFormSection::BaseModel,
                                            SearchFormSection::BaseModel => SearchFormSection::Query,
                                        };
                                    }
                                }
                                KeyCode::Left => {
                                    if app.search_form.mode == SearchFormMode::Builder {
                                        match app.search_form.focused_section {
                                            SearchFormSection::Sort => {
                                                if app.search_form.selected_sort > 0 {
                                                    app.search_form.selected_sort -= 1;
                                                } else {
                                                    app.search_form.selected_sort = app.search_form.sort_options.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::Period => {
                                                if app.search_form.selected_period > 0 {
                                                    app.search_form.selected_period -= 1;
                                                } else {
                                                    app.search_form.selected_period = app.search_form.periods.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::Type => {
                                                if app.search_form.type_cursor > 0 {
                                                    app.search_form.type_cursor -= 1;
                                                } else {
                                                    app.search_form.type_cursor = app.search_form.type_options.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::BaseModel => {
                                                if app.search_form.base_cursor > 0 {
                                                    app.search_form.base_cursor -= 1;
                                                } else {
                                                    app.search_form.base_cursor = app.search_form.base_options.len().saturating_sub(1);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Right => {
                                    if app.search_form.mode == SearchFormMode::Builder {
                                        match app.search_form.focused_section {
                                            SearchFormSection::Sort => {
                                                app.search_form.selected_sort =
                                                    (app.search_form.selected_sort + 1) % app.search_form.sort_options.len();
                                            }
                                            SearchFormSection::Period => {
                                                app.search_form.selected_period =
                                                    (app.search_form.selected_period + 1) % app.search_form.periods.len();
                                            }
                                            SearchFormSection::Type => {
                                                app.search_form.type_cursor =
                                                    (app.search_form.type_cursor + 1) % app.search_form.type_options.len();
                                            }
                                            SearchFormSection::BaseModel => {
                                                app.search_form.base_cursor =
                                                    (app.search_form.base_cursor + 1) % app.search_form.base_options.len();
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Char('f') => {
                                    app.search_form.begin_builder();
                                }
                                KeyCode::Char(' ') => {
                                    if app.search_form.mode == SearchFormMode::Builder {
                                        match app.search_form.focused_section {
                                            SearchFormSection::Type => {
                                                if let Some(item) = app.search_form.type_options.get(app.search_form.type_cursor).cloned() {
                                                    if !app.search_form.selected_types.insert(item.clone()) {
                                                        app.search_form.selected_types.remove(&item);
                                                    }
                                                }
                                            }
                                            SearchFormSection::BaseModel => {
                                                if let Some(item) = app.search_form.base_options.get(app.search_form.base_cursor).cloned() {
                                                    if !app.search_form.selected_base_models.insert(item.clone()) {
                                                        app.search_form.selected_base_models.remove(&item);
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Char(c) => {
                                    if app.search_form.focused_section == SearchFormSection::Query {
                                        app.search_form.query.push(c);
                                    }
                                }
                                KeyCode::Backspace => {
                                    if app.search_form.focused_section == SearchFormSection::Query {
                                        app.search_form.query.pop();
                                    }
                                }
                                _ => {}
                            }
                            continue; // Skip global navigation if form is active
                        }

                        if app.mode == AppMode::SearchImages {
                            match key.code {
                                KeyCode::Esc => {
                                    app.mode = AppMode::Browsing;
                                }
                                KeyCode::Up => {
                                    if app.image_search_form.focused_field > 0 {
                                        app.image_search_form.focused_field -= 1;
                                    }
                                }
                                KeyCode::Down => {
                                    if app.image_search_form.focused_field < 4 {
                                        app.image_search_form.focused_field += 1;
                                    }
                                }
                                KeyCode::Left => match app.image_search_form.focused_field {
                                    0 => {
                                        if app.image_search_form.selected_nsfw > 0 {
                                            app.image_search_form.selected_nsfw -= 1;
                                        } else {
                                            app.image_search_form.selected_nsfw =
                                                app.image_search_form.nsfw_options.len() - 1;
                                        }
                                    }
                                    1 => {
                                        if app.image_search_form.selected_sort > 0 {
                                            app.image_search_form.selected_sort -= 1;
                                        } else {
                                            app.image_search_form.selected_sort =
                                                app.image_search_form.sort_options.len() - 1;
                                        }
                                    }
                                    2 => {
                                        if app.image_search_form.selected_period > 0 {
                                            app.image_search_form.selected_period -= 1;
                                        } else {
                                            app.image_search_form.selected_period =
                                                app.image_search_form.period_options.len() - 1;
                                        }
                                    }
                                    _ => {}
                                },
                                KeyCode::Right => match app.image_search_form.focused_field {
                                    0 => {
                                        app.image_search_form.selected_nsfw =
                                            (app.image_search_form.selected_nsfw + 1)
                                                % app.image_search_form.nsfw_options.len();
                                    }
                                    1 => {
                                        app.image_search_form.selected_sort =
                                            (app.image_search_form.selected_sort + 1)
                                                % app.image_search_form.sort_options.len();
                                    }
                                    2 => {
                                        app.image_search_form.selected_period =
                                            (app.image_search_form.selected_period + 1)
                                                % app.image_search_form.period_options.len();
                                    }
                                    _ => {}
                                },
                                KeyCode::Enter => {
                                    if !app.image_search_form.tag_text.trim().is_empty()
                                        && app.image_search_form.build_options().tags.is_none()
                                    {
                                        app.last_error = Some(format!(
                                            "Unknown image tag: {}",
                                            app.image_search_form.tag_text.trim()
                                        ));
                                        app.show_status_modal = true;
                                        app.status = "Invalid image tag".into();
                                        continue;
                                    }

                                    app.mode = AppMode::Browsing;
                                    app.images.clear();
                                    app.image_cache.clear();
                                    app.selected_index = 0;
                                    app.image_feed_loaded = false;
                                    app.image_feed_loading = false;
                                    app.image_feed_next_page = None;
                                    app.image_feed_has_more = true;
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::FetchImages(
                                            app.image_search_form.build_options(),
                                            None,
                                        ));
                                        app.image_feed_loading = true;
                                        app.status = "Searching image feed...".into();
                                    }
                                }
                                KeyCode::Char(c) => {
                                    if app.image_search_form.focused_field == 3 {
                                        if c.is_ascii_digit() {
                                            app.image_search_form.model_version_id.push(c);
                                        }
                                    } else if app.image_search_form.focused_field == 4 {
                                        app.image_search_form.tag_text.push(c);
                                    }
                                }
                                KeyCode::Backspace => {
                                    if app.image_search_form.focused_field == 3 {
                                        app.image_search_form.model_version_id.pop();
                                    } else if app.image_search_form.focused_field == 4 {
                                        app.image_search_form.tag_text.pop();
                                    }
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.mode == AppMode::SearchBookmarks {
                            match key.code {
                                KeyCode::Esc => {
                                    app.cancel_bookmark_search();
                                }
                                KeyCode::Enter => {
                                    app.apply_bookmark_query();
                                }
                                KeyCode::Char(c) => {
                                    app.bookmark_query_draft.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.bookmark_query_draft.pop();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.mode == AppMode::SearchImageBookmarks {
                            match key.code {
                                KeyCode::Esc => {
                                    app.cancel_image_bookmark_search();
                                }
                                KeyCode::Enter => {
                                    app.apply_image_bookmark_query();
                                }
                                KeyCode::Char(c) => {
                                    app.image_bookmark_query_draft.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.image_bookmark_query_draft.pop();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.mode == AppMode::BookmarkPathPrompt {
                            match key.code {
                                KeyCode::Esc => {
                                    app.cancel_bookmark_path_prompt();
                                }
                                KeyCode::Enter => {
                                    app.apply_bookmark_path_prompt();
                                }
                                KeyCode::Char(c) => {
                                    app.bookmark_path_draft.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.bookmark_path_draft.pop();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_status_modal {
                            match key.code {
                                KeyCode::Char('m') | KeyCode::Esc | KeyCode::Enter => {
                                    app.show_status_modal = false;
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_bookmark_confirm_modal {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    app.confirm_remove_selected_bookmark();
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    app.cancel_bookmark_remove();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_resume_download_modal {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    let sessions = app.interrupted_download_sessions.clone();
                                    for session in sessions {
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::ResumeDownloadModel(
                                                session.model_id,
                                                session.version_id,
                                                session.file_path.clone(),
                                                session.downloaded_bytes,
                                                session.total_bytes,
                                            ));
                                        }
                                    }
                                    app.clear_interrupted_download_sessions();
                                    app.status = "Interrupted downloads resumed.".into();
                                }
                                KeyCode::Char('d') | KeyCode::Char('D') => {
                                    let sessions = app.interrupted_download_sessions.clone();
                                    for session in sessions {
                                        if let Some(path) = session.file_path.clone() {
                                            match fs::remove_file(&path) {
                                                Ok(()) => {}
                                                Err(err) if err.kind() == ErrorKind::NotFound => {}
                                                Err(err) => {
                                                    app.last_error =
                                                        Some(format!("Failed to delete file: {}", err));
                                                    app.show_status_modal = true;
                                                    app.status =
                                                        format!("Failed to delete file for {}", session.filename);
                                                }
                                            }
                                        }
                                        let _ = app.remove_history_for_session(
                                            session.model_id,
                                            session.version_id,
                                        );
                                    }
                                    app.clear_interrupted_download_sessions();
                                    app.status = "Interrupted downloads removed.".into();
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    app.cancel_resume_download_modal();
                                    app.status = "Resume interrupted downloads cancelled.".into();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_exit_confirm_modal {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    let sessions = app.collect_interrupt_sessions_from_active();
                                    for session in &sessions {
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::PauseDownload(session.model_id));
                                        }
                                        app.record_interrupted_session_to_history(session);
                                    }
                                    app.interrupted_download_sessions = sessions;
                                    app.persist_interrupted_downloads();
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::Quit);
                                    }
                                    app.show_exit_confirm_modal = false;
                                    break;
                                }
                                KeyCode::Char('d') | KeyCode::Char('D') => {
                                    let sessions = app.collect_interrupt_sessions_from_active();
                                    for session in sessions.iter() {
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::CancelDownload(session.model_id));
                                        }
                                        if let Some(path) = session.file_path.clone() {
                                            let _ = fs::remove_file(&path);
                                        }
                                        app.remove_history_for_session(session.model_id, session.version_id);
                                    }
                                    app.interrupted_download_sessions.clear();
                                    app.persist_interrupted_downloads();
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::Quit);
                                    }
                                    app.show_exit_confirm_modal = false;
                                    break;
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    app.cancel_exit_confirm_modal();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.active_tab == MainTab::Settings && app.settings_form.editing {
                            match key.code {
                                KeyCode::Up => {
                                    if app.settings_form.focused_field > 0 {
                                        app.settings_form.focused_field -= 1;
                                    }
                                }
                                KeyCode::Down => {
                                    if app.settings_form.focused_field < 8 {
                                        app.settings_form.focused_field += 1;
                                    }
                                }
                                KeyCode::Esc => {
                                    app.settings_form.editing = false;
                                }
                                KeyCode::Enter => {
                                    if app.settings_form.focused_field == 0 {
                                        app.config.api_key = if app.settings_form.input_buffer.is_empty() { None } else { Some(app.settings_form.input_buffer.clone()) };
                                    } else if app.settings_form.focused_field == 1 {
                                        app.config.comfyui_path = if app.settings_form.input_buffer.is_empty() { None } else { Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone())) };
                                    } else if app.settings_form.focused_field == 3 {
                                        app.config.model_search_cache_path = if app.settings_form.input_buffer.is_empty() {
                                            None
                                        } else {
                                            Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone()))
                                        };
                                    } else if app.settings_form.focused_field == 4 {
                                        match app.settings_form.input_buffer.trim().parse::<u64>() {
                                            Ok(value) if value > 0 => {
                                                app.config.model_search_cache_ttl_hours = value;
                                            }
                                            _ => {
                                                app.last_error = Some("Cache TTL must be a positive integer".into());
                                                app.show_status_modal = true;
                                                app.status = "Invalid cache TTL value".into();
                                                continue;
                                            }
                                        }
                                    } else if app.settings_form.focused_field == 5 {
                                        app.config.image_cache_path = if app.settings_form.input_buffer.is_empty() {
                                            None
                                        } else {
                                            Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone()))
                                        };
                                    } else if app.settings_form.focused_field == 6 {
                                        match app.settings_form.input_buffer.trim().parse::<u64>() {
                                            Ok(value) if value > 0 => {
                                                app.config.image_search_cache_ttl_minutes = value;
                                            }
                                            _ => {
                                                app.last_error = Some("Image search cache TTL must be a positive integer".into());
                                                app.show_status_modal = true;
                                                app.status = "Invalid image search cache TTL value".into();
                                                continue;
                                            }
                                        }
                                    } else if app.settings_form.focused_field == 7 {
                                        match app.settings_form.input_buffer.trim().parse::<u64>() {
                                            Ok(value) => {
                                                app.config.image_cache_ttl_minutes = value;
                                            }
                                            _ => {
                                                app.last_error = Some("Image cache TTL must be a non-negative integer".into());
                                                app.show_status_modal = true;
                                                app.status = "Invalid image cache TTL value".into();
                                                continue;
                                            }
                                        }
                                    } else if app.settings_form.focused_field == 2 {
                                        let path = if app.settings_form.input_buffer.is_empty() {
                                            None
                                        } else {
                                            Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone()))
                                        };
                                        app.config.bookmark_file_path = path.clone();
                                        app.bookmark_file_path = path;
                                    } else if app.settings_form.focused_field == 8 {
                                        let path = if app.settings_form.input_buffer.is_empty() {
                                            None
                                        } else {
                                            Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone()))
                                        };
                                        if let Some(path) = path {
                                            app.set_download_history_file_path(path);
                                        } else {
                                            app.config.download_history_file_path = None;
                                            app.download_history_file_path = None;
                                        }
                                    }
                                    if let Err(e) = app.config.save() {
                                        app.last_error = Some(format!("Failed to save config: {}", e));
                                        app.show_status_modal = true;
                                    } else {
                                        app.last_error = None;
                                        app.show_status_modal = false;
                                        app.status = "Settings saved to config.json".into();
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::UpdateConfig(app.config.clone()));
                                        }
                                    }
                                    app.settings_form.editing = false;
                                }
                                KeyCode::Char(c) => {
                                    app.settings_form.input_buffer.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.settings_form.input_buffer.pop();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                match key.code {
                                    KeyCode::Char('d') => {
                                        app.move_list_selection_by(10);
                                        send_cover_priority(app);
                                        send_cover_prefetch(app);
                                        continue;
                                    }
                                    KeyCode::Char('u') => {
                                        app.move_list_selection_by(-10);
                                        send_cover_priority(app);
                                        send_cover_prefetch(app);
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        match key.code {
                            KeyCode::Tab => {
                                let next_tab = match app.active_tab {
                                    MainTab::Models => MainTab::Bookmarks,
                                    MainTab::Bookmarks => MainTab::Images,
                                    MainTab::Images => MainTab::ImageBookmarks,
                                    MainTab::ImageBookmarks => MainTab::Downloads,
                                    MainTab::Downloads => MainTab::Settings,
                                    MainTab::Settings => MainTab::Models,
                                };
                                let prev_tab = app.active_tab;
                                app.active_tab = next_tab;
                                if prev_tab != next_tab && next_tab == MainTab::Images {
                                    request_image_feed_if_needed(app, None);
                                }
                                if app.active_tab == MainTab::Bookmarks {
                                    app.clamp_bookmark_selection();
                                } else if app.active_tab == MainTab::ImageBookmarks {
                                    app.clamp_image_bookmark_selection();
                                }
                            }
                            KeyCode::Char('1') => {
                                app.active_tab = MainTab::Models;
                            }
                            KeyCode::Char('2') => {
                                app.active_tab = MainTab::Bookmarks;
                                app.clamp_bookmark_selection();
                            }
                            KeyCode::Char('3') => {
                                let prev_tab = app.active_tab;
                                app.active_tab = MainTab::Images;
                                if prev_tab != MainTab::Images {
                                    request_image_feed_if_needed(app, None);
                                }
                            }
                            KeyCode::Char('4') => {
                                app.active_tab = MainTab::ImageBookmarks;
                                app.clamp_image_bookmark_selection();
                            }
                            KeyCode::Char('5') => {
                                app.active_tab = MainTab::Downloads;
                            }
                            KeyCode::Char('6') => {
                                app.active_tab = MainTab::Settings;
                            }
                            KeyCode::Char('q') | KeyCode::Esc if !is_ctrl_c_exit => {
                                if app.has_active_download() {
                                    app.begin_exit_confirm_modal();
                                } else if let Some(tx) = &app.tx {
                                    let _ = tx.try_send(WorkerCommand::Quit);
                                    break;
                                }
                            }
                            KeyCode::Char('c') if is_ctrl_c_exit => {
                                if app.has_active_download() {
                                    app.begin_exit_confirm_modal();
                                } else if let Some(tx) = &app.tx {
                                    let _ = tx.try_send(WorkerCommand::Quit);
                                    break;
                                }
                            }
                            KeyCode::Enter => {
                                if app.active_tab == MainTab::Settings {
                                    app.settings_form.editing = true;
                                    app.settings_form.input_buffer = if app.settings_form.focused_field == 0 {
                                        app.config.api_key.clone().unwrap_or_default()
                                    } else if app.settings_form.focused_field == 1 {
                                        app.config.comfyui_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
                                    } else if app.settings_form.focused_field == 2 {
                                        app.config.bookmark_file_path.as_ref().map(|path| path.to_string_lossy().to_string()).unwrap_or_default()
                                    } else if app.settings_form.focused_field == 3 {
                                        app.config
                                            .model_search_cache_path
                                            .as_ref()
                                            .map(|path| path.to_string_lossy().to_string())
                                            .unwrap_or_default()
                                    } else if app.settings_form.focused_field == 5 {
                                        app.config
                                            .image_cache_path
                                            .as_ref()
                                            .map(|path| path.to_string_lossy().to_string())
                                            .unwrap_or_default()
                                    } else if app.settings_form.focused_field == 6 {
                                        app.config.image_search_cache_ttl_minutes.to_string()
                                    } else if app.settings_form.focused_field == 7 {
                                        app.config.image_cache_ttl_minutes.to_string()
                                    } else if app.settings_form.focused_field == 8 {
                                        app.config
                                            .download_history_file_path
                                            .as_ref()
                                            .map(|path| path.to_string_lossy().to_string())
                                            .unwrap_or_default()
                                    } else {
                                        app.config.model_search_cache_ttl_hours.to_string()
                                    };
                                }
                            }
                            KeyCode::Char('b') => {
                                if app.active_tab == MainTab::Models {
                                    if let Some(model) = app.selected_model_in_active_view().cloned() {
                                        app.toggle_bookmark_for_selected_model(&model);
                                    }
                                } else if app.active_tab == MainTab::Bookmarks {
                                    app.request_bookmark_remove_selected();
                                } else if app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks {
                                    if let Some(image) = app.selected_image_in_active_view().cloned() {
                                        app.toggle_bookmark_for_selected_image(&image);
                                    }
                                }
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field < 8 { app.settings_form.focused_field += 1; }
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
                                        let load_more = app
                                            .model_list_state
                                            .selected()
                                            .is_some_and(|selected| {
                                                let trigger_idx = app.models.len().saturating_sub(prefetch_threshold);
                                                selected >= trigger_idx
                                            });
                                        if load_more {
                                            if let Some((opts, next_page)) = app.next_model_search_options_if_needed() {
                                                debug_fetch_log(
                                                    &app.config,
                                                    &format!(
                                                    "UI: request more models append=true query=\"{}\" next_page={}",
                                                    opts.query.clone().unwrap_or_default()
                                                    ,
                                                    next_page.is_some()
                                                ),
                                            );
                                            if let Some(tx) = &app.tx {
                                                let _ = tx.try_send(WorkerCommand::SearchModels(
                                                    opts,
                                                    None,
                                                    None,
                                                    false,
                                                    true,
                                                    next_page,
                                                ));
                                                    app.status = "Loading more results...".to_string();
                                                }
                                            }
                                        }
                                    }
                                    if app.active_tab == MainTab::Images && app.can_request_more_images(5) {
                                        if let Some(next_page) = app.next_image_feed_page() {
                                            request_image_feed_if_needed(app, Some(next_page));
                                        }
                                    }
                                    if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                        send_cover_priority(app);
                                        send_cover_prefetch(app);
                                    }
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field > 0 { app.settings_form.focused_field -= 1; }
                                } else if app.active_tab == MainTab::Downloads {
                                    if app.active_downloads.is_empty() {
                                        app.select_previous_history();
                                    } else {
                                        app.select_previous_download();
                                    }
                                } else {
                                    app.select_previous();
                                    if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                        send_cover_priority(app);
                                        send_cover_prefetch(app);
                                    }
                                }
                            }
                            KeyCode::Left | KeyCode::Char('[') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_previous_version();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                }
                            },
                            KeyCode::Right | KeyCode::Char(']') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_next_version();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                }
                            },
                            KeyCode::Char('J') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_next_file();
                                } else if app.active_tab == MainTab::Downloads {
                                    app.select_next_history();
                                }
                            }
                            KeyCode::Char('K') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_previous_file();
                                } else if app.active_tab == MainTab::Downloads {
                                    app.select_previous_history();
                                }
                            }
                            KeyCode::Char('d') => {
                                if app.active_tab == MainTab::Downloads {
                                    if let Some(removed) = app.remove_selected_history() {
                                        let was_active = app.active_downloads.contains_key(&removed.model_id);
                                        if was_active {
                                            if let Some(tx) = &app.tx {
                                                let _ = tx.try_send(WorkerCommand::CancelDownload(removed.model_id));
                                            }
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
                            KeyCode::Char('D') => {
                                if app.active_tab == MainTab::Downloads {
                                    if let Some(removed) = app.remove_selected_history() {
                                        let was_active = app.active_downloads.contains_key(&removed.model_id);
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::CancelDownload(removed.model_id));
                                        }

                                        match removed.file_path {
                                        Some(path) => match fs::remove_file(&path) {
                                                Ok(()) => {
                                                    app.last_error = None;
                                                    app.status = format!(
                                                        "Deleted history and file for {}",
                                                        removed.model_name
                                                    );
                                                }
                                                Err(err) => {
                                                    app.last_error = Some(err.to_string());
                                                    app.show_status_modal = true;
                                                    app.status = if err.kind() == ErrorKind::NotFound {
                                                        format!("No file found for {}", removed.model_name)
                                                    } else {
                                                        format!(
                                                            "Failed to delete file for {}: {}",
                                                            removed.model_name,
                                                            err
                                                        )
                                                    };
                                                }
                                            },
                                            None => {
                                                app.last_error = None;
                                                app.status = format!("No file path recorded for {}", removed.model_name);
                                            }
                                        }

                                        if was_active {
                                            app.status = format!(
                                                "{} (and cancelled active download)",
                                                app.status
                                            );
                                        }
                                    } else {
                                        app.status = "No download history selected".into();
                                    }
                                } else {
                                    app.request_download();
                                }
                            }
                            KeyCode::Char('p') => {
                                if app.active_tab == MainTab::Downloads {
                                    if let Some(download_id) = app.selected_download_id() {
                                        if let Some(tracker) = app.active_downloads.get(&download_id) {
                                            if tracker.state == DownloadState::Running {
                                                if let Some(tx) = &app.tx {
                                                    let _ = tx.try_send(WorkerCommand::PauseDownload(download_id));
                                                }
                                            } else {
                                                if let Some(tx) = &app.tx {
                                                    let _ = tx.try_send(WorkerCommand::ResumeDownload(download_id));
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    app.request_download();
                                }
                            }
                            KeyCode::Char('c') => {
                                if app.active_tab == MainTab::Models {
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::ClearSearchCache);
                                        app.status = "Clearing cached search results...".into();
                                    }
                                } else if app.active_tab == MainTab::Downloads {
                                    if let Some(download_id) = app.selected_download_id() {
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::CancelDownload(download_id));
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('m') => { app.show_status_modal = true; }
                            KeyCode::Char('v') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.show_model_details = !app.show_model_details;
                                    app.status = if app.show_model_details {
                                        "Model details panel enabled".into()
                                    } else {
                                        "Model details panel disabled".into()
                                    };
                                }
                            }
                            KeyCode::Char('r') => {
                                if app.active_tab == MainTab::Models {
                                    if let Some(tx) = &app.tx {
                                        let selected_model_id = app.selected_model_version().map(|(model_id, _)| model_id);
                                        let selected_version_id = app.selected_model_version().map(|(_, version_id)| version_id);
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
                                        app.status = format!(
                                            "Refreshing search cache for '{}'",
                                            app.search_form.query
                                        );
                                    }
                                } else if app.active_tab == MainTab::Downloads {
                                    if let Some(entry) = app.selected_download_history_entry().cloned() {
                                        if entry.total_bytes > 0 && entry.downloaded_bytes >= entry.total_bytes {
                                            app.status = "Selected item already complete.".into();
                                            continue;
                                        }
                                        if app.active_downloads.contains_key(&entry.model_id) {
                                            app.status = "Download already active for selected model.".into();
                                            continue;
                                        }
                                        if entry.file_path.is_none() {
                                            app.status = "Selected history has no file path.".into();
                                            continue;
                                        }
                                        let has_file = entry
                                            .file_path
                                            .as_deref()
                                            .is_some_and(|path| path.exists());
                                        if !has_file {
                                            app.last_error = Some("Missing partial file".to_string());
                                            app.show_status_modal = true;
                                            app.status = "Cannot resume: partial file not found.".into();
                                            continue;
                                        }
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::ResumeDownloadModel(
                                                entry.model_id,
                                                entry.version_id,
                                                entry.file_path.clone(),
                                                entry.downloaded_bytes,
                                                entry.total_bytes,
                                            ));
                                            app.remove_history_for_session(entry.model_id, entry.version_id);
                                            app.status = format!("Resuming {} (v{})...", entry.model_name, entry.version_id);
                                        }
                                    } else {
                                        app.status = "No download history selected".into();
                                    }
                                }
                            }
                            KeyCode::Char('/') => {
                                if app.active_tab == MainTab::Models {
                                    app.mode = AppMode::SearchForm;
                                    app.search_form.begin_quick_search();
                                    app.status = "Quick search. Type query, Enter apply, Esc cancel.".into();
                                } else if app.active_tab == MainTab::Images {
                                    app.mode = AppMode::SearchImages;
                                    app.status = "Configure image search options. Press Enter to submit, Esc to cancel.".into();
                                } else if app.active_tab == MainTab::ImageBookmarks {
                                    app.begin_image_bookmark_search();
                                } else if app.active_tab == MainTab::Bookmarks {
                                    app.begin_bookmark_search();
                                }
                            }
                            KeyCode::Char('f') => {
                                if app.active_tab == MainTab::Models {
                                    app.mode = AppMode::SearchForm;
                                    app.search_form.begin_builder();
                                    app.status = "Search builder. Tab section, arrows move, Space toggle, Enter apply.".into();
                                }
                            }
                            KeyCode::Char('g') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_list_first();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                }
                            }
                            KeyCode::Char('G') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_list_last();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                }
                            }
                            KeyCode::Char('?') => {
                                if app.active_tab == MainTab::Models {
                                    app.last_error = None;
                                    app.status = "Model keys\nj/k move  g/G top/bottom  Ctrl-u/Ctrl-d jump  / quick search  f filters  v details  [ ] version  d download  b bookmark  r refresh  c clear cache".into();
                                    app.show_status_modal = true;
                                }
                            }
                            KeyCode::Char('e') => {
                                    if app.active_tab == MainTab::Bookmarks {
                                        app.begin_bookmark_export_prompt();
                                    }
                                }
                                KeyCode::Char('i') => {
                                    if app.active_tab == MainTab::Bookmarks {
                                        app.begin_bookmark_import_prompt();
                                    }
                                }
                            _ => {}
                        }

                     }
                 }
             }
             // Receiving decoded image bytes and status ticks from worker
             Some(msg) = rx.recv() => {
                 match msg {
                    AppMessage::ImagesLoaded(new_images, append, next_page) => {
                        let loaded_count = new_images.len();
                        if append {
                            let before = app.images.len();
                            app.append_image_feed_results(new_images, next_page);
                            if app.active_tab == MainTab::Images {
                                app.status = format!(
                                    "Loaded {} more images (total {})",
                                    app.images.len().saturating_sub(before),
                                    app.images.len()
                                );
                            }
                        } else {
                            app.set_image_feed_results(new_images, next_page);
                            if app.active_tab == MainTab::Images {
                                app.status = format!("Loaded {} images", app.images.len());
                            }
                        }
                        if app.status.is_empty() && app.active_tab == MainTab::Images {
                            app.status = format!("Loaded {} images", app.images.len());
                        }
                        if loaded_count == 0 {
                            if let Some(next_page) = app.next_image_feed_page() {
                                request_image_feed_if_needed(app, Some(next_page));
                            }
                        } else if app.can_request_more_images(5) {
                            if let Some(next_page) = app.next_image_feed_page() {
                                request_image_feed_if_needed(app, Some(next_page));
                            }
                        }
                    }
                     AppMessage::ImageDecoded(id, protocol) => {
                         app.image_cache.insert(id, protocol);
                     }
                     AppMessage::ModelCoverDecoded(version_id, protocol) => {
                         app.model_version_image_cache.entry(version_id).or_insert_with(|| vec![protocol]);
                         app.model_version_image_failed.remove(&version_id);
                     }
                     AppMessage::ModelCoversDecoded(version_id, protocols) => {
                         app.model_version_image_cache.insert(version_id, protocols);
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
                             app.status = format!(
                                 "Loaded {} more models (total {})",
                                 appended_len,
                                 app.models.len()
                             );
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
                             app.status = format!("Found {} models", app.models.len());
                         }
                     }
                     AppMessage::StatusUpdate(status) => {
                         app.status = status;
                         if app.status.contains("Error fetching images") {
                            app.image_feed_loading = false;
                         }
                         if is_error_status(&app.status) {
                             app.last_error = Some(app.status.clone());
                             app.show_status_modal = true;
                         } else {
                             app.last_error = None;
                             app.show_status_modal = false;
                         }
                     }
                     AppMessage::DownloadProgress(model_id, filename, progress, downloaded_bytes, total_bytes) => {
                         if let Some(existing) = app.active_downloads.get_mut(&model_id) {
                             existing.filename = filename;
                             existing.progress = progress;
                             existing.downloaded_bytes = downloaded_bytes;
                             existing.total_bytes = total_bytes;
                         }
                     }
                     AppMessage::DownloadStarted(
                        model_id,
                        filename,
                        version_id,
                        model_name,
                        total_bytes,
                        file_path,
                    ) => {
                         if !app.active_download_order.contains(&model_id) {
                             app.active_download_order.push(model_id);
                         }

                         app.active_downloads.insert(
                             model_id,
                             crate::tui::app::DownloadTracker {
                                 filename,
                                 progress: 0.0,
                                 downloaded_bytes: 0,
                                 total_bytes,
                                 file_path,
                                 model_name,
                                 version_id,
                                 state: DownloadState::Running,
                            },
                        );
                         app.status = format!("Download started for model {} ({})", model_id, version_id);
                     }
                     AppMessage::DownloadPaused(model_id) => {
                         if let Some(tracker) = app.active_downloads.get_mut(&model_id) {
                             tracker.state = DownloadState::Paused;
                             app.status = format!("Download paused: {}", tracker.filename);
                         }
                     }
                     AppMessage::DownloadResumed(model_id) => {
                         if let Some(tracker) = app.active_downloads.get_mut(&model_id) {
                             tracker.state = DownloadState::Running;
                             app.status = format!("Download resumed: {}", tracker.filename);
                         }
                     }
                     AppMessage::DownloadCompleted(model_id) => {
                         app.last_error = None;
                         if let Some(tracker) = app.active_downloads.remove(&model_id) {
                             app.push_download_history(
                                 model_id,
                                     tracker.version_id,
                                     tracker.filename,
                                     tracker.model_name,
                                     tracker.file_path,
                                     tracker.downloaded_bytes,
                                     tracker.total_bytes,
                                     DownloadHistoryStatus::Completed,
                                     tracker.progress,
                                 );
                         }
                         app.active_download_order.retain(|id| *id != model_id);
                         app.clamp_selected_download_index();
                         app.clamp_selected_history_index();
                         app.status = format!("Download complete: {}", model_id);
                     }
                     AppMessage::DownloadFailed(model_id, reason) => {
                            if let Some(tracker) = app.active_downloads.remove(&model_id) {
                            app.push_download_history(
                                 model_id,
                                 tracker.version_id,
                                 tracker.filename,
                                 tracker.model_name,
                                 tracker.file_path,
                                 tracker.downloaded_bytes,
                                 tracker.total_bytes,
                                 DownloadHistoryStatus::Failed(reason.clone()),
                                 tracker.progress,
                             );
                         }
                         app.active_download_order.retain(|id| *id != model_id);
                         app.clamp_selected_download_index();
                         app.clamp_selected_history_index();
                         app.last_error = Some(reason.clone());
                         app.show_status_modal = true;
                         app.status = format!("Download failed: {}", reason);
                     }
                    AppMessage::DownloadCancelled(model_id) => {
                         if let Some(tracker) = app.active_downloads.remove(&model_id) {
                             app.push_download_history(
                                 model_id,
                                 tracker.version_id,
                                 tracker.filename,
                                 tracker.model_name,
                                 tracker.file_path,
                                 tracker.downloaded_bytes,
                                 tracker.total_bytes,
                                 DownloadHistoryStatus::Cancelled,
                                 tracker.progress,
                             );
                         }
                         app.active_download_order.retain(|id| *id != model_id);
                         app.clamp_selected_download_index();
                         app.clamp_selected_history_index();
                         app.status = format!("Download cancelled: {}", model_id);
                     }
                 }
             }
        }
    }
    Ok(())
}

fn is_error_status(value: &str) -> bool {
    let lowered = value.to_lowercase();
    lowered.contains("error") || lowered.contains("failed") || lowered.contains("fail")
}
