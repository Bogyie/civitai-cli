use anyhow::{Context, Result};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
    },
};
use ratatui_image::{StatefulImage, protocol::StatefulProtocol};
use std::io::{self, Stdout};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::tui::app::{
    App, AppMode, DownloadHistoryStatus, DownloadState, ImageSearchFormSection, MainTab,
    SearchFormMode, SearchFormSection, SearchFormState,
};
use crate::tui::image::{
    comfy_workflow_json, comfy_workflow_node_count, image_prompt, image_stats, image_tags,
    image_username,
};
use crate::tui::model::{
    build_model_url, category_name, creator_name, default_base_model, model_metrics, model_name,
    model_versions, tag_names,
};

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
            Constraint::Length(3), // Footer Status + shortcuts
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
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
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
                draw_search_popup(f, &app.search_form, "Search Builder", "Quick Search");
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
                draw_search_popup(
                    f,
                    &app.bookmark_search_form_draft,
                    "Bookmark Filters",
                    "Bookmark Search",
                );
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

fn draw_settings_tab(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Settings ");
    let fm = &app.settings_form;

    let mut lines = vec![
        Line::from(Span::styled(
            "--- Civitai CLI Configuration ---",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let api_key_val = if fm.editing && fm.focused_field == 0 {
        format!("{}█", fm.input_buffer)
    } else if let Some(key) = &app.config.api_key {
        format!(
            "Present (starts with {})",
            &key.chars().take(5).collect::<String>()
        )
    } else {
        "None (Restricted search and downloads)".to_string()
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 0 {
                "> API Key: "
            } else {
                "  API Key: "
            },
            Style::default().fg(if fm.focused_field == 0 {
                Color::Yellow
            } else {
                Color::White
            }),
        ),
        Span::styled(
            api_key_val,
            if fm.focused_field == 0 && fm.editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            },
        ),
    ]));

    let path_val = if fm.editing && fm.focused_field == 1 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config
            .comfyui_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 1 {
                "> ComfyUI Path: "
            } else {
                "  ComfyUI Path: "
            },
            Style::default().fg(if fm.focused_field == 1 {
                Color::Yellow
            } else {
                Color::White
            }),
        ),
        Span::styled(
            path_val,
            if fm.focused_field == 1 && fm.editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            },
        ),
    ]));

    let bookmark_path_val = if fm.editing && fm.focused_field == 2 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config
            .bookmark_file_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| {
                crate::config::AppConfig::bookmark_path()
                    .map(|path| path.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 2 {
                "> Bookmark File: "
            } else {
                "  Bookmark File: "
            },
            Style::default().fg(if fm.focused_field == 2 {
                Color::Yellow
            } else {
                Color::White
            }),
        ),
        Span::styled(
            bookmark_path_val,
            if fm.focused_field == 2 && fm.editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            },
        ),
    ]));

    let model_search_cache_path_val = if fm.editing && fm.focused_field == 3 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config
            .model_search_cache_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| {
                app.config
                    .search_cache_path()
                    .map(|path| path.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 3 {
                "> Model Search Cache Folder: "
            } else {
                "  Model Search Cache Folder: "
            },
            Style::default().fg(if fm.focused_field == 3 {
                Color::Yellow
            } else {
                Color::White
            }),
        ),
        Span::styled(
            model_search_cache_path_val,
            if fm.focused_field == 3 && fm.editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            },
        ),
    ]));

    let cache_ttl_val = if fm.editing && fm.focused_field == 4 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config.model_search_cache_ttl_hours.to_string()
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 4 {
                "> Model Search Cache TTL (hours): "
            } else {
                "  Model Search Cache TTL (hours): "
            },
            Style::default().fg(if fm.focused_field == 4 {
                Color::Yellow
            } else {
                Color::White
            }),
        ),
        Span::styled(
            cache_ttl_val,
            if fm.focused_field == 4 && fm.editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            },
        ),
    ]));

    let image_cache_path_val = if fm.editing && fm.focused_field == 5 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config
            .image_cache_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| {
                app.config
                    .image_cache_path()
                    .map(|path| path.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 5 {
                "> Image Cache Folder: "
            } else {
                "  Image Cache Folder: "
            },
            Style::default().fg(if fm.focused_field == 5 {
                Color::Yellow
            } else {
                Color::White
            }),
        ),
        Span::styled(
            image_cache_path_val,
            if fm.focused_field == 5 && fm.editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            },
        ),
    ]));

    let image_search_ttl_val = if fm.editing && fm.focused_field == 6 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config.image_search_cache_ttl_minutes.to_string()
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 6 {
                "> Image Search Cache TTL (minutes): "
            } else {
                "  Image Search Cache TTL (minutes): "
            },
            Style::default().fg(if fm.focused_field == 6 {
                Color::Yellow
            } else {
                Color::White
            }),
        ),
        Span::styled(
            image_search_ttl_val,
            if fm.focused_field == 6 && fm.editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            },
        ),
    ]));

    let image_cache_ttl_val = if fm.editing && fm.focused_field == 7 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config.image_cache_ttl_minutes.to_string()
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 7 {
                "> Image Cache TTL (minutes, 0 = persistent): "
            } else {
                "  Image Cache TTL (minutes, 0 = persistent): "
            },
            Style::default().fg(if fm.focused_field == 7 {
                Color::Yellow
            } else {
                Color::White
            }),
        ),
        Span::styled(
            image_cache_ttl_val,
            if fm.focused_field == 7 && fm.editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            },
        ),
    ]));

    let download_history_path_val = if fm.editing && fm.focused_field == 8 {
        format!("{}█", fm.input_buffer)
    } else {
        app.config
            .download_history_file_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| {
                app.config
                    .download_history_path()
                    .map(|path| path.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "Not Configured".to_string())
    };

    lines.push(Line::from(vec![
        Span::styled(
            if fm.focused_field == 8 {
                "> Download History File: "
            } else {
                "  Download History File: "
            },
            Style::default().fg(if fm.focused_field == 8 {
                Color::Yellow
            } else {
                Color::White
            }),
        ),
        Span::styled(
            download_history_path_val,
            if fm.focused_field == 8 && fm.editing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            },
        ),
    ]));

    lines.push(Line::from(""));
    if fm.editing {
        lines.push(Line::from(Span::styled(
            " [Type to edit] | [Enter] Save | [Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            " [Up/Down] Highlight | [Enter] Edit string",
            Style::default().fg(Color::DarkGray),
        )));
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
        let text = format!("Loading image {}/{}...", selected_index + 1, items.len());
        f.render_widget(Paragraph::new(text), inner_area);
    }
}

