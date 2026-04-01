mod downloads;
mod footer;
mod helpers;
mod images;
mod modals;
mod models;
mod tabs;

use self::helpers::{
    ImageFilterBoxProps, TextFilterBoxProps, build_horizontal_item_window,
    build_image_filter_box_lines, build_image_modal_constraints, build_model_modal_constraints,
    build_text_filter_box_lines, build_wrapped_option_lines, centered_rect, compact_cell_text,
    compact_file_size, compact_number, help_text_style, inactive_box_style,
    model_key_value_spans, render_description_lines, rotate_left_chars, styled_search_block,
    wrap_joined_tags, wrap_text_lines,
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
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};
use ratatui_image::{StatefulImage, protocol::StatefulProtocol};
use std::io::{self, Stdout};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::tui::app::{
    App, AppMode, ImageSearchFormSection, MainTab, SearchFormMode, SearchFormSection,
    SearchFormState,
};
use crate::tui::image::{
    comfy_workflow_json, comfy_workflow_node_count, image_generation_info, image_stats,
    image_tags, image_used_models, image_username,
};
use crate::tui::model::{build_model_url, category_name, creator_name, model_name, tag_names};

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

    tabs::draw_active_tab(f, app, chunks[1], enable_name_rolling);
    footer::draw_footer_section(f, app, chunks[2]);
    modals::draw_active_modals(f, app);
}

pub(super) fn draw_image_panel(f: &mut Frame, app: &mut App, area: Rect) {
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

pub(super) fn draw_image_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
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
                let prefix = if idx == selected_model_idx {
                    "> "
                } else {
                    "  "
                };
                let available_width = sections[3].width.saturating_sub(4) as usize;
                let display_item =
                    if idx == selected_model_idx && item.chars().count() > available_width.max(1) {
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
                Line::from(vec![Span::styled(format!("{prefix}{display_item}"), style)])
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
        Line::from(model_key_value_spans("Image", &image_meta_value)),
        Line::from(model_key_value_spans("Link", &image_link)),
    ];
    let stats_lines = vec![
        Line::from(model_key_value_spans("Stats", &stats_primary_value)),
        Line::from(model_key_value_spans("More", &stats_secondary_value)),
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
            if workflow_json.is_some() {
                "[c]"
            } else {
                "<none>"
            },
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(if app.image_advanced_visible {
                        " Tags / Advanced "
                    } else {
                        " Tags "
                    }),
            )
            .wrap(Wrap { trim: false }),
        sections[4],
    );
}

