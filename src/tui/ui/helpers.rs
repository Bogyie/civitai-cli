use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders},
};

use crate::tui::app::{ImageSearchFormSection, SearchFormSection};

pub(super) fn rotate_left_chars(src: &str, shift: usize) -> String {
    let mut chars: Vec<char> = src.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let shift = shift % chars.len();
    chars.rotate_left(shift);
    chars.into_iter().collect()
}

pub(super) fn compact_cell_text(src: String, width: usize) -> String {
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

pub(super) fn compact_cell_text_with_ellipsis(src: &str, width: usize) -> String {
    let value_chars: Vec<char> = src.chars().collect();
    if width == 0 || value_chars.is_empty() {
        return String::new();
    }

    if value_chars.len() <= width {
        return src.to_string();
    }

    if width <= 3 {
        return ".".repeat(width);
    }

    let mut truncated: String = value_chars.into_iter().take(width - 3).collect();
    truncated.push_str("...");
    truncated
}

pub(super) fn model_key_value_spans<'a>(key: &'a str, value: &'a str) -> Vec<Span<'a>> {
    vec![
        Span::styled(
            format!("{key}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ]
}

pub(super) fn inactive_box_style(is_configured: bool) -> Style {
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

pub(super) fn help_text_style() -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM)
}

pub(super) fn inactive_box_border_style(is_configured: bool) -> Style {
    if is_configured {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    }
}

pub(super) fn styled_search_block<'a>(
    title: &'a str,
    focused: bool,
    is_configured: bool,
) -> Block<'a> {
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

pub(super) fn build_wrapped_option_lines(
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
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if box_focused && *checked {
            Style::default().fg(Color::Green)
        } else if *checked {
            Style::default().fg(Color::Yellow)
        } else if box_focused {
            Style::default().fg(Color::White)
        } else {
            inactive_box_style(false)
        };

        spans.push(Span::styled(compact_cell_text(label, max_width), style));
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

pub(super) fn build_horizontal_option_window_lines(
    items: &[(String, bool, bool)],
    max_width: usize,
    box_focused: bool,
) -> Vec<Line<'static>> {
    if items.is_empty() || max_width == 0 {
        return Vec::new();
    }

    let item_count = items.len();
    let selected_idx = items
        .iter()
        .position(|(_, focused, _)| *focused)
        .unwrap_or(0)
        .min(item_count - 1);
    let separator = 2usize;

    let prepared: Vec<(String, bool, bool, usize)> = items
        .iter()
        .map(|(text, focused, checked)| {
            let label = format!("{} {}", if *checked { "[x]" } else { "[ ]" }, text);
            let width = label.chars().count().max(1);
            (text.clone(), *focused, *checked, width)
        })
        .collect();

    let ellipsis_width = if item_count > 1 { 4usize } else { 0usize };
    let available_width = max_width.saturating_sub(ellipsis_width * 2).max(1);

    let mut start = selected_idx;
    let mut end = selected_idx + 1;
    let mut used_width = prepared[selected_idx].3.min(available_width);

    while start > 0 || end < item_count {
        let mut extended = false;

        if end < item_count {
            let next_width = prepared[end].3 + separator;
            if used_width + next_width <= available_width {
                used_width += next_width;
                end += 1;
                extended = true;
            }
        }

        if start > 0 {
            let next_width = prepared[start - 1].3 + separator;
            if used_width + next_width <= available_width {
                used_width += next_width;
                start = start.saturating_sub(1);
                extended = true;
            }
        }

        if !extended {
            break;
        }
    }

    let mut spans = Vec::new();
    if start > 0 {
        spans.push(Span::styled("... ", help_text_style()));
    }

    spans.extend(prepared[start..end].iter().enumerate().flat_map(
        |(idx, (text, focused, checked, _))| {
            let style = if box_focused && *focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if box_focused && *checked {
                Style::default().fg(Color::Green)
            } else if *checked {
                Style::default().fg(Color::Yellow)
            } else if box_focused {
                Style::default().fg(Color::White)
            } else {
                inactive_box_style(false)
            };
            let mut spans = Vec::new();
            if idx > 0 || start > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                format!("{} {}", if *checked { "[x]" } else { "[ ]" }, text),
                style,
            ));
            spans
        },
    ));

    if end < item_count {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("...", help_text_style()));
    }

    vec![Line::from(spans)]
}

