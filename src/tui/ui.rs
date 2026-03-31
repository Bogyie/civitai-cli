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
    widgets::{Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, Tabs, Wrap},
    Frame, Terminal,
};
use ratatui_image::{protocol::StatefulProtocol, StatefulImage};
use std::io::{self, Stdout};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::tui::app::{App, AppMode, DownloadState, DownloadHistoryStatus, MainTab};

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
            Constraint::Length(2), // Footer Status
        ])
        .split(f.area());

    let titles = vec![
        " Models (1) ",
        " Bookmarks (2) ",
        " Image Feed (3) ",
        " Image Bookmarks (4) ",
        " Downloads (5) ",
        " Settings (6) ",
    ];
    let active_idx = match app.active_tab {
        MainTab::Models => 0,
        MainTab::Bookmarks => 1,
        MainTab::Images => 2,
        MainTab::ImageBookmarks => 3,
        MainTab::Downloads => 4,
        MainTab::Settings => 5,
    };
    let enable_name_rolling = !matches!(
        app.mode,
        AppMode::SearchForm
            | AppMode::SearchImages
            | AppMode::SearchBookmarks
            | AppMode::SearchImageBookmarks
            | AppMode::BookmarkPathPrompt
    );
    
    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Civitai CLI | [1-6] Switch tab | Tab: cycle tabs "),
        )
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .select(active_idx)
        .divider(" | ");
    
    f.render_widget(tabs, chunks[0]);

    match app.active_tab {
        MainTab::Models => {
            let model_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(chunks[1]);

            draw_model_search_summary(f, app, model_chunks[0]);
            let show_model_details = app.show_model_details;
            let selected_model = app.selected_model_in_active_view().cloned();
            let bookmarked_ids: Vec<u64> = app.bookmarks.iter().map(|model| model.id).collect();

            if app.show_model_details {
                let split = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
                    .split(model_chunks[1]);

                draw_model_list(
                    f,
                    split[0],
                    &app.models,
                    &app.model_list_state,
                    show_model_details,
                    &bookmarked_ids,
                    enable_name_rolling,
                );
                draw_model_sidebar(f, app, split[1], selected_model.as_ref());
            } else {
                draw_model_list(
                    f,
                    model_chunks[1],
                    &app.models,
                    &app.model_list_state,
                    show_model_details,
                    &bookmarked_ids,
                    enable_name_rolling,
                );
            }
            
            if app.mode == AppMode::SearchForm {
                draw_search_popup(f, app);
            }
        }
        MainTab::Bookmarks => {
            let bookmark_items = app.visible_bookmarks();
            let bookmark_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(chunks[1]);
            let selected_bookmark_model = app.selected_model_in_active_view().cloned();

            draw_bookmark_search_summary(f, app, bookmark_chunks[0]);
            if app.show_model_details {
                let split = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
                    .split(bookmark_chunks[1]);

                draw_model_list(
                    f,
                    split[0],
                    &bookmark_items,
                    &app.bookmark_list_state,
                    app.show_model_details,
                    &[],
                    enable_name_rolling,
                );
                draw_model_sidebar(f, app, split[1], selected_bookmark_model.as_ref());
            } else {
                draw_model_list(
                    f,
                    bookmark_chunks[1],
                    &bookmark_items,
                    &app.bookmark_list_state,
                    app.show_model_details,
                    &[],
                    enable_name_rolling,
                );
            }

            if app.mode == AppMode::SearchBookmarks {
                draw_bookmark_search_popup(f, app);
            }
            if app.mode == AppMode::BookmarkPathPrompt {
                draw_bookmark_path_prompt(f, app);
            }
        }
        MainTab::Images => {
            let image_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(2), Constraint::Min(0)])
                .split(chunks[1]);
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(image_chunks[1]);
            draw_image_search_summary(f, app, image_chunks[0]);
            draw_image_panel(f, app, main_chunks[0]);
            draw_image_sidebar(f, app, main_chunks[1]);
            if app.mode == AppMode::SearchImages {
                draw_image_search_popup(f, app);
            }
        }
        MainTab::ImageBookmarks => {
            let image_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(2), Constraint::Min(0)])
                .split(chunks[1]);
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(image_chunks[1]);
            draw_image_bookmark_search_summary(f, app, image_chunks[0]);
            draw_image_panel(f, app, main_chunks[0]);
            draw_image_sidebar(f, app, main_chunks[1]);
            if app.mode == AppMode::SearchImageBookmarks {
                draw_image_bookmark_search_popup(f, app);
            }
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

    if app.show_bookmark_confirm_modal {
        draw_bookmark_confirm_modal(f, app);
    }

    if app.show_exit_confirm_modal {
        draw_exit_confirm_modal(f, app);
    }

    if app.show_resume_download_modal {
        draw_resume_download_modal(f, app);
    }

}

fn draw_downloads_tab(f: &mut Frame, app: &App, area: Rect) {
    let has_active = !app.active_downloads.is_empty();
    let has_history = !app.download_history.is_empty();
    let title = format!(
        " Downloads (Active: {}, History: {}) ",
        app.active_downloads.len(),
        app.download_history.len()
    );

    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(block.clone(), area);
    let inner_area = block.inner(area);

    if !has_active && !has_history {
        let p = Paragraph::new("No active downloads or history.").alignment(Alignment::Center);
        f.render_widget(p, inner_area);
        return;
    }

    let mut constraints = Vec::new();
    let mut sections: Vec<Rect> = Vec::new();
    if has_active {
        constraints.push(if has_history {
            Constraint::Percentage(55)
        } else {
            Constraint::Min(0)
        });
    }
    if has_history {
        constraints.push(Constraint::Min(0));
    }

    if constraints.len() > 1 {
        sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner_area)
            .to_vec();
    } else if has_active {
        sections.push(inner_area);
    } else {
        sections.push(inner_area);
    }

    let mut section_index = 0;
    if has_active {
        let active_area = sections[section_index];
        section_index += 1;
        draw_active_download_list(f, app, active_area);
    }

    if has_history {
        let history_area = sections[section_index];
        draw_download_history_list(f, app, history_area);
    }
}

