use anyhow::{Context, Result};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use ratatui_image::StatefulImage;
use std::io::{self, Stdout};

use crate::tui::app::{App, AppMode};

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
    let mut root_constraints = vec![Constraint::Min(10)];
    let has_downloads = !app.active_downloads.is_empty();
    
    if has_downloads {
        root_constraints.push(Constraint::Length((app.active_downloads.len() + 2) as u16));
    }
    root_constraints.push(Constraint::Length(3)); // Footer Status
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(root_constraints)
        .split(f.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Left view (List / Feed)
            Constraint::Percentage(50), // Right view (Details & Images)
        ])
        .split(chunks[0]);

    if app.mode == AppMode::ImageFeed {
        draw_image_panel(f, app, main_chunks[0]);
        draw_image_sidebar(f, app, main_chunks[1]);
    } else {
        draw_model_list(f, app, main_chunks[0]);
        draw_model_sidebar(f, app, main_chunks[1]);
    }
    
    // Draw downloads tracker if present
    if has_downloads {
        draw_downloads_tracker(f, app, chunks[1]);
        draw_footer(f, app, chunks[2]);
    } else {
        draw_footer(f, app, chunks[1]);
    }

    if app.mode == AppMode::SearchForm {
        draw_search_popup(f, app);
    }
}

fn draw_image_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Image View ");

    if app.images.is_empty() {
        f.render_widget(Paragraph::new("Loading feed...").block(block), area);
        return;
    }

    let img = &app.images[app.selected_index];
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if let Some(protocol) = app.image_cache.get_mut(&img.id) {
        let image_widget = StatefulImage::new();
        f.render_stateful_widget(image_widget, inner_area, protocol);
    } else {
        let text = format!("Decoding image {}/{}...", app.selected_index + 1, app.images.len());
        f.render_widget(Paragraph::new(text), inner_area);
    }
}

fn draw_image_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Metadata ");

    if let Some(img) = app.images.get(app.selected_index) {
        let mut lines = vec![
            Line::from(vec![Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(img.id.to_string())]),
            Line::from(vec![Span::styled("Hash: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(&img.hash)]),
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
        }
        f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: true }), area);
    } else {
        f.render_widget(Paragraph::new("No metadata available.").block(block), area);
    }
}

fn draw_model_list(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Searched Models ");

    if app.models.is_empty() {
        f.render_widget(Paragraph::new("No models found. Press '/' to search.").block(block), area);
        return;
    }

    let items: Vec<ListItem> = app.models.iter().map(|m| {
        let mut tokens_line = vec![
            Span::styled(format!("[{}] ", m.id), Style::default().fg(Color::DarkGray)),
            Span::styled(m.name.clone(), Style::default().fg(if m.nsfw { Color::Red } else { Color::Green })),
        ];

        if let Some(stats) = &m.stats {
            tokens_line.push(Span::raw(format!(" (DLs: {})", stats.download_count)));
        }

        ListItem::new(Line::from(tokens_line))
    }).collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");
    f.render_stateful_widget(list, area, &mut app.model_list_state);
}