pub(super) fn build_horizontal_item_window(
    items: &[(String, bool)],
    max_width: usize,
) -> Vec<(String, bool)> {
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

pub(super) fn build_image_modal_constraints(
    focused: ImageSearchFormSection,
    total_height: u16,
) -> Vec<Constraint> {
    let help_height = 2u16;
    let collapsed = [3u16, 3, 3, 3, 3, 3, 3, 3, 3];
    let focused_index = match focused {
        ImageSearchFormSection::Query => 0,
        ImageSearchFormSection::Sort => 1,
        ImageSearchFormSection::Period => 2,
        ImageSearchFormSection::MediaType => 3,
        ImageSearchFormSection::Tag => 4,
        ImageSearchFormSection::ExcludedTag => 5,
        ImageSearchFormSection::BaseModel => 6,
        ImageSearchFormSection::ExcludedBaseModel => 7,
        ImageSearchFormSection::AspectRatio => 8,
    };
    let collapsed_total = collapsed.iter().sum::<u16>() + help_height;
    let extra = total_height.saturating_sub(collapsed_total);

    let mut constraints = Vec::with_capacity(10);
    for (idx, base) in collapsed.into_iter().enumerate() {
        let height = if idx == focused_index {
            base.saturating_add(extra)
        } else {
            base
        };
        constraints.push(Constraint::Length(height));
    }
    constraints.push(Constraint::Length(help_height));
    constraints
}

pub(super) fn build_model_modal_constraints(
    focused: SearchFormSection,
    total_height: u16,
) -> Vec<Constraint> {
    let help_height = 2u16;
    let collapsed = [3u16, 3, 3, 3, 3, 3];
    let focused_index = match focused {
        SearchFormSection::Query => 0,
        SearchFormSection::Sort => 1,
        SearchFormSection::Period => 2,
        SearchFormSection::Type => 3,
        SearchFormSection::Tag => 4,
        SearchFormSection::BaseModel => 5,
    };
    let collapsed_total = collapsed.iter().sum::<u16>() + help_height;
    let extra = total_height.saturating_sub(collapsed_total);

    let mut constraints = Vec::with_capacity(7);
    for (idx, base) in collapsed.into_iter().enumerate() {
        let height = if idx == focused_index {
            base.saturating_add(extra)
        } else {
            base
        };
        constraints.push(Constraint::Length(height));
    }
    constraints.push(Constraint::Length(help_height));
    constraints
}

pub(super) struct TextFilterBoxProps<'a> {
    pub label: &'a str,
    pub focused: bool,
    pub configured: bool,
    pub value: &'a str,
    pub placeholder: &'a str,
    pub suggestions: Option<&'a [String]>,
    pub width: usize,
    pub height: usize,
}

