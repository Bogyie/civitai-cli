use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::tui::{
    app::{App, MainTab},
    image::{image_negative_prompt, image_prompt},
    model::model_name,
};

use super::{
    helpers::{centered_rect, help_text_style},
    models::draw_model_sidebar,
};

pub(super) fn draw_active_modals(f: &mut Frame, app: &mut App) {
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Prompt Viewer ");
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
        format!(
            " Model Details | [←/→] Version | [J/K] Files | [d] Download | [{bookmark_label}] | [Esc] Close "
        )
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Keyboard Help ");
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
            Span::styled(
                title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
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
            Line::from(" [j/k] or [↑/↓] Move selection"),
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
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Search & Input "),
    )
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
            Line::from(" [↑/↓] Change image"),
            Line::from(" [d] Download current image"),
            Line::from(" [b] Bookmark current image"),
            Line::from(" [m] Open full prompt viewer"),
            Line::from(" [a] Toggle advanced metadata"),
            Line::from(" [o] Copy image page link"),
            Line::from(" [Shift+↑/↓] Change used model"),
            Line::from(" [Enter] Open selected model details"),
            Line::from(" [c] Copy workflow  [W] Save workflow JSON"),
        ],
        MainTab::ImageBookmarks => vec![
            Line::from(" [↑/↓] Change image"),
            Line::from(" [d] Download current image"),
            Line::from(" [b] Remove current bookmark"),
            Line::from(" [m] Open full prompt viewer"),
            Line::from(" [a] Toggle advanced metadata"),
            Line::from(" [o] Copy image page link"),
            Line::from(" [Shift+↑/↓] Change used model"),
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Context Actions "),
        )
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