fn draw_model_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // Text details
            Constraint::Percentage(50), // Graphic Thumbnail
        ])
        .split(area);

    let block = Block::default().borders(Borders::ALL).title(" Model Details ");

    let selected_idx = app.model_list_state.selected().unwrap_or(0);
    if let Some(model) = app.models.get(selected_idx) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled(&model.name, Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan)),
                Span::raw(if model.nsfw { " [NSFW]" } else { " [SFW]" }),
            ]),
            Line::from(format!("Type: {}", model.r#type)),
        ];

        if let Some(stats) = &model.stats {
            lines.push(Line::from(vec![
                Span::styled("Stats: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("⭐ {:.1} ({}) | ❤️ {} | ⬇️ {}", stats.rating, stats.rating_count, stats.favorite_count, stats.download_count)),
            ]));
        }

        // Safe file parsing
        if let Some(version) = model.model_versions.first() {
            lines.push(Line::from(format!("Base Model: {}", version.base_model)));
            if let Some(file) = version.files.first() {
                lines.push(Line::from(format!("File Size: {:.2} MB", file.size_kb / 1024.0)));
                if let Some(meta) = &file.metadata {
                    lines.push(Line::from(format!("Format: {} {}", 
                        meta.format.as_deref().unwrap_or("?"), 
                        meta.fp.as_deref().unwrap_or("?")
                    )));
                }
            }
        }

        if let Some(desc) = &model.description {
            lines.push(Line::from(""));
            let plain_text = html2text::from_read(desc.as_bytes(), 80).unwrap_or_else(|_| desc.clone());
            for subline in plain_text.lines() {
                lines.push(Line::from(subline.to_string()));
            }
        }

        f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), split[0]);

        // Draw model cover logic
        let img_block = Block::default().borders(Borders::ALL).title(" Cover Image ");
        let inner_img_area = img_block.inner(split[1]);
        f.render_widget(img_block, split[1]);

        if let Some(protocol) = app.model_image_cache.get_mut(&model.id) {
            let image_widget = StatefulImage::new();
            f.render_stateful_widget(image_widget, inner_img_area, protocol);
        } else {
            f.render_widget(Paragraph::new("Loading thumbnail...").alignment(Alignment::Center), inner_img_area);
        }

    } else {
        f.render_widget(Paragraph::new("Select a model.").block(block), area);
    }
}

fn draw_downloads_tracker(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Active Downloads ");
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(1); app.active_downloads.len()])
        .split(block.inner(area));

    f.render_widget(block, area);

    for (i, dl) in app.active_downloads.values().enumerate() {
        if i >= layout.len() { break; } // safety bounds

        let label = format!("{} ({:.1}%)", dl.filename, dl.progress);
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
            .ratio((dl.progress / 100.0).clamp(0.0, 1.0))
            .label(label);
        
        f.render_widget(gauge, layout[i]);
    }
}

fn draw_search_popup(f: &mut Frame, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Search Filter Options ");
    
    let fm = &app.search_form;

    let list = vec![
        Line::from(vec![
            Span::styled(if fm.focused_field == 0 { "> Query: " } else { "  Query: " }, Style::default().fg(if fm.focused_field == 0 { Color::Yellow } else { Color::White })),
            Span::raw(format!("{}█", fm.query)),
        ]),
        Line::from(vec![
            Span::styled(if fm.focused_field == 1 { "> Type: " } else { "  Type: " }, Style::default().fg(if fm.focused_field == 1 { Color::Yellow } else { Color::White })),
            Span::raw(format!("< {} >", fm.types[fm.selected_type])),
        ]),
        Line::from(vec![
            Span::styled(if fm.focused_field == 2 { "> Sort: " } else { "  Sort: " }, Style::default().fg(if fm.focused_field == 2 { Color::Yellow } else { Color::White })),
            Span::raw(format!("< {} >", fm.sorts[fm.selected_sort])),
        ]),
        Line::from(vec![
            Span::styled(if fm.focused_field == 3 { "> Base: " } else { "  Base: " }, Style::default().fg(if fm.focused_field == 3 { Color::Yellow } else { Color::White })),
            Span::raw(format!("< {} >", fm.bases[fm.selected_base])),
        ]),
        Line::from(""),
        Line::from(Span::styled(" [Up/Down] Select Field | [Left/Right] Cycle Options | [Enter] Search", Style::default().fg(Color::DarkGray))),
    ];

    let p = Paragraph::new(list).block(block);

    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let info = if app.mode == AppMode::ImageFeed {
        format!("Image: {}/{} | Status: {}",
            if app.images.is_empty() { 0 } else { app.selected_index + 1 },
            app.images.len(),
            app.status)
    } else {
        format!("Model: {}/{} | Status: {}",
            if app.models.is_empty() { 0 } else { app.model_list_state.selected().unwrap_or(0) + 1 },
            app.models.len(),
            app.status)
    };

    let text = vec![
        Line::from(Span::styled(" [i] Image Feed | [/] Search | [q] Quit | [j/k] Navigate | [d] Download ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(Span::raw(info)),
    ];

    f.render_widget(Paragraph::new(text).block(Block::default().borders(Borders::ALL)), area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
