mod helpers;

use self::helpers::{
    build_horizontal_item_window, build_image_filter_box_lines,
    build_image_modal_constraints, build_model_modal_constraints, build_text_filter_box_lines,
    build_wrapped_option_lines, centered_rect, compact_bytes, compact_cell_text,
    compact_file_size, compact_number, help_text_style, inactive_box_style,
    model_key_value_spans, render_description_lines, rotate_left_chars,
    styled_search_block, wrap_joined_tags, wrap_text_lines,
};
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
    comfy_workflow_json, comfy_workflow_node_count, image_generation_info, image_negative_prompt,
    image_prompt, image_stats, image_tags, image_used_models, image_username,
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
                .title(" Civitai CLI | [1-6] Switch tab "),
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
            f.render_widget(Clear, chunks[1]);
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
            f.render_widget(Clear, chunks[1]);
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

    if app.show_help_modal {
        draw_help_modal(f, app);
    }

    if app.show_image_prompt_modal {
        draw_image_prompt_modal(f, app);
    }

    if app.show_image_model_detail_modal {
        draw_image_model_detail_modal(f, app);
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

    let layout_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner_area);

    let summary = Paragraph::new(Line::from(vec![
        Span::styled(" Focus ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            if has_active { "Active queue" } else { "History" },
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("Actions ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[p] pause/resume  [c] cancel  [r] resume  [d] remove  [D] purge file",
            Style::default().fg(Color::Gray),
        ),
    ]))
    .block(Block::default().borders(Borders::BOTTOM).title(" Control Panel "));
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
    } else if has_active {
        sections.push(layout_sections[1]);
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
    let outer = Block::default().borders(Borders::ALL).title(" Settings Control Panel ");
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
                .or_else(|| crate::config::AppConfig::bookmark_path().map(|p| p.to_string_lossy().to_string()))
                .unwrap_or_else(|| "Default".to_string()),
            3 => app
                .config
                .model_search_cache_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| app.config.search_cache_path().map(|p| p.to_string_lossy().to_string()))
                .unwrap_or_else(|| "Default".to_string()),
            4 => format!("{}h", app.config.model_search_cache_ttl_hours),
            5 => app
                .config
                .image_cache_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| app.config.image_cache_path().map(|p| p.to_string_lossy().to_string()))
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
                .or_else(|| app.config.download_history_path().map(|p| p.to_string_lossy().to_string()))
                .unwrap_or_else(|| "Default".to_string()),
            11 => "Delete search/detail/media caches".to_string(),
            _ => String::new(),
        }
    };

    let item_line = |idx: usize, label: &str| -> Line<'static> {
        let focused = fm.focused_field == idx;
        let label_style = if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
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
    .block(Block::default().borders(Borders::ALL).title(" Access & Paths "))
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
    .block(Block::default().borders(Borders::ALL).title(" Model Cache "))
    .wrap(Wrap { trim: true });
    f.render_widget(model_cache, middle[0]);

    let image_cache = Paragraph::new(vec![
        item_line(5, "Image Cache Folder"),
        item_line(6, "Image Search TTL"),
        item_line(7, "Image Detail TTL"),
        item_line(8, "Image Binary TTL"),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Image Cache "))
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

    let item_count = items.len();
    let selected_image_id = items.get(selected_index).map(|img| img.id);
    let Some(image_id) = selected_image_id else {
        f.render_widget(Paragraph::new("No image selected").block(block), area);
        return;
    };
    let inner_area = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Clear, inner_area);

    if let Some(protocol) = app.image_cache.get_mut(&image_id) {
        let image_widget = StatefulImage::new();
        f.render_stateful_widget(image_widget, inner_area, protocol);
    } else {
        let text = format!("Loading image {}/{}...", selected_index + 1, item_count);
        f.render_widget(Paragraph::new(text), inner_area);
    }
}

