use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use ratatui_image::{StatefulImage, protocol::StatefulProtocol};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::tui::{
    app::{App, AppMode, SearchFormMode, SearchFormSection, SearchFormState},
    model::{build_model_url, category_name, creator_name, model_name, tag_names},
};

use super::helpers::{
    ImageFilterBoxProps, TextFilterBoxProps, build_horizontal_item_window,
    build_image_filter_box_lines, build_model_modal_constraints, build_text_filter_box_lines,
    build_wrapped_option_lines, centered_rect, compact_cell_text, compact_file_size,
    compact_number, help_text_style, inactive_box_style, render_description_lines,
    rotate_left_chars, styled_search_block, wrap_joined_tags,
};

pub(super) fn draw_models_tab(f: &mut Frame, app: &mut App, area: Rect, enable_name_rolling: bool) {
    let model_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    draw_model_search_summary(f, app, model_chunks[0]);
    let selected_model = app.selected_model_in_active_view().cloned();
    let bookmarked_ids: Vec<u64> = app.bookmarks.iter().map(|model| model.id).collect();

    if app.show_model_details {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
            .split(model_chunks[1]);

        draw_model_list(
            app,
            f,
            split[0],
            &app.models,
            &app.model_list_state,
            &bookmarked_ids,
            enable_name_rolling,
        );
        draw_model_sidebar(f, app, split[1], selected_model.as_ref());
    } else {
        draw_model_list(
            app,
            f,
            model_chunks[1],
            &app.models,
            &app.model_list_state,
            &bookmarked_ids,
            enable_name_rolling,
        );
    }

    if app.mode == AppMode::SearchForm {
        draw_search_popup(f, &app.search_form, "Search Builder", "Quick Search");
    }
}

pub(super) fn draw_bookmarks_tab(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    enable_name_rolling: bool,
) {
    let bookmark_items = app.visible_bookmarks();
    let bookmark_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    let selected_bookmark_model = app.selected_model_in_active_view().cloned();

    draw_bookmark_search_summary(f, app, bookmark_chunks[0]);
    if app.show_model_details {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
            .split(bookmark_chunks[1]);

        draw_model_list(
            app,
            f,
            split[0],
            bookmark_items,
            &app.bookmark_list_state,
            &[],
            enable_name_rolling,
        );
        draw_model_sidebar(f, app, split[1], selected_bookmark_model.as_ref());
    } else {
        draw_model_list(
            app,
            f,
            bookmark_chunks[1],
            bookmark_items,
            &app.bookmark_list_state,
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

fn draw_model_list(
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
                        let size_label = file
                            .size_kb
                            .map(compact_file_size)
                            .unwrap_or_else(|| "N/A".to_string());
                        let summary = format!(
                            "{}[{}] {} | {}{}",
                            prefix,
                            size_label,
                            file.name,
                            file.format.as_deref().unwrap_or("file"),
                            file.fp
                                .as_deref()
                                .map(|value| format!("/{value}"))
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
            } else if app.model_version_image_failed.contains(&image_version_id) {
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
        } else if has_any_cached_image || is_waiting_for_selected_version {
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

    let help = Paragraph::new(
        " [Up/Down] Section | [Left/Right] Change | [Space] Toggle | [Type] Query/Tag | [Enter] Apply | [Esc] Cancel ",
    )
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: true });
    f.render_widget(help, sections[6]);
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
