use anyhow::{Context, Result};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use ratatui_image::StatefulImage;
use std::io::{self, Stdout};

use crate::tui::app::App;

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    let mut stdout = io::stdout();
    enable_raw_mode().context("failed to enable raw mode")?;
    execute!(stdout, EnterAlternateScreen).context("unable to enter alternate screen")?;
    Terminal::new(CrosstermBackend::new(stdout)).context("creating terminal failed")
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("unable to switch to main screen")?;
    terminal.show_cursor().context("unable to show cursor")
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),   // Main generic area
            Constraint::Length(3), // Footer Status
        ])
        .split(f.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // Image Gallery
            Constraint::Percentage(30), // Sidebar
        ])
        .split(chunks[0]);

    draw_image_panel(f, app, main_chunks[0]);
    draw_sidebar(f, app, main_chunks[1]);
    draw_footer(f, app, chunks[1]);
}

fn draw_image_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Image View ");

    if app.images.is_empty() {
        let p = Paragraph::new("Loading feed...").block(block);
        f.render_widget(p, area);
        return;
    }

    let img = &app.images[app.selected_index];

    // Render the border base first
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Check if we have the protocol rendered in our cache
    if let Some(protocol) = app.image_cache.get_mut(&img.id) {
        let image_widget = StatefulImage::new();
        f.render_stateful_widget(image_widget, inner_area, protocol);
    } else {
        let text = format!("Decoding image {}/{}...", app.selected_index + 1, app.images.len());
        let p = Paragraph::new(text);
        f.render_widget(p, inner_area);
    }
}

fn draw_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Image Metadata ");

    if let Some(img) = app.images.get(app.selected_index) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(img.id.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Hash: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&img.hash),
            ]),
        ];

        if let Some(meta) = &img.meta {
            if let Some(prompt) = meta.get("prompt").and_then(|v| v.as_str()) {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled("Prompt:", Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))));
                lines.push(Line::from(prompt));
            }
            if let Some(negative) = meta.get("negativePrompt").and_then(|v| v.as_str()) {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled("Negative Prompt:", Style::default().add_modifier(Modifier::BOLD).fg(Color::Red))));
                lines.push(Line::from(negative));
            }
            if let Some(sampler) = meta.get("sampler").and_then(|v| v.as_str()) {
                lines.push(Line::from(""));
                lines.push(Line::from(format!("Sampler: {}", sampler)));
            }
        }

        let p = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: true });
        f.render_widget(p, area);
    } else {
        let p = Paragraph::new("No metadata available.").block(block);
        f.render_widget(p, area);
    }
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let info = format!(" | Selected: {}/{} | Status: {}",
        if app.images.is_empty() { 0 } else { app.selected_index + 1 },
        app.images.len(),
        app.status
    );

    let text = vec![Line::from(vec![
        Span::styled(" q: Quit | j/↓: Next | k/↑: Prev | d: Download Model ", Style::default().fg(Color::Cyan)),
        Span::raw(info),
    ])];

    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}