fn draw_image_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Image Details ");

    let Some(img) = app.selected_image_in_active_view() else {
        f.render_widget(Paragraph::new("No metadata available.").block(block), area);
        return;
    };

    let inner = block.inner(area);
    f.render_widget(&block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(if app.image_detail_expanded { 8 } else { 5 }),
            Constraint::Length(4),
            Constraint::Min(6),
        ])
        .split(inner);

    let dimensions = match (img.width, img.height) {
        (Some(width), Some(height)) => format!("{width}x{height}"),
        _ => "<none>".to_string(),
    };
    let username = image_username(img).unwrap_or_else(|| "<unknown>".to_string());
    let stats = image_stats(img);
    let prompt = image_prompt(img).unwrap_or_else(|| {
        if img.hide_meta.unwrap_or(false) {
            "Metadata hidden by source".to_string()
        } else {
            "<no prompt>".to_string()
        }
    });
    let image_meta_value = format!(
        "{} | {} | nsfw {}",
        img.r#type.as_deref().unwrap_or("image"),
        dimensions,
        img.combined_nsfw_level
            .or(img.nsfw_level)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "0".to_string())
    );
    let stats_primary_value = format!(
        "react {}  like {}  heart {}",
        compact_number(stats.reactions),
        compact_number(stats.likes),
        compact_number(stats.hearts)
    );
    let stats_secondary_value = format!(
        "cmt {}  collect {}  buzz {}",
        compact_number(stats.comments),
        compact_number(stats.collected),
        compact_number(stats.buzz)
    );
    let workflow_json = comfy_workflow_json(img);
    let workflow_label = workflow_json
        .as_ref()
        .map(|_| {
            format!(
                "Workflow available | nodes {}",
                comfy_workflow_node_count(img).unwrap_or(0)
            )
        })
        .unwrap_or_else(|| "No Comfy workflow metadata".to_string());
    let tags = image_tags(img);
    let tag_lines = if tags.is_empty() {
        vec![Line::from("<none>")]
    } else {
        wrap_joined_tags(&tags, sections[4].width.saturating_sub(2) as usize, 2)
    };
    let tag_count_value = format!("{} tag(s)", tags.len());
    let image_link = img.image_page_url();
    let advanced_json = img
        .metadata
        .as_ref()
        .and_then(|meta| serde_json::to_string_pretty(meta).ok())
        .unwrap_or_else(|| "<no metadata>".to_string());

    let image_lines = vec![
        Line::from(model_key_value_spans("Author", &username)),
        Line::from(model_key_value_spans(
            "Created",
            img.created_at.as_deref().unwrap_or("<none>"),
        )),
        Line::from(model_key_value_spans(
            "Image",
            &image_meta_value,
        )),
        Line::from(model_key_value_spans(
            "Base",
            img.base_model.as_deref().unwrap_or("<none>"),
        )),
    ];
    let stats_lines = vec![
        Line::from(model_key_value_spans(
            "Stats",
            &stats_primary_value,
        )),
        Line::from(model_key_value_spans(
            "More",
            &stats_secondary_value,
        )),
    ];
    let prompt_lines = wrap_text_lines(
        &prompt,
        sections[2].width.saturating_sub(2) as usize,
        if app.image_detail_expanded { 6 } else { 3 },
    );
    let comfy_lines = vec![
        Line::from(model_key_value_spans("Comfy", &workflow_label)),
        Line::from(model_key_value_spans(
            "Actions",
            "[w] Copy workflow | [W] Save workflow | [o] Copy image link",
        )),
    ];

    f.render_widget(
        Paragraph::new(image_lines)
            .block(Block::default().borders(Borders::ALL).title(" Image "))
            .wrap(Wrap { trim: true }),
        sections[0],
    );
    f.render_widget(
        Paragraph::new(stats_lines)
            .block(Block::default().borders(Borders::ALL).title(" Stats "))
            .wrap(Wrap { trim: true }),
        sections[1],
    );
    f.render_widget(
        Paragraph::new(prompt_lines)
            .block(Block::default().borders(Borders::ALL).title(" Prompt "))
            .wrap(Wrap { trim: true }),
        sections[2],
    );
    let mut tag_block_lines = vec![Line::from(model_key_value_spans(
        "Tags",
        &tag_count_value,
    ))];
    tag_block_lines.extend(tag_lines);
    tag_block_lines.push(Line::from(model_key_value_spans("Link", &image_link)));
    f.render_widget(
        Paragraph::new(comfy_lines)
            .block(Block::default().borders(Borders::ALL).title(" Comfy "))
            .wrap(Wrap { trim: true }),
        sections[3],
    );

    if app.image_advanced_visible {
        tag_block_lines.push(Line::from(""));
        tag_block_lines.extend(wrap_text_lines(
            &advanced_json,
            sections[4].width.saturating_sub(2) as usize,
            sections[4].height.saturating_sub(5) as usize,
        ));
    }
    f.render_widget(
        Paragraph::new(tag_block_lines)
            .block(Block::default().borders(Borders::ALL).title(if app.image_advanced_visible {
                " Tags / Advanced "
            } else {
                " Tags "
            }))
            .wrap(Wrap { trim: false }),
        sections[4],
    );
}

