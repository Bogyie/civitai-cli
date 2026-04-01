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
        let poll_timeout_ms = match app.mode {
            AppMode::SearchForm
            | AppMode::SearchImages
            | AppMode::SearchBookmarks
            | AppMode::SearchImageBookmarks
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

                        if let Some(control) = handle_tab_switch_key(app, key.code) {
                            match control {
                                LoopControl::Continue => continue,
                                LoopControl::Break => break,
                            }
                        }

                        if let Some(control) = modes::handle_mode_key(app, key) {
                            match control {
                                LoopControl::Continue => continue,
                                LoopControl::Break => break,
                            }
                        }

                        if let Some(outcome) = handle_modal_key(app, key) {
                            match outcome {
                                ModalKeyOutcome::Consumed => continue,
                                ModalKeyOutcome::Break => break,
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
        MainTab::Models | MainTab::Bookmarks => {
            reload_selected_model_cover(app);
            send_cover_prefetch(app);
        }
        MainTab::Images | MainTab::ImageBookmarks => {
            reload_selected_image(app);
        }
        _ => {}
    }
}

fn handle_tab_switch_key(app: &mut App, code: KeyCode) -> Option<LoopControl> {
    if app.show_status_modal
        || app.show_help_modal
        || app.show_image_prompt_modal
        || app.show_image_model_detail_modal
        || app.show_bookmark_confirm_modal
        || app.show_exit_confirm_modal
    {
        return None;
    }

    match code {
        KeyCode::Char('1') => switch_tab(app, MainTab::Models),
        KeyCode::Char('2') => switch_tab(app, MainTab::Bookmarks),
        KeyCode::Char('3') => switch_tab(app, MainTab::Images),
        KeyCode::Char('4') => switch_tab(app, MainTab::ImageBookmarks),
        KeyCode::Char('5') => switch_tab(app, MainTab::Downloads),
        KeyCode::Char('6') => switch_tab(app, MainTab::Settings),
        KeyCode::Tab => {
            let next = match app.active_tab {
                MainTab::Models => MainTab::Bookmarks,
                MainTab::Bookmarks => MainTab::Images,
                MainTab::Images => MainTab::ImageBookmarks,
                MainTab::ImageBookmarks => MainTab::Downloads,
                MainTab::Downloads => MainTab::Settings,
                MainTab::Settings => MainTab::Models,
            };
            switch_tab(app, next);
        }
        KeyCode::BackTab => {
            let prev = match app.active_tab {
                MainTab::Models => MainTab::Settings,
                MainTab::Bookmarks => MainTab::Models,
                MainTab::Images => MainTab::Bookmarks,
                MainTab::ImageBookmarks => MainTab::Images,
                MainTab::Downloads => MainTab::ImageBookmarks,
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
        MainTab::Bookmarks => app.clamp_bookmark_selection(),
        MainTab::Images => {
            if prev_tab != MainTab::Images {
                request_image_feed_if_needed(app, None);
            }
            ensure_selected_image_loaded(app);
        }
        MainTab::ImageBookmarks => {
            app.clamp_image_bookmark_selection();
            ensure_selected_image_loaded(app);
        }
        _ => {}
    }
}
