mod modes;
mod tabs;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::Stdout;
use tokio::sync::mpsc;

use super::actions::{
    ensure_selected_image_loaded, reload_selected_image, reload_selected_model_cover,
    request_image_feed_if_needed, send_cover_prefetch, send_image_model_detail_cover_prefetch,
    send_image_model_detail_cover_priority,
};
use super::messages::handle_app_message;
use super::modals::{ModalKeyOutcome, handle_modal_key};
use crate::tui::app::{App, AppMessage, AppMode, MainTab};
use crate::tui::ui;

pub(super) enum LoopControl {
    Continue,
    Break,
}

pub async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    mut rx: mpsc::Receiver<AppMessage>,
) -> Result<()> {
    loop {
        app.sync_status_history_from_fields();

        let poll_timeout_ms = match app.mode {
            AppMode::SearchForm
            | AppMode::SearchImages
            | AppMode::SearchSavedModels
            | AppMode::SearchSavedImages
            | AppMode::BookmarkPathPrompt => 200,
            _ => 50,
        };

        terminal.draw(|f| ui::draw(f, app))?;

        tokio::select! {
            event_res = tokio::task::spawn_blocking(move || {
                event::poll(std::time::Duration::from_millis(poll_timeout_ms))
            }) => {
                if let Ok(Ok(true)) = event_res
                    && let Ok(evt) = event::read()
                {
                    if let Event::Resize(_, _) = evt {
                        handle_resize(app);
                        continue;
                    }

                    if let Event::Key(key) = evt {
                        let is_ctrl_c_exit = matches!(key.code, KeyCode::Char('c'))
                            && key.modifiers.contains(KeyModifiers::CONTROL);

                        if let Some(outcome) = handle_modal_key(app, key) {
                            match outcome {
                                ModalKeyOutcome::Consumed => continue,
                                ModalKeyOutcome::Break => break,
                            }
                        }

                        if let Some(control) = modes::handle_mode_key(app, key) {
                            match control {
                                LoopControl::Continue => continue,
                                LoopControl::Break => break,
                            }
                        }

                        if let Some(control) = handle_tab_switch_key(app, key.code) {
                            match control {
                                LoopControl::Continue => continue,
                                LoopControl::Break => break,
                            }
                        }

                        if let Some(control) = tabs::handle_modifier_key(app, key) {
                            match control {
                                LoopControl::Continue => continue,
                                LoopControl::Break => break,
                            }
                        }

                        if let Some(control) = tabs::handle_tab_key(app, key, is_ctrl_c_exit) {
                            match control {
                                LoopControl::Continue => continue,
                                LoopControl::Break => break,
                            }
                        }
                    }
                }
            }
            Some(msg) = rx.recv() => {
                handle_app_message(app, msg);
            }
        }
    }
    Ok(())
}

fn handle_resize(app: &mut App) {
    if app.show_image_model_detail_modal {
        send_image_model_detail_cover_priority(app);
        send_image_model_detail_cover_prefetch(app);
    }

    match app.active_tab {
        MainTab::Models | MainTab::SavedModels => {
            reload_selected_model_cover(app);
            send_cover_prefetch(app);
        }
        MainTab::Images | MainTab::SavedImages => {
            reload_selected_image(app);
        }
        _ => {}
    }
}

fn handle_tab_switch_key(app: &mut App, code: KeyCode) -> Option<LoopControl> {
    if app.show_status_modal
        || app.show_status_history_modal
        || app.show_help_modal
        || app.show_image_prompt_modal
        || app.show_image_tags_modal
        || app.show_image_model_detail_modal
        || app.show_bookmark_confirm_modal
        || app.show_search_template_modal
        || app.show_exit_confirm_modal
        || app.show_resume_download_modal
        || matches!(
            app.mode,
            AppMode::SearchForm
                | AppMode::SearchImages
                | AppMode::SearchSavedModels
                | AppMode::SearchSavedImages
                | AppMode::BookmarkPathPrompt
        )
        || (app.active_tab == MainTab::Settings && app.settings_form.editing)
    {
        return None;
    }

    match code {
        KeyCode::Char('1') => switch_tab(app, MainTab::Models),
        KeyCode::Char('2') => switch_tab(app, MainTab::SavedModels),
        KeyCode::Char('3') => switch_tab(app, MainTab::Images),
        KeyCode::Char('4') => switch_tab(app, MainTab::SavedImages),
        KeyCode::Char('5') => switch_tab(app, MainTab::Downloads),
        KeyCode::Char('6') => switch_tab(app, MainTab::Settings),
        KeyCode::Tab => {
            let next = match app.active_tab {
                MainTab::Models => MainTab::SavedModels,
                MainTab::SavedModels => MainTab::Images,
                MainTab::Images => MainTab::SavedImages,
                MainTab::SavedImages => MainTab::Downloads,
                MainTab::Downloads => MainTab::Settings,
                MainTab::Settings => MainTab::Models,
            };
            switch_tab(app, next);
        }
        KeyCode::BackTab => {
            let prev = match app.active_tab {
                MainTab::Models => MainTab::Settings,
                MainTab::SavedModels => MainTab::Models,
                MainTab::Images => MainTab::SavedModels,
                MainTab::SavedImages => MainTab::Images,
                MainTab::Downloads => MainTab::SavedImages,
                MainTab::Settings => MainTab::Downloads,
            };
            switch_tab(app, prev);
        }
        _ => return None,
    }

    Some(LoopControl::Continue)
}

fn switch_tab(app: &mut App, target: MainTab) {
    let prev_tab = app.active_tab;
    app.active_tab = target;
    app.mode = AppMode::Browsing;
    match app.active_tab {
        MainTab::SavedModels => app.clamp_bookmark_selection(),
        MainTab::Images => {
            if prev_tab != MainTab::Images {
                request_image_feed_if_needed(app, None);
            }
            ensure_selected_image_loaded(app);
        }
        MainTab::SavedImages => {
            app.clamp_image_bookmark_selection();
            ensure_selected_image_loaded(app);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    fn isolated_config() -> AppConfig {
        AppConfig::default()
    }

    #[test]
    fn tab_switch_is_blocked_during_search_mode() {
        let mut app = App::new(isolated_config());
        app.mode = AppMode::SearchForm;

        let result = handle_tab_switch_key(&mut app, KeyCode::Char('3'));

        assert!(result.is_none());
        assert_eq!(app.active_tab, MainTab::Models);
    }

    #[test]
    fn tab_switch_is_blocked_while_editing_settings() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Settings;
        app.settings_form.editing = true;

        let result = handle_tab_switch_key(&mut app, KeyCode::Char('2'));

        assert!(result.is_none());
        assert_eq!(app.active_tab, MainTab::Settings);
    }
}
