use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use ratatui_image::{Resize, StatefulImage};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::tui::{
    app::{App, AppMode, ImageSearchFormSection, MainTab, SearchFormMode},
    image::{
        comfy_workflow_json, comfy_workflow_node_count, image_generation_info, image_stats,
        image_tags, image_used_models, image_username,
    },
};

use super::helpers::{
    ImageFilterBoxProps, TextFilterBoxProps, build_image_filter_box_lines,
    build_image_modal_constraints, build_text_filter_box_lines, build_wrapped_option_lines,
    centered_rect, compact_number, help_text_style, inactive_box_style, model_key_value_spans,
    rotate_left_chars, styled_search_block, wrap_joined_tags, wrap_text_lines,
};

pub(super) fn draw_images_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let image_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);
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

pub(super) fn draw_liked_images_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let image_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(image_chunks[1]);
    draw_liked_image_search_summary(f, app, image_chunks[0]);
    draw_image_panel(f, app, main_chunks[0]);
    draw_image_sidebar(f, app, main_chunks[1]);
    if app.mode == AppMode::SearchLikedImages {
        draw_liked_image_search_popup(f, app);
    }
}

fn centered_media_rect(area: Rect, fitted_size: Rect) -> Rect {
    let target_width = fitted_size.width.clamp(1, area.width);
    let target_height = fitted_size.height.clamp(1, area.height);

    let x = area.x + area.width.saturating_sub(target_width) / 2;
    let y = area.y + area.height.saturating_sub(target_height) / 2;

    Rect {
        x,
        y,
        width: target_width,
        height: target_height,
    }
}

