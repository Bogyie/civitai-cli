use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::fs;
use std::io::ErrorKind;

use crate::tui::app::{App, WorkerCommand};

use super::actions::{
    send_image_model_detail_cover_prefetch, send_image_model_detail_cover_priority,
};
use super::artifacts::copy_to_clipboard;

pub(super) enum ModalKeyOutcome {
    Consumed,
    Break,
}

pub(super) fn handle_modal_key(app: &mut App, key: KeyEvent) -> Option<ModalKeyOutcome> {
    if app.show_status_modal {
        if matches!(key.code, KeyCode::Char('m') | KeyCode::Esc | KeyCode::Enter) {
            app.show_status_modal = false;
        }
        return Some(ModalKeyOutcome::Consumed);
    }

    if app.show_status_history_modal {
        match key.code {
            KeyCode::Char('M') | KeyCode::Esc | KeyCode::Enter => {
                app.close_status_history_modal();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.select_next_status_history();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.select_previous_status_history();
            }
            KeyCode::Char('g') => {
                app.select_first_status_history();
            }
            KeyCode::Char('G') => {
                app.select_last_status_history();
            }
            KeyCode::Char('y') => {
                if let Some(message) = app.selected_status_history_entry().map(|entry| entry.message.clone()) {
                    match copy_to_clipboard(&message) {
                        Ok(()) => {
                            app.status = "Copied status history message".into();
                            app.last_error = None;
                        }
                        Err(err) => {
                            app.last_error = Some(err.to_string());
                            app.show_status_modal = true;
                            app.status = "Failed to copy status history message".into();
                        }
                    }
                }
            }
            _ => {}
        }
        return Some(ModalKeyOutcome::Consumed);
    }

    if app.show_help_modal {
        if matches!(key.code, KeyCode::Char('?') | KeyCode::Esc | KeyCode::Enter) {
            app.show_help_modal = false;
        }
        return Some(ModalKeyOutcome::Consumed);
    }

    if app.show_image_prompt_modal {
        match key.code {
            KeyCode::Char('m') | KeyCode::Esc | KeyCode::Enter => {
                app.show_image_prompt_modal = false;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.image_prompt_scroll = app.image_prompt_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.image_prompt_scroll = app.image_prompt_scroll.saturating_sub(1);
            }
            _ => {}
        }
        return Some(ModalKeyOutcome::Consumed);
    }

    if app.show_image_model_detail_modal {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                app.close_image_model_detail_modal();
            }
            KeyCode::Char('b') => {
                if let Some(model) = app.image_model_detail_model.clone() {
                    app.toggle_bookmark_for_selected_model(&model);
                }
            }
            KeyCode::Char('d') => {
                if let Some(model) = app.image_model_detail_model.clone() {
                    app.request_download_for_model(&model);
                }
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('[') => {
                if let Some(model) = app.image_model_detail_model.clone() {
                    app.select_previous_version_for_model(&model);
                    send_image_model_detail_cover_priority(app);
                    send_image_model_detail_cover_prefetch(app);
                }
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(']') => {
                if let Some(model) = app.image_model_detail_model.clone() {
                    app.select_next_version_for_model(&model);
                    send_image_model_detail_cover_priority(app);
                    send_image_model_detail_cover_prefetch(app);
                }
            }
            KeyCode::Char('K') => {
                if let Some(model) = app.image_model_detail_model.clone() {
                    app.select_previous_file_for_model(&model);
                }
            }
            KeyCode::Char('J') => {
                if let Some(model) = app.image_model_detail_model.clone() {
                    app.select_next_file_for_model(&model);
                }
            }
            KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                if let Some(model) = app.image_model_detail_model.clone() {
                    app.select_previous_file_for_model(&model);
                }
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
                if let Some(model) = app.image_model_detail_model.clone() {
                    app.select_next_file_for_model(&model);
                }
            }
            _ => {}
        }
        return Some(ModalKeyOutcome::Consumed);
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
        return Some(ModalKeyOutcome::Consumed);
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
                                app.last_error = Some(format!("Failed to delete file: {}", err));
                                app.show_status_modal = true;
                                app.status =
                                    format!("Failed to delete file for {}", session.filename);
                            }
                        }
                    }
                    let _ = app.remove_history_for_session(
                        session.model_id,
                        session.version_id,
                        &session.filename,
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
        return Some(ModalKeyOutcome::Consumed);
    }

    if app.show_exit_confirm_modal {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let sessions = app.collect_interrupt_sessions_from_active();
                for session in &sessions {
                    if let Some(download_key) = app.active_download_key_for_session(session)
                        && let Some(tx) = &app.tx
                    {
                        let _ = tx.try_send(WorkerCommand::PauseDownload(download_key));
                    }
                    app.record_interrupted_session_to_history(session);
                }
                app.interrupted_download_sessions = sessions;
                app.persist_interrupted_downloads();
                if let Some(tx) = &app.tx {
                    let _ = tx.try_send(WorkerCommand::Quit);
                }
                app.show_exit_confirm_modal = false;
                return Some(ModalKeyOutcome::Break);
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                let sessions = app.collect_interrupt_sessions_from_active();
                for session in &sessions {
                    if let Some(download_key) = app.active_download_key_for_session(session)
                        && let Some(tx) = &app.tx
                    {
                        let _ = tx.try_send(WorkerCommand::CancelDownload(download_key));
                    }
                    if let Some(path) = session.file_path.clone() {
                        let _ = fs::remove_file(&path);
                    }
                    app.remove_history_for_session(
                        session.model_id,
                        session.version_id,
                        &session.filename,
                    );
                }
                app.interrupted_download_sessions.clear();
                app.persist_interrupted_downloads();
                if let Some(tx) = &app.tx {
                    let _ = tx.try_send(WorkerCommand::Quit);
                }
                app.show_exit_confirm_modal = false;
                return Some(ModalKeyOutcome::Break);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.cancel_exit_confirm_modal();
            }
            _ => {}
        }
        return Some(ModalKeyOutcome::Consumed);
    }

    None
}