fn draw_active_download_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Active Downloads ");
    f.render_widget(&block, area);
    let inner_area = block.inner(area);

    let mut tracked_rows =
        Vec::<(u64, &str, &str, u64, f64, u64, u64, DownloadState)>::new();
    for model_id in &app.active_download_order {
        if let Some(tracker) = app.active_downloads.get(model_id) {
            tracked_rows.push((
                *model_id,
                &tracker.model_name,
                &tracker.filename,
                tracker.version_id,
                tracker.progress,
                tracker.downloaded_bytes,
                tracker.total_bytes,
                tracker.state,
            ));
        }
    }

    if tracked_rows.is_empty() {
        f.render_widget(Paragraph::new("No active download tasks.").alignment(Alignment::Center), inner_area);
        return;
    }

    let inner_width = inner_area.width.saturating_sub(2) as usize;
    let model_width = (inner_width * 38 / 100).max(16).min(34);
    let file_width = (inner_width * 30 / 100).max(12).min(26);
    let size_width = 13usize;

    let columns = format!(
        "{:<3} {:<4} {:<w1$} {:<w2$} {:>6} {:>w4$} {:>w4$}",
        "No",
        "St",
        "Model",
        "File",
        "Progress",
        "Downloaded",
        "Total",
        w1 = model_width,
        w2 = file_width,
        w4 = size_width,
    );
    let header_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner_area);
    f.render_widget(
        Paragraph::new(columns).style(Style::default().fg(Color::DarkGray)),
        header_area[0],
    );

    let mut rows: Vec<ListItem> = Vec::with_capacity(tracked_rows.len());
    for (i, (_model_id, model_name, filename, version_id, progress, downloaded_bytes, total_bytes, state)) in
        tracked_rows.iter().enumerate()
    {
        let state_text = if *state == DownloadState::Running {
            "RUN"
        } else {
            "PAU"
        };
        let downloaded_text = compact_bytes(*downloaded_bytes);
        let total_text = if *total_bytes > 0 {
            compact_bytes(*total_bytes)
        } else {
            "Unknown".to_string()
        };
        let downloaded_text = compact_cell_text(downloaded_text, size_width);
        let total_text = compact_cell_text(total_text, size_width);
    let status = format!(
            "{:<3} {:<4} {:<w1$} {:<w2$} {:>6.1}% {:>w4$} {:>w4$}",
            i + 1,
            state_text,
            compact_cell_text(format!("{}  (v{})", model_name, version_id), model_width),
            compact_cell_text(filename.to_string(), file_width),
            progress,
            downloaded_text,
            total_text,
            w1 = model_width,
            w2 = file_width,
            w4 = size_width,
        );
        rows.push(ListItem::new(Line::from(status)));
    }

    let visible_height = header_area[1].height.max(1) as usize;
    let selected_idx = app.selected_download_index.min(tracked_rows.len().saturating_sub(1));
    let total = rows.len();
    let start_idx = if total <= visible_height {
        0
    } else {
        let half = visible_height / 2;
        if selected_idx <= half {
            0
        } else if selected_idx + half >= total {
            total.saturating_sub(visible_height)
        } else {
            selected_idx.saturating_sub(half)
        }
    };

    let visible_items = rows
        .iter()
        .skip(start_idx)
        .take(visible_height)
        .cloned()
        .collect::<Vec<_>>();
    let list = List::new(visible_items)
        .block(Block::default())
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD))
        .highlight_symbol("");

    let mut list_state = ListState::default();
    if !rows.is_empty() {
        list_state.select(Some(selected_idx.saturating_sub(start_idx)));
    }
    f.render_stateful_widget(list, header_area[1], &mut list_state);
}

fn draw_download_history_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Download History ");

    if app.download_history.is_empty() {
        f.render_widget(&block, area);
        f.render_widget(
            Paragraph::new("No download history.").alignment(Alignment::Center),
            block.inner(area),
        );
        return;
    }

    f.render_widget(&block, area);
    let inner_area = block.inner(area);
    let history_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner_area);
    let header = format!(
        "{:<5} {:<10} {:<9} {:<10} {:>6} {:<34} Size",
        "Age",
        "Status",
        "Model",
        "Version",
        "Prog",
        "File",
    );
    f.render_widget(
        Paragraph::new(header).style(Style::default().fg(Color::DarkGray)),
        history_layout[0],
    );

    let mut items: Vec<ListItem> = Vec::with_capacity(app.download_history.len());
    for entry in app.download_history.iter().rev() {
        let age = format_time_ago(entry.created_at);
        let status = match &entry.status {
            DownloadHistoryStatus::Completed => "Completed".to_string(),
            DownloadHistoryStatus::Failed(reason) => format!("Failed ({})", reason),
            DownloadHistoryStatus::Cancelled => "Cancelled".to_string(),
            DownloadHistoryStatus::Paused => "Paused".to_string(),
        };
        let size_text = if entry.total_bytes > 0 {
            format!(
                "{}/{}",
                compact_bytes(entry.downloaded_bytes),
                compact_bytes(entry.total_bytes),
            )
        } else {
            format!("{}/{}", compact_bytes(entry.downloaded_bytes), "Unknown")
        };
        let row = format!(
            "{:<5} {:<10} {:<9} {:<10} {:>4.1}% {:<34} {}",
            age,
            status,
            format!("m:{}", entry.model_id),
            format!("v:{}", entry.version_id),
            entry.progress,
            compact_cell_text(entry.filename.clone(), 34),
            size_text,
        );
        items.push(ListItem::new(Line::from(row)));
    }

    let visible_rows = history_layout[1].height.saturating_sub(1) as usize;
    let selected_idx = app.selected_history_index.min(items.len().saturating_sub(1));
    let total = items.len();
    let start_idx = if total <= visible_rows {
        0
    } else {
        let half = visible_rows / 2;
        if selected_idx <= half {
            0
        } else if selected_idx + half >= total {
            total.saturating_sub(visible_rows)
        } else {
            selected_idx - half
        }
    };

    let visible_items = items
        .iter()
        .skip(start_idx)
        .take(visible_rows)
        .cloned()
        .collect::<Vec<_>>();
    let list = List::new(visible_items)
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD))
        .highlight_symbol("");
    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(selected_idx.saturating_sub(start_idx)));
    }

    f.render_stateful_widget(list, history_layout[1], &mut list_state);
}

