pub mod app;
pub mod events;
pub mod ui;
pub mod worker;

use anyhow::{Context, Result};
use app::App;
use events::run_event_loop;
use crate::config::AppConfig;

pub async fn run_tui(config: AppConfig) -> Result<()> {
    let mut app = App::new();
    
    // Spawn background worker, get back channels
    let (tx, rx) = worker::spawn_worker(config).await;
    app.set_worker_tx(tx);

    let mut terminal = ui::setup_terminal().context("setup failed")?;
    
    // Run the main UI event loop
    let result = run_event_loop(&mut terminal, &mut app, rx).await;
    
    ui::restore_terminal(&mut terminal).context("restore terminal failed")?;
    result
}