fn draw_model_search_summary(f: &mut Frame, app: &mut App, area: Rect) {
    let model_query = if app.search_form.query.is_empty() {
        "<empty>"
    } else {
        &app.search_form.query
    };
    let selected_types = if app.search_form.selected_types.is_empty() {
        "All".to_string()
    } else {
        app.search_form
            .selected_types
            .iter()
            .map(|item| item.label().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let selected_bases = if app.search_form.selected_base_models.is_empty() {
        "All".to_string()
    } else {
        app.search_form
            .selected_base_models
            .iter()
            .map(|item| item.label().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let summary = format!(
        "🔍 Query: \"{}\" | Type: {} | Sort: {} | Base: {} | Period: {}",
        model_query,
        selected_types,
        app.search_form
            .sort_options
            .get(app.search_form.selected_sort)
            .map(|sort| sort.label().to_string())
            .unwrap_or_else(|| "Relevance".into()),
        selected_bases,
        app.search_form
            .periods
            .get(app.search_form.selected_period)
            .map(|period| period.label())
            .unwrap_or("AllTime"),
    );

    let para = Paragraph::new(summary)
        .alignment(Alignment::Left)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::ITALIC),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

fn draw_bookmark_search_summary(f: &mut Frame, app: &App, area: Rect) {
    let query = if app.bookmark_search_form.query.is_empty() {
        "<all>"
    } else {
        &app.bookmark_search_form.query
    };
    let selected_types = if app.bookmark_search_form.selected_types.is_empty() {
        "All".to_string()
    } else {
        app.bookmark_search_form
            .selected_types
            .iter()
            .map(|item| item.label().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let selected_bases = if app.bookmark_search_form.selected_base_models.is_empty() {
        "All".to_string()
    } else {
        app.bookmark_search_form
            .selected_base_models
            .iter()
            .map(|item| item.label().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let summary = format!(
        "🔖 Query: \"{}\" | Type: {} | Sort: {} | Base: {} | Period: {} | Total: {}",
        query,
        selected_types,
        app.bookmark_search_form
            .sort_options
            .get(app.bookmark_search_form.selected_sort)
            .map(|sort| sort.label().to_string())
            .unwrap_or_else(|| "Relevance".into()),
        selected_bases,
        app.bookmark_search_form
            .periods
            .get(app.bookmark_search_form.selected_period)
            .map(|period| period.label())
            .unwrap_or("AllTime"),
        app.visible_bookmarks().len()
    );

    let para = Paragraph::new(summary)
        .alignment(Alignment::Left)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::ITALIC),
        )
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
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::ITALIC),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

fn draw_image_search_summary(f: &mut Frame, app: &App, area: Rect) {
    let query = if app.image_search_form.query.trim().is_empty() {
        "<all>"
    } else {
        app.image_search_form.query.trim()
    };
    let media_types = if app.image_search_form.selected_media_types.is_empty() {
        "All".to_string()
    } else {
        app.image_search_form
            .selected_media_types
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    let base_models = if app.image_search_form.selected_base_models.is_empty() {
        "All".to_string()
    } else {
        app.image_search_form
            .selected_base_models
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ratios = if app.image_search_form.selected_aspect_ratios.is_empty() {
        "All".to_string()
    } else {
        app.image_search_form
            .selected_aspect_ratios
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    let summary = format!(
        "🖼 Query: \"{}\" | Type: {} | Sort: {} | Base: {} | Ratio: {} | Period: {}",
        query,
        media_types,
        app.image_search_form
            .sort_options
            .get(app.image_search_form.selected_sort)
            .map(|value| value.label().to_string())
            .unwrap_or_else(|| "Relevance".into()),
        base_models,
        ratios,
        app.image_search_form
            .periods
            .get(app.image_search_form.selected_period)
            .map(|value| value.label())
            .unwrap_or("AllTime"),
    );

    let para = Paragraph::new(summary)
        .alignment(Alignment::Left)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::ITALIC),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

fn draw_model_list(
    f: &mut Frame,
    area: Rect,
    models: &[civitai_cli::sdk::SearchModelHit],
    list_state: &ListState,
    _show_model_details: bool,
    bookmarked_ids: &[u64],
    enable_name_rolling: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Searched Models ");

    if models.is_empty() {
        f.render_widget(
            Paragraph::new("No models found. Press '/' to search.").block(block),
            area,
        );
        return;
    }

    let selected_idx = list_state
        .selected()
        .unwrap_or(0)
        .min(models.len().saturating_sub(1));
    let inner_width = area.width.saturating_sub(2) as usize;
    let name_width = inner_width.max(1);

    let mut items: Vec<ListItem> = Vec::with_capacity(models.len());
    for (idx, model) in models.iter().enumerate() {
        let is_selected = idx == selected_idx;
        let metrics = model_metrics(model);
        let base_model = default_base_model(model).unwrap_or_else(|| "Unknown".to_string());
        let creator = creator_name(model).unwrap_or_else(|| "unknown".to_string());
        let mut display_name = model_name(model);
        let is_bookmarked = bookmarked_ids.contains(&model.id);

        if enable_name_rolling && is_selected && display_name.chars().count() > name_width {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_millis(0))
                .as_millis();
            let shift = ((now_ms / 260) as usize) % display_name.chars().count().max(1);
            display_name = rotate_left_chars(&display_name, shift);
        }

        let title_style = if is_bookmarked {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        };
        let mut line_two = format!(
            "{} | {} | dl {} like {} cmt {} | by {}",
            model.r#type.as_deref().unwrap_or("Model"),
            base_model,
            compact_number(metrics.download_count),
            compact_number(metrics.thumbs_up_count),
            compact_number(metrics.comment_count),
            creator
        );
        if model.nsfw.unwrap_or(false) {
            line_two.push_str(" | NSFW");
        }

        let second_line = if model.nsfw.unwrap_or(false) {
            let safe_prefix = line_two.trim_end_matches(" | NSFW").to_string();
            Line::from(vec![
                Span::styled(
                    compact_cell_text(safe_prefix, name_width.saturating_sub(7)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled("NSFW", Style::default().fg(Color::Red)),
            ])
        } else {
            Line::from(Span::styled(
                compact_cell_text(line_two, name_width),
                Style::default().fg(Color::DarkGray),
            ))
        };

        items.push(ListItem::new(vec![
            Line::from(Span::styled(
                compact_cell_text(display_name, name_width),
                title_style,
            )),
            second_line,
        ]));
    }

    let inner_area = block.inner(area);
    let visible_rows = (inner_area.height.max(2) as usize / 2).max(1);
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
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
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
    selected_model: Option<&civitai_cli::sdk::SearchModelHit>,
) {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Min(0),
        ])
        .split(area);

    if let Some(model) = selected_model {
        let versions = model_versions(model);
        let v_idx = *app.selected_version_index.get(&model.id).unwrap_or(&0);
        let safe_v_idx = v_idx.min(versions.len().saturating_sub(1));
        let selected_version = versions.get(safe_v_idx);
        let metrics = selected_version
            .map(|version| version.stats.clone())
            .unwrap_or_else(|| model_metrics(model));
        let creator = creator_name(model).unwrap_or_else(|| "unknown".to_string());
        let model_title = model_name(model);
        let model_url = build_model_url(model, selected_version.map(|version| version.id));
        let mut header_lines = vec![
            Line::from(Span::styled(
                compact_cell_text(model_title, split[0].width.saturating_sub(2) as usize),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                compact_cell_text(
                    format!(
                        "{} | by {}{}",
                        model.r#type.as_deref().unwrap_or("Model"),
                        creator,
                        if model.nsfw.unwrap_or(false) {
                            " | NSFW"
                        } else {
                            ""
                        }
                    ),
                    split[0].width.saturating_sub(2) as usize,
                ),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                compact_cell_text(
                    model_url,
                    split[0].width.saturating_sub(2) as usize,
                ),
                Style::default().fg(Color::Cyan),
            )),
        ];
        if let Some(version) = selected_version {
            let meta_line = format!(
                "Selected: {} | {}",
                version.name,
                version.base_model.as_deref().unwrap_or("Unknown base")
            );
            header_lines.push(Line::from(Span::styled(
                compact_cell_text(meta_line, split[0].width.saturating_sub(2) as usize),
                Style::default().fg(Color::Yellow),
            )));
        }
        let header = Paragraph::new(header_lines)
            .block(Block::default().borders(Borders::ALL).title(" Model "));
        f.render_widget(header, split[0]);

        let version_data: Vec<(String, bool)> = versions
            .iter()
            .enumerate()
            .map(|(idx, version)| (version.name.clone(), idx == safe_v_idx))
            .collect();
        let version_total = version_data.len();
        let version_position = if version_total > 0 {
            format!(" ({} / {})", safe_v_idx + 1, version_total)
        } else {
            " (0 / 0)".to_string()
        };
        let version_window =
            build_horizontal_item_window(&version_data, split[1].width.saturating_sub(2) as usize);
        let mut version_spans = Vec::with_capacity(version_window.len() + 2);
        for (i, (text, is_selected)) in version_window.iter().enumerate() {
            if !text.is_empty() {
                if i > 0 {
                    version_spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                }
                version_spans.push(Span::styled(
                    text,
                    if *is_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ));
            }
        }

        if version_spans.is_empty() {
            version_spans.push(Span::styled(
                "No versions",
                Style::default().fg(Color::DarkGray),
            ));
        }
        let version_row = Paragraph::new(Line::from(version_spans))
            .alignment(Alignment::Left)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Versions{}", version_position)),
            );
        f.render_widget(version_row, split[1]);

        let active_file = selected_version.and_then(|version| {
            let file_idx = *app.selected_file_index.get(&version.id).unwrap_or(&0);
            version
                .files
                .get(file_idx)
                .or_else(|| version.files.iter().find(|file| file.primary))
                .or_else(|| version.files.first())
        });
        let detail_row = vec![
            format!(
                "{:<10} {}",
                "Type",
                model.r#type.as_deref().unwrap_or("Model")
            ),
            format!(
                "{:<10} {}",
                "Base",
                selected_version
                    .and_then(|version| version.base_model.clone())
                    .or_else(|| default_base_model(model))
                    .unwrap_or_else(|| "Unknown".to_string())
            ),
            format!(
                "{:<10} {}",
                "Down",
                compact_number(metrics.download_count)
            ),
            format!(
                "{:<10} {}",
                "Likes",
                compact_number(metrics.thumbs_up_count)
            ),
            format!(
                "{:<10} {}",
                "Comments",
                compact_number(metrics.comment_count)
            ),
            format!(
                "{:<10} {:.1}",
                "Rating",
                metrics.rating
            ),
            format!(
                "{:<10} {}",
                "Format",
                active_file
                    .and_then(|file| file.format.clone())
                    .unwrap_or_else(|| "N/A".to_string())
            ),
            format!(
                "{:<10} {}",
                "Size",
                active_file
                    .and_then(|file| file.size_kb)
                    .map(compact_file_size)
                    .unwrap_or_else(|| "N/A".to_string())
            ),
        ];
        let metadata_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(split[2]);
        let metadata = Paragraph::new(
            detail_row
                .into_iter()
                .map(Line::from)
                .collect::<Vec<_>>(),
        )
        .block(Block::default().borders(Borders::ALL).title(" Metadata "));
        f.render_widget(metadata, metadata_split[0]);
        let tags = tag_names(model);
        let tag_lines = if tags.is_empty() {
            vec![Line::from("No tags available.")]
        } else {
            wrap_joined_tags(
                &tags,
                metadata_split[1].width.saturating_sub(2) as usize,
                metadata_split[1].height.saturating_sub(2) as usize,
            )
        };
        let tags_widget = Paragraph::new(tag_lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Tags "));
        f.render_widget(tags_widget, metadata_split[1]);

        let file_lines = if let Some(version) = selected_version {
            if version.files.is_empty() {
                vec![Line::from("No files available for this version.")]
            } else {
                version
                    .files
                    .iter()
                    .enumerate()
                    .take(split[3].height.saturating_sub(2) as usize)
                    .map(|(idx, file)| {
                        let selected_idx = *app.selected_file_index.get(&version.id).unwrap_or(&0);
                        let is_selected = idx == selected_idx;
                        let prefix = if is_selected {
                            "> "
                        } else if file.primary {
                            "* "
                        } else {
                            "  "
                        };
                        let summary = format!(
                            "{}{} | {}{}{}",
                            prefix,
                            file.name,
                            file.format.as_deref().unwrap_or("file"),
                            file.fp
                                .as_deref()
                                .map(|value| format!("/{value}"))
                                .unwrap_or_default(),
                            file.size_kb
                                .map(|value| format!(" | {}", compact_file_size(value)))
                                .unwrap_or_default(),
                        );
                        Line::from(Span::styled(
                            compact_cell_text(summary, split[3].width.saturating_sub(2) as usize),
                            if is_selected {
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD)
                            } else if file.primary {
                                Style::default().fg(Color::Yellow)
                            } else {
                                Style::default()
                            },
                        ))
                    })
                    .collect::<Vec<_>>()
            }
        } else {
            vec![Line::from("No versions available.")]
        };
        let files = Paragraph::new(file_lines)
            .block(Block::default().borders(Borders::ALL).title(" Files "));
        f.render_widget(files, split[3]);

        let desc_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(split[4]);

        let description_text_value = model
            .extras
            .get("description")
            .and_then(|value| value.as_str().map(str::to_string))
            .or_else(|| {
                selected_version
                    .and_then(|version| version.description.as_deref())
                    .filter(|desc| !desc.is_empty())
                    .map(str::to_string)
            })
            .or_else(|| category_name(model).map(|category| format!("Category: {category}")))
            .unwrap_or_else(|| "No description available.".to_string());
        let description_lines = render_description_lines(&description_text_value);
        let description_text = Paragraph::new(description_lines)
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Description "),
            );
        f.render_widget(description_text, desc_split[0]);

        let image_block = Block::default()
            .borders(Borders::ALL)
            .title(" Cover Image ");
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

        let has_any_cached_image = versions
            .iter()
            .any(|version| app.model_version_image_cache.contains_key(&version.id));
        let is_waiting_for_selected_version = selected_version
            .map(|version| !app.model_version_image_failed.contains(&version.id))
            .unwrap_or(false);

        let mut image_version_id = selected_version.map(|version| version.id);
        if image_version_id.is_none() {
            image_version_id = versions.iter().find_map(|version| {
                if app.model_version_image_cache.contains_key(&version.id) {
                    Some(version.id)
                } else {
                    None
                }
            });
        }

        if let Some(image_version_id) = image_version_id {
            let protocol = app
                .model_version_image_cache
                .get_mut(&image_version_id)
                .and_then(|protocols| protocols.get_mut(0));
            if let Some(mut protocol) = protocol {
                let image_widget: StatefulImage<StatefulProtocol> = StatefulImage::new();
                f.render_stateful_widget(image_widget, inner_img_area, &mut protocol);
            } else {
                if app.model_version_image_failed.contains(&image_version_id) {
                    f.render_widget(
                        Paragraph::new("No thumbnail available.").alignment(Alignment::Center),
                        inner_img_area,
                    );
                } else {
                    f.render_widget(
                        Paragraph::new("Loading thumbnail...").alignment(Alignment::Center),
                        inner_img_area,
                    );
                }
            }
        } else {
            if has_any_cached_image || is_waiting_for_selected_version {
                f.render_widget(
                    Paragraph::new("Loading thumbnail...").alignment(Alignment::Center),
                    inner_img_area,
                );
            } else {
                f.render_widget(
                    Paragraph::new("No thumbnail available.").alignment(Alignment::Center),
                    inner_img_area,
                );
            }
        }
    } else {
        f.render_widget(
            Paragraph::new("Select a model.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Model Details "),
            ),
            area,
        );
    }
}

fn draw_search_popup(f: &mut Frame, fm: &SearchFormState, builder_title: &str, quick_title: &str) {
    if fm.mode == SearchFormMode::Quick {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {quick_title} "));
        let lines = vec![
            Line::from(vec![
                Span::styled(" Query: ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{}█", fm.query)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                format!(
                    " Current: sort={} | types={} | bases={} | period={} ",
                    fm.sort_options
                        .get(fm.selected_sort)
                        .map(|sort| sort.label().to_string())
                        .unwrap_or_else(|| "Relevance".to_string()),
                    fm.selected_types.len(),
                    fm.selected_base_models.len(),
                    fm.periods
                        .get(fm.selected_period)
                        .map(|period| period.label())
                        .unwrap_or("AllTime")
                ),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                " [Type] Query | [Enter] Apply | [Esc] Cancel | [f] Open Builder ",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
        let area = centered_rect(60, 24, f.area());
        f.render_widget(Clear, area);
        f.render_widget(p, area);
        return;
    }

    let area = centered_rect(84, 82, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {builder_title} "));
    f.render_widget(&block, area);
    let inner = block.inner(area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(inner);

    let current_type = fm
        .type_options
        .get(fm.type_cursor)
        .map(|item| item.label().to_string())
        .unwrap_or_else(|| "None".to_string());
    let current_base = fm
        .base_options
        .get(fm.base_cursor)
        .map(|item| item.label().to_string())
        .unwrap_or_else(|| "None".to_string());
    let query_focused = fm.focused_section == SearchFormSection::Query;
    let sort_focused = fm.focused_section == SearchFormSection::Sort;
    let period_focused = fm.focused_section == SearchFormSection::Period;
    let type_focused = fm.focused_section == SearchFormSection::Type;
    let base_focused = fm.focused_section == SearchFormSection::BaseModel;
    let query_is_configured = !fm.query.trim().is_empty();
    let sort_is_configured = true;
    let period_is_configured = true;
    let type_is_configured = !fm.selected_types.is_empty();
    let base_is_configured = !fm.selected_base_models.is_empty();
    let sort_items = fm
        .sort_options
        .iter()
        .enumerate()
        .map(|(idx, sort)| {
            (
                sort.label().to_string(),
                idx == fm.selected_sort,
                idx == fm.selected_sort,
            )
        })
        .collect::<Vec<_>>();
    let period_items = fm
        .periods
        .iter()
        .enumerate()
        .map(|(idx, period)| {
            (
                period.label().to_string(),
                idx == fm.selected_period,
                idx == fm.selected_period,
            )
        })
        .collect::<Vec<_>>();
    let type_items = fm
        .type_options
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            (
                item.label().to_string(),
                idx == fm.type_cursor,
                fm.selected_types.contains(item),
            )
        })
        .collect::<Vec<_>>();
    let base_items = fm
        .base_options
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            (
                item.label().to_string(),
                idx == fm.base_cursor,
                fm.selected_base_models.contains(item),
            )
        })
        .collect::<Vec<_>>();
    let query_box = Paragraph::new(vec![Line::from(vec![
        Span::styled(
            if query_focused { "> " } else { "  " },
            if query_focused {
                Style::default().fg(Color::Yellow)
            } else {
                inactive_box_style(query_is_configured)
            },
        ),
        Span::styled(
            format!("{}{}", fm.query, if query_focused { "█" } else { "" }),
            if query_focused {
                Style::default().fg(Color::White)
            } else {
                inactive_box_style(query_is_configured)
            },
        ),
    ])])
    .block(styled_search_block(" Query ", query_focused, query_is_configured))
    .wrap(Wrap { trim: true });
    f.render_widget(query_box, sections[0]);

    let mut sort_lines = vec![Line::from(Span::styled(
        "Browse with Left/Right",
        if sort_focused {
            Style::default().fg(Color::Yellow)
        } else {
            inactive_box_style(sort_is_configured)
        },
    ))];
    sort_lines.extend(build_wrapped_option_lines(
        &sort_items,
        sections[1].width.saturating_sub(4) as usize,
        sections[1].height.saturating_sub(3) as usize,
        sort_focused,
    ));
    let sort_box = Paragraph::new(sort_lines)
        .block(styled_search_block(" Sort ", sort_focused, sort_is_configured))
        .wrap(Wrap { trim: true });
    f.render_widget(sort_box, sections[1]);

    let mut period_lines = vec![Line::from(Span::styled(
        "Browse with Left/Right",
        if period_focused {
            Style::default().fg(Color::Yellow)
        } else {
            inactive_box_style(period_is_configured)
        },
    ))];
    period_lines.extend(build_wrapped_option_lines(
        &period_items,
        sections[2].width.saturating_sub(4) as usize,
        sections[2].height.saturating_sub(3) as usize,
        period_focused,
    ));
    let period_box = Paragraph::new(period_lines)
        .block(styled_search_block(" Period ", period_focused, period_is_configured))
        .wrap(Wrap { trim: true });
    f.render_widget(period_box, sections[2]);

    let type_header = format!(
        "{} Current: {} {}",
        if type_focused { ">" } else { " " },
        current_type,
        if fm
            .type_options
            .get(fm.type_cursor)
            .is_some_and(|item| fm.selected_types.contains(item))
        {
            "[x]"
        } else {
            "[ ]"
        }
    );
    let mut type_lines = vec![Line::from(Span::styled(
        "Browse with Left/Right, toggle with Space",
        if type_focused {
            Style::default().fg(Color::Yellow)
        } else {
            inactive_box_style(type_is_configured)
        },
    ))];
    type_lines.extend(build_wrapped_option_lines(
        &type_items,
        sections[3].width.saturating_sub(4) as usize,
        3,
        type_focused,
    ));
    type_lines.push(Line::from(""));
    type_lines.push(Line::from(Span::styled(
        "Selected",
        if type_focused {
            Style::default().fg(Color::DarkGray)
        } else {
            inactive_box_style(type_is_configured)
        },
    )));
    type_lines.extend(if fm.selected_types.is_empty() {
        vec![Line::from(Span::styled(
            "All",
            if type_focused {
                Style::default().fg(Color::White)
            } else {
                inactive_box_style(type_is_configured)
            },
        ))]
    } else {
        let selected = fm
            .selected_types
            .iter()
            .map(|item| item.label().to_string())
            .collect::<Vec<_>>();
        style_lines(
            wrap_joined_tags(
                &selected,
                sections[3].width.saturating_sub(4) as usize,
                sections[3].height.saturating_sub(7) as usize,
            ),
            if type_focused {
                Style::default().fg(Color::White)
            } else {
                inactive_box_style(type_is_configured)
            },
        )
    });
    let mut type_block_lines = vec![Line::from(Span::styled(
        type_header,
        if type_focused {
            Style::default().fg(Color::Yellow)
        } else if type_is_configured {
            inactive_box_style(true)
        } else {
            inactive_box_style(false)
        },
    ))];
    type_block_lines.extend(type_lines);
    let type_widget = Paragraph::new(type_block_lines)
        .block(styled_search_block(" Type ", type_focused, type_is_configured))
        .wrap(Wrap { trim: true });
    f.render_widget(type_widget, sections[3]);

    let base_header = format!(
        "{} Current: {} {}",
        if base_focused { ">" } else { " " },
        current_base,
        if fm
            .base_options
            .get(fm.base_cursor)
            .is_some_and(|item| fm.selected_base_models.contains(item))
        {
            "[x]"
        } else {
            "[ ]"
        }
    );
    let mut base_lines = vec![Line::from(Span::styled(
        "Browse with Left/Right, toggle with Space",
        if base_focused {
            Style::default().fg(Color::Yellow)
        } else {
            inactive_box_style(base_is_configured)
        },
    ))];
    base_lines.extend(build_wrapped_option_lines(
        &base_items,
        sections[4].width.saturating_sub(4) as usize,
        4,
        base_focused,
    ));
    base_lines.push(Line::from(""));
    base_lines.push(Line::from(Span::styled(
        "Selected",
        if base_focused {
            Style::default().fg(Color::DarkGray)
        } else {
            inactive_box_style(base_is_configured)
        },
    )));
    base_lines.extend(if fm.selected_base_models.is_empty() {
        vec![Line::from(Span::styled(
            "All",
            if base_focused {
                Style::default().fg(Color::White)
            } else {
                inactive_box_style(base_is_configured)
            },
        ))]
    } else {
        let selected = fm
            .selected_base_models
            .iter()
            .map(|item| item.label().to_string())
            .collect::<Vec<_>>();
        style_lines(
            wrap_joined_tags(
                &selected,
                sections[4].width.saturating_sub(4) as usize,
                sections[4].height.saturating_sub(8) as usize,
            ),
            if base_focused {
                Style::default().fg(Color::White)
            } else {
                inactive_box_style(base_is_configured)
            },
        )
    });
    let mut base_block_lines = vec![Line::from(Span::styled(
        base_header,
        if base_focused {
            Style::default().fg(Color::Yellow)
        } else if base_is_configured {
            inactive_box_style(true)
        } else {
            inactive_box_style(false)
        },
    ))];
    base_block_lines.extend(base_lines);
    let base_widget = Paragraph::new(base_block_lines)
        .block(styled_search_block(" Base Model ", base_focused, base_is_configured))
        .wrap(Wrap { trim: true });
    f.render_widget(base_widget, sections[4]);

    let help = Paragraph::new(" [Up/Down] Section | [Left/Right] Change | [Space] Toggle | [Enter] Apply | [Esc] Cancel ")
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    f.render_widget(help, sections[5]);
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
            Span::styled(
                format!("{} Path: ", action),
                Style::default().fg(Color::Yellow),
            ),
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
    let form = &app.image_search_form;
    let area = centered_rect(84, 82, f.area());
    f.render_widget(Clear, area);

    if form.mode == SearchFormMode::Quick {
        let lines = vec![
            Line::from(vec![
                Span::styled(" Query: ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{}█", form.query)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                format!(
                    " Current: sort={} | types={} | bases={} | ratios={} | period={} ",
                    form.sort_options
                        .get(form.selected_sort)
                        .map(|sort| sort.label().to_string())
                        .unwrap_or_else(|| "Relevance".to_string()),
                    form.selected_media_types.len(),
                    form.selected_base_models.len(),
                    form.selected_aspect_ratios.len(),
                    form.periods
                        .get(form.selected_period)
                        .map(|period| period.label())
                        .unwrap_or("AllTime")
                ),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                " [Type] Query | [Enter] Apply | [Esc] Cancel | [f] Open Builder ",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        let p = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Image Search "))
            .wrap(Wrap { trim: true });
        let quick_area = centered_rect(60, 24, f.area());
        f.render_widget(Clear, quick_area);
        f.render_widget(p, quick_area);
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Image Filters ");
    f.render_widget(&block, area);
    let inner = block.inner(area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(7),
            Constraint::Length(14),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
        .split(inner);

    let query_focused = form.focused_section == ImageSearchFormSection::Query;
    let sort_focused = form.focused_section == ImageSearchFormSection::Sort;
    let period_focused = form.focused_section == ImageSearchFormSection::Period;
    let type_focused = form.focused_section == ImageSearchFormSection::MediaType;
    let base_focused = form.focused_section == ImageSearchFormSection::BaseModel;
    let ratio_focused = form.focused_section == ImageSearchFormSection::AspectRatio;

    let sort_items = form
        .sort_options
        .iter()
        .enumerate()
        .map(|(idx, sort)| {
            (
                sort.label().to_string(),
                idx == form.selected_sort,
                idx == form.selected_sort,
            )
        })
        .collect::<Vec<_>>();
    let period_items = form
        .periods
        .iter()
        .enumerate()
        .map(|(idx, period)| {
            (
                period.label().to_string(),
                idx == form.selected_period,
                idx == form.selected_period,
            )
        })
        .collect::<Vec<_>>();
    let type_items = form
        .media_type_options
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            (
                item.label().to_string(),
                idx == form.media_type_cursor,
                form.selected_media_types.contains(item.as_query_value()),
            )
        })
        .collect::<Vec<_>>();
    let base_items = form
        .base_options
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            (
                item.label().to_string(),
                idx == form.base_cursor,
                form.selected_base_models.contains(item.as_query_value()),
            )
        })
        .collect::<Vec<_>>();
    let ratio_items = form
        .aspect_ratio_options
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            (
                item.label().to_string(),
                idx == form.aspect_ratio_cursor,
                form.selected_aspect_ratios.contains(item.as_query_value()),
            )
        })
        .collect::<Vec<_>>();

    let query_box = Paragraph::new(vec![Line::from(vec![
        Span::styled(
            if query_focused { "> " } else { "  " },
            if query_focused {
                Style::default().fg(Color::Yellow)
            } else {
                inactive_box_style(!form.query.trim().is_empty())
            },
        ),
        Span::styled(
            format!("{}{}", form.query, if query_focused { "█" } else { "" }),
            if query_focused {
                Style::default().fg(Color::White)
            } else {
                inactive_box_style(!form.query.trim().is_empty())
            },
        ),
    ])])
    .block(styled_search_block(" Query ", query_focused, !form.query.trim().is_empty()))
    .wrap(Wrap { trim: true });
    f.render_widget(query_box, sections[0]);

    let mut sort_lines = vec![Line::from(Span::styled(
        "Browse with Left/Right",
        if sort_focused {
            Style::default().fg(Color::Yellow)
        } else {
            inactive_box_style(true)
        },
    ))];
    sort_lines.extend(build_wrapped_option_lines(
        &sort_items,
        sections[1].width.saturating_sub(4) as usize,
        sections[1].height.saturating_sub(3) as usize,
        sort_focused,
    ));
    f.render_widget(
        Paragraph::new(sort_lines)
            .block(styled_search_block(" Sort ", sort_focused, true))
            .wrap(Wrap { trim: true }),
        sections[1],
    );

    let mut period_lines = vec![Line::from(Span::styled(
        "Browse with Left/Right",
        if period_focused {
            Style::default().fg(Color::Yellow)
        } else {
            inactive_box_style(true)
        },
    ))];
    period_lines.extend(build_wrapped_option_lines(
        &period_items,
        sections[2].width.saturating_sub(4) as usize,
        sections[2].height.saturating_sub(3) as usize,
        period_focused,
    ));
    f.render_widget(
        Paragraph::new(period_lines)
            .block(styled_search_block(" Period ", period_focused, true))
            .wrap(Wrap { trim: true }),
        sections[2],
    );

    f.render_widget(
        Paragraph::new(build_image_filter_box_lines(
            "Media Type",
            type_focused,
            !form.selected_media_types.is_empty(),
            &type_items,
            &form
                .selected_media_types
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            false,
            sections[3].width.saturating_sub(4) as usize,
            sections[3].height.saturating_sub(3) as usize,
        ))
        .block(styled_search_block(
            " Media Type ",
            type_focused,
            !form.selected_media_types.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[3],
    );

    f.render_widget(
        Paragraph::new(build_image_filter_box_lines(
            "Base Model",
            base_focused,
            !form.selected_base_models.is_empty(),
            &base_items,
            &form
                .selected_base_models
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            false,
            sections[4].width.saturating_sub(4) as usize,
            sections[4].height.saturating_sub(3) as usize,
        ))
        .block(styled_search_block(
            " Base Model ",
            base_focused,
            !form.selected_base_models.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[4],
    );

    f.render_widget(
        Paragraph::new(build_image_filter_box_lines(
            "Aspect Ratio",
            ratio_focused,
            !form.selected_aspect_ratios.is_empty(),
            &ratio_items,
            &form
                .selected_aspect_ratios
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            false,
            sections[5].width.saturating_sub(4) as usize,
            sections[5].height.saturating_sub(3) as usize,
        ))
        .block(styled_search_block(
            " Aspect Ratio ",
            ratio_focused,
            !form.selected_aspect_ratios.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[5],
    );

    f.render_widget(
        Paragraph::new(
            " [Up/Down] Section | [Left/Right] Change | [Space] Toggle | [Enter] Apply | [Esc] Cancel ",
        )
        .wrap(Wrap { trim: true }),
        sections[6],
    );
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
        Line::from(Span::styled(
            " [m] Close | [Esc] Close ",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::DarkGray),
        )),
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
        .map(model_name)
        .unwrap_or_else(|| "selected bookmark".to_string());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Remove Bookmark ");
    let lines = vec![
        Line::from(format!("Remove bookmark: {}", name)),
        Line::from(""),
        Line::from(Span::styled(
            "Press Y to confirm, N or Esc to cancel.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let p = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);
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
        .constraints([Constraint::Length(1), Constraint::Min(2)])
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
        MainTab::Models => {
            "[j/k] Move | [g/G] Top/Bottom | [Ctrl-u/d] Jump | [/] Search | [f] Filter | [v] Details | [[/]] Version | [J/K] File | [d] Download | [b] Bookmark | [r] Refresh | [c] Clear | [?] Help"
        }
        MainTab::Bookmarks => {
            "[j/k] Move | [g/G] Top/Bottom | [Ctrl-u/d] Jump | [/] Search | [f] Filter | [v] Details | [[/]] Version | [J/K] File | [d] Download | [b] Remove | [e] Export | [i] Import"
        }
        MainTab::Images => "[j/k] Move | [g/G] Top/Bottom | [Ctrl-u/d] Jump | [/] Search | [f] Filter | [d] Download | [w/W] Workflow | [o] Copy Link | [m] Expand | [a] Advanced | [b] Bookmark",
        MainTab::ImageBookmarks => "[/] Search | [b] Remove | [d] Download | [m] Status",
        MainTab::Downloads => {
            "[j/k or J/K] Move | [d] Delete history | [D] Delete history + file | [r] Resume | [p] Pause/Resume | [c] Cancel"
        }
        MainTab::Settings => "[Enter] Edit | [m] Status",
    };

    let shortcuts_row = Paragraph::new(Span::styled(
        shortcuts,
        Style::default().fg(Color::DarkGray),
    ))
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: true });
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

fn model_key_value_spans<'a>(key: &'a str, value: &'a str) -> Vec<Span<'a>> {
    vec![
        Span::styled(
            format!("{key}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ]
}

fn inactive_box_style(is_configured: bool) -> Style {
    if is_configured {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::DIM)
    } else {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    }
}

fn inactive_box_border_style(is_configured: bool) -> Style {
    if is_configured {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    }
}

fn styled_search_block<'a>(title: &'a str, focused: bool, is_configured: bool) -> Block<'a> {
    let style = if focused {
        Style::default().fg(Color::White)
    } else {
        inactive_box_border_style(is_configured)
    };

    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(Span::styled(title, style))
}

fn build_wrapped_option_lines(
    items: &[(String, bool, bool)],
    max_width: usize,
    max_lines: usize,
    box_focused: bool,
) -> Vec<Line<'static>> {
    if items.is_empty() || max_width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut spans = Vec::new();
    let mut used_width = 0usize;

    for (idx, (text, focused, checked)) in items.iter().enumerate() {
        let label = if *checked {
            format!("[x] {text}")
        } else {
            format!("[ ] {text}")
        };
        let token_width = label.chars().count();
        let separator_width = if spans.is_empty() { 0 } else { 2 };

        if !spans.is_empty() && used_width + separator_width + token_width > max_width {
            lines.push(Line::from(std::mem::take(&mut spans)));
            if lines.len() >= max_lines {
                break;
            }
            used_width = 0;
        }

        if !spans.is_empty() {
            spans.push(Span::raw("  "));
            used_width += 2;
        }

        let style = if box_focused && *focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else if box_focused && *checked {
            Style::default().fg(Color::Green)
        } else if *checked {
            Style::default().fg(Color::Yellow)
        } else {
            if box_focused {
                Style::default().fg(Color::White)
            } else {
                inactive_box_style(false)
            }
        };

        spans.push(Span::styled(
            compact_cell_text(label, max_width),
            style,
        ));
        used_width += token_width.min(max_width);

        if idx == items.len() - 1 && !spans.is_empty() && lines.len() < max_lines {
            lines.push(Line::from(std::mem::take(&mut spans)));
        }
    }

    if lines.len() == max_lines {
        return lines;
    }

    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }

    lines
}

fn build_horizontal_item_window(items: &[(String, bool)], max_width: usize) -> Vec<(String, bool)> {
    if items.is_empty() || max_width == 0 {
        return Vec::new();
    }

    let item_count = items.len();
    let selected_idx = items
        .iter()
        .position(|(_, selected)| *selected)
        .unwrap_or(0)
        .min(item_count - 1);
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

fn build_image_filter_box_lines(
    label: &str,
    focused: bool,
    configured: bool,
    items: &[(String, bool, bool)],
    selected: &[String],
    show_selected: bool,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let current = items
        .iter()
        .find(|(_, current, _)| *current)
        .map(|(text, _, checked)| format!(
            "{} Current: {} {}",
            if focused { ">" } else { " " },
            text,
            if *checked { "[x]" } else { "[ ]" }
        ))
        .unwrap_or_else(|| format!("{} Current: <none>", if focused { ">" } else { " " }));

    let mut lines = vec![Line::from(Span::styled(
        current,
        if focused {
            Style::default().fg(Color::Yellow)
        } else if configured {
            inactive_box_style(true)
        } else {
            inactive_box_style(false)
        },
    ))];
    lines.push(Line::from(Span::styled(
        format!("Browse {} with Left/Right, toggle with Space", label),
        if focused {
            Style::default().fg(Color::Yellow)
        } else if configured {
            inactive_box_style(true)
        } else {
            inactive_box_style(false)
        },
    )));
    lines.extend(build_wrapped_option_lines(items, width, 2, focused));
    if show_selected {
        lines.push(Line::from(Span::styled(
            "Selected",
            if focused {
                Style::default().fg(Color::DarkGray)
            } else if configured {
                inactive_box_style(true)
            } else {
                inactive_box_style(false)
            },
        )));
        if selected.is_empty() {
            lines.push(Line::from(Span::styled(
                "All",
                if focused {
                    Style::default().fg(Color::White)
                } else if configured {
                    inactive_box_style(true)
                } else {
                    inactive_box_style(false)
                },
            )));
        } else {
            lines.extend(style_lines(
                wrap_joined_tags(selected, width, height.saturating_sub(5)),
                if focused {
                    Style::default().fg(Color::White)
                } else if configured {
                    inactive_box_style(true)
                } else {
                    inactive_box_style(false)
                },
            ));
        }
    }
    lines
}

fn wrap_text_lines(text: &str, width: usize, height: usize) -> Vec<Line<'static>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let chars = text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return vec![Line::from("")];
    }

    let mut lines = Vec::new();
    let mut start = 0usize;
    while start < chars.len() && lines.len() < height {
        let end = (start + width).min(chars.len());
        let mut line = chars[start..end].iter().collect::<String>();
        if end < chars.len() && lines.len() + 1 == height && width > 3 {
            line = compact_cell_text(format!("{}...", line), width);
        }
        lines.push(Line::from(line));
        start = end;
    }
    lines
}

fn style_lines(lines: Vec<Line<'static>>, style: Style) -> Vec<Line<'static>> {
    lines.into_iter()
        .map(|line| {
            let text = line
                .spans
                .into_iter()
                .map(|span| span.content)
                .collect::<Vec<_>>()
                .join("");
            Line::from(Span::styled(text, style))
        })
        .collect()
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

fn compact_file_size(size_kb: f64) -> String {
    if size_kb <= 0.0 {
        return "0.0 MB".to_string();
    }

    let mb = 1024.0; // 1 MB = 1024 KB
    let gb = mb * 1024.0; // 1 GB = 1024 MB
    let tb = gb * 1024.0; // 1 TB = 1024 GB

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

fn wrap_joined_tags(tags: &[String], width: usize, height: usize) -> Vec<Line<'static>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let joined = tags.join(", ");
    let chars: Vec<char> = joined.chars().collect();
    if chars.is_empty() {
        return vec![Line::from("")];
    }

    let mut lines = Vec::new();
    let mut start = 0usize;
    while start < chars.len() && lines.len() < height {
        let end = (start + width).min(chars.len());
        let mut line = chars[start..end].iter().collect::<String>();
        if end < chars.len() {
            if let Some(last_sep) = line.rfind(", ") {
                if last_sep > 0 {
                    line.truncate(last_sep + 1);
                    start += last_sep + 2;
                } else {
                    start = end;
                }
            } else {
                start = end;
            }
        } else {
            start = end;
        }
        lines.push(Line::from(line.trim().to_string()));
    }

    if start < chars.len() && !lines.is_empty() {
        let last_index = lines.len() - 1;
        let mut last = lines[last_index]
            .spans
            .first()
            .map(|span| span.content.to_string())
            .unwrap_or_default();
        let target_width = width.saturating_sub(3);
        if last.chars().count() > target_width {
            last = compact_cell_text(last, target_width);
        }
        lines[last_index] = Line::from(format!("{last}..."));
    }

    lines
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
            spans.extend(parse_styled_markdown(
                &line[3..],
                Style::default().fg(Color::White),
            ));
            lines.push(Line::from(spans));
            continue;
        }

        if line.starts_with("› ") {
            let mut spans = vec![Span::styled("› ", Style::default().fg(Color::DarkGray))];
            spans.extend(parse_styled_markdown(
                &line[3..],
                Style::default().fg(Color::Gray),
            ));
            lines.push(Line::from(spans));
            continue;
        }

        lines.push(Line::from(parse_styled_markdown(
            line,
            Style::default().fg(Color::White),
        )));
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
                        spans.push(Span::styled(
                            styled.to_string(),
                            base_style.add_modifier(Modifier::BOLD),
                        ));
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
                        spans.push(Span::styled(
                            styled.to_string(),
                            base_style.add_modifier(Modifier::BOLD),
                        ));
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
                let trimmed = raw_url.trim_end_matches(|ch| {
                    ch == '.' || ch == ',' || ch == ')' || ch == ']' || ch == '}' || ch == '>'
                });
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
