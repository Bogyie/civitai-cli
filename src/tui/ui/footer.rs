use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::tui::app::{App, MainTab};
use crate::tui::status::is_status_stale;

pub(super) fn draw_footer_section(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(2)])
        .split(area);

    let status_color = match app.status_level {
        crate::tui::status::StatusLevel::Info => Color::Cyan,
        crate::tui::status::StatusLevel::Warn => Color::Yellow,
        crate::tui::status::StatusLevel::Debug => Color::DarkGray,
        crate::tui::status::StatusLevel::Error => Color::Red,
    };
    let left_status = format!(" [{}] {}", app.status_level.label(), app.status);
    let mut status_style = Style::default().fg(status_color);
    if is_status_stale(app.status_recorded_at) {
        status_style = status_style.add_modifier(Modifier::DIM);
    }
    let status_line = Line::from(Span::styled(left_status, status_style));
    let status = Paragraph::new(status_line).alignment(ratatui::layout::Alignment::Left);
    f.render_widget(status, rows[0]);

    let shortcuts = match app.active_tab {
        MainTab::Models => {
            "[?] Help  [M] Status Log  [↑/↓ or j/k] Move  [/] Search  [f] Filter  [v] Detail  [←/→] Ver  [⇧↑/↓] File  [d] Download"
        }
        MainTab::LikedModels => {
            "[?] Help  [M] Status Log  [↑/↓ or j/k] Move  [/] Search  [f] Filter  [v] Detail  [←/→] Ver  [⇧↑/↓] File  [b] Remove"
        }
        MainTab::Images => {
            "[?] Help  [M] Status Log  [↑/↓ or j/k] Image  [:] Jump  [⇧↑/↓] Models  [Enter] Model  [m] Prompt  [d] Download  [c] Comfy"
        }
        MainTab::LikedImages => {
            "[?] Help  [M] Status Log  [↑/↓ or j/k] Image  [:] Jump  [⇧↑/↓] Models  [Enter] Model  [m] Prompt  [d] Download  [c] Comfy"
        }
        MainTab::Downloads => {
            "[?] Help  [M] Status Log  [j/k] Select  [p] Pause/Resume  [c] Cancel  [r] Resume  [d] Remove"
        }
        MainTab::Settings => {
            "[?] Help  [M] Status Log  [j/k] Select  [Enter] Edit/Run  [h/l] Cycle  [Esc] Cancel"
        }
    };

    let shortcuts_row = Paragraph::new(Span::styled(
        shortcuts,
        Style::default().fg(Color::DarkGray),
    ))
    .alignment(ratatui::layout::Alignment::Left)
    .wrap(Wrap { trim: true });
    f.render_widget(shortcuts_row, rows[1]);
}