pub(super) fn draw_model_search_summary(f: &mut Frame, app: &mut App, area: Rect) {
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

pub(super) fn draw_bookmark_search_summary(f: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_image_bookmark_search_summary(f: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_image_search_summary(f: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_model_list(
    app: &App,
    f: &mut Frame,
    area: Rect,
    models: &[civitai_cli::sdk::SearchModelHit],
    list_state: &ListState,
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
        let metrics = app.parsed_model_metrics(model);
        let creator = creator_name(model).unwrap_or_else(|| "unknown".to_string());
        let mut display_name = model_name(model);
        let is_bookmarked = bookmarked_ids.contains(&model.id);
        let has_version_id = !app.parsed_model_versions(model).is_empty();

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
    let mut state = *list_state;
    if !models.is_empty() {
        state.select(Some(selected_idx.saturating_sub(start_idx)));
    }
    f.render_stateful_widget(list, area, &mut state);
}

pub(super) fn draw_model_sidebar(
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
        let versions = app.parsed_model_versions(model);
        let v_idx = *app.selected_version_index.get(&model.id).unwrap_or(&0);
        let safe_v_idx = v_idx.min(versions.len().saturating_sub(1));
        let selected_version = versions.get(safe_v_idx);
        let metrics = selected_version
            .map(|version| version.stats.clone())
            .unwrap_or_else(|| app.parsed_model_metrics(model));
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
                compact_cell_text(model_url, split[0].width.saturating_sub(2) as usize),
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
                    .or_else(|| app.parsed_default_base_model(model).map(str::to_string))
                    .unwrap_or_else(|| "Unknown".to_string())
            ),
            format!("{:<10} {}", "Down", compact_number(metrics.download_count)),
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
            format!("{:<10} {:.1}", "Rating", metrics.rating),
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
        let metadata = Paragraph::new(detail_row.into_iter().map(Line::from).collect::<Vec<_>>())
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
            if let Some(protocol) = protocol {
                let image_widget: StatefulImage<StatefulProtocol> = StatefulImage::new();
                f.render_stateful_widget(image_widget, inner_img_area, protocol);
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

pub(super) fn draw_search_popup(
    f: &mut Frame,
    fm: &SearchFormState,
    builder_title: &str,
    quick_title: &str,
) {
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
                    if fm.tag_query.trim().is_empty() {
                        0
                    } else {
                        fm.tag_query
                            .split(',')
                            .filter(|tag| !tag.trim().is_empty())
                            .count()
                    },
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
    .block(styled_search_block(
        " Query ",
        query_focused,
        query_is_configured,
    ))
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
        .block(styled_search_block(
            " Sort ",
            sort_focused,
            sort_is_configured,
        ))
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
        .block(styled_search_block(
            " Period ",
            period_focused,
            period_is_configured,
        ))
        .wrap(Wrap { trim: true });
    f.render_widget(period_box, sections[2]);

    let selected_types = fm
        .selected_types
        .iter()
        .map(|item| item.label().to_string())
        .collect::<Vec<_>>();
    let type_widget = Paragraph::new(build_image_filter_box_lines(ImageFilterBoxProps {
        label: "Type",
        focused: type_focused,
        configured: type_is_configured,
        items: &type_items,
        selected: &selected_types,
        show_selected: false,
        width: sections[3].width.saturating_sub(4) as usize,
        height: sections[3].height.saturating_sub(3) as usize,
    }))
    .block(styled_search_block(
        " Type ",
        type_focused,
        type_is_configured,
    ))
    .wrap(Wrap { trim: true });
    f.render_widget(type_widget, sections[3]);

    let tag_widget = Paragraph::new(build_text_filter_box_lines(TextFilterBoxProps {
        label: "Tag",
        focused: tag_focused,
        configured: tag_is_configured,
        value: &fm.tag_query,
        placeholder: "Comma-separated tags",
        suggestions: None,
        width: sections[4].width.saturating_sub(4) as usize,
        height: sections[4].height.saturating_sub(3) as usize,
    }))
    .block(styled_search_block(
        " Tags ",
        tag_focused,
        tag_is_configured,
    ))
    .wrap(Wrap { trim: true });
    f.render_widget(tag_widget, sections[4]);

    let selected_base_models = fm
        .selected_base_models
        .iter()
        .map(|item| item.label().to_string())
        .collect::<Vec<_>>();
    let base_widget = Paragraph::new(build_image_filter_box_lines(ImageFilterBoxProps {
        label: "Base Model",
        focused: base_focused,
        configured: base_is_configured,
        items: &base_items,
        selected: &selected_base_models,
        show_selected: false,
        width: sections[5].width.saturating_sub(4) as usize,
        height: sections[5].height.saturating_sub(3) as usize,
    }))
    .block(styled_search_block(
        " Base Model ",
        base_focused,
        base_is_configured,
    ))
    .wrap(Wrap { trim: true });
    f.render_widget(base_widget, sections[5]);

    let help = Paragraph::new(" [Up/Down] Section | [Left/Right] Change | [Space] Toggle | [Type] Query/Tag | [Enter] Apply | [Esc] Cancel ")
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    f.render_widget(help, sections[6]);
}

pub(super) fn draw_image_bookmark_search_popup(f: &mut Frame, app: &App) {
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Image Bookmark Search "),
        )
        .wrap(Wrap { trim: true });
    let area = centered_rect(60, 24, f.area());
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

pub(super) fn draw_bookmark_path_prompt(f: &mut Frame, app: &App) {
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

pub(super) fn draw_image_search_popup(f: &mut Frame, app: &App) {
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
                    if form.tag_query.trim().is_empty() {
                        0
                    } else {
                        form.tag_query
                            .split(',')
                            .filter(|tag| !tag.trim().is_empty())
                            .count()
                    },
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Image Search "),
            )
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
    let selected_media_types = form
        .selected_media_types
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let selected_base_models = form
        .selected_base_models
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let selected_aspect_ratios = form
        .selected_aspect_ratios
        .iter()
        .cloned()
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
    .block(styled_search_block(
        " Query ",
        query_focused,
        !form.query.trim().is_empty(),
    ))
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
        Paragraph::new(build_image_filter_box_lines(ImageFilterBoxProps {
            label: "Media Type",
            focused: type_focused,
            configured: !form.selected_media_types.is_empty(),
            items: &type_items,
            selected: &selected_media_types,
            show_selected: false,
            width: sections[3].width.saturating_sub(4) as usize,
            height: sections[3].height.saturating_sub(3) as usize,
        }))
        .block(styled_search_block(
            " Media Type ",
            type_focused,
            !form.selected_media_types.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[3],
    );

    let tag_box = Paragraph::new(build_text_filter_box_lines(TextFilterBoxProps {
        label: "Tag",
        focused: tag_focused,
        configured: !form.tag_query.trim().is_empty(),
        value: &form.tag_query,
        placeholder: "Comma-separated tags",
        suggestions: Some(tag_suggestions.as_slice()),
        width: sections[4].width.saturating_sub(4) as usize,
        height: sections[4].height.saturating_sub(3) as usize,
    }))
    .block(styled_search_block(
        " Tags ",
        tag_focused,
        !form.tag_query.trim().is_empty(),
    ))
    .wrap(Wrap { trim: true });
    f.render_widget(tag_box, sections[4]);

    f.render_widget(
        Paragraph::new(build_image_filter_box_lines(ImageFilterBoxProps {
            label: "Base Model",
            focused: base_focused,
            configured: !form.selected_base_models.is_empty(),
            items: &base_items,
            selected: &selected_base_models,
            show_selected: false,
            width: sections[5].width.saturating_sub(4) as usize,
            height: sections[5].height.saturating_sub(3) as usize,
        }))
        .block(styled_search_block(
            " Base Model ",
            base_focused,
            !form.selected_base_models.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[5],
    );

    f.render_widget(
        Paragraph::new(build_image_filter_box_lines(ImageFilterBoxProps {
            label: "Aspect Ratio",
            focused: ratio_focused,
            configured: !form.selected_aspect_ratios.is_empty(),
            items: &ratio_items,
            selected: &selected_aspect_ratios,
            show_selected: false,
            width: sections[6].width.saturating_sub(4) as usize,
            height: sections[6].height.saturating_sub(3) as usize,
        }))
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
