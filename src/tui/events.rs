use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use tokio::sync::mpsc;

use crate::tui::app::{App, AppMessage, AppMode, MainTab, WorkerCommand};
use crate::tui::ui;

pub async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    mut rx: mpsc::Receiver<AppMessage>,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Wait for either terminal input or worker message update
        tokio::select! {
             // Polling keypresses
             event_res = tokio::task::spawn_blocking(|| event::poll(std::time::Duration::from_millis(50))) => {
                 if let Ok(Ok(true)) = event_res {
                     if let Ok(Event::Key(key)) = event::read() {
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
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::SearchModels(app.search_form.build_options()));
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

                        if app.show_status_modal {
                            match key.code {
                                KeyCode::Char('m') | KeyCode::Esc | KeyCode::Enter => {
                                    app.show_status_modal = false;
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.active_tab == MainTab::Settings && app.settings_form.editing {
                            match key.code {
                                KeyCode::Esc => {
                                    app.settings_form.editing = false;
                                }
                                KeyCode::Enter => {
                                    if app.settings_form.focused_field == 0 {
                                        app.config.api_key = if app.settings_form.input_buffer.is_empty() { None } else { Some(app.settings_form.input_buffer.clone()) };
                                    } else {
                                        app.config.comfyui_path = if app.settings_form.input_buffer.is_empty() { None } else { Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone())) };
                                    }
                                    if let Err(e) = app.config.save() {
                                        app.last_error = Some(format!("Failed to save config: {}", e));
                                    } else {
                                        app.last_error = None;
                                        app.status = "Settings saved to config.json".into();
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
                                    MainTab::Models => MainTab::Images,
                                    MainTab::Images => MainTab::Downloads,
                                    MainTab::Downloads => MainTab::Settings,
                                    MainTab::Settings => MainTab::Models,
                                };
                            }
                            KeyCode::Char('1') => app.active_tab = MainTab::Models,
                            KeyCode::Char('2') => app.active_tab = MainTab::Images,
                            KeyCode::Char('3') => app.active_tab = MainTab::Downloads,
                            KeyCode::Char('4') => app.active_tab = MainTab::Settings,
                            KeyCode::Char('q') | KeyCode::Esc => {
                                if let Some(tx) = &app.tx {
                                    let _ = tx.try_send(WorkerCommand::Quit);
                                }
                                break;
                            }
                            KeyCode::Enter => {
                                if app.active_tab == MainTab::Settings {
                                    app.settings_form.editing = true;
                                    app.settings_form.input_buffer = if app.settings_form.focused_field == 0 {
                                        app.config.api_key.clone().unwrap_or_default()
                                    } else {
                                        app.config.comfyui_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
                                    };
                                }
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field < 1 { app.settings_form.focused_field += 1; }
                                } else {
                                    app.select_next();
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field > 0 { app.settings_form.focused_field -= 1; }
                                } else {
                                    app.select_previous();
                                }
                            }
                            KeyCode::Char('h') | KeyCode::Left => app.select_previous_version(),
                            KeyCode::Char('l') | KeyCode::Right => app.select_next_version(),
                            KeyCode::Char('d') => app.request_download(),
                            KeyCode::Char('m') => { app.show_status_modal = true; }
                            KeyCode::Char('/') | KeyCode::Char('s') => {
                                if app.active_tab == MainTab::Models {
                                    app.mode = AppMode::SearchForm;
                                    app.status = "Configure search options. Press Enter to submit, Esc to cancel.".into();
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
                     AppMessage::ModelCoverDecoded(id, protocol) => {
                         app.model_image_cache.insert(id, protocol);
                     }
                     AppMessage::ModelsSearched(results) => {
                         app.models = results;
                         app.model_list_state.select(Some(0));
                         app.status = format!("Found {} models", app.models.len());
                     }
                     AppMessage::StatusUpdate(status) => {
                         app.status = status;
                     }
                     AppMessage::DownloadProgress(model_id, filename, progress) => {
                         if progress >= 100.0 {
                             app.active_downloads.remove(&model_id);
                             app.status = format!("Finished downloading {}", filename);
                         } else {
                             app.active_downloads.insert(model_id, crate::tui::app::DownloadTracker { filename, progress });
                         }
                     }
                 }
             }
        }
    }
    Ok(())
}
