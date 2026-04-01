use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::time::SystemTime;

use crate::tui::app::{App, DownloadHistoryStatus, DownloadState};

use super::helpers::{compact_bytes, compact_cell_text, help_text_style};

pub(super) fn draw_downloads_view(f: &mut Frame, app: &App, area: Rect) {
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

    let layout_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner_area);

    let summary = Paragraph::new(Line::from(vec![
        Span::styled(" Focus ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            if has_active {
                "Active queue"
            } else {
                "History"
            },
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("Actions ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[p] pause/resume  [c] cancel  [r] resume  [d] remove  [D] purge file",
            Style::default().fg(Color::Gray),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .title(" Control Panel "),
    );
    f.render_widget(summary, layout_sections[0]);

    if !has_active && !has_history {
        let p = Paragraph::new("No active downloads or history.").alignment(Alignment::Center);
        f.render_widget(p, layout_sections[1]);
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
            .split(layout_sections[1])
            .to_vec();
    } else {
        sections.push(layout_sections[1]);
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Active Downloads ");
    f.render_widget(&block, area);
    let inner_area = block.inner(area);

    let mut tracked_rows = Vec::<(u64, &str, &str, u64, f64, u64, u64, DownloadState)>::new();
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
        f.render_widget(
            Paragraph::new("No active download tasks.").alignment(Alignment::Center),
            inner_area,
        );
        return;
    }

    let inner_width = inner_area.width.saturating_sub(2) as usize;
    let model_width = (inner_width * 38 / 100).clamp(16, 34);
    let file_width = (inner_width * 30 / 100).clamp(12, 26);
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
    for (
        i,
        (
            _model_id,
            model_name,
            filename,
            version_id,
            progress,
            downloaded_bytes,
            total_bytes,
            state,
        ),
    ) in tracked_rows.iter().enumerate()
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
    let selected_idx = app
        .selected_download_index
        .min(tracked_rows.len().saturating_sub(1));
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
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    let mut list_state = ListState::default();
    if !rows.is_empty() {
        list_state.select(Some(selected_idx.saturating_sub(start_idx)));
    }
    f.render_stateful_widget(list, header_area[1], &mut list_state);
}

fn draw_download_history_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Download History ");

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
        "Age", "Status", "Model", "Version", "Prog", "File",
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
    let selected_idx = app
        .selected_history_index
        .min(items.len().saturating_sub(1));
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
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
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

pub(super) fn draw_settings_view(f: &mut Frame, app: &App, area: Rect) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(" Settings Control Panel ");
    f.render_widget(outer.clone(), area);
    let inner = outer.inner(area);
    let fm = &app.settings_form;

    let field_value = |idx: usize| -> String {
        if fm.editing && fm.focused_field == idx {
            return format!("{}█", fm.input_buffer);
        }
        match idx {
            0 => app
                .config
                .api_key
                .as_ref()
                .map(|key| format!("Present ({})", key.chars().take(5).collect::<String>()))
                .unwrap_or_else(|| "Not configured".to_string()),
            1 => app
                .config
                .comfyui_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Not configured".to_string()),
            2 => app
                .config
                .bookmark_file_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| {
                    crate::config::AppConfig::bookmark_path()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "Default".to_string()),
            3 => app
                .config
                .model_search_cache_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| {
                    app.config
                        .search_cache_path()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "Default".to_string()),
            4 => format!("{}h", app.config.model_search_cache_ttl_hours),
            5 => app
                .config
                .image_cache_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| {
                    app.config
                        .image_cache_path()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "Default".to_string()),
            6 => format!("{}m", app.config.image_search_cache_ttl_minutes),
            7 => format!("{}m", app.config.image_detail_cache_ttl_minutes),
            8 => {
                if app.config.image_cache_ttl_minutes == 0 {
                    "Persistent".to_string()
                } else {
                    format!("{}m", app.config.image_cache_ttl_minutes)
                }
            }
            9 => app.config.media_quality.label().to_string(),
            10 => app
                .config
                .download_history_file_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| {
                    app.config
                        .download_history_path()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "Default".to_string()),
            11 => "Delete search/detail/media caches".to_string(),
            _ => String::new(),
        }
    };

    let item_line = |idx: usize, label: &str| -> Line<'static> {
        let focused = fm.focused_field == idx;
        let label_style = if focused {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let value_style = if focused && fm.editing {
            Style::default().fg(Color::Yellow)
        } else if idx == 11 {
            Style::default().fg(Color::LightRed)
        } else {
            Style::default().fg(Color::Cyan)
        };
        Line::from(vec![
            Span::styled(if focused { "> " } else { "  " }, label_style),
            Span::styled(format!("{label}: "), label_style),
            Span::styled(field_value(idx), value_style),
        ])
    };

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Min(3),
        ])
        .split(inner);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(sections[0]);
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(sections[1]);
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(sections[2]);

    let access = Paragraph::new(vec![
        item_line(0, "API Key"),
        item_line(1, "ComfyUI"),
        item_line(2, "Bookmark File"),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Access & Paths "),
    )
    .wrap(Wrap { trim: true });
    f.render_widget(access, top[0]);

    let media = Paragraph::new(vec![
        item_line(9, "Media Quality"),
        Line::from(Span::styled(
            "  Left/Right cycles render preference",
            help_text_style(),
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Media "))
    .wrap(Wrap { trim: true });
    f.render_widget(media, top[1]);

    let model_cache = Paragraph::new(vec![
        item_line(3, "Model Cache Folder"),
        item_line(4, "Model Search TTL"),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Model Cache "),
    )
    .wrap(Wrap { trim: true });
    f.render_widget(model_cache, middle[0]);

    let image_cache = Paragraph::new(vec![
        item_line(5, "Image Cache Folder"),
        item_line(6, "Image Search TTL"),
        item_line(7, "Image Detail TTL"),
        item_line(8, "Image Binary TTL"),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Image Cache "),
    )
    .wrap(Wrap { trim: true });
    f.render_widget(image_cache, middle[1]);

    let storage = Paragraph::new(vec![item_line(10, "Download History File")])
        .block(Block::default().borders(Borders::ALL).title(" Storage "))
        .wrap(Wrap { trim: true });
    f.render_widget(storage, bottom[0]);

    let actions = Paragraph::new(vec![
        item_line(11, "Clear All Caches"),
        Line::from(Span::styled(
            "  Keeps settings, bookmarks, tags, and history",
            help_text_style(),
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Actions "))
    .wrap(Wrap { trim: true });
    f.render_widget(actions, bottom[1]);

    let hints = if fm.editing {
        " Type to edit the selected field. [Enter] Save  [Esc] Cancel "
    } else {
        " [j/k] Move  [Enter] Edit/Run  [h/l] Cycle selected enum/action "
    };
    let help = Paragraph::new(Line::from(Span::styled(hints, help_text_style())))
        .block(Block::default().borders(Borders::TOP).title(" Input "));
    f.render_widget(help, sections[3]);
}
