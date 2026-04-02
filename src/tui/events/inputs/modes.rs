use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::actions::{
    refresh_visible_media, reload_selected_image, reload_selected_model_cover,
};
use super::LoopControl;
use crate::tui::{
    app::{
        App, AppMode, ImageSearchFormSection, MainTab, SearchFormMode, SearchFormSection,
        WorkerCommand,
    },
    runtime::{current_image_protocol_area, current_image_render_request, debug_fetch_log},
};

pub(super) fn handle_mode_key(app: &mut App, key: KeyEvent) -> Option<LoopControl> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('r')) {
        match app.mode {
            AppMode::SearchForm => {
                app.search_form.reset();
                app.set_status("Reset model filters");
                return Some(LoopControl::Continue);
            }
            AppMode::SearchLikedModels => {
                app.liked_model_search_form_draft.reset();
                app.set_status("Reset liked model filters");
                return Some(LoopControl::Continue);
            }
            AppMode::SearchImages => {
                app.image_search_form.reset();
                app.set_status("Reset image filters");
                return Some(LoopControl::Continue);
            }
            AppMode::SearchLikedImages => {
                app.liked_image_query_draft.clear();
                app.set_status("Reset liked image filter");
                return Some(LoopControl::Continue);
            }
            _ => {}
        }
    }

    if app.mode == AppMode::SearchForm {
        handle_model_search_mode(app, key.code);
        return Some(LoopControl::Continue);
    }

    if app.mode == AppMode::SearchLikedModels {
        handle_liked_search_mode(app, key.code);
        return Some(LoopControl::Continue);
    }

    if app.mode == AppMode::SearchImages {
        handle_image_search_mode(app, key.code);
        return Some(LoopControl::Continue);
    }

    if app.mode == AppMode::SearchLikedImages {
        handle_image_liked_search_mode(app, key.code);
        return Some(LoopControl::Continue);
    }

    if app.mode == AppMode::LikedPathPrompt {
        handle_liked_path_prompt_mode(app, key.code);
        return Some(LoopControl::Continue);
    }

    if app.active_tab == MainTab::Settings && app.settings_form.editing {
        handle_settings_edit_mode(app, key.code);
        return Some(LoopControl::Continue);
    }

    None
}

