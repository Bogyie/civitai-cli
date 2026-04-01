use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::tui::app::{App, MainTab};

pub(super) fn draw_footer_section(f: &mut Frame, app: &App, area: Rect) {
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
    let status = Paragraph::new(status_line).alignment(ratatui::layout::Alignment::Left);
    f.render_widget(status, rows[0]);

    let shortcuts = match app.active_tab {
        MainTab::Models => {
            "[?] Help  [↑/↓ or j/k] Move  [/] Search  [f] Filter  [v] Detail  [←/→] Ver  [⇧↑/↓] File  [d] Download"
        }
        MainTab::Bookmarks => {
            "[?] Help  [↑/↓ or j/k] Move  [/] Search  [f] Filter  [v] Detail  [←/→] Ver  [⇧↑/↓] File  [b] Remove"
        }
        MainTab::Images => {
            "[?] Help  [↑/↓ or j/k] Image  [⇧↑/↓] Models  [Enter] Model  [m] Prompt  [d] Download  [c] Comfy"
        }
        MainTab::ImageBookmarks => {
            "[?] Help  [↑/↓ or j/k] Image  [⇧↑/↓] Models  [Enter] Model  [m] Prompt  [d] Download  [c] Comfy"
        }
        MainTab::Downloads => {
            "[?] Help  [j/k] Select  [p] Pause/Resume  [c] Cancel  [r] Resume  [d] Remove"
        }
        MainTab::Settings => "[?] Help  [j/k] Select  [Enter] Edit/Run  [h/l] Cycle  [Esc] Cancel",
    };

    let shortcuts_row = Paragraph::new(Span::styled(
        shortcuts,
        Style::default().fg(Color::DarkGray),
    ))
    .alignment(ratatui::layout::Alignment::Left)
    .wrap(Wrap { trim: true });
    f.render_widget(shortcuts_row, rows[1]);
}
