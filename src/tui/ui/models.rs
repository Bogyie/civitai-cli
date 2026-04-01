use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::tui::app::{App, AppMode};

pub(super) fn draw_models_tab(f: &mut Frame, app: &mut App, area: Rect, enable_name_rolling: bool) {
    let model_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    super::draw_model_search_summary(f, app, model_chunks[0]);
    let selected_model = app.selected_model_in_active_view().cloned();
    let bookmarked_ids: Vec<u64> = app.bookmarks.iter().map(|model| model.id).collect();

    if app.show_model_details {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
            .split(model_chunks[1]);

        super::draw_model_list(
            app,
            f,
            split[0],
            &app.models,
            &app.model_list_state,
            &bookmarked_ids,
            enable_name_rolling,
        );
        super::draw_model_sidebar(f, app, split[1], selected_model.as_ref());
    } else {
        super::draw_model_list(
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
        super::draw_search_popup(f, &app.search_form, "Search Builder", "Quick Search");
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

    super::draw_bookmark_search_summary(f, app, bookmark_chunks[0]);
    if app.show_model_details {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
            .split(bookmark_chunks[1]);

        super::draw_model_list(
            app,
            f,
            split[0],
            bookmark_items,
            &app.bookmark_list_state,
            &[],
            enable_name_rolling,
        );
        super::draw_model_sidebar(f, app, split[1], selected_bookmark_model.as_ref());
    } else {
        super::draw_model_list(
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
        super::draw_search_popup(
            f,
            &app.bookmark_search_form_draft,
            "Bookmark Filters",
            "Bookmark Search",
        );
    }
    if app.mode == AppMode::BookmarkPathPrompt {
        super::draw_bookmark_path_prompt(f, app);
    }
}