pub(super) fn build_text_filter_box_lines(props: TextFilterBoxProps<'_>) -> Vec<Line<'static>> {
    let TextFilterBoxProps {
        label,
        focused,
        configured,
        value,
        placeholder,
        suggestions,
        width,
        height,
    } = props;
    let mut lines = Vec::new();
    let display_value = if value.trim().is_empty() {
        placeholder.to_string()
    } else {
        value.to_string()
    };
    let current_style = if focused || configured {
        Style::default().fg(Color::Yellow)
    } else {
        inactive_box_style(false)
    };
    if !focused {
        let summary_width = width.saturating_sub(label.chars().count() + 4).max(1);
        let summary = compact_cell_text_with_ellipsis(&display_value, summary_width);
        lines.push(Line::from(Span::styled(
            format!("  {label}: {summary}"),
            if configured {
                inactive_box_style(true)
            } else {
                inactive_box_style(false)
            },
        )));
        return lines;
    }

    lines.push(Line::from(Span::styled(
        format!(
            "{} {}{}",
            if focused { ">" } else { " " },
            label,
            if focused { ":" } else { "" }
        ),
        current_style,
    )));
    let rendered_value = if focused {
        format!("{display_value}█")
    } else {
        display_value.clone()
    };

    let suggestion_lines = suggestions
        .filter(|items| focused && !items.is_empty())
        .map(|items| items.len().min(height.saturating_sub(3)))
        .unwrap_or(0);
    let value_height = height.saturating_sub(1 + suggestion_lines).max(1);
    lines.extend(style_lines(
        wrap_text_lines(&rendered_value, width.max(1), value_height),
        if !configured && !focused {
            help_text_style()
        } else if focused {
            Style::default().fg(Color::White)
        } else if configured {
            Style::default().fg(Color::Yellow)
        } else {
            inactive_box_style(false)
        },
    ));

    if let Some(items) = suggestions.filter(|items| focused && !items.is_empty()) {
        let hint = if value.trim().is_empty() {
            "Right: insert suggestion"
        } else {
            "Right: complete first match"
        };
        lines.push(Line::from(Span::styled(hint, help_text_style())));
        lines.extend(build_autocomplete_lines(
            items,
            width.max(1),
            suggestion_lines.max(1),
        ));
    }
    lines
}

pub(super) fn build_autocomplete_lines(
    suggestions: &[String],
    max_width: usize,
    max_lines: usize,
) -> Vec<Line<'static>> {
    if suggestions.is_empty() || max_width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let items = suggestions
        .iter()
        .enumerate()
        .map(|(idx, tag)| {
            let prefix = if idx == 0 { "> " } else { "  " };
            format!("{prefix}{tag}")
        })
        .collect::<Vec<_>>();

    style_lines(
        wrap_text_lines(&items.join(", "), max_width, max_lines),
        Style::default().fg(Color::Yellow),
    )
}

pub(super) struct ImageFilterBoxProps<'a> {
    pub label: &'a str,
    pub focused: bool,
    pub configured: bool,
    pub items: &'a [(String, bool, bool)],
    pub selected: &'a [String],
    pub show_selected: bool,
    pub empty_summary: &'a str,
    pub width: usize,
    pub height: usize,
}

pub(super) fn build_image_filter_box_lines(props: ImageFilterBoxProps<'_>) -> Vec<Line<'static>> {
    let ImageFilterBoxProps {
        label,
        focused,
        configured,
        items,
        selected,
        show_selected,
        empty_summary,
        width,
        height,
    } = props;
    if !focused {
        let summary = if selected.is_empty() {
            empty_summary.to_string()
        } else {
            selected.join(", ")
        };
        let summary_width = width.saturating_sub(label.chars().count() + 4).max(1);
        return vec![Line::from(Span::styled(
            format!(
                "  {label}: {}",
                compact_cell_text_with_ellipsis(&summary, summary_width)
            ),
            if configured {
                inactive_box_style(true)
            } else {
                inactive_box_style(false)
            },
        ))];
    }

    let current = items
        .iter()
        .find(|(_, current, _)| *current)
        .map(|(text, _, checked)| {
            format!(
                "{} Cursor: {} {}",
                if focused { ">" } else { " " },
                text,
                if *checked { "[x]" } else { "[ ]" }
            )
        })
        .unwrap_or_else(|| format!("{} Cursor: <none>", if focused { ">" } else { " " }));

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
        help_text_style(),
    )));
    let _ = height;
    lines.extend(build_horizontal_option_window_lines(items, width, focused));
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

pub(super) fn wrap_text_lines(text: &str, width: usize, height: usize) -> Vec<Line<'static>> {
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

