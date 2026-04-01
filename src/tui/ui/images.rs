use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::tui::app::{App, AppMode};

pub(super) fn draw_images_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let image_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(image_chunks[1]);
    super::draw_image_search_summary(f, app, image_chunks[0]);
    super::draw_image_panel(f, app, main_chunks[0]);
    super::draw_image_sidebar(f, app, main_chunks[1]);
    if app.mode == AppMode::SearchImages {
        super::draw_image_search_popup(f, app);
    }
}

pub(super) fn draw_image_bookmarks_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let image_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(image_chunks[1]);
    super::draw_image_bookmark_search_summary(f, app, image_chunks[0]);
    super::draw_image_panel(f, app, main_chunks[0]);
    super::draw_image_sidebar(f, app, main_chunks[1]);
    if app.mode == AppMode::SearchImageBookmarks {
        super::draw_image_bookmark_search_popup(f, app);
    }
}