fn format_time_ago(ts: SystemTime) -> String {
    let now = SystemTime::now();
    let ago = now.duration_since(ts).unwrap_or_default();
    if ago.as_secs() > 86_400 {
        format!("{}d", ago.as_secs() / 86_400)
    } else if ago.as_secs() > 3600 {
        format!("{}h", ago.as_secs() / 3600)
    } else if ago.as_secs() > 60 {
        format!("{}m", ago.as_secs() / 60)
    } else {
        format!("{}s", ago.as_secs())
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

    let bookmark_path_val = if fm.editing && fm.focused_field == 2 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config
            .bookmark_file_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| crate::config::AppConfig::bookmark_path().map(|path| path.to_string_lossy().to_string()))
            .unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 2 { "> Bookmark File: " } else { "  Bookmark File: " },
            Style::default().fg(if fm.focused_field == 2 { Color::Yellow } else { Color::White }),
        ),
        Span::styled(
            bookmark_path_val,
            if fm.focused_field == 2 && fm.editing { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Cyan) },
        ),
    ]));

    let model_search_cache_path_val = if fm.editing && fm.focused_field == 3 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config
            .model_search_cache_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| app.config.search_cache_path().map(|path| path.to_string_lossy().to_string()))
            .unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 3 { "> Model Search Cache Folder: " } else { "  Model Search Cache Folder: " },
            Style::default().fg(if fm.focused_field == 3 { Color::Yellow } else { Color::White }),
        ),
        Span::styled(
            model_search_cache_path_val,
            if fm.focused_field == 3 && fm.editing { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Cyan) },
        ),
    ]));

    let cache_ttl_val = if fm.editing && fm.focused_field == 4 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config.model_search_cache_ttl_hours.to_string()
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 4 { "> Model Search Cache TTL (hours): " } else { "  Model Search Cache TTL (hours): " },
            Style::default().fg(if fm.focused_field == 4 { Color::Yellow } else { Color::White }),
        ),
        Span::styled(
            cache_ttl_val,
            if fm.focused_field == 4 && fm.editing { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Cyan) },
        ),
    ]));

    let image_cache_path_val = if fm.editing && fm.focused_field == 5 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config
            .image_cache_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| app.config.image_cache_path().map(|path| path.to_string_lossy().to_string()))
            .unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 5 { "> Image Cache Folder: " } else { "  Image Cache Folder: " },
            Style::default().fg(if fm.focused_field == 5 { Color::Yellow } else { Color::White }),
        ),
        Span::styled(
            image_cache_path_val,
            if fm.focused_field == 5 && fm.editing { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Cyan) },
        ),
    ]));

    let image_search_ttl_val = if fm.editing && fm.focused_field == 6 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config.image_search_cache_ttl_minutes.to_string()
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 6 { "> Image Search Cache TTL (minutes): " } else { "  Image Search Cache TTL (minutes): " },
            Style::default().fg(if fm.focused_field == 6 { Color::Yellow } else { Color::White }),
        ),
        Span::styled(
            image_search_ttl_val,
            if fm.focused_field == 6 && fm.editing { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Cyan) },
        ),
    ]));

    let image_cache_ttl_val = if fm.editing && fm.focused_field == 7 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config.image_cache_ttl_minutes.to_string()
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 7 { "> Image Cache TTL (minutes, 0 = persistent): " } else { "  Image Cache TTL (minutes, 0 = persistent): " },
            Style::default().fg(if fm.focused_field == 7 { Color::Yellow } else { Color::White }),
        ),
        Span::styled(
            image_cache_ttl_val,
            if fm.focused_field == 7 && fm.editing { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Cyan) },
        ),
    ]));

    let download_history_path_val = if fm.editing && fm.focused_field == 8 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config
            .download_history_file_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| app.config.download_history_path().map(|path| path.to_string_lossy().to_string()))
            .unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 8 { "> Download History File: " } else { "  Download History File: " },
            Style::default().fg(if fm.focused_field == 8 { Color::Yellow } else { Color::White }),
        ),
        Span::styled(
            download_history_path_val,
            if fm.focused_field == 8 && fm.editing { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Cyan) },
        ),
    ]));

    lines.push(Line::from(""));
    if fm.editing {
        lines.push(Line::from(Span::styled(
            " [Type to edit] | [Enter] Save | [Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(" [Up/Down] Highlight | [Enter] Edit string", Style::default().fg(Color::DarkGray))));
    }

    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_image_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Image View ");
    let items = app.active_image_items();
    let selected_index = app.active_image_selected_index();

    if items.is_empty() {
        let empty_message = if app.active_tab == MainTab::ImageBookmarks {
            "No bookmarked images."
        } else {
            "Loading feed..."
        };
        f.render_widget(Paragraph::new(empty_message).block(block), area);
        return;
    }

    let Some(img) = items.get(selected_index) else {
        f.render_widget(Paragraph::new("No image selected").block(block), area);
        return;
    };
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if let Some(protocol) = app.image_cache.get_mut(&img.id) {
        let image_widget = StatefulImage::new();
        f.render_stateful_widget(image_widget, inner_area, protocol);
    } else {
        let text = format!("Decoding media {}/{}...", selected_index + 1, items.len());
        f.render_widget(Paragraph::new(text), inner_area);
    }
}

fn draw_image_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Metadata ");

    if let Some(img) = app.selected_image_in_active_view() {
        let dimensions = match (img.width, img.height) {
            (Some(width), Some(height)) => format!("{} x {}", width, height),
            _ => "<none>".to_string(),
        };
        let model_version_ids = if img.model_version_ids.is_empty() {
            "<none>".to_string()
        } else {
            img.model_version_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut lines = vec![
            Line::from(vec![Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(img.id.to_string())]),
            Line::from(vec![
                Span::styled("Link: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("https://civitai.com/images/{}", img.id)),
            ]),
            Line::from(vec![
                Span::styled("URL: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(img.url.as_str()),
            ]),
            Line::from(vec![
                Span::styled("Hash: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(img.hash.as_deref().unwrap_or("<none>")),
            ]),
            Line::from(vec![
                Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(img.r#type.as_deref().unwrap_or("<none>")),
            ]),
            Line::from(vec![
                Span::styled("Dimensions: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(dimensions),
            ]),
            Line::from(vec![
                Span::styled("NSFW: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(
                    img.nsfw
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "<none>".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("NSFW Level: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(img.nsfw_level.as_deref().unwrap_or("<none>")),
            ]),
            Line::from(vec![
                Span::styled("Browsing Level: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(img.browsing_level.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Created At: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(img.created_at.as_deref().unwrap_or("<none>")),
            ]),
            Line::from(vec![
                Span::styled("Post ID: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(
                    img.post_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "<none>".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("Username: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(img.username.as_deref().unwrap_or("<none>")),
            ]),
            Line::from(vec![
                Span::styled("Base Model: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(img.base_model.as_deref().unwrap_or("<none>")),
            ]),
            Line::from(vec![
                Span::styled("ModelVersionIds: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(model_version_ids),
            ]),
        ];

        if let Some(stats) = &img.stats {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Stats:",
                Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan),
            )));
            lines.push(Line::from(format!(
                "cry={} laugh={} like={} dislike={} heart={} comment={}",
                stats.cry_count,
                stats.laugh_count,
                stats.like_count,
                stats.dislike_count,
                stats.heart_count,
                stats.comment_count,
            )));
        }

        if let Some(meta) = &img.meta {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Meta:",
                Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow),
            )));
            match serde_json::to_string_pretty(meta) {
                Ok(pretty) => {
                    for line in pretty.lines() {
                        lines.push(Line::from(line.to_string()));
                    }
                }
                Err(_) => {
                    lines.push(Line::from("<failed to render meta json>"));
                }
            }
        }
        f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: true }), area);
    } else {
        f.render_widget(Paragraph::new("No metadata available.").block(block), area);
    }
}

fn draw_model_search_summary(f: &mut Frame, app: &mut App, area: Rect) {
    let model_query = if app.search_form.query.is_empty() {
        "<empty>"
    } else {
        &app.search_form.query
    };
    let summary = format!(
        "🔍 Query: \"{}\" | Type: {} | Sort: {} | Base: {}",
        model_query,
        app.search_form.types.get(app.search_form.selected_type).cloned().unwrap_or_else(|| "All".into()),
        app.search_form.sorts.get(app.search_form.selected_sort).cloned().unwrap_or_else(|| "Highest Rated".into()),
        app.search_form.bases.get(app.search_form.selected_base).cloned().unwrap_or_else(|| "All".into()),
    );

    let para = Paragraph::new(summary)
        .alignment(Alignment::Left)
        .style(Style::default().fg(Color::White).add_modifier(Modifier::ITALIC))
        .wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

fn draw_bookmark_search_summary(f: &mut Frame, app: &App, area: Rect) {
    let query = if app.bookmark_query.is_empty() {
        "<all>"
    } else {
        &app.bookmark_query
    };
    let summary = format!(
        "🔍 Bookmarks Query: \"{}\" | Total: {}",
        query,
        app.visible_bookmarks().len()
    );

    let para = Paragraph::new(summary)
        .alignment(Alignment::Left)
        .style(Style::default().fg(Color::White).add_modifier(Modifier::ITALIC))
        .wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

fn draw_image_bookmark_search_summary(f: &mut Frame, app: &App, area: Rect) {
    let query = if app.image_bookmark_query.is_empty() {
        "<all>"
    } else {
        &app.image_bookmark_query
    };
    let summary = format!(
        "🔍 Image Bookmarks Query: \"{}\" | Total: {}",
        query,
        app.visible_image_bookmarks().len()
    );

    let para = Paragraph::new(summary)
        .alignment(Alignment::Left)
        .style(Style::default().fg(Color::White).add_modifier(Modifier::ITALIC))
        .wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

fn draw_image_search_summary(f: &mut Frame, app: &App, area: Rect) {
    let tag_text = if app.image_search_form.tag_text.trim().is_empty() {
        "<all>"
    } else {
        app.image_search_form.tag_text.trim()
    };
    let model_version_id = if app.image_search_form.model_version_id.trim().is_empty() {
        "<all>"
    } else {
        app.image_search_form.model_version_id.trim()
    };
    let tag_id = app
        .image_search_form
        .build_options()
        .tags
        .map(|value| value.to_string())
        .unwrap_or_else(|| "<none>".to_string());
    let summary = format!(
        " Image Search | NSFW: {} | Sort: {} | Period: {} | ModelVersionId: {} | Tag: {} ({}) ",
        app.image_search_form
            .nsfw_options
            .get(app.image_search_form.selected_nsfw)
            .cloned()
            .unwrap_or_else(|| "All".into()),
        app.image_search_form
            .sort_options
            .get(app.image_search_form.selected_sort)
            .cloned()
            .unwrap_or_else(|| "Newest".into()),
        app.image_search_form
            .period_options
            .get(app.image_search_form.selected_period)
            .cloned()
            .unwrap_or_else(|| "AllTime".into()),
        model_version_id,
        tag_text,
        tag_id,
    );

    let para = Paragraph::new(summary)
        .alignment(Alignment::Left)
        .style(Style::default().fg(Color::White).add_modifier(Modifier::ITALIC))
        .wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

fn draw_model_list(
    f: &mut Frame,
    area: Rect,
    models: &[crate::api::Model],
    list_state: &ListState,
    show_model_details: bool,
    bookmarked_ids: &[u64],
    enable_name_rolling: bool,
) {
    let block = Block::default().borders(Borders::ALL).title(" Searched Models ");

    if models.is_empty() {
        f.render_widget(Paragraph::new("No models found. Press '/' to search.").block(block), area);
        return;
    }

    let selected_idx = list_state
        .selected()
        .unwrap_or(0)
        .min(models.len().saturating_sub(1));
    let show_metadata = !show_model_details;
    let mut rows_cache: Vec<(String, String, String, bool, bool)> = Vec::with_capacity(models.len());
    let mut down_width = "⬇".chars().count();
    let mut thumbs_width = "👍".chars().count();

    for model in models.iter() {
        let downloads = model.stats.as_ref().map(|s| s.download_count).unwrap_or(0);
        let thumbs_up = model.stats.as_ref().map(|s| s.thumbs_up_count).unwrap_or(0);
        let down_text = if show_metadata {
            format!("⬇ {}", compact_number(downloads))
        } else {
            String::new()
        };
        let rate_text = if show_metadata {
            format!("👍 {}", compact_number(thumbs_up))
        } else {
            String::new()
        };

        down_width = down_width.max(down_text.chars().count());
        thumbs_width = thumbs_width.max(rate_text.chars().count());
        let is_bookmarked = bookmarked_ids.contains(&model.id);
        rows_cache.push((down_text, rate_text, model.name.clone(), model.nsfw, is_bookmarked));
    }

    if down_width < 6 {
        down_width = 6;
    }
    if thumbs_width < 3 {
        thumbs_width = 3;
    }

    let inner_width = area.width.saturating_sub(2) as usize;
    if inner_width <= down_width.saturating_add(thumbs_width) + 2 {
        down_width = down_width.min(inner_width / 2).max(4);
        thumbs_width = thumbs_width.min(inner_width / 2).max(3);
    }

    let name_width = if show_metadata {
        inner_width
            .saturating_sub(down_width.saturating_add(thumbs_width))
            .saturating_sub(4)
            .max(1)
    } else {
        inner_width.max(1)
    };

    let mut items: Vec<ListItem> = Vec::with_capacity(rows_cache.len());
    for (idx, (down_text, rate_text, name, is_nsfw, is_bookmarked)) in
        rows_cache.into_iter().enumerate()
    {
        let is_selected = idx == selected_idx;
        let down_text = compact_cell_text(down_text, down_width);
        let rate_text = compact_cell_text(rate_text, thumbs_width);
        let mut display_name = if is_bookmarked {
            format!("★ {}", name)
        } else {
            name
        };

        if enable_name_rolling
            && is_selected
            && display_name.chars().count() > name_width
        {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_millis(0))
                .as_millis();
            let shift = ((now_ms / 260) as usize) % display_name.chars().count().max(1);
            display_name = rotate_left_chars(&display_name, shift);
        }

        if show_metadata {
            let style = if is_bookmarked {
                Style::default().fg(Color::Green)
            } else if is_nsfw {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::White)
            };
            let spans = vec![
                Span::styled(
                    format!(
                        "{:>width$}",
                        down_text,
                        width = down_width
                    ),
                    Style::default().fg(Color::White),
                ),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "{:>width$}",
                        rate_text,
                        width = thumbs_width
                    ),
                    Style::default().fg(Color::White),
                ),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled(compact_cell_text(display_name.clone(), name_width), style),
            ];
            let item = ListItem::new(Line::from(spans));
            items.push(item);
        } else {
            let style = if is_bookmarked {
                Style::default().fg(Color::Green)
            } else if is_nsfw {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::White)
            };
            let item = ListItem::new(Line::from(Span::styled(
                compact_cell_text(display_name, name_width),
                style,
            )));
            items.push(item);
        }
    }

    let inner_area = block.inner(area);
    let visible_rows = inner_area.height.max(1) as usize;
    let total = items.len();
    let start_idx = if total <= visible_rows {
        0
    } else {
        let half = visible_rows / 2;
        if selected_idx <= half {
            0
        } else if selected_idx + half >= total {
            total.saturating_sub(visible_rows)
        } else {
            selected_idx - half
        }
    };

    let visible_items = items
        .into_iter()
        .skip(start_idx)
        .take(visible_rows)
        .collect::<Vec<_>>();

    let list = List::new(visible_items)
        .block(block)
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD))
        .highlight_symbol("");
    let mut state = list_state.clone();
    if !models.is_empty() {
        state.select(Some(selected_idx.saturating_sub(start_idx)));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_model_sidebar(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    selected_model: Option<&crate::api::Model>,
) {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(area);

    if let Some(model) = selected_model {
        let v_idx = *app.selected_version_index.get(&model.id).unwrap_or(&0);
        let safe_v_idx = v_idx.min(model.model_versions.len().saturating_sub(1));
        let selected_version = model.model_versions.get(safe_v_idx);

        let model_url = if let Some(version) = selected_version {
            format!("https://civitai.com/models/{}?modelVersionId={}", model.id, version.id)
        } else {
            format!("https://civitai.com/models/{}", model.id)
        };
        let url_line = Paragraph::new(format!("URL: {}", model_url))
            .alignment(Alignment::Left)
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL).title(" Model URL "));
        f.render_widget(url_line, split[0]);

        let version_data: Vec<(String, bool)> = model
            .model_versions
            .iter()
            .enumerate()
            .map(|(idx, version)| {
                let is_selected = idx == safe_v_idx;
                (version.name.clone(), is_selected)
            })
            .collect();
        let version_total = version_data.len();
        let version_position = if version_total > 0 {
            format!(" ({} / {})", safe_v_idx + 1, version_total)
        } else {
            " (0 / 0)".to_string()
        };
        let version_window = build_horizontal_item_window(
            &version_data,
            split[1].width.saturating_sub(2) as usize,
        );
        let mut version_spans = Vec::with_capacity(version_window.len() + 2);
        for (i, (text, is_selected)) in version_window.iter().enumerate() {
            if !text.is_empty() {
                if i > 0 {
                    version_spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                }
                version_spans.push(Span::styled(
                    text,
                    if *is_selected {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ));
            }
        }

        if version_spans.is_empty() {
            version_spans.push(Span::styled("No versions", Style::default().fg(Color::DarkGray)));
        }
        let version_row = Paragraph::new(Line::from(version_spans))
            .alignment(Alignment::Left)
            .block(Block::default().borders(Borders::ALL).title(format!(" Versions{}", version_position)));
        f.render_widget(version_row, split[1]);

        let stats = selected_version.and_then(|version| version.stats.as_ref());
        let down_val = compact_number(stats.map(|s| s.download_count).unwrap_or(0));
        let rate_val = normalized_version_rating(stats);
        let thumbs_up_val = compact_number(stats.map(|s| s.thumbs_up_count).unwrap_or(0));
        let active_file = selected_version
            .and_then(|version| version.files.iter().find(|f| f.primary).or_else(|| version.files.first()));
        let format_value = active_file
            .and_then(|file| file.metadata.as_ref())
            .and_then(|meta| meta.format.as_deref())
            .unwrap_or("N/A");
        let file_size = active_file
            .map(|file| compact_file_size(file.size_kb))
            .unwrap_or_else(|| "N/A".to_string());
        let base_model = selected_version
            .map(|version| version.base_model.clone())
            .unwrap_or_else(|| "N/A".to_string());

        let detail_row = vec![
            down_val,
            format!("{:.1}", rate_val),
            thumbs_up_val,
            model.r#type.clone(),
            format_value.to_string(),
            base_model,
            file_size,
        ];
        let headers = ["Down", "Rate", "Thumbs Up", "Type", "Format", "Base Model", "File Size"];
        let mut widths = [5usize; 7];
        for i in 0..7 {
            widths[i] = widths[i]
                .max(detail_row[i].chars().count())
                .max(headers[i].chars().count())
                .saturating_add(1);
        }

        let total_with_separators = widths.iter().sum::<usize>() + widths.len();
        let max_width = split[2].width as usize;
        if total_with_separators > max_width {
            for i in (3..7).rev() {
                while widths.iter().sum::<usize>() + widths.len() > max_width && widths[i] > 5 {
                    widths[i] -= 1;
                }
            }
        }

        let detail_cells: Vec<Cell> = detail_row
            .into_iter()
            .enumerate()
            .map(|(i, value)| {
                Cell::from(center_text(compact_cell_text(value, widths[i]), widths[i]))
            })
            .collect();

        let header_cells: Vec<Cell> = headers
            .iter()
            .enumerate()
            .map(|(i, value)| Cell::from(center_text(value.to_string(), widths[i])))
            .collect();

        let metadata_block = Block::default().borders(Borders::ALL).title(" Metadata ");
        f.render_widget(&metadata_block, split[2]);
        let metadata_inner = metadata_block.inner(split[2]);
        let metadata_center_v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(metadata_inner);
        let metadata_center_h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(5),
                Constraint::Min(0),
                Constraint::Percentage(5),
            ])
            .split(metadata_center_v[1]);
        let down_rate_table = Table::new(
            vec![Row::new(detail_cells)],
            [
                Constraint::Length(widths[0] as u16),
                Constraint::Length(widths[1] as u16),
                Constraint::Length(widths[2] as u16),
                Constraint::Length(widths[3] as u16),
                Constraint::Length(widths[4] as u16),
                Constraint::Length(widths[5] as u16),
                Constraint::Length(widths[6] as u16),
            ],
        )
        .header(Row::new(header_cells).style(Style::default().add_modifier(Modifier::BOLD)))
        .column_spacing(2);
        f.render_widget(down_rate_table, metadata_center_h[1]);

        let desc_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(67), Constraint::Percentage(33)])
            .split(split[3]);

        let description = model
            .description
            .as_deref()
            .unwrap_or("No description available.");
        let version_description = selected_version
            .and_then(|version| version.description.as_deref())
            .filter(|desc| !desc.is_empty())
            .unwrap_or(description);
        let description_lines = render_description_lines(version_description);
        let description_text = Paragraph::new(description_lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Description "));
        f.render_widget(description_text, desc_split[0]);

        let image_block = Block::default().borders(Borders::ALL).title(" Cover Image ");
        let image_area = desc_split[1];
        let image_inner = image_block.inner(image_area);
        f.render_widget(&image_block, image_area);
        let image_center_outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(8),
                Constraint::Min(0),
                Constraint::Percentage(8),
            ])
            .split(image_inner);
        let image_center_inner = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(8),
                Constraint::Min(0),
                Constraint::Percentage(8),
            ])
            .split(image_center_outer[1]);
        let inner_img_area = image_center_inner[1];

        let selected_version_has_image = selected_version
            .map(|version| !version.images.is_empty())
            .unwrap_or(false);
        let has_any_version_image = model
            .model_versions
            .iter()
            .any(|version| !version.images.is_empty());

        let mut image_version_id = selected_version.map(|version| version.id);
        if image_version_id.is_none() {
            image_version_id = model
                .model_versions
                .iter()
                .find_map(|version| {
                    if app.model_version_image_cache.contains_key(&version.id) {
                        Some(version.id)
                    } else {
                        None
                    }
                });
        }

        if let Some(image_version_id) = image_version_id {
            let protocol = app.model_version_image_cache.get_mut(&image_version_id);
            if let Some(mut protocol) = protocol {
                let image_widget: StatefulImage<StatefulProtocol> = StatefulImage::new();
                f.render_stateful_widget(image_widget, inner_img_area, &mut protocol);
            } else {
                if !selected_version_has_image {
                    f.render_widget(Paragraph::new("No thumbnail available.").alignment(Alignment::Center), inner_img_area);
                } else if app.model_version_image_failed.contains(&image_version_id) {
                    f.render_widget(Paragraph::new("No thumbnail available.").alignment(Alignment::Center), inner_img_area);
                } else if has_any_version_image {
                    f.render_widget(Paragraph::new("Loading thumbnail...").alignment(Alignment::Center), inner_img_area);
                } else {
                    f.render_widget(Paragraph::new("No thumbnail available.").alignment(Alignment::Center), inner_img_area);
                }
            }
        } else {
            if has_any_version_image {
                f.render_widget(Paragraph::new("Loading thumbnail...").alignment(Alignment::Center), inner_img_area);
            } else {
                f.render_widget(Paragraph::new("No thumbnail available.").alignment(Alignment::Center), inner_img_area);
            }
        }
    } else {
        f.render_widget(Paragraph::new("Select a model.").block(Block::default().borders(Borders::ALL).title(" Model Details ")), area);
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
        Line::from(vec![
            Span::styled(if fm.focused_field == 4 { "> Period: " } else { "  Period: " }, Style::default().fg(if fm.focused_field == 4 { Color::Yellow } else { Color::White })),
            Span::raw(format!("< {} >", fm.periods[fm.selected_period])),
        ]),
        Line::from(""),
        Line::from(Span::styled(" [Up/Down] Select Field | [Left/Right] Cycle Options | [Enter] Search", Style::default().fg(Color::DarkGray))),
    ];

    let p = Paragraph::new(list).block(block);

    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