fn draw_image_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let items = app.active_image_items();
    let selected_index = app.active_image_selected_index();
    let selected_image = items.get(selected_index);
    let selected_image_id = selected_image.map(|img| img.id);
    let total_count = if app.active_tab == MainTab::Images {
        app.image_feed_total_hits
            .unwrap_or(items.len() as u64)
            .max(items.len() as u64)
    } else {
        items.len() as u64
    };
    let current_index = if items.is_empty() {
        0
    } else {
        (selected_index + 1).min(items.len()) as u64
    };
    let is_liked = selected_image_id
        .map(|image_id| app.active_tab == MainTab::LikedImages || app.is_image_liked(image_id))
        .unwrap_or(false);
    let title = if is_liked {
        format!(" Image View | Liked! ({current_index} / {total_count}) ")
    } else {
        format!(" Image View ({current_index} / {total_count}) ")
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_liked {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        })
        .title(title);

    if items.is_empty() {
        let empty_message = if app.active_tab == MainTab::LikedImages {
            "No liked images."
        } else {
            "Loading images..."
        };
        f.render_widget(Paragraph::new(empty_message).block(block), area);
        return;
    }

    let item_count = items.len();
    let Some(image_id) = selected_image_id else {
        f.render_widget(Paragraph::new("No image selected").block(block), area);
        return;
    };
    let inner_area = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Clear, inner_area);

    if let Some(protocol) = app.image_cache.get_mut(&image_id) {
        let resize_mode = Resize::Scale(None);
        let fitted_size = protocol.size_for(resize_mode.clone(), inner_area);
        let render_area = centered_media_rect(inner_area, fitted_size);
        let image_widget = StatefulImage::new().resize(resize_mode);
        f.render_stateful_widget(image_widget, render_area, protocol);
    } else {
        let text = format!("Loading image {}/{}...", selected_index + 1, item_count);
        f.render_widget(
            Paragraph::new(text).alignment(Alignment::Center),
            inner_area,
        );
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
        let available_height = if app.image_advanced_visible {
            sections[4].height.saturating_sub(4) as usize
        } else {
            sections[4].height.saturating_sub(2) as usize
        };
        wrap_joined_tags(
            &tags,
            sections[4].width.saturating_sub(2) as usize,
            available_height.max(1),
        )
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

fn draw_liked_image_search_summary(f: &mut Frame, app: &App, area: Rect) {
    let query = if app.liked_image_query.is_empty() {
        "<all>"
    } else {
        &app.liked_image_query
    };
    let summary = format!(
        "🔍 Liked Images Query: \"{}\" | Total: {}",
        query,
        app.visible_liked_images().len()
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
    let excluded_base_models = if app.image_search_form.excluded_base_models.is_empty() {
        "None".to_string()
    } else {
        app.image_search_form
            .excluded_base_models
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
    let excluded_tags = if app.image_search_form.excluded_tag_query.trim().is_empty() {
        "None".to_string()
    } else {
        app.image_search_form.excluded_tag_query.trim().to_string()
    };
    let summary = format!(
        "🖼 Query: \"{}\" | Type: {} | Tags: {} | Exclude: {} | Sort: {} | Base: {} | Excl Base: {} | Ratio: {} | Period: {}",
        query,
        media_types,
        tags,
        excluded_tags,
        app.image_search_form
            .sort_options
            .get(app.image_search_form.selected_sort)
            .map(|value| value.label().to_string())
            .unwrap_or_else(|| "Relevance".into()),
        base_models,
        excluded_base_models,
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

fn draw_liked_image_search_popup(f: &mut Frame, app: &App) {
    let visible_count = app.visible_liked_images().len();
    let lines = vec![
        Line::from(vec![
            Span::styled(" Query: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}█", app.liked_image_query_draft)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!(" Current: total={} ", visible_count),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            " [Type] Query | [Enter] Apply | [Esc] Apply & Close ",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Liked Image Search "),
        )
        .wrap(Wrap { trim: true });
    let area = centered_rect(60, 24, f.area());
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
                    " Current: sort={} | types={} | tags={} | excluded={} | bases={} | excl-bases={} | ratios={} | period={} ",
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
                    if form.excluded_tag_query.trim().is_empty() {
                        0
                    } else {
                        form.excluded_tag_query
                            .split(',')
                            .filter(|tag| !tag.trim().is_empty())
                            .count()
                    },
                    form.selected_base_models.len(),
                    form.excluded_base_models.len(),
                    form.selected_aspect_ratios.len(),
                    form.periods
                        .get(form.selected_period)
                        .map(|period| period.label())
                        .unwrap_or("AllTime")
                ),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                " [Type] Query | [Enter] Apply | [Esc] Apply & Close | [Ctrl+R] Reset | [f] Open Builder ",
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
    let excluded_tag_focused = form.focused_section == ImageSearchFormSection::ExcludedTag;
    let base_focused = form.focused_section == ImageSearchFormSection::BaseModel;
    let excluded_base_focused = form.focused_section == ImageSearchFormSection::ExcludedBaseModel;
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
    let excluded_base_items = form
        .base_options
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            (
                item.label().to_string(),
                idx == form.excluded_base_cursor,
                form.excluded_base_models.contains(item.as_query_value()),
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
    let excluded_tag_suggestions =
        app.image_excluded_tag_suggestions(sections[5].height.saturating_sub(4) as usize);
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
    let excluded_base_models = form
        .excluded_base_models
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

    let sort_lines = if sort_focused {
        let mut lines = vec![Line::from(Span::styled(
            "Browse with Left/Right",
            help_text_style(),
        ))];
        lines.extend(build_wrapped_option_lines(
            &sort_items,
            sections[1].width.saturating_sub(4) as usize,
            sections[1].height.saturating_sub(3) as usize,
            sort_focused,
        ));
        lines
    } else {
        vec![Line::from(Span::styled(
            format!(
                "  Sort: {}",
                form.sort_options
                    .get(form.selected_sort)
                    .map(|value| value.label().to_string())
                    .unwrap_or_default()
            ),
            inactive_box_style(true),
        ))]
    };
    f.render_widget(
        Paragraph::new(sort_lines)
            .block(styled_search_block(" Sort ", sort_focused, true))
            .wrap(Wrap { trim: true }),
        sections[1],
    );

    let period_lines = if period_focused {
        let mut lines = vec![Line::from(Span::styled(
            "Browse with Left/Right",
            help_text_style(),
        ))];
        lines.extend(build_wrapped_option_lines(
            &period_items,
            sections[2].width.saturating_sub(4) as usize,
            sections[2].height.saturating_sub(3) as usize,
            period_focused,
        ));
        lines
    } else {
        vec![Line::from(Span::styled(
            format!(
                "  Period: {}",
                form.periods
                    .get(form.selected_period)
                    .map(|value| value.label().to_string())
                    .unwrap_or_default()
            ),
            inactive_box_style(true),
        ))]
    };
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
            empty_summary: "All",
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

    let excluded_tag_box = Paragraph::new(build_text_filter_box_lines(TextFilterBoxProps {
        label: "Excluded Tag",
        focused: excluded_tag_focused,
        configured: !form.excluded_tag_query.trim().is_empty(),
        value: &form.excluded_tag_query,
        placeholder: "Comma-separated tags to skip",
        suggestions: Some(excluded_tag_suggestions.as_slice()),
        width: sections[5].width.saturating_sub(4) as usize,
        height: sections[5].height.saturating_sub(3) as usize,
    }))
    .block(styled_search_block(
        " Excluded Tags ",
        excluded_tag_focused,
        !form.excluded_tag_query.trim().is_empty(),
    ))
    .wrap(Wrap { trim: true });
    f.render_widget(excluded_tag_box, sections[5]);

    f.render_widget(
        Paragraph::new(build_image_filter_box_lines(ImageFilterBoxProps {
            label: "Base Model",
            focused: base_focused,
            configured: !form.selected_base_models.is_empty(),
            items: &base_items,
            selected: &selected_base_models,
            show_selected: false,
            empty_summary: "All",
            width: sections[6].width.saturating_sub(4) as usize,
            height: sections[6].height.saturating_sub(3) as usize,
        }))
        .block(styled_search_block(
            " Base Model ",
            base_focused,
            !form.selected_base_models.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[6],
    );

    f.render_widget(
        Paragraph::new(build_image_filter_box_lines(ImageFilterBoxProps {
            label: "Excluded Base Model",
            focused: excluded_base_focused,
            configured: !form.excluded_base_models.is_empty(),
            items: &excluded_base_items,
            selected: &excluded_base_models,
            show_selected: false,
            empty_summary: "None",
            width: sections[7].width.saturating_sub(4) as usize,
            height: sections[7].height.saturating_sub(3) as usize,
        }))
        .block(styled_search_block(
            " Excluded Base Model ",
            excluded_base_focused,
            !form.excluded_base_models.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[7],
    );

    f.render_widget(
        Paragraph::new(build_image_filter_box_lines(ImageFilterBoxProps {
            label: "Aspect Ratio",
            focused: ratio_focused,
            configured: !form.selected_aspect_ratios.is_empty(),
            items: &ratio_items,
            selected: &selected_aspect_ratios,
            show_selected: false,
            empty_summary: "All",
            width: sections[8].width.saturating_sub(4) as usize,
            height: sections[8].height.saturating_sub(3) as usize,
        }))
        .block(styled_search_block(
            " Aspect Ratio ",
            ratio_focused,
            !form.selected_aspect_ratios.is_empty(),
        ))
        .wrap(Wrap { trim: true }),
        sections[8],
    );

    f.render_widget(
        Paragraph::new(
            " [Up/Down] Section | [Left/Right] Change | [Space] Toggle | [Type] Query/Tag | [Ctrl+R] Reset | [T] Templates | [Enter] Apply | [Esc] Apply & Close ",
        )
        .wrap(Wrap { trim: true }),
        sections[9],
    );
}