pub(super) fn style_lines(lines: Vec<Line<'static>>, style: Style) -> Vec<Line<'static>> {
    lines
        .into_iter()
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

pub(super) fn compact_number(v: u64) -> String {
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

pub(super) fn compact_file_size(size_kb: f64) -> String {
    if size_kb <= 0.0 {
        return "0 B".to_string();
    }

    let mb = 1024.0;
    let gb = mb * 1024.0;
    let tb = gb * 1024.0;
    let size_bytes = size_kb * 1024.0;

    if size_kb >= tb {
        format!("{:.1} TB", size_kb / tb)
    } else if size_kb >= gb {
        format!("{:.1} GB", size_kb / gb)
    } else if size_kb >= mb {
        format!("{:.1} MB", size_kb / mb)
    } else if size_bytes >= 1024.0 {
        format!("{:.1} KB", size_kb)
    } else {
        format!("{:.0} B", size_bytes.round())
    }
}

pub(super) fn compact_bytes(size_bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    let value = size_bytes as f64;
    if value >= TB as f64 {
        format!("{:.1} TB", value / TB as f64)
    } else if value >= GB as f64 {
        format!("{:.1} GB", value / GB as f64)
    } else if value >= MB as f64 {
        format!("{:.1} MB", value / MB as f64)
    } else if value >= KB as f64 {
        format!("{:.1} KB", value / KB as f64)
    } else {
        format!("{} B", size_bytes)
    }
}

pub(super) fn wrap_joined_tags(tags: &[String], width: usize, height: usize) -> Vec<Line<'static>> {
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

pub(super) fn render_description_lines(raw: &str) -> Vec<Line<'static>> {
    let text = if raw.contains('<') && raw.contains('>') {
        html2text::from_read(raw.as_bytes(), 120).unwrap_or_else(|_| raw.to_string())
    } else {
        raw.to_string()
    };

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

        if let Some(rest) = line.strip_prefix("• ") {
            let mut spans = vec![Span::styled("• ", Style::default().fg(Color::LightGreen))];
            spans.extend(parse_styled_markdown(
                rest,
                Style::default().fg(Color::White),
            ));
            lines.push(Line::from(spans));
            continue;
        }

        if let Some(rest) = line.strip_prefix("› ") {
            let mut spans = vec![Span::styled("› ", Style::default().fg(Color::DarkGray))];
            spans.extend(parse_styled_markdown(
                rest,
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

pub(super) fn parse_styled_markdown(raw: &str, base_style: Style) -> Vec<Span<'static>> {
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
            "**" | "__" => {
                let body = &raw[i + 2..];
                if let Some(end) = body.find(token) {
                    let styled = &body[..end];
                    if !styled.is_empty() {
                        spans.push(Span::styled(
                            styled.to_string(),
                            base_style.add_modifier(Modifier::BOLD),
                        ));
                    }
                    2 + end + 2
                } else {
                    spans.push(Span::styled(token.to_string(), base_style));
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
                for separator in [' ', '\t', '\n'] {
                    if let Some(idx) = rest.find(separator) {
                        end = end.min(idx);
                    }
                }

                let raw_url = &rest[..end];
                let trimmed = raw_url.trim_end_matches(['.', ',', ')', ']', '}', '>']);
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

pub(super) fn next_markdown_token(line: &str) -> Option<(usize, &'static str)> {
    let mut best: Option<(usize, &'static str)> = None;

    for token in ["https://", "http://", "**", "__", "*", "`", "["] {
        if let Some(pos) = line.find(token) {
            best = match best {
                Some((best_pos, best_token)) => {
                    if pos < best_pos {
                        Some((pos, token))
                    } else if pos == best_pos {
                        let next = if matches!(token, "**" | "__") {
                            token
                        } else if matches!(best_token, "**" | "__") {
                            best_token
                        } else {
                            token
                        };
                        Some((best_pos, next))
                    } else {
                        Some((best_pos, best_token))
                    }
                }
                None => Some((pos, token)),
            };
        }
    }

    best
}

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_text_by_character_count() {
        assert_eq!(rotate_left_chars("abcd", 1), "bcda");
        assert_eq!(rotate_left_chars("한글ab", 2), "ab한글");
    }

    #[test]
    fn compacts_byte_units() {
        assert_eq!(compact_bytes(512), "512 B");
        assert_eq!(compact_bytes(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn compacts_file_sizes_below_one_mb_without_showing_zero_mb() {
        assert_eq!(compact_file_size(0.25), "256 B");
        assert_eq!(compact_file_size(512.0), "512.0 KB");
        assert_eq!(compact_file_size(1024.0), "1.0 MB");
    }

    #[test]
    fn finds_preferred_markdown_token() {
        assert_eq!(next_markdown_token("**bold**"), Some((0, "**")));
        assert_eq!(next_markdown_token("x [link](url)"), Some((2, "[")));
    }

    #[test]
    fn centers_rect_with_expected_size() {
        let rect = centered_rect(
            50,
            40,
            Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 60,
            },
        );

        assert_eq!(rect.width, 60);
        assert_eq!(rect.height, 24);
        assert_eq!(rect.x, 30);
        assert_eq!(rect.y, 18);
    }
}