fn handle_model_search_mode(app: &mut App, code: KeyCode) {
    if let KeyCode::Char(c) = code
        && matches!(
            app.search_form.focused_section,
            SearchFormSection::Query | SearchFormSection::Tag
        )
    {
        if app.search_form.focused_section == SearchFormSection::Query {
            app.search_form.query.push(c);
        } else {
            app.search_form.tag_query.push(c);
        }
        return;
    }

    if matches!(code, KeyCode::Backspace)
        && matches!(
            app.search_form.focused_section,
            SearchFormSection::Query | SearchFormSection::Tag
        )
    {
        if app.search_form.focused_section == SearchFormSection::Query {
            app.search_form.query.pop();
        } else {
            app.search_form.tag_query.pop();
        }
        return;
    }

    match code {
        KeyCode::Esc => {
            let selected_model_id = app.selected_model_version().map(|(model_id, _)| model_id);
            let selected_version_id = app
                .selected_model_version()
                .map(|(_, version_id)| version_id);
            let search_options = app.search_form.build_options();
            if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::SearchModels(
                    search_options,
                    selected_model_id,
                    selected_version_id,
                    false,
                    false,
                    None,
                ));
                app.model_search_has_more = true;
                app.model_search_loading_more = false;
                app.mode = AppMode::Browsing;
                app.status = format!("Searching for models: '{}'...", app.search_form.query);
            } else {
                app.mode = AppMode::Browsing;
                reload_selected_model_cover(app);
            }
        }
        KeyCode::Enter => {
            app.mode = AppMode::Browsing;
            let selected_model_id = app.selected_model_version().map(|(model_id, _)| model_id);
            let selected_version_id = app
                .selected_model_version()
                .map(|(_, version_id)| version_id);
            let search_options = app.search_form.build_options();
            debug_fetch_log(
                &app.config,
                &format!(
                    "UI: search submit query=\"{}\" limit={} append=false force_refresh=false",
                    app.search_form.query,
                    search_options.limit.unwrap_or(50)
                ),
            );
            if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::SearchModels(
                    search_options,
                    selected_model_id,
                    selected_version_id,
                    false,
                    false,
                    None,
                ));
                app.model_search_has_more = true;
                app.model_search_loading_more = false;
                app.status = format!("Searching for models: '{}'...", app.search_form.query);
            }
        }
        KeyCode::Char('T') => {
            app.open_search_template_modal(crate::tui::app::SearchTemplateKind::Model);
        }
        KeyCode::Up => {
            if app.search_form.mode == SearchFormMode::Builder {
                app.search_form.focused_section = match app.search_form.focused_section {
                    SearchFormSection::Query => SearchFormSection::BaseModel,
                    SearchFormSection::Sort => SearchFormSection::Query,
                    SearchFormSection::Period => SearchFormSection::Sort,
                    SearchFormSection::Type => SearchFormSection::Period,
                    SearchFormSection::Tag => SearchFormSection::Type,
                    SearchFormSection::BaseModel => SearchFormSection::Tag,
                };
            }
        }
        KeyCode::Down => {
            if app.search_form.mode == SearchFormMode::Builder {
                app.search_form.focused_section = match app.search_form.focused_section {
                    SearchFormSection::Query => SearchFormSection::Sort,
                    SearchFormSection::Sort => SearchFormSection::Period,
                    SearchFormSection::Period => SearchFormSection::Type,
                    SearchFormSection::Type => SearchFormSection::Tag,
                    SearchFormSection::Tag => SearchFormSection::BaseModel,
                    SearchFormSection::BaseModel => SearchFormSection::Query,
                };
            }
        }
        KeyCode::Left => {
            if app.search_form.mode == SearchFormMode::Builder {
                match app.search_form.focused_section {
                    SearchFormSection::Sort => {
                        if app.search_form.selected_sort > 0 {
                            app.search_form.selected_sort -= 1;
                        } else {
                            app.search_form.selected_sort =
                                app.search_form.sort_options.len().saturating_sub(1);
                        }
                    }
                    SearchFormSection::Period => {
                        if app.search_form.selected_period > 0 {
                            app.search_form.selected_period -= 1;
                        } else {
                            app.search_form.selected_period =
                                app.search_form.periods.len().saturating_sub(1);
                        }
                    }
                    SearchFormSection::Type => {
                        if app.search_form.type_cursor > 0 {
                            app.search_form.type_cursor -= 1;
                        } else {
                            app.search_form.type_cursor =
                                app.search_form.type_options.len().saturating_sub(1);
                        }
                    }
                    SearchFormSection::BaseModel => {
                        if app.search_form.base_cursor > 0 {
                            app.search_form.base_cursor -= 1;
                        } else {
                            app.search_form.base_cursor =
                                app.search_form.base_options.len().saturating_sub(1);
                        }
                    }
                    SearchFormSection::Tag | SearchFormSection::Query => {}
                }
            }
        }
        KeyCode::Right => {
            if app.search_form.mode == SearchFormMode::Builder {
                match app.search_form.focused_section {
                    SearchFormSection::Sort => {
                        app.search_form.selected_sort = (app.search_form.selected_sort + 1)
                            % app.search_form.sort_options.len();
                    }
                    SearchFormSection::Period => {
                        app.search_form.selected_period =
                            (app.search_form.selected_period + 1) % app.search_form.periods.len();
                    }
                    SearchFormSection::Type => {
                        app.search_form.type_cursor =
                            (app.search_form.type_cursor + 1) % app.search_form.type_options.len();
                    }
                    SearchFormSection::BaseModel => {
                        app.search_form.base_cursor =
                            (app.search_form.base_cursor + 1) % app.search_form.base_options.len();
                    }
                    SearchFormSection::Tag | SearchFormSection::Query => {}
                }
            }
        }
        KeyCode::Char('f') => app.search_form.begin_builder(),
        KeyCode::Char(' ') => {
            if app.search_form.mode == SearchFormMode::Builder {
                match app.search_form.focused_section {
                    SearchFormSection::Type => {
                        if let Some(item) = app
                            .search_form
                            .type_options
                            .get(app.search_form.type_cursor)
                            .cloned()
                            && !app.search_form.selected_types.insert(item.clone())
                        {
                            app.search_form.selected_types.remove(&item);
                        }
                    }
                    SearchFormSection::BaseModel => {
                        if let Some(item) = app
                            .search_form
                            .base_options
                            .get(app.search_form.base_cursor)
                            .cloned()
                            && !app.search_form.selected_base_models.insert(item.clone())
                        {
                            app.search_form.selected_base_models.remove(&item);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn handle_liked_search_mode(app: &mut App, code: KeyCode) {
    if let KeyCode::Char(c) = code
        && matches!(
            app.liked_model_search_form_draft.focused_section,
            SearchFormSection::Query | SearchFormSection::Tag
        )
    {
        if app.liked_model_search_form_draft.focused_section == SearchFormSection::Query {
            app.liked_model_search_form_draft.query.push(c);
        } else {
            app.liked_model_search_form_draft.tag_query.push(c);
        }
        return;
    }

    if matches!(code, KeyCode::Backspace)
        && matches!(
            app.liked_model_search_form_draft.focused_section,
            SearchFormSection::Query | SearchFormSection::Tag
        )
    {
        if app.liked_model_search_form_draft.focused_section == SearchFormSection::Query {
            app.liked_model_search_form_draft.query.pop();
        } else {
            app.liked_model_search_form_draft.tag_query.pop();
        }
        return;
    }

    match code {
        KeyCode::Esc => app.apply_liked_model_query(),
        KeyCode::Enter => app.apply_liked_model_query(),
        KeyCode::Up => {
            if app.liked_model_search_form_draft.mode == SearchFormMode::Builder {
                app.liked_model_search_form_draft.focused_section =
                    match app.liked_model_search_form_draft.focused_section {
                        SearchFormSection::Query => SearchFormSection::BaseModel,
                        SearchFormSection::Sort => SearchFormSection::Query,
                        SearchFormSection::Period => SearchFormSection::Sort,
                        SearchFormSection::Type => SearchFormSection::Period,
                        SearchFormSection::Tag => SearchFormSection::Type,
                        SearchFormSection::BaseModel => SearchFormSection::Tag,
                    };
            }
        }
        KeyCode::Down => {
            if app.liked_model_search_form_draft.mode == SearchFormMode::Builder {
                app.liked_model_search_form_draft.focused_section =
                    match app.liked_model_search_form_draft.focused_section {
                        SearchFormSection::Query => SearchFormSection::Sort,
                        SearchFormSection::Sort => SearchFormSection::Period,
                        SearchFormSection::Period => SearchFormSection::Type,
                        SearchFormSection::Type => SearchFormSection::Tag,
                        SearchFormSection::Tag => SearchFormSection::BaseModel,
                        SearchFormSection::BaseModel => SearchFormSection::Query,
                    };
            }
        }
        KeyCode::Left => {
            if app.liked_model_search_form_draft.mode == SearchFormMode::Builder {
                match app.liked_model_search_form_draft.focused_section {
                    SearchFormSection::Sort => {
                        if app.liked_model_search_form_draft.selected_sort > 0 {
                            app.liked_model_search_form_draft.selected_sort -= 1;
                        } else {
                            app.liked_model_search_form_draft.selected_sort = app
                                .liked_model_search_form_draft
                                .sort_options
                                .len()
                                .saturating_sub(1);
                        }
                    }
                    SearchFormSection::Period => {
                        if app.liked_model_search_form_draft.selected_period > 0 {
                            app.liked_model_search_form_draft.selected_period -= 1;
                        } else {
                            app.liked_model_search_form_draft.selected_period = app
                                .liked_model_search_form_draft
                                .periods
                                .len()
                                .saturating_sub(1);
                        }
                    }
                    SearchFormSection::Type => {
                        if app.liked_model_search_form_draft.type_cursor > 0 {
                            app.liked_model_search_form_draft.type_cursor -= 1;
                        } else {
                            app.liked_model_search_form_draft.type_cursor = app
                                .liked_model_search_form_draft
                                .type_options
                                .len()
                                .saturating_sub(1);
                        }
                    }
                    SearchFormSection::BaseModel => {
                        if app.liked_model_search_form_draft.base_cursor > 0 {
                            app.liked_model_search_form_draft.base_cursor -= 1;
                        } else {
                            app.liked_model_search_form_draft.base_cursor = app
                                .liked_model_search_form_draft
                                .base_options
                                .len()
                                .saturating_sub(1);
                        }
                    }
                    SearchFormSection::Tag | SearchFormSection::Query => {}
                }
            }
        }
        KeyCode::Right => {
            if app.liked_model_search_form_draft.mode == SearchFormMode::Builder {
                match app.liked_model_search_form_draft.focused_section {
                    SearchFormSection::Sort => {
                        app.liked_model_search_form_draft.selected_sort =
                            (app.liked_model_search_form_draft.selected_sort + 1)
                                % app.liked_model_search_form_draft.sort_options.len();
                    }
                    SearchFormSection::Period => {
                        app.liked_model_search_form_draft.selected_period =
                            (app.liked_model_search_form_draft.selected_period + 1)
                                % app.liked_model_search_form_draft.periods.len();
                    }
                    SearchFormSection::Type => {
                        app.liked_model_search_form_draft.type_cursor =
                            (app.liked_model_search_form_draft.type_cursor + 1)
                                % app.liked_model_search_form_draft.type_options.len();
                    }
                    SearchFormSection::BaseModel => {
                        app.liked_model_search_form_draft.base_cursor =
                            (app.liked_model_search_form_draft.base_cursor + 1)
                                % app.liked_model_search_form_draft.base_options.len();
                    }
                    SearchFormSection::Tag | SearchFormSection::Query => {}
                }
            }
        }
        KeyCode::Char('f') => app.liked_model_search_form_draft.begin_builder(),
        KeyCode::Char(' ') => {
            if app.liked_model_search_form_draft.mode == SearchFormMode::Builder {
                match app.liked_model_search_form_draft.focused_section {
                    SearchFormSection::Type => {
                        if let Some(item) = app
                            .liked_model_search_form_draft
                            .type_options
                            .get(app.liked_model_search_form_draft.type_cursor)
                            .cloned()
                            && !app
                                .liked_model_search_form_draft
                                .selected_types
                                .insert(item.clone())
                        {
                            app.liked_model_search_form_draft
                                .selected_types
                                .remove(&item);
                        }
                    }
                    SearchFormSection::BaseModel => {
                        if let Some(item) = app
                            .liked_model_search_form_draft
                            .base_options
                            .get(app.liked_model_search_form_draft.base_cursor)
                            .cloned()
                            && !app
                                .liked_model_search_form_draft
                                .selected_base_models
                                .insert(item.clone())
                        {
                            app.liked_model_search_form_draft
                                .selected_base_models
                                .remove(&item);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn handle_image_search_mode(app: &mut App, code: KeyCode) {
    if let KeyCode::Char(c) = code
        && matches!(
            app.image_search_form.focused_section,
            ImageSearchFormSection::Query
                | ImageSearchFormSection::Tag
                | ImageSearchFormSection::ExcludedTag
        )
    {
        match app.image_search_form.focused_section {
            ImageSearchFormSection::Query => app.image_search_form.query.push(c),
            ImageSearchFormSection::Tag => app.image_search_form.tag_query.push(c),
            ImageSearchFormSection::ExcludedTag => app.image_search_form.excluded_tag_query.push(c),
            _ => {}
        }
        return;
    }

    if matches!(code, KeyCode::Backspace)
        && matches!(
            app.image_search_form.focused_section,
            ImageSearchFormSection::Query
                | ImageSearchFormSection::Tag
                | ImageSearchFormSection::ExcludedTag
        )
    {
        match app.image_search_form.focused_section {
            ImageSearchFormSection::Query => {
                app.image_search_form.query.pop();
            }
            ImageSearchFormSection::Tag => {
                app.image_search_form.tag_query.pop();
            }
            ImageSearchFormSection::ExcludedTag => {
                app.image_search_form.excluded_tag_query.pop();
            }
            _ => {}
        }
        return;
    }

    match code {
        KeyCode::Esc => {
            app.mode = AppMode::Browsing;
            app.images.clear();
            app.image_cache.clear();
            app.image_bytes_cache.clear();
            app.selected_index = 0;
            app.image_feed_loaded = false;
            app.image_feed_loading = false;
            app.image_feed_next_page = None;
            app.image_feed_has_more = true;
            if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::FetchImages(
                    app.image_search_form.build_options(),
                    None,
                    current_image_render_request(),
                    current_image_protocol_area(),
                ));
                app.image_feed_loading = true;
                app.status = "Searching image feed...".into();
            } else {
                reload_selected_image(app);
            }
        }
        KeyCode::Up => {
            if app.image_search_form.mode == SearchFormMode::Builder {
                app.image_search_form.focused_section = match app.image_search_form.focused_section
                {
                    ImageSearchFormSection::Query => ImageSearchFormSection::AspectRatio,
                    ImageSearchFormSection::Sort => ImageSearchFormSection::Query,
                    ImageSearchFormSection::Period => ImageSearchFormSection::Sort,
                    ImageSearchFormSection::MediaType => ImageSearchFormSection::Period,
                    ImageSearchFormSection::Tag => ImageSearchFormSection::MediaType,
                    ImageSearchFormSection::ExcludedTag => ImageSearchFormSection::Tag,
                    ImageSearchFormSection::BaseModel => ImageSearchFormSection::ExcludedTag,
                    ImageSearchFormSection::ExcludedBaseModel => ImageSearchFormSection::BaseModel,
                    ImageSearchFormSection::AspectRatio => {
                        ImageSearchFormSection::ExcludedBaseModel
                    }
                };
            }
        }
        KeyCode::Down => {
            if app.image_search_form.mode == SearchFormMode::Builder {
                app.image_search_form.focused_section = match app.image_search_form.focused_section
                {
                    ImageSearchFormSection::Query => ImageSearchFormSection::Sort,
                    ImageSearchFormSection::Sort => ImageSearchFormSection::Period,
                    ImageSearchFormSection::Period => ImageSearchFormSection::MediaType,
                    ImageSearchFormSection::MediaType => ImageSearchFormSection::Tag,
                    ImageSearchFormSection::Tag => ImageSearchFormSection::ExcludedTag,
                    ImageSearchFormSection::ExcludedTag => ImageSearchFormSection::BaseModel,
                    ImageSearchFormSection::BaseModel => ImageSearchFormSection::ExcludedBaseModel,
                    ImageSearchFormSection::ExcludedBaseModel => {
                        ImageSearchFormSection::AspectRatio
                    }
                    ImageSearchFormSection::AspectRatio => ImageSearchFormSection::Query,
                };
            }
        }
        KeyCode::Left => {
            if app.image_search_form.mode == SearchFormMode::Builder {
                match app.image_search_form.focused_section {
                    ImageSearchFormSection::Sort => {
                        if app.image_search_form.selected_sort > 0 {
                            app.image_search_form.selected_sort -= 1;
                        } else {
                            app.image_search_form.selected_sort =
                                app.image_search_form.sort_options.len().saturating_sub(1);
                        }
                    }
                    ImageSearchFormSection::Period => {
                        if app.image_search_form.selected_period > 0 {
                            app.image_search_form.selected_period -= 1;
                        } else {
                            app.image_search_form.selected_period =
                                app.image_search_form.periods.len().saturating_sub(1);
                        }
                    }
                    ImageSearchFormSection::MediaType => {
                        if app.image_search_form.media_type_cursor > 0 {
                            app.image_search_form.media_type_cursor -= 1;
                        } else {
                            app.image_search_form.media_type_cursor = app
                                .image_search_form
                                .media_type_options
                                .len()
                                .saturating_sub(1);
                        }
                    }
                    ImageSearchFormSection::BaseModel => {
                        if app.image_search_form.base_cursor > 0 {
                            app.image_search_form.base_cursor -= 1;
                        } else {
                            app.image_search_form.base_cursor =
                                app.image_search_form.base_options.len().saturating_sub(1);
                        }
                    }
                    ImageSearchFormSection::ExcludedBaseModel => {
                        if app.image_search_form.excluded_base_cursor > 0 {
                            app.image_search_form.excluded_base_cursor -= 1;
                        } else {
                            app.image_search_form.excluded_base_cursor =
                                app.image_search_form.base_options.len().saturating_sub(1);
                        }
                    }
                    ImageSearchFormSection::AspectRatio => {
                        if app.image_search_form.aspect_ratio_cursor > 0 {
                            app.image_search_form.aspect_ratio_cursor -= 1;
                        } else {
                            app.image_search_form.aspect_ratio_cursor = app
                                .image_search_form
                                .aspect_ratio_options
                                .len()
                                .saturating_sub(1);
                        }
                    }
                    ImageSearchFormSection::Tag
                    | ImageSearchFormSection::ExcludedTag
                    | ImageSearchFormSection::Query => {}
                }
            }
        }
        KeyCode::Right => {
            if app.image_search_form.mode == SearchFormMode::Builder {
                match app.image_search_form.focused_section {
                    ImageSearchFormSection::Sort => {
                        app.image_search_form.selected_sort = (app.image_search_form.selected_sort
                            + 1)
                            % app.image_search_form.sort_options.len();
                    }
                    ImageSearchFormSection::Period => {
                        app.image_search_form.selected_period =
                            (app.image_search_form.selected_period + 1)
                                % app.image_search_form.periods.len();
                    }
                    ImageSearchFormSection::MediaType => {
                        app.image_search_form.media_type_cursor =
                            (app.image_search_form.media_type_cursor + 1)
                                % app.image_search_form.media_type_options.len();
                    }
                    ImageSearchFormSection::BaseModel => {
                        app.image_search_form.base_cursor = (app.image_search_form.base_cursor + 1)
                            % app.image_search_form.base_options.len();
                    }
                    ImageSearchFormSection::ExcludedBaseModel => {
                        app.image_search_form.excluded_base_cursor =
                            (app.image_search_form.excluded_base_cursor + 1)
                                % app.image_search_form.base_options.len();
                    }
                    ImageSearchFormSection::AspectRatio => {
                        app.image_search_form.aspect_ratio_cursor =
                            (app.image_search_form.aspect_ratio_cursor + 1)
                                % app.image_search_form.aspect_ratio_options.len();
                    }
                    ImageSearchFormSection::Tag => {
                        if app.accept_image_tag_suggestion() {
                            app.status = "Accepted image tag suggestion.".into();
                        }
                    }
                    ImageSearchFormSection::ExcludedTag => {
                        if app.accept_image_excluded_tag_suggestion() {
                            app.status = "Accepted excluded image tag suggestion.".into();
                        }
                    }
                    ImageSearchFormSection::Query => {}
                }
            }
        }
        KeyCode::Enter => {
            app.mode = AppMode::Browsing;
            app.images.clear();
            app.image_cache.clear();
            app.image_bytes_cache.clear();
            app.selected_index = 0;
            app.image_feed_loaded = false;
            app.image_feed_loading = false;
            app.image_feed_next_page = None;
            app.image_feed_has_more = true;
            if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::FetchImages(
                    app.image_search_form.build_options(),
                    None,
                    current_image_render_request(),
                    current_image_protocol_area(),
                ));
                app.image_feed_loading = true;
                app.status = "Searching image feed...".into();
            }
        }
        KeyCode::Char('T') => {
            app.open_search_template_modal(crate::tui::app::SearchTemplateKind::Image);
        }
        KeyCode::Char('f') => app.image_search_form.begin_builder(),
        KeyCode::Char(' ') => {
            if app.image_search_form.mode == SearchFormMode::Builder {
                match app.image_search_form.focused_section {
                    ImageSearchFormSection::MediaType => {
                        if let Some(item) = app
                            .image_search_form
                            .media_type_options
                            .get(app.image_search_form.media_type_cursor)
                            .cloned()
                        {
                            let key = item.as_query_value().to_string();
                            if !app
                                .image_search_form
                                .selected_media_types
                                .insert(key.clone())
                            {
                                app.image_search_form.selected_media_types.remove(&key);
                            }
                        }
                    }
                    ImageSearchFormSection::BaseModel => {
                        if let Some(item) = app
                            .image_search_form
                            .base_options
                            .get(app.image_search_form.base_cursor)
                            .cloned()
                        {
                            let key = item.as_query_value().to_string();
                            if !app
                                .image_search_form
                                .selected_base_models
                                .insert(key.clone())
                            {
                                app.image_search_form.selected_base_models.remove(&key);
                            }
                        }
                    }
                    ImageSearchFormSection::ExcludedBaseModel => {
                        if let Some(item) = app
                            .image_search_form
                            .base_options
                            .get(app.image_search_form.excluded_base_cursor)
                            .cloned()
                        {
                            let key = item.as_query_value().to_string();
                            if !app
                                .image_search_form
                                .excluded_base_models
                                .insert(key.clone())
                            {
                                app.image_search_form.excluded_base_models.remove(&key);
                            }
                        }
                    }
                    ImageSearchFormSection::AspectRatio => {
                        if let Some(item) = app
                            .image_search_form
                            .aspect_ratio_options
                            .get(app.image_search_form.aspect_ratio_cursor)
                            .cloned()
                        {
                            let key = item.as_query_value().to_string();
                            if !app
                                .image_search_form
                                .selected_aspect_ratios
                                .insert(key.clone())
                            {
                                app.image_search_form.selected_aspect_ratios.remove(&key);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn handle_image_liked_search_mode(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.apply_liked_image_query(),
        KeyCode::Enter => app.apply_liked_image_query(),
        KeyCode::Char(c) => app.liked_image_query_draft.push(c),
        KeyCode::Backspace => {
            app.liked_image_query_draft.pop();
        }
        _ => {}
    }
}

fn handle_liked_path_prompt_mode(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.cancel_liked_model_path_prompt(),
        KeyCode::Enter => app.apply_liked_model_path_prompt(),
        KeyCode::Char(c) => app.liked_model_path_draft.push(c),
        KeyCode::Backspace => {
            app.liked_model_path_draft.pop();
        }
        _ => {}
    }
}

fn handle_settings_edit_mode(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.settings_form.editing = false;
        }
        KeyCode::Enter => {
            if app.settings_form.focused_field == 12 {
                app.image_cache.clear();
                app.image_bytes_cache.clear();
                app.image_request_keys.clear();
                app.model_version_image_cache.clear();
                app.model_version_image_bytes_cache.clear();
                app.model_version_image_request_keys.clear();
                app.model_version_image_failed.clear();
                if let Some(tx) = &app.tx {
                    let _ = tx.try_send(WorkerCommand::ClearAllCaches);
                }
                app.status = "Clearing cache storage...".into();
                return;
            }
            if app.settings_form.focused_field == 0 {
                app.config.api_key = if app.settings_form.input_buffer.is_empty() {
                    None
                } else {
                    Some(app.settings_form.input_buffer.clone())
                };
            } else if app.settings_form.focused_field == 1 {
                let input = app.settings_form.input_buffer.trim();
                if input.is_empty() {
                    app.config.comfyui_path = None;
                } else if let Err(err) = app.config.set_comfyui_path(Some(input)) {
                    app.last_error = Some(err.to_string());
                    app.show_status_modal = true;
                    app.status = "Invalid ComfyUI path".into();
                    return;
                }
            } else if app.settings_form.focused_field == 3 {
                app.config.model_search_cache_path = if app.settings_form.input_buffer.is_empty() {
                    None
                } else {
                    Some(std::path::PathBuf::from(
                        app.settings_form.input_buffer.clone(),
                    ))
                };
            } else if app.settings_form.focused_field == 4 {
                match app.settings_form.input_buffer.trim().parse::<u64>() {
                    Ok(value) if value > 0 => app.config.model_search_cache_ttl_hours = value,
                    _ => {
                        app.last_error = Some("Cache TTL must be a positive integer".into());
                        app.show_status_modal = true;
                        app.status = "Invalid cache TTL value".into();
                        return;
                    }
                }
            } else if app.settings_form.focused_field == 5 {
                app.config.image_cache_path = if app.settings_form.input_buffer.is_empty() {
                    None
                } else {
                    Some(std::path::PathBuf::from(
                        app.settings_form.input_buffer.clone(),
                    ))
                };
            } else if app.settings_form.focused_field == 6 {
                match app.settings_form.input_buffer.trim().parse::<u64>() {
                    Ok(value) if value > 0 => app.config.image_search_cache_ttl_minutes = value,
                    _ => {
                        app.last_error =
                            Some("Image search cache TTL must be a positive integer".into());
                        app.show_status_modal = true;
                        app.status = "Invalid image search cache TTL value".into();
                        return;
                    }
                }
            } else if app.settings_form.focused_field == 7 {
                match app.settings_form.input_buffer.trim().parse::<u64>() {
                    Ok(value) if value > 0 => app.config.image_detail_cache_ttl_minutes = value,
                    _ => {
                        app.last_error =
                            Some("Image detail cache TTL must be a positive integer".into());
                        app.show_status_modal = true;
                        app.status = "Invalid image detail cache TTL value".into();
                        return;
                    }
                }
            } else if app.settings_form.focused_field == 8 {
                match app.settings_form.input_buffer.trim().parse::<u64>() {
                    Ok(value) => app.config.image_cache_ttl_minutes = value,
                    _ => {
                        app.last_error =
                            Some("Image cache TTL must be a non-negative integer".into());
                        app.show_status_modal = true;
                        app.status = "Invalid image cache TTL value".into();
                        return;
                    }
                }
            } else if app.settings_form.focused_field == 2 {
                let path = if app.settings_form.input_buffer.is_empty() {
                    None
                } else {
                    Some(std::path::PathBuf::from(
                        app.settings_form.input_buffer.clone(),
                    ))
                };
                app.config.liked_model_file_path = path.clone();
                app.liked_model_file_path = path;
            } else if app.settings_form.focused_field == 11 {
                let path = if app.settings_form.input_buffer.is_empty() {
                    None
                } else {
                    Some(std::path::PathBuf::from(
                        app.settings_form.input_buffer.clone(),
                    ))
                };
                if let Some(path) = path {
                    app.set_download_history_file_path(path);
                } else {
                    app.config.download_history_file_path = None;
                    app.download_history_file_path = None;
                }
            } else if app.settings_form.focused_field == 9 {
                app.config.media_quality = app.config.media_quality.next();
                refresh_visible_media(app);
            } else if app.settings_form.focused_field == 10 {
                app.config.debug_logging = !app.config.debug_logging;
            }
            if let Err(e) = app.config.save() {
                app.last_error = Some(format!("Failed to save config: {}", e));
                app.show_status_modal = true;
            } else {
                app.last_error = None;
                app.show_status_modal = false;
                app.status = "Settings saved to config.json".into();
                if let Some(tx) = &app.tx {
                    let _ = tx.try_send(WorkerCommand::UpdateConfig(app.config.clone()));
                }
            }
            app.settings_form.editing = false;
        }
        KeyCode::Char(c) => app.settings_form.input_buffer.push(c),
        KeyCode::Backspace => {
            app.settings_form.input_buffer.pop();
        }
        _ => {}
    }
}
