use ratatui::{Frame, layout::Rect, widgets::Clear};

use crate::tui::app::{App, MainTab};

use super::{downloads, images, models};

pub(super) fn draw_active_tab(f: &mut Frame, app: &mut App, area: Rect, enable_name_rolling: bool) {
    match app.active_tab {
        MainTab::Models => models::draw_models_tab(f, app, area, enable_name_rolling),
        MainTab::Bookmarks => models::draw_bookmarks_tab(f, app, area, enable_name_rolling),
        MainTab::Images => {
            f.render_widget(Clear, area);
            images::draw_images_tab(f, app, area);
        }
        MainTab::ImageBookmarks => {
            f.render_widget(Clear, area);
            images::draw_image_bookmarks_tab(f, app, area);
        }
        MainTab::Downloads => downloads::draw_downloads_view(f, app, area),
        MainTab::Settings => downloads::draw_settings_view(f, app, area),
    }
}
