use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use tokio::sync::mpsc;

use crate::tui::app::{App, AppMessage, AppMode, WorkerCommand};
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
                        match app.mode {
                            AppMode::SearchForm => {
                                match key.code {
                                    KeyCode::Esc => {
                                        app.mode = AppMode::ModelResults;
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
                                        app.mode = AppMode::ModelResults;
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
                            }
                            AppMode::ImageFeed | AppMode::ModelResults => {
                                match key.code {
                                    KeyCode::Char('q') | KeyCode::Esc => {
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::Quit);
                                        }
                                        break;
                                    }
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        app.select_next();
                                    }
                                    KeyCode::Char('k') | KeyCode::Up => {
                                        app.select_previous();
                                    }
                                    KeyCode::Char('d') => {
                                        app.request_download();
                                    }
                                    KeyCode::Char('/') | KeyCode::Char('s') => {
                                        app.mode = AppMode::SearchForm;
                                        app.status = "Configure search options. Press Enter to submit, Esc to cancel.".into();
                                    }
                                    KeyCode::Char('i') => {
                                        app.mode = AppMode::ImageFeed;
                                    }
                                    _ => {}
                                }
                            }
                        }
                     }
                 }
             }
             // Receiving decoded image bytes and status ticks from worker
             Some(msg) = rx.recv() => {
                 match msg {
                     AppMessage::ImagesLoaded(new_images) => {
                         app.images = new_images;
                         if app.mode == AppMode::ImageFeed {
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
                         if app.mode == AppMode::ModelResults {
                             app.status = format!("Found {} models", app.models.len());
                         }
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
