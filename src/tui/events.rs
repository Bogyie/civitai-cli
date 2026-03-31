use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use tokio::sync::mpsc;

use crate::tui::app::{App, AppMessage, WorkerCommand};
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
                         app.status = format!("Loaded {} images", app.images.len());
                     }
                     AppMessage::ImageDecoded(id, protocol) => {
                         app.image_cache.insert(id, protocol);
                     }
                     AppMessage::StatusUpdate(status) => {
                         app.status = status;
                     }
                 }
             }
        }
    }
    Ok(())
}
