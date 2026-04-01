use ratatui::{Frame, layout::Rect};

use crate::tui::app::App;

pub(super) fn draw_footer_section(f: &mut Frame, app: &App, area: Rect) {
    super::draw_footer(f, app, area);
}
