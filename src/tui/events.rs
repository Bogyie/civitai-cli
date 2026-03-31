use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::fs;
use std::io::Stdout;
use std::io::ErrorKind;
use tokio::sync::mpsc;

use crate::tui::app::{App, AppMessage, AppMode, DownloadState, MainTab, WorkerCommand, DownloadHistoryStatus};
use crate::tui::ui;

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

    loop {
        let poll_timeout_ms = match app.mode {
            AppMode::SearchForm | AppMode::SearchBookmarks | AppMode::BookmarkPathPrompt => 200,
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
                                KeyCode::Up => {
                                    if app.search_form.focused_field > 0 {
                                        app.search_form.focused_field -= 1;
                                    }
                                }
                                KeyCode::Down => {
                                    if app.search_form.focused_field < 3 {
                                        app.search_form.focused_field += 1;
                                    }
                                }
                                KeyCode::Left => {
                                    match app.search_form.focused_field {
                                        1 => {
                                            if app.search_form.selected_type > 0 { app.search_form.selected_type -= 1; }
                                            else { app.search_form.selected_type = app.search_form.types.len() - 1; }
                                        }
                                        2 => {
                                            if app.search_form.selected_sort > 0 { app.search_form.selected_sort -= 1; }
                                            else { app.search_form.selected_sort = app.search_form.sorts.len() - 1; }
                                        }
                                        3 => {
                                            if app.search_form.selected_base > 0 { app.search_form.selected_base -= 1; }
                                            else { app.search_form.selected_base = app.search_form.bases.len() - 1; }
                                        }
                                        _ => {}
                                    }
                                }
                                KeyCode::Right => {
                                    match app.search_form.focused_field {
                                        1 => app.search_form.selected_type = (app.search_form.selected_type + 1) % app.search_form.types.len(),
                                        2 => app.search_form.selected_sort = (app.search_form.selected_sort + 1) % app.search_form.sorts.len(),
                                        3 => app.search_form.selected_base = (app.search_form.selected_base + 1) % app.search_form.bases.len(),
                                        _ => {}
                                    }
                                }
                                KeyCode::Enter => {
                                    app.mode = AppMode::Browsing;
                                    let selected_model_id = app.selected_model_version().map(|(model_id, _)| model_id);
                                    let selected_version_id = app.selected_model_version().map(|(_, version_id)| version_id);
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::SearchModels(
                                            app.search_form.build_options(),
                                            selected_model_id,
                                            selected_version_id,
                                            false,
                                            false,
                                        ));
                                        app.model_search_page = 1;
                                        app.model_search_has_more = true;
                                        app.model_search_loading_more = false;
                                        app.status = format!("Searching for models: '{}'...", app.search_form.query);
                                    }
                                }
                                KeyCode::Char(c) => {
                                    if app.search_form.focused_field == 0 {
                                        app.search_form.query.push(c);
                                    }
                                }
                                KeyCode::Backspace => {
                                    if app.search_form.focused_field == 0 {
                                        app.search_form.query.pop();
                                    }
                                }
                                _ => {}
                            }
                            continue; // Skip global navigation if form is active
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
                                    if app.settings_form.focused_field < 5 {
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
                                    } else if app.settings_form.focused_field == 2 {
                                        let path = if app.settings_form.input_buffer.is_empty() {
                                            None
                                        } else {
                                            Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone()))
                                        };
                                        app.config.bookmark_file_path = path.clone();
                                        app.bookmark_file_path = path;
                                    } else if app.settings_form.focused_field == 5 {
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

                        match key.code {
                            KeyCode::Tab => {
                                app.active_tab = match app.active_tab {
                                    MainTab::Models => MainTab::Bookmarks,
                                    MainTab::Bookmarks => MainTab::Images,
                                    MainTab::Images => MainTab::Downloads,
                                    MainTab::Downloads => MainTab::Settings,
                                    MainTab::Settings => MainTab::Models,
                                };
                                if app.active_tab == MainTab::Bookmarks {
                                    app.clamp_bookmark_selection();
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
                                app.active_tab = MainTab::Images;
                            }
                            KeyCode::Char('4') => {
                                app.active_tab = MainTab::Downloads;
                            }
                            KeyCode::Char('5') => {
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
                                    } else {
                                        app.config.model_search_cache_ttl_hours.to_string()
                                    };
                                } else if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.show_model_details = !app.show_model_details;
                                    app.status = if app.show_model_details {
                                        "Model details panel enabled".into()
                                    } else {
                                        "Model details panel disabled".into()
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
                                }
                            }
                                KeyCode::Char('j') | KeyCode::Down => {
                                if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field < 4 { app.settings_form.focused_field += 1; }
                                } else if app.active_tab == MainTab::Downloads {
                                    if app.active_downloads.is_empty() {
                                        app.select_next_history();
                                    } else {
                                        app.select_next_download();
                                    }
                                } else {
                                    app.select_next();
                                    if app.active_tab == MainTab::Models && app.can_request_more_models() {
                                        let load_more = app
                                            .model_list_state
                                            .selected()
                                            .is_some_and(|selected| selected + 1 >= app.models.len());
                                        if load_more {
                                            if let Some(opts) = app.next_model_search_options_if_needed() {
                                                if let Some(tx) = &app.tx {
                                                    let _ = tx.try_send(WorkerCommand::SearchModels(
                                                        opts,
                                                        None,
                                                        None,
                                                        false,
                                                        true,
                                                    ));
                                                    app.status =
                                                        format!("Loading more results (page {})...", app.model_search_page);
                                                }
                                            }
                                        }
                                    }
                                    if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                        send_cover_priority(app);
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
                                    }
                                }
                            }
                            KeyCode::Char('h') | KeyCode::Left => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_previous_version();
                                    send_cover_priority(app);
                                }
                            },
                            KeyCode::Char('l') | KeyCode::Right => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_next_version();
                                    send_cover_priority(app);
                                }
                            },
                            KeyCode::Char('J') => {
                        if app.active_tab == MainTab::Downloads {
                                    app.select_next_history();
                                }
                            }
                            KeyCode::Char('K') => {
                                if app.active_tab == MainTab::Downloads {
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
                                if app.active_tab == MainTab::Downloads {
                                    if let Some(download_id) = app.selected_download_id() {
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::CancelDownload(download_id));
                                        }
                                    }
                                } else {
                                    app.request_download();
                                }
                            }
                            KeyCode::Char('m') => { app.show_status_modal = true; }
                            KeyCode::Char('r') => {
                                if app.active_tab == MainTab::Downloads {
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
                                } else {
                                    app.request_download();
                                }
                            }
                            KeyCode::Char(' ') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.show_model_details = !app.show_model_details;
                                    app.status = if app.show_model_details {
                                        "Model details panel enabled".into()
                                    } else {
                                        "Model details panel disabled".into()
                                    };
                                }
                            }
                            KeyCode::Char('R') => {
                                if app.active_tab == MainTab::Models {
                                    if let Some(tx) = &app.tx {
                                        let selected_model_id = app.selected_model_version().map(|(model_id, _)| model_id);
                                        let selected_version_id = app.selected_model_version().map(|(_, version_id)| version_id);
                                        let _ = tx.try_send(WorkerCommand::SearchModels(
                                            app.search_form.build_options(),
                                            selected_model_id,
                                            selected_version_id,
                                            true,
                                            false,
                                        ));
                                        app.model_search_page = 1;
                                        app.model_search_has_more = true;
                                        app.model_search_loading_more = false;
                                        app.status = format!(
                                            "Refreshing search cache for '{}'",
                                            app.search_form.query
                                        );
                                    }
                                }
                            }
                            KeyCode::Char('x') | KeyCode::Char('X') => {
                                if app.active_tab == MainTab::Models {
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::ClearSearchCache);
                                        app.status = "Clearing cached search results...".into();
                                    }
                                }
                            }
                            KeyCode::Char('/') | KeyCode::Char('s') => {
                                if app.active_tab == MainTab::Models {
                                    app.mode = AppMode::SearchForm;
                                    app.status = "Configure search options. Press Enter to submit, Esc to cancel.".into();
                                } else if app.active_tab == MainTab::Bookmarks {
                                    app.begin_bookmark_search();
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
                     AppMessage::ImagesLoaded(new_images) => {
                         app.images = new_images;
                         if app.active_tab == MainTab::Images {
                             app.status = format!("Loaded {} images", app.images.len());
                         }
                     }
                     AppMessage::ImageDecoded(id, protocol) => {
                         app.image_cache.insert(id, protocol);
                     }
                     AppMessage::ModelCoverDecoded(version_id, protocol) => {
                         app.model_version_image_cache.insert(version_id, protocol);
                         app.model_version_image_failed.remove(&version_id);
                     }
                     AppMessage::ModelCoverLoadFailed(version_id) => {
                         app.model_version_image_failed.insert(version_id);
                     }
                     AppMessage::ModelsSearchedChunk(results, append, has_more) => {
                         if append {
                             let appended_len = results.len();
                             app.append_models_results(results, has_more);
                             app.status = format!(
                                 "Loaded {} more models (page {}, total {})",
                                 appended_len,
                                 app.model_search_page,
                                 app.models.len()
                             );
                         } else {
                             app.set_models_results(results, has_more);
                             send_cover_priority(app);
                             app.status = format!("Found {} models", app.models.len());
                         }
                     }
                     AppMessage::StatusUpdate(status) => {
                         app.status = status;
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
