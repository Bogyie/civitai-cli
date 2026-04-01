use ratatui::{Frame, layout::Rect};

use crate::tui::app::App;

pub(super) fn draw_downloads_view(f: &mut Frame, app: &App, area: Rect) {
    super::draw_downloads_tab(f, app, area);
}

pub(super) fn draw_settings_view(f: &mut Frame, app: &App, area: Rect) {
    super::draw_settings_tab(f, app, area);
}