fn draw_bookmark_search_popup(f: &mut Frame, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Bookmark Search ");

    let lines = vec![
        Line::from(vec![
            Span::styled(" Query: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}█", app.bookmark_query_draft)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "[Enter] Apply | [Esc] Cancel | [Type] Query",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    let area = centered_rect(40, 25, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

fn draw_image_bookmark_search_popup(f: &mut Frame, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Image Bookmark Search ");

    let lines = vec![
        Line::from(vec![
            Span::styled(" Query: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}█", app.image_bookmark_query_draft)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "[Enter] Apply | [Esc] Cancel | [Type] Query",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    let area = centered_rect(40, 25, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

fn draw_bookmark_path_prompt(f: &mut Frame, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Bookmark File Path ");

    let action = if app.is_bookmark_export_prompt() {
        "Export"
    } else {
        "Import"
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{} Path: ", action), Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}█", app.bookmark_path_draft)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "[Enter] Run | [Esc] Cancel | [Backspace] Delete | [Type] Path",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    let area = centered_rect(55, 28, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

fn draw_image_search_popup(f: &mut Frame, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Image Search ");

    let form = &app.image_search_form;
    let field_style = |focused: bool| {
        if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        }
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(" NSFW: ", field_style(form.focused_field == 0)),
            Span::raw(form.nsfw_options[form.selected_nsfw].clone()),
        ]),
        Line::from(vec![
            Span::styled(" Sort: ", field_style(form.focused_field == 1)),
            Span::raw(form.sort_options[form.selected_sort].clone()),
        ]),
        Line::from(vec![
            Span::styled(" Period: ", field_style(form.focused_field == 2)),
            Span::raw(form.period_options[form.selected_period].clone()),
        ]),
        Line::from(vec![
            Span::styled(" ModelVersionId: ", field_style(form.focused_field == 3)),
            Span::raw(format!("{}{}", form.model_version_id, if form.focused_field == 3 { "█" } else { "" })),
        ]),
        Line::from(vec![
            Span::styled(" Tag: ", field_style(form.focused_field == 4)),
            Span::raw(format!("{}{}", form.tag_text, if form.focused_field == 4 { "█" } else { "" })),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Known tags: Realistic=5248 | Nude=304 | Woman=5133",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "[Up/Down] Field | [Left/Right] Cycle | [Type] Input | [Enter] Apply | [Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    let area = centered_rect(60, 42, f.area());
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

fn draw_bookmark_confirm_modal(f: &mut Frame, app: &App) {
    let name = app
        .pending_bookmark_remove_id
        .and_then(|model_id| app.bookmarks.iter().find(|model| model.id == model_id))
        .map(|model| model.name.clone())
        .unwrap_or_else(|| "selected bookmark".to_string());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Remove Bookmark ");
    let lines = vec![
        Line::from(format!("Remove bookmark: {}", name)),
        Line::from(""),
        Line::from(Span::styled("Press Y to confirm, N or Esc to cancel.", Style::default().fg(Color::DarkGray))),
    ];
    let p = Paragraph::new(lines).block(block).alignment(Alignment::Center);
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

fn draw_exit_confirm_modal(f: &mut Frame, app: &App) {
    let active_download_count = app.active_downloads.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Exit Confirmation ");

    let lines = vec![
        Line::from(format!(
            "There are {} active download(s). Exit now?",
            active_download_count
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Confirm: [Y] Save and exit | [D] Delete and exit | [N]/[Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let p = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    let area = centered_rect(60, 24, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

fn draw_resume_download_modal(f: &mut Frame, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Resume Interrupted Downloads ");

    let count = app.interrupted_download_sessions.len();
    let lines = vec![
        Line::from(format!(
            "There are {} interrupted download session(s) detected.",
            count
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Resume: [Y]  Delete files + keep history: [D]  Ignore for now: [N] / [Esc]",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let p = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    let area = centered_rect(60, 24, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let left_status = if let Some(error) = app.last_error.as_deref() {
        format!(" {} | ERROR: {}", app.status, error)
    } else {
        format!(" {}", app.status)
    };
    let status_line = Line::from(Span::styled(left_status, Style::default().fg(Color::Cyan)));
    let status = Paragraph::new(status_line).alignment(Alignment::Left);
    f.render_widget(status, rows[0]);

    let shortcuts = match app.active_tab {
        MainTab::Models => "[/] Search | [R] Refresh cache | [x] Clear cache | [b] Bookmark | [Space/Enter] Details",
        MainTab::Bookmarks => "[/] Search | [b] Remove | [e] Export | [i] Import | [Space/Enter] Details",
        MainTab::Images => "[/] Search | [b] Bookmark | [d] Download | [m] Status",
        MainTab::ImageBookmarks => "[/] Search | [b] Remove | [d] Download | [m] Status",
        MainTab::Downloads => "[j/k or J/K] Move | [d] Delete history | [D] Delete history + file | [r] Resume | [p] Pause/Resume | [c] Cancel",
        MainTab::Settings => "[Enter] Edit | [m] Status",
    };

    let shortcuts_row = Paragraph::new(Span::styled(shortcuts, Style::default().fg(Color::DarkGray))).alignment(Alignment::Left);
    f.render_widget(shortcuts_row, rows[1]);
}

fn rotate_left_chars(src: &str, shift: usize) -> String {
    let mut chars: Vec<char> = src.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let shift = shift % chars.len();
    chars.rotate_left(shift);
    chars.into_iter().collect()
}

fn compact_cell_text(src: String, width: usize) -> String {
    let value_chars: Vec<char> = src.chars().collect();
    if width == 0 || value_chars.is_empty() {
        return String::new();
    }

    if value_chars.len() <= width {
        src
    } else {
        value_chars.into_iter().take(width).collect()
    }
}

fn build_horizontal_item_window(items: &[(String, bool)], max_width: usize) -> Vec<(String, bool)> {
    if items.is_empty() || max_width == 0 {
        return Vec::new();
    }

    let item_count = items.len();
    let selected_idx = items.iter().position(|(_, selected)| *selected).unwrap_or(0).min(item_count - 1);
    let separator = 2usize;

    let prepared: Vec<(String, bool, usize)> = items
        .iter()
        .map(|(value, selected)| {
            let text = compact_cell_text(value.clone(), max_width);
            let width = text.chars().count().max(1);
            (text, *selected, width)
        })
        .collect();

    let mut start = selected_idx;
    let mut end = selected_idx + 1;
    let mut used_width = prepared[selected_idx].2;

    while start > 0 || end < item_count {
        let mut extended = false;

        if end < item_count {
            let next_width = prepared[end].2 + separator;
            if used_width + next_width <= max_width {
                used_width += next_width;
                end += 1;
                extended = true;
            }
        }

        if start > 0 {
            let next_width = prepared[start - 1].2 + separator;
            if used_width + next_width <= max_width {
                used_width += next_width;
                start = start.saturating_sub(1);
                extended = true;
            }
        }

        if !extended {
            break;
        }
    }

    prepared[start..end]
        .iter()
        .map(|(text, selected, _)| (text.clone(), *selected))
        .collect()
}

fn center_text(src: String, width: usize) -> String {
    let src_chars: Vec<char> = src.chars().collect();
    if width == 0 {
        return String::new();
    }
    if src_chars.len() >= width {
        return compact_cell_text(src, width);
    }

    let diff = width - src_chars.len();
    let left = diff / 2;
    let right = diff - left;
    format!("{}{}{}", " ".repeat(left), src, " ".repeat(right))
}

fn compact_number(v: u64) -> String {
    const UNITS: [&str; 7] = ["", "K", "M", "B", "T", "P", "E"];
    let mut value = v as f64;
    let mut idx = 0usize;

    while value >= 1000.0 && idx + 1 < UNITS.len() {
        value /= 1000.0;
        idx += 1;
    }

    let rounded = format!("{:>5.1}", value);
    format!("{}{}", rounded, UNITS[idx])
}

fn normalized_version_rating(stats: Option<&crate::api::VersionStats>) -> f64 {
    let Some(stats) = stats else {
        return 0.0;
    };

    if stats.rating.is_finite() && stats.rating > 0.0 {
        return stats.rating;
    }

    let thumbs_up = stats.thumbs_up_count as f64;
    let thumbs_down = stats.thumbs_down_count as f64;
    let total = thumbs_up + thumbs_down;
    if total > 0.0 {
        (thumbs_up / total) * 5.0
    } else {
        0.0
    }
}

fn compact_file_size(size_kb: f64) -> String {
    if size_kb <= 0.0 {
        return "0.0 MB".to_string();
    }

    let mb = 1024.0;             // 1 MB = 1024 KB
    let gb = mb * 1024.0;        // 1 GB = 1024 MB
    let tb = gb * 1024.0;        // 1 TB = 1024 GB

    if size_kb >= tb {
        format!("{:.1} TB", size_kb / tb)
    } else if size_kb >= gb {
        format!("{:.1} GB", size_kb / gb)
    } else if size_kb >= mb {
        format!("{:.1} MB", size_kb / mb)
    } else {
        format!("{:.1} MB", size_kb / mb)
    }
}

fn compact_bytes(size_bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    let value = size_bytes as f64;
    if value >= TB as f64 {
        format!("{:.1} TB", value / TB as f64)
    } else if value >= GB as f64 {
        format!("{:.1} GB", value as f64 / GB as f64)
    } else if value >= MB as f64 {
        format!("{:.1} MB", value as f64 / MB as f64)
    } else if value >= KB as f64 {
        format!("{:.1} KB", value as f64 / KB as f64)
    } else {
        format!("{} B", size_bytes)
    }
}

fn render_description_lines(raw: &str) -> Vec<Line<'static>> {
    let text = if raw.contains('<') && raw.contains('>') {
        html2text::from_read(raw.as_bytes(), 120).unwrap_or_else(|_| raw.to_string())
    } else {
        raw.to_string()
    };

    // Best effort markdown cleanup. We don't depend on a markdown renderer currently.
    let mut normalized = String::new();
    for line in text.lines() {
        let mut line = line.trim().to_string();
        while line.starts_with('#') {
            line = line.trim_start_matches('#').trim_start().to_string();
        }
        if let Some(rest) = line.strip_prefix("- ") {
            line = format!("• {}", rest.trim_start());
        }
        if let Some(rest) = line.strip_prefix("* ") {
            line = format!("• {}", rest.trim_start());
        }
        if let Some(rest) = line.strip_prefix("+ ") {
            line = format!("• {}", rest.trim_start());
        }
        if let Some(rest) = line.strip_prefix("> ") {
            line = format!("› {}", rest.trim_start());
        }
        normalized.push_str(&line);
        normalized.push('\n');
    }

    let normalized = normalized.trim_end().to_string();
    if normalized.is_empty() {
        return vec![Line::from("No description available.")];
    }

    let mut lines = Vec::new();
    for raw_line in normalized.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        if line.starts_with("### ") || line.starts_with("## ") || line.starts_with("# ") {
            let header_level = line.chars().take_while(|ch| *ch == '#').count();
            let title = line.trim_start_matches('#').trim_start();
            let style = Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(if header_level == 1 {
                    Color::Yellow
                } else if header_level == 2 {
                    Color::LightBlue
                } else {
                    Color::Cyan
                });
            lines.push(Line::from(Span::styled(title.to_string(), style)));
            continue;
        }

        if line.starts_with("• ") {
            let mut spans = vec![Span::styled("• ", Style::default().fg(Color::LightGreen))];
            spans.extend(parse_styled_markdown(&line[3..], Style::default().fg(Color::White)));
            lines.push(Line::from(spans));
            continue;
        }

        if line.starts_with("› ") {
            let mut spans = vec![Span::styled("› ", Style::default().fg(Color::DarkGray))];
            spans.extend(parse_styled_markdown(&line[3..], Style::default().fg(Color::Gray)));
            lines.push(Line::from(spans));
            continue;
        }

        lines.push(Line::from(parse_styled_markdown(line, Style::default().fg(Color::White))));
    }

    if lines.is_empty() {
        vec![Line::from("No description available.")]
    } else {
        lines
    }
}

fn parse_styled_markdown(raw: &str, base_style: Style) -> Vec<Span<'static>> {
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0usize;

    while i < raw.len() {
        let rest = &raw[i..];
        let next = match next_markdown_token(rest) {
            Some(v) => v,
            None => {
                if !rest.is_empty() {
                    spans.push(Span::styled(rest.to_string(), base_style));
                }
                break;
            }
        };

        let (token_idx, token) = next;
        if token_idx > 0 {
            spans.push(Span::styled(raw[i..i + token_idx].to_string(), base_style));
            i += token_idx;
        }

        let consumed = match token {
            "**" => {
                let body = &raw[i + 2..];
                if let Some(end) = body.find("**") {
                    let styled = &body[..end];
                    if !styled.is_empty() {
                        spans.push(Span::styled(styled.to_string(), base_style.add_modifier(Modifier::BOLD)));
                    }
                    2 + end + 2
                } else {
                    spans.push(Span::styled("**".to_string(), base_style));
                    2
                }
            }
            "__" => {
                let body = &raw[i + 2..];
                if let Some(end) = body.find("__") {
                    let styled = &body[..end];
                    if !styled.is_empty() {
                        spans.push(Span::styled(styled.to_string(), base_style.add_modifier(Modifier::BOLD)));
                    }
                    2 + end + 2
                } else {
                    spans.push(Span::styled("__".to_string(), base_style));
                    2
                }
            }
            "*" => {
                let body = &raw[i + 1..];
                if let Some(end) = body.find('*') {
                    let styled = &body[..end];
                    if !styled.is_empty() {
                        spans.push(Span::styled(
                            styled.to_string(),
                            base_style.add_modifier(Modifier::ITALIC),
                        ));
                    }
                    1 + end + 1
                } else {
                    spans.push(Span::styled("*".to_string(), base_style));
                    1
                }
            }
            "`" => {
                let body = &raw[i + 1..];
                if let Some(end) = body.find('`') {
                    let code = &body[..end];
                    spans.push(Span::styled(
                        code.to_string(),
                        Style::default().bg(Color::DarkGray).fg(Color::White),
                    ));
                    1 + end + 1
                } else {
                    spans.push(Span::styled("`".to_string(), base_style));
                    1
                }
            }
            "https://" | "http://" => {
                let rest = &raw[i..];
                let mut end = rest.len();
                if let Some(space_idx) = rest.find(' ') {
                    end = space_idx;
                }
                if let Some(space_idx) = rest.find('\t') {
                    end = end.min(space_idx);
                }
                if let Some(space_idx) = rest.find('\n') {
                    end = end.min(space_idx);
                }

                let raw_url = &rest[..end];
                let trimmed = raw_url.trim_end_matches(|ch| ch == '.' || ch == ',' || ch == ')' || ch == ']' || ch == '}' || ch == '>' );
                if !trimmed.is_empty() {
                    spans.push(Span::styled(
                        trimmed.to_string(),
                        Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::UNDERLINED),
                    ));
                }
                if trimmed.len() < raw_url.len() {
                    spans.push(Span::styled(
                        raw_url[trimmed.len()..].to_string(),
                        base_style,
                    ));
                }

                end
            }
            "[" => {
                let body = &raw[i + 1..];
                if let Some(close_text) = body.find("](") {
                    let url_start = close_text + 2;
                    if let Some(close_url_rel) = body[url_start..].find(')') {
                        let link_text = &body[..close_text];
                        let link_url = &body[url_start..url_start + close_url_rel];
                        if !link_text.is_empty() {
                            spans.push(Span::styled(
                                link_text.to_string(),
                                Style::default()
                                    .fg(Color::Blue)
                                    .add_modifier(Modifier::UNDERLINED),
                            ));
                        }
                        if !link_url.is_empty() {
                            spans.push(Span::styled(
                                format!(" [{}]", link_url),
                                Style::default().fg(Color::DarkGray),
                            ));
                        }
                        1 + close_text + 2 + close_url_rel + 1
                    } else {
                        spans.push(Span::styled("[".to_string(), base_style));
                        1
                    }
                } else {
                    spans.push(Span::styled("[".to_string(), base_style));
                    1
                }
            }
            _ => 0,
        };

        i += consumed;
    }

    if spans.is_empty() {
        vec![Span::styled(String::new(), base_style)]
    } else {
        spans
    }
}

fn next_markdown_token(line: &str) -> Option<(usize, &'static str)> {
    let mut best: Option<(usize, &'static str)> = None;

    for token in ["https://", "http://", "**", "__", "*", "`", "["].iter() {
        if let Some(pos) = line.find(token) {
            best = match best {
                Some((best_pos, best_token)) => {
                    if pos < best_pos {
                        Some((pos, *token))
                    } else if pos == best_pos {
                        let next = if *token == "**" || *token == "__" {
                            *token
                        } else if best_token == "**" || best_token == "__" {
                            best_token
                        } else {
                            *token
                        };
                        Some((best_pos, next))
                    } else {
                        Some((best_pos, best_token))
                    }
                }
                None => Some((pos, *token)),
            };
        }
    }

    best
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
