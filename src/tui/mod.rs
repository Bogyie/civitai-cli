pub mod app;
pub mod events;
pub mod image;
pub mod model;
mod runtime;
pub mod status;
pub mod ui;
pub mod worker;

use crate::config::AppConfig;
use anyhow::{Context, Result};
use app::App;
use events::run_event_loop;

pub async fn run_tui(config: AppConfig) -> Result<()> {
    let mut app = App::new(config.clone());

    // Spawn background worker, get back channels
    let (tx, rx) = worker::spawn_worker(config).await;
    app.set_worker_tx(tx);
    if let Some(tx) = app.tx.as_ref() {
        let _ = tx
            .send(app::WorkerCommand::SearchModels(
                app.search_form.build_options(),
                None,
                None,
                false,
                false,
                None,
            ))
            .await;
    }
    app.set_status("Searching default model list...");

    let mut terminal = ui::setup_terminal().context("setup failed")?;

    // Run the main UI event loop
    let result = run_event_loop(&mut terminal, &mut app, rx).await;

    ui::restore_terminal(&mut terminal).context("restore terminal failed")?;
    result
}