fn draw_image_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(img) = app.selected_image_in_active_view() else {
        f.render_widget(
            Paragraph::new("No metadata available.")
                .block(Block::default().borders(Borders::ALL).title(" Image ")),
            area,
        );
        return;
    };

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(4),
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Min(6),
        ])
        .split(area);

    let dimensions = match (img.width, img.height) {
        (Some(width), Some(height)) => format!("{width}x{height}"),
        _ => "<none>".to_string(),
    };
    let username = image_username(img).unwrap_or_else(|| "<unknown>".to_string());
    let stats = image_stats(img);
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
    let generation = image_generation_info(img);
    let workflow_label = workflow_json
        .as_ref()
        .map(|_| format!("nodes {}", comfy_workflow_node_count(img).unwrap_or(0)))
        .unwrap_or_else(|| "<none>".to_string());
    let used_models = image_used_models(img);
    let tags = image_tags(img);
    let tag_lines = if tags.is_empty() {
        vec![Line::from("<none>")]
    } else {
        wrap_joined_tags(&tags, sections[4].width.saturating_sub(2) as usize, 2)
    };
    let selected_model_idx = app
        .selected_image_model_index
        .get(&img.id)
        .copied()
        .unwrap_or(0)
        .min(used_models.len().saturating_sub(1));
    let used_model_lines = if used_models.is_empty() {
        vec![Line::from("<none>")]
    } else {
        let visible_rows = sections[3].height.saturating_sub(2) as usize;
        let start_idx = if visible_rows == 0 || selected_model_idx < visible_rows {
            0
        } else {
            selected_model_idx + 1 - visible_rows
        };
        used_models
            .iter()
            .enumerate()
            .skip(start_idx)
            .take(visible_rows.max(1))
            .map(|(idx, item)| {
                let prefix = if idx == selected_model_idx { "> " } else { "  " };
                let available_width = sections[3].width.saturating_sub(4) as usize;
                let display_item = if idx == selected_model_idx
                    && item.chars().count() > available_width.max(1)
                {
                    let now_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_else(|_| Duration::from_millis(0))
                        .as_millis();
                    let shift = ((now_ms / 260) as usize) % item.chars().count().max(1);
                    rotate_left_chars(item, shift)
                } else {
                    item.clone()
                };
                let style = if idx == selected_model_idx {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                Line::from(vec![Span::styled(
                    format!("{prefix}{display_item}"),
                    style,
                )])
            })
            .collect::<Vec<_>>()
    };
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
        Line::from(model_key_value_spans("Link", &image_link)),
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
    let generation_lines = vec![
        Line::from(model_key_value_spans(
            "CFG",
            generation.cfg_scale.as_deref().unwrap_or("<none>"),
        )),
        Line::from(model_key_value_spans(
            "Steps",
            generation.steps.as_deref().unwrap_or("<none>"),
        )),
        Line::from(model_key_value_spans(
            "Sampler",
            generation.sampler.as_deref().unwrap_or("<none>"),
        )),
        Line::from(model_key_value_spans(
            "Seed",
            generation.seed.as_deref().unwrap_or("<none>"),
        )),
    ];
    let comfy_lines = vec![
        Line::from(model_key_value_spans("Comfy", &workflow_label)),
        Line::from(model_key_value_spans(
            "Copy",
            if workflow_json.is_some() { "[c]" } else { "<none>" },
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
    let mut tag_block_lines = tag_lines;
    let generation_block = Block::default().borders(Borders::ALL).title(" Generation ");
    let generation_inner = generation_block.inner(sections[2]);
    let generation_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(generation_inner);
    f.render_widget(generation_block, sections[2]);
    f.render_widget(
        Paragraph::new(generation_lines)
            .block(Block::default().borders(Borders::RIGHT).title(" Params "))
            .wrap(Wrap { trim: true }),
        generation_split[0],
    );
    f.render_widget(
        Paragraph::new(comfy_lines)
            .block(Block::default().title(" Comfy "))
            .wrap(Wrap { trim: true }),
        generation_split[1],
    );
    f.render_widget(
        Paragraph::new(used_model_lines)
            .block(Block::default().borders(Borders::ALL).title(" Models ")),
        sections[3],
    );

    if app.image_advanced_visible {
        tag_block_lines.push(Line::from(""));
        tag_block_lines.extend(wrap_text_lines(
            &advanced_json,
            sections[4].width.saturating_sub(2) as usize,
            sections[4].height.saturating_sub(3) as usize,
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

fn draw_image_prompt_modal(f: &mut Frame, app: &App) {
    let Some(img) = app.selected_image_in_active_view() else {
        return;
    };

    let positive = image_prompt(img).unwrap_or_else(|| {
        if img.hide_meta.unwrap_or(false) {
            "Metadata hidden by source".to_string()
        } else {
            "<no prompt>".to_string()
        }
    });
    let negative = image_negative_prompt(img).unwrap_or_else(|| "<no negative prompt>".to_string());

    let content = format!(
        "Positive Prompt\n{}\n\nNegative Prompt\n{}",
        positive, negative
    );

    let area = centered_rect(78, 82, f.area());
    let block = Block::default().borders(Borders::ALL).title(" Prompt Viewer ");
    f.render_widget(Clear, area);
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(inner);

    let prompt = Paragraph::new(content)
        .wrap(Wrap { trim: false })
        .scroll((app.image_prompt_scroll, 0));
    f.render_widget(prompt, sections[0]);

    let help = Paragraph::new(Line::from(Span::styled(
        " [j/k or ↑/↓] Scroll  [m] Close  [Esc] Close ",
        help_text_style(),
    )))
    .alignment(Alignment::Center);
    f.render_widget(help, sections[1]);
}

fn draw_image_model_detail_modal(f: &mut Frame, app: &mut App) {
    let area = centered_rect(84, 86, f.area());
    f.render_widget(Clear, area);

    let title = if let Some(model) = app.image_model_detail_model.as_ref() {
        let bookmark_label = if app.is_model_bookmarked(model.id) {
            "Bookmarked"
        } else {
            "b: Bookmark"
        };
        format!(" Model Details | [←/→] Version | [J/K] Files | [d] Download | [{bookmark_label}] | [Esc] Close ")
    } else {
        " Model Details | Loading... | [Esc] Close ".to_string()
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(model) = app.image_model_detail_model.clone() {
        draw_model_sidebar(f, app, inner, Some(&model));
    } else {
        let loading = Paragraph::new("Loading model details...")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Model "));
        f.render_widget(loading, inner);
    }
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
    let selected_tags = if app.search_form.tag_query.trim().is_empty() {
        "All".to_string()
    } else {
        app.search_form.tag_query.trim().to_string()
    };
    let summary = format!(
        "🔍 Query: \"{}\" | Type: {} | Tags: {} | Sort: {} | Base: {} | Period: {}",
        model_query,
        selected_types,
        selected_tags,
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
    let selected_tags = if app.bookmark_search_form.tag_query.trim().is_empty() {
        "All".to_string()
    } else {
        app.bookmark_search_form.tag_query.trim().to_string()
    };
    let summary = format!(
        "🔖 Query: \"{}\" | Type: {} | Tags: {} | Sort: {} | Base: {} | Period: {} | Total: {}",
        query,
        selected_types,
        selected_tags,
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
    let tags = if app.image_search_form.tag_query.trim().is_empty() {
        "All".to_string()
    } else {
        app.image_search_form.tag_query.trim().to_string()
    };
    let summary = format!(
        "🖼 Query: \"{}\" | Type: {} | Tags: {} | Sort: {} | Base: {} | Ratio: {} | Period: {}",
        query,
        media_types,
        tags,
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
        let creator = creator_name(model).unwrap_or_else(|| "unknown".to_string());
        let mut display_name = model_name(model);
        let is_bookmarked = bookmarked_ids.contains(&model.id);
        let has_version_id = !model_versions(model).is_empty();

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
        } else if !has_version_id {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };
        let mut line_two = format!(
            "{} | dl {} like {} cmt {} | by {}",
            model.r#type.as_deref().unwrap_or("Model"),
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
                    if has_version_id {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::Rgb(90, 90, 90))
                    },
                ),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled("NSFW", Style::default().fg(Color::Red)),
            ])
        } else {
            Line::from(Span::styled(
                compact_cell_text(line_two, name_width),
                if has_version_id {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Rgb(90, 90, 90))
                },
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
        f.render_widget(Clear, inner_img_area);

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
                    " Current: sort={} | types={} | tags={} | bases={} | period={} ",
                    fm.sort_options
                        .get(fm.selected_sort)
                        .map(|sort| sort.label().to_string())
                        .unwrap_or_else(|| "Relevance".to_string()),
                    fm.selected_types.len(),
                    if fm.tag_query.trim().is_empty() { 0 } else { fm.tag_query.split(',').filter(|tag| !tag.trim().is_empty()).count() },
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
        let area = centered_rect(54, 22, f.area());
        f.render_widget(Clear, area);
        f.render_widget(p, area);
        return;
    }

    let area = centered_rect(72, 68, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {builder_title} "));
    f.render_widget(&block, area);
    let inner = block.inner(area);
    let section_constraints = build_model_modal_constraints(fm.focused_section, inner.height);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints(section_constraints)
        .split(inner);

    let query_focused = fm.focused_section == SearchFormSection::Query;
    let sort_focused = fm.focused_section == SearchFormSection::Sort;
    let period_focused = fm.focused_section == SearchFormSection::Period;
    let type_focused = fm.focused_section == SearchFormSection::Type;
    let tag_focused = fm.focused_section == SearchFormSection::Tag;
    let base_focused = fm.focused_section == SearchFormSection::BaseModel;
    let query_is_configured = !fm.query.trim().is_empty();
    let sort_is_configured = true;
    let period_is_configured = true;
    let type_is_configured = !fm.selected_types.is_empty();
    let tag_is_configured = !fm.tag_query.trim().is_empty();
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
        help_text_style(),
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
        help_text_style(),
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

    let type_widget = Paragraph::new(build_image_filter_box_lines(
        "Type",
        type_focused,
        type_is_configured,
        &type_items,
        &fm.selected_types.iter().map(|item| item.label().to_string()).collect::<Vec<_>>(),
        false,
        sections[3].width.saturating_sub(4) as usize,
        sections[3].height.saturating_sub(3) as usize,
    ))
        .block(styled_search_block(" Type ", type_focused, type_is_configured))
        .wrap(Wrap { trim: true });
    f.render_widget(type_widget, sections[3]);

    let tag_widget = Paragraph::new(build_text_filter_box_lines(
        "Tag",
        tag_focused,
        tag_is_configured,
        &fm.tag_query,
        "Comma-separated tags",
        None,
        sections[4].width.saturating_sub(4) as usize,
        sections[4].height.saturating_sub(3) as usize,
    ))
    .block(styled_search_block(" Tags ", tag_focused, tag_is_configured))
    .wrap(Wrap { trim: true });
    f.render_widget(tag_widget, sections[4]);

    let base_widget = Paragraph::new(build_image_filter_box_lines(
        "Base Model",
        base_focused,
        base_is_configured,
        &base_items,
        &fm
            .selected_base_models
            .iter()
            .map(|item| item.label().to_string())
            .collect::<Vec<_>>(),
        false,
        sections[5].width.saturating_sub(4) as usize,
        sections[5].height.saturating_sub(3) as usize,
    ))
        .block(styled_search_block(" Base Model ", base_focused, base_is_configured))
        .wrap(Wrap { trim: true });
    f.render_widget(base_widget, sections[5]);

    let help = Paragraph::new(" [Up/Down] Section | [Left/Right] Change | [Space] Toggle | [Type] Query/Tag | [Enter] Apply | [Esc] Cancel ")
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    f.render_widget(help, sections[6]);
}

fn draw_image_bookmark_search_popup(f: &mut Frame, app: &App) {
    let visible_count = app.visible_image_bookmarks().len();
    let lines = vec![
        Line::from(vec![
            Span::styled(" Query: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}█", app.image_bookmark_query_draft)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!(" Current: total={} ", visible_count),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            " [Type] Query | [Enter] Apply | [Esc] Cancel ",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Image Bookmark Search "))
        .wrap(Wrap { trim: true });
    let area = centered_rect(60, 24, f.area());
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
                    " Current: sort={} | types={} | tags={} | bases={} | ratios={} | period={} ",
                    form.sort_options
                        .get(form.selected_sort)
                        .map(|sort| sort.label().to_string())
                        .unwrap_or_else(|| "Relevance".to_string()),
                    form.selected_media_types.len(),
                    if form.tag_query.trim().is_empty() { 0 } else { form.tag_query.split(',').filter(|tag| !tag.trim().is_empty()).count() },
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
    let section_constraints = build_image_modal_constraints(form.focused_section, inner.height);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints(section_constraints)
        .split(inner);

    let query_focused = form.focused_section == ImageSearchFormSection::Query;
    let sort_focused = form.focused_section == ImageSearchFormSection::Sort;
    let period_focused = form.focused_section == ImageSearchFormSection::Period;
    let type_focused = form.focused_section == ImageSearchFormSection::MediaType;
    let tag_focused = form.focused_section == ImageSearchFormSection::Tag;
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
    let tag_suggestions = app.image_tag_suggestions(sections[4].height.saturating_sub(4) as usize);

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
        help_text_style(),
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
        help_text_style(),
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

    let tag_box = Paragraph::new(build_text_filter_box_lines(
        "Tag",
        tag_focused,
        !form.tag_query.trim().is_empty(),
        &form.tag_query,
        "Comma-separated tags",
        Some(tag_suggestions.as_slice()),
        sections[4].width.saturating_sub(4) as usize,
        sections[4].height.saturating_sub(3) as usize,
    ))
    .block(styled_search_block(
        " Tags ",
        tag_focused,
        !form.tag_query.trim().is_empty(),
    ))
    .wrap(Wrap { trim: true });
    f.render_widget(tag_box, sections[4]);

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
            sections[5].width.saturating_sub(4) as usize,
            sections[5].height.saturating_sub(3) as usize,
        ))
        .block(styled_search_block(
            " Base Model ",
            base_focused,
            !form.selected_base_models.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[5],
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
            sections[6].width.saturating_sub(4) as usize,
            sections[6].height.saturating_sub(3) as usize,
        ))
        .block(styled_search_block(
            " Aspect Ratio ",
            ratio_focused,
            !form.selected_aspect_ratios.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[6],
    );

    f.render_widget(
        Paragraph::new(
            " [Up/Down] Section | [Left/Right] Change | [Space] Toggle | [Type] Query/Tag | [Enter] Apply | [Esc] Cancel ",
        )
        .wrap(Wrap { trim: true }),
        sections[7],
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

fn draw_help_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(72, 60, f.area());
    let block = Block::default().borders(Borders::ALL).title(" Keyboard Help ");
    f.render_widget(Clear, area);
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(inner);

    let title = match app.active_tab {
        MainTab::Models => "Models",
        MainTab::Bookmarks => "Bookmarks",
        MainTab::Images => "Image Feed",
        MainTab::ImageBookmarks => "Image Bookmarks",
        MainTab::Downloads => "Downloads",
        MainTab::Settings => "Settings",
    };

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Tab ", Style::default().fg(Color::DarkGray)),
            Span::styled(title, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(Span::styled(
            "Global navigation is consistent across tabs. Context actions change by tab.",
            help_text_style(),
        )),
    ]);
    f.render_widget(header, sections[0]);

    let nav = Paragraph::new(match app.active_tab {
        MainTab::Models | MainTab::Bookmarks | MainTab::Images | MainTab::ImageBookmarks => vec![
            Line::from(" [1-6] Switch tabs"),
            Line::from(" [j/k] Move selection"),
            Line::from(" [g/G] First/Last item"),
            Line::from(" [Ctrl-u / Ctrl-d] Jump faster"),
        ],
        MainTab::Downloads | MainTab::Settings => vec![
            Line::from(" [1-6] Switch tabs"),
            Line::from(" [j/k] Move selection"),
            Line::from(" [Esc] Close modal / cancel"),
            Line::from(" [?] Toggle this help"),
        ],
    })
    .block(Block::default().borders(Borders::ALL).title(" Navigation "))
    .wrap(Wrap { trim: true });
    f.render_widget(nav, sections[1]);

    let search = Paragraph::new(match app.active_tab {
        MainTab::Models | MainTab::Bookmarks | MainTab::Images | MainTab::ImageBookmarks => vec![
            Line::from(" [/] Quick search"),
            Line::from(" [f] Open filter builder"),
            Line::from(" [Enter] Apply search / run selected action"),
            Line::from(" Filter modal: [↑/↓] section  [←/→] option  [Space] toggle"),
            Line::from(" Text sections accept typing directly"),
        ],
        MainTab::Downloads => vec![
            Line::from(" No search in this tab"),
            Line::from(" Focus stays on active queue or history"),
            Line::from(" Actions run against current selection"),
        ],
        MainTab::Settings => vec![
            Line::from(" [Enter] Edit selected text field"),
            Line::from(" [h/l] Cycle selected enum action"),
            Line::from(" While editing: type text, [Enter] save, [Esc] cancel"),
        ],
    })
    .block(Block::default().borders(Borders::ALL).title(" Search & Input "))
    .wrap(Wrap { trim: true });
    f.render_widget(search, sections[2]);

    let actions_lines = match app.active_tab {
        MainTab::Models => vec![
            Line::from(" [v] Toggle detail panel"),
            Line::from(" [←/→] Change version"),
            Line::from(" [Shift+↑/↓] or [J/K] Change file"),
            Line::from(" [d] Download selected file"),
            Line::from(" [b] Bookmark selected model"),
            Line::from(" [r] Refresh current search  [c] Clear model search cache"),
        ],
        MainTab::Bookmarks => vec![
            Line::from(" [v] Toggle detail panel"),
            Line::from(" [←/→] Change version"),
            Line::from(" [Shift+↑/↓] or [J/K] Change file"),
            Line::from(" [d] Download selected file"),
            Line::from(" [b] Remove selected bookmark"),
            Line::from(" [e] Export bookmarks  [i] Import bookmarks"),
        ],
        MainTab::Images => vec![
            Line::from(" [←/→] Change image"),
            Line::from(" [d] Download current image"),
            Line::from(" [b] Bookmark current image"),
            Line::from(" [m] Open full prompt viewer"),
            Line::from(" [a] Toggle advanced metadata"),
            Line::from(" [o] Copy image page link"),
            Line::from(" [Shift+↑/↓] or [J/K] Change used model"),
            Line::from(" [Enter] Open selected model details"),
            Line::from(" [c] Copy workflow  [W] Save workflow JSON"),
        ],
        MainTab::ImageBookmarks => vec![
            Line::from(" [←/→] Change image"),
            Line::from(" [d] Download current image"),
            Line::from(" [b] Remove current bookmark"),
            Line::from(" [m] Open full prompt viewer"),
            Line::from(" [a] Toggle advanced metadata"),
            Line::from(" [o] Copy image page link"),
            Line::from(" [Shift+↑/↓] or [J/K] Change used model"),
            Line::from(" [Enter] Open selected model details"),
            Line::from(" [c] Copy workflow  [W] Save workflow JSON"),
        ],
        MainTab::Downloads => vec![
            Line::from(" [p] Pause / resume active download"),
            Line::from(" [c] Cancel active download"),
            Line::from(" [r] Resume from selected history entry"),
            Line::from(" [d] Remove history entry"),
            Line::from(" [D] Remove history entry and file"),
        ],
        MainTab::Settings => vec![
            Line::from(" [j/k] Move between controls"),
            Line::from(" [Enter] Edit or run selected control"),
            Line::from(" [h/l] Cycle media quality"),
            Line::from(" Clear All Caches keeps bookmarks, tags, settings, and history"),
        ],
    };
    let actions = Paragraph::new(actions_lines)
        .block(Block::default().borders(Borders::ALL).title(" Context Actions "))
        .wrap(Wrap { trim: true });
    f.render_widget(actions, sections[3]);

    let footer = Paragraph::new(Line::from(Span::styled(
        " [Esc] Close  [?] Close ",
        help_text_style(),
    )))
    .alignment(Alignment::Center);
    f.render_widget(footer, sections[4]);
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
        MainTab::Models => "[?] Help  [j/k] Move  [/] Search  [f] Filter  [v] Detail  [←/→] Ver  [⇧↑/↓] File  [d] Download",
        MainTab::Bookmarks => "[?] Help  [j/k] Move  [/] Search  [f] Filter  [v] Detail  [←/→] Ver  [⇧↑/↓] File  [b] Remove",
        MainTab::Images => "[?] Help  [←/→] Image  [J/K] Models  [Enter] Model  [m] Prompt  [d] Download  [c] Comfy",
        MainTab::ImageBookmarks => "[?] Help  [←/→] Image  [J/K] Models  [Enter] Model  [m] Prompt  [d] Download  [c] Comfy",
        MainTab::Downloads => "[?] Help  [j/k] Select  [p] Pause/Resume  [c] Cancel  [r] Resume  [d] Remove",
        MainTab::Settings => "[?] Help  [j/k] Select  [Enter] Edit/Run  [h/l] Cycle  [Esc] Cancel",
    };

    let shortcuts_row = Paragraph::new(Span::styled(
        shortcuts,
        Style::default().fg(Color::DarkGray),
    ))
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: true });
    f.render_widget(shortcuts_row, rows[1]);
}
