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
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};
use ratatui_image::StatefulImage;
use std::io::{self, Stdout};

use crate::tui::app::{App, AppMode, MainTab};

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
            Constraint::Length(3), // Tabs
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Footer Status
        ])
        .split(f.area());

    let titles = vec![
        " Models (1) ",
        " Image Feed (2) ",
        " Downloads (3) ",
        " Settings (4) ",
    ];
    let active_idx = match app.active_tab {
        MainTab::Models => 0,
        MainTab::Images => 1,
        MainTab::Downloads => 2,
        MainTab::Settings => 3,
    };
    
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Civitai CLI "))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .select(active_idx)
        .divider(" | ");
    
    f.render_widget(tabs, chunks[0]);

    match app.active_tab {
        MainTab::Models => {
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);
            draw_model_list(f, app, main_chunks[0]);
            draw_model_sidebar(f, app, main_chunks[1]);
            
            if app.mode == AppMode::SearchForm {
                draw_search_popup(f, app);
            }
        }
        MainTab::Images => {
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);
            draw_image_panel(f, app, main_chunks[0]);
            draw_image_sidebar(f, app, main_chunks[1]);
        }
        MainTab::Downloads => {
            draw_downloads_tab(f, app, chunks[1]);
        }
        MainTab::Settings => {
            draw_settings_tab(f, app, chunks[1]);
        }
    }

    draw_footer(f, app, chunks[2]);

    if app.show_status_modal {
        draw_status_modal(f, app);
    }
}

fn draw_downloads_tab(f: &mut Frame, app: &App, area: Rect) {
    if app.active_downloads.is_empty() {
        let p = Paragraph::new("No active downloads.").alignment(Alignment::Center).block(Block::default().borders(Borders::ALL).title(" Downloads "));
        f.render_widget(p, area);
        return;
    }

    let block = Block::default().borders(Borders::ALL).title(" Downloads ");
    f.render_widget(block.clone(), area);
    let inner_area = block.inner(area);

    let constraints = std::iter::repeat(Constraint::Length(1))
        .take(app.active_downloads.len())
        .collect::<Vec<_>>();
    let sub_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    for (i, (&id, tracker)) in app.active_downloads.iter().enumerate() {
        if i < sub_chunks.len() {
            let ratio = (tracker.progress / 100.0).clamp(0.0, 1.0);
            let label = format!("Model {}: {} ({:.1}%)", id, tracker.filename, tracker.progress);
            let gauge = Gauge::default()
                .block(Block::default())
                .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
                .ratio(ratio)
                .label(label);
            f.render_widget(gauge, sub_chunks[i]);
        }
    }
}

fn draw_settings_tab(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Settings ");
    let fm = &app.settings_form;
    
    let mut lines = vec![
        Line::from(Span::styled("--- Civitai CLI Configuration ---", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
    ];

    let api_key_val = if fm.editing && fm.focused_field == 0 {
        format!("{}█", fm.input_buffer)
    } else if let Some(key) = &app.config.api_key {
        format!("Present (starts with {})", &key.chars().take(5).collect::<String>())
    } else {
        "None (Restricted search and downloads)".to_string()
    };

    lines.push(Line::from(vec![
        Span::styled(if fm.focused_field == 0 { "> API Key: " } else { "  API Key: " }, Style::default().fg(if fm.focused_field == 0 { Color::Yellow } else { Color::White })),
        Span::styled(api_key_val, if fm.focused_field == 0 && fm.editing { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Cyan) })
    ]));

    let path_val = if fm.editing && fm.focused_field == 1 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config.comfyui_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(if fm.focused_field == 1 { "> ComfyUI Path: " } else { "  ComfyUI Path: " }, Style::default().fg(if fm.focused_field == 1 { Color::Yellow } else { Color::White })),
        Span::styled(path_val, if fm.focused_field == 1 && fm.editing { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Cyan) })
    ]));

    lines.push(Line::from(""));
    if fm.editing {
        lines.push(Line::from(Span::styled(" [Type to edit] | [Enter] Save | [Esc] Cancel", Style::default().fg(Color::DarkGray))));
    } else {
        lines.push(Line::from(Span::styled(" [Up/Down] Highlight | [Enter] Edit string", Style::default().fg(Color::DarkGray))));
    }

    f.render_widget(Paragraph::new(lines).block(block), area);
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

        let v_idx = *app.selected_version_index.get(&model.id).unwrap_or(&0);
        if let Some(version) = model.model_versions.get(v_idx) {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(format!("Version: {} ", version.name), Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow)),
                Span::styled(format!("({} of {})", v_idx + 1, model.model_versions.len()), Style::default().fg(Color::DarkGray)),
            ]));
            
            lines.push(Line::from(format!("Base Model: {}", version.base_model)));

            if let Some(stats) = &version.stats {
                lines.push(Line::from(vec![
                    Span::styled("Ver. Stats: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!("⭐ {:.1} ({}) | ⬇️ {}", stats.rating, stats.rating_count, stats.download_count)),
                ]));
            }

            if let Some(file) = version.files.first() {
                lines.push(Line::from(format!("File: {} ({:.2} MB)", file.name, file.size_kb / 1024.0)));
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

fn draw_status_modal(f: &mut Frame, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Full Status Message ");
        
    let err_msg = app.last_error.as_deref().unwrap_or("");
    let full_text = if !err_msg.is_empty() {
        format!("{}\n\nERROR:\n{}", app.status, err_msg)
    } else {
        app.status.clone()
    };

    let text = vec![
        Line::from(full_text),
        Line::from(""),
        Line::from(Span::styled(" [m] Close | [Esc] Close ", Style::default().add_modifier(Modifier::BOLD).fg(Color::DarkGray))),
    ];

    let p = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    let area = centered_rect(80, 60, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL);
    let err_msg = app.last_error.as_deref().unwrap_or("");
    
    let left_width_budget = area.width.saturating_sub(40) as usize; // Account for menus and error text footprint
    
    let mut trunc_status = app.status.clone();
    if trunc_status.chars().count() > left_width_budget {
        trunc_status = format!("{}...", trunc_status.chars().take(left_width_budget.saturating_sub(3)).collect::<String>());
    }
    
    let left_text = match app.active_tab {
        MainTab::Models => {
            let m_len = app.models.len();
            let m_curr = app.model_list_state.selected().unwrap_or(0);
            format!(" Models: {}/{} | {} | [m] Modal | [h/l] Version | [j/k] Browse | [d] DL ", if m_len == 0 { 0 } else { m_curr + 1 }, m_len, trunc_status)
        }
        MainTab::Images => {
            let i_len = app.images.len();
            let i_curr = app.selected_index;
            format!(" Images: {}/{} | {} | [m] Modal", if i_len == 0 { 0 } else { i_curr + 1 }, i_len, trunc_status)
        }
        _ => format!(" {} | [m] Modal (Press 1..4 or Tab to Navigate) ", trunc_status),
    };

    let p = if !err_msg.is_empty() {
        Paragraph::new(Line::from(vec![
            Span::raw(left_text),
            Span::styled(format!(" | ERROR: {}", err_msg), Style::default().fg(Color::Red)),
        ])).block(block)
    } else {
        Paragraph::new(Span::styled(left_text, Style::default().fg(Color::Cyan))).block(block)
    };

    f.render_widget(p, area);
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
