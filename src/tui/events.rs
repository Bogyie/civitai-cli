use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::fs::{self, create_dir_all};
use std::io::Stdout;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tokio::sync::mpsc;

use crate::tui::app::{
    App, AppMessage, AppMode, DownloadHistoryStatus, DownloadState, ImageSearchFormSection,
    MainTab, NewDownloadHistoryEntry, SearchFormMode, SearchFormSection, WorkerCommand,
};
use crate::tui::image::comfy_workflow_json;
use crate::tui::runtime::{
    current_image_render_request, current_model_cover_render_request, debug_fetch_log,
    render_request_key,
};
use crate::tui::ui;

fn copy_to_clipboard(value: &str) -> Result<()> {
    let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(value.as_bytes())?;
    }
    let _ = child.wait()?;
    Ok(())
}

fn save_text_artifact(prefix: &str, extension: &str, value: &str) -> Result<PathBuf> {
    let dir = std::env::current_dir()?.join("downloads").join("artifacts");
    create_dir_all(&dir)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let path = dir.join(format!("{prefix}-{ts}.{extension}"));
    fs::write(&path, value)?;
    Ok(path)
}

pub async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    mut rx: mpsc::Receiver<AppMessage>,
) -> Result<()> {
    let reload_selected_image = |app: &mut App| {
        if !matches!(app.active_tab, MainTab::Images | MainTab::ImageBookmarks) {
            return;
        }
        if let Some(image) = app.selected_image_in_active_view().cloned()
            && let Some(tx) = &app.tx
        {
            let request = current_image_render_request();
            let request_key = render_request_key(request, app.config.media_quality);
            if app.has_cached_image_request(image.id, &request_key) {
                if let Some(bytes) = app.image_bytes_cache.get(&image.id).cloned() {
                    let _ = tx.try_send(WorkerCommand::RebuildImageProtocol(image.id, bytes));
                }
            } else {
                app.image_cache.remove(&image.id);
                app.image_request_keys.remove(&image.id);
                let _ = tx.try_send(WorkerCommand::LoadImage(image, request));
            }
        }
    };

    let ensure_selected_image_loaded = |app: &mut App| {
        if !matches!(app.active_tab, MainTab::Images | MainTab::ImageBookmarks) {
            return;
        }
        if let Some(image) = app.selected_image_in_active_view().cloned() {
            let request = current_image_render_request();
            let request_key = render_request_key(request, app.config.media_quality);
            if !app.has_cached_image_request(image.id, &request_key)
                && let Some(tx) = &app.tx
            {
                let _ = tx.try_send(WorkerCommand::LoadImage(image, request));
            }
        }
    };

    let send_cover_priority = |app: &mut App| {
        if let Some((_, version_id, cover_url, source_dims)) =
            app.selected_model_version_with_cover_url()
        {
            let request = current_model_cover_render_request();
            let request_key = render_request_key(request, app.config.media_quality);
            if app.has_cached_model_cover_request(version_id, &request_key) {
                return;
            }
            if let Some(tx) = &app.tx {
                let _ = tx.try_send(WorkerCommand::PrioritizeModelCover(
                    version_id,
                    cover_url,
                    source_dims,
                    request,
                ));
            }
        }
    };

    let reload_selected_model_cover = |app: &mut App| {
        if !matches!(app.active_tab, MainTab::Models | MainTab::Bookmarks) {
            return;
        }
        let Some((_, version_id)) = app.selected_model_version() else {
            return;
        };
        if let Some(tx) = &app.tx {
            let request = current_model_cover_render_request();
            let request_key = render_request_key(request, app.config.media_quality);
            if app.has_cached_model_cover_request(version_id, &request_key) {
                if let Some(bytes) = app
                    .model_version_image_bytes_cache
                    .get(&version_id)
                    .and_then(|entries| entries.first())
                    .cloned()
                {
                    let _ = tx.try_send(WorkerCommand::RebuildModelCover(version_id, bytes));
                }
            } else {
                app.model_version_image_cache.remove(&version_id);
                app.model_version_image_request_keys.remove(&version_id);
                let selected_cover = app.selected_model_version_with_cover_url();
                let cover_url = selected_cover
                    .as_ref()
                    .and_then(|(_, _, cover_url, _)| cover_url.clone());
                let source_dims = selected_cover.and_then(|(_, _, _, source_dims)| source_dims);
                let _ = tx.try_send(WorkerCommand::PrioritizeModelCover(
                    version_id,
                    cover_url,
                    source_dims,
                    request,
                ));
            }
        }
    };

    let send_cover_prefetch = |app: &mut App| {
        let neighbors = app.selected_model_neighbor_cover_urls(2);
        if neighbors.is_empty() {
            return;
        }
        if let Some(tx) = &app.tx {
            let _ = tx.try_send(WorkerCommand::PrefetchModelCovers(
                neighbors,
                current_model_cover_render_request(),
            ));
        }
    };

    let send_image_model_detail_cover_priority = |app: &mut App| {
        if !app.show_image_model_detail_modal {
            return;
        }
        let Some((version_id, cover_url, source_dims)) = app.image_model_detail_selected_cover()
        else {
            return;
        };
        let request = current_model_cover_render_request();
        let request_key = render_request_key(request, app.config.media_quality);
        if app.has_cached_model_cover_request(version_id, &request_key) {
            return;
        }
        if let Some(tx) = &app.tx {
            let _ = tx.try_send(WorkerCommand::PrioritizeModelCover(
                version_id,
                cover_url,
                source_dims,
                request,
            ));
        }
    };

    let send_image_model_detail_cover_prefetch = |app: &mut App| {
        if !app.show_image_model_detail_modal {
            return;
        }
        let request = current_model_cover_render_request();
        let jobs = app.image_model_detail_neighbor_cover_urls(2);
        if jobs.is_empty() {
            return;
        }
        if let Some(tx) = &app.tx {
            let _ = tx.try_send(WorkerCommand::PrefetchModelCovers(jobs, request));
        }
    };

    let refresh_visible_media = |app: &mut App| {
        app.image_cache.clear();
        app.image_request_keys.clear();
        app.model_version_image_cache.clear();
        app.model_version_image_request_keys.clear();
        app.model_version_image_failed.clear();
        reload_selected_image(app);
        reload_selected_model_cover(app);
        send_cover_prefetch(app);
    };

    let request_image_feed_if_needed = |app: &mut App, next_page: Option<u32>| {
        match &next_page {
            None => {
                if app.image_feed_loaded {
                    return;
                }
            }
            Some(requested_next_page) => {
                if app.image_feed_loading
                    || app.image_feed_next_page.is_none()
                    || app.image_feed_next_page.as_ref() != Some(requested_next_page)
                {
                    return;
                }
            }
        };

        if app.image_feed_loading {
            return;
        }

        if let Some(tx) = &app.tx
            && tx
                .try_send(WorkerCommand::FetchImages(
                    app.image_search_form.build_options(),
                    next_page,
                    current_image_render_request(),
                ))
                .is_ok()
        {
            app.image_feed_loading = true;
            app.status = if app.image_feed_loaded {
                "Loading more images...".to_string()
            } else {
                "Fetching image feed...".to_string()
            };
        }
    };

    loop {
        let poll_timeout_ms = match app.mode {
            AppMode::SearchForm
            | AppMode::SearchImages
            | AppMode::SearchBookmarks
            | AppMode::SearchImageBookmarks
            | AppMode::BookmarkPathPrompt => 200,
            _ => 50,
        };

        terminal.draw(|f| ui::draw(f, app))?;

        // Wait for either terminal input or worker message update
        tokio::select! {
             // Polling keypresses
                 event_res = tokio::task::spawn_blocking(move || {
                    event::poll(std::time::Duration::from_millis(poll_timeout_ms))
                 }) => {
                 if let Ok(Ok(true)) = event_res
                     && let Ok(evt) = event::read() {
                        if let Event::Resize(_, _) = evt {
                            if app.show_image_model_detail_modal {
                                send_image_model_detail_cover_priority(app);
                                send_image_model_detail_cover_prefetch(app);
                            }
                            match app.active_tab {
                                MainTab::Models | MainTab::Bookmarks => {
                                    reload_selected_model_cover(app);
                                    send_cover_prefetch(app);
                                }
                                MainTab::Images | MainTab::ImageBookmarks => {
                                    reload_selected_image(app);
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if let Event::Key(key) = evt {
                        let is_ctrl_c_exit = matches!(key.code, KeyCode::Char('c'))
                            && key.modifiers.contains(KeyModifiers::CONTROL);
                        let switch_tab = |target: MainTab, app: &mut App| {
                            let prev_tab = app.active_tab;
                            app.active_tab = target;
                            app.mode = AppMode::Browsing;
                            match app.active_tab {
                                MainTab::Bookmarks => app.clamp_bookmark_selection(),
                                MainTab::Images => {
                                    if prev_tab != MainTab::Images {
                                        request_image_feed_if_needed(app, None);
                                    }
                                    ensure_selected_image_loaded(app);
                                }
                                MainTab::ImageBookmarks => {
                                    app.clamp_image_bookmark_selection();
                                    ensure_selected_image_loaded(app);
                                }
                                _ => {}
                            }
                        };

                        if !app.show_status_modal
                            && !app.show_help_modal
                            && !app.show_image_prompt_modal
                            && !app.show_image_model_detail_modal
                            && !app.show_bookmark_confirm_modal
                            && !app.show_exit_confirm_modal
                        {
                            match key.code {
                                KeyCode::Char('1') => {
                                    switch_tab(MainTab::Models, app);
                                    continue;
                                }
                                KeyCode::Char('2') => {
                                    switch_tab(MainTab::Bookmarks, app);
                                    continue;
                                }
                                KeyCode::Char('3') => {
                                    switch_tab(MainTab::Images, app);
                                    continue;
                                }
                                KeyCode::Char('4') => {
                                    switch_tab(MainTab::ImageBookmarks, app);
                                    continue;
                                }
                                KeyCode::Char('5') => {
                                    switch_tab(MainTab::Downloads, app);
                                    continue;
                                }
                                KeyCode::Char('6') => {
                                    switch_tab(MainTab::Settings, app);
                                    continue;
                                }
                                KeyCode::Tab => {
                                    let next = match app.active_tab {
                                        MainTab::Models => MainTab::Bookmarks,
                                        MainTab::Bookmarks => MainTab::Images,
                                        MainTab::Images => MainTab::ImageBookmarks,
                                        MainTab::ImageBookmarks => MainTab::Downloads,
                                        MainTab::Downloads => MainTab::Settings,
                                        MainTab::Settings => MainTab::Models,
                                    };
                                    switch_tab(next, app);
                                    continue;
                                }
                                KeyCode::BackTab => {
                                    let prev = match app.active_tab {
                                        MainTab::Models => MainTab::Settings,
                                        MainTab::Bookmarks => MainTab::Models,
                                        MainTab::Images => MainTab::Bookmarks,
                                        MainTab::ImageBookmarks => MainTab::Images,
                                        MainTab::Downloads => MainTab::ImageBookmarks,
                                        MainTab::Settings => MainTab::Downloads,
                                    };
                                    switch_tab(prev, app);
                                    continue;
                                }
                                _ => {}
                            }
                        }

                        if app.mode == AppMode::SearchForm {
                            match key.code {
                                KeyCode::Esc => {
                                    app.mode = AppMode::Browsing;
                                    reload_selected_model_cover(app);
                                }
                                KeyCode::Enter => {
                                    app.mode = AppMode::Browsing;
                                    let selected_model_id = app.selected_model_version().map(|(model_id, _)| model_id);
                                    let selected_version_id = app.selected_model_version().map(|(_, version_id)| version_id);
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
                                                    app.search_form.selected_sort = app.search_form.sort_options.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::Period => {
                                                if app.search_form.selected_period > 0 {
                                                    app.search_form.selected_period -= 1;
                                                } else {
                                                    app.search_form.selected_period = app.search_form.periods.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::Type => {
                                                if app.search_form.type_cursor > 0 {
                                                    app.search_form.type_cursor -= 1;
                                                } else {
                                                    app.search_form.type_cursor = app.search_form.type_options.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::BaseModel => {
                                                if app.search_form.base_cursor > 0 {
                                                    app.search_form.base_cursor -= 1;
                                                } else {
                                                    app.search_form.base_cursor = app.search_form.base_options.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::Tag => {}
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Right => {
                                    if app.search_form.mode == SearchFormMode::Builder {
                                        match app.search_form.focused_section {
                                            SearchFormSection::Sort => {
                                                app.search_form.selected_sort =
                                                    (app.search_form.selected_sort + 1) % app.search_form.sort_options.len();
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
                                            SearchFormSection::Tag => {}
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Char('f') => {
                                    app.search_form.begin_builder();
                                }
                                KeyCode::Char(' ') => {
                                    if app.search_form.mode == SearchFormMode::Builder {
                                        match app.search_form.focused_section {
                                            SearchFormSection::Type => {
                                                if let Some(item) = app.search_form.type_options.get(app.search_form.type_cursor).cloned()
                                                    && !app.search_form.selected_types.insert(item.clone()) {
                                                        app.search_form.selected_types.remove(&item);
                                                    }
                                            }
                                            SearchFormSection::BaseModel => {
                                                if let Some(item) = app.search_form.base_options.get(app.search_form.base_cursor).cloned()
                                                    && !app.search_form.selected_base_models.insert(item.clone()) {
                                                        app.search_form.selected_base_models.remove(&item);
                                                    }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Char(c) => {
                                    if app.search_form.focused_section == SearchFormSection::Query {
                                        app.search_form.query.push(c);
                                    } else if app.search_form.focused_section == SearchFormSection::Tag {
                                        app.search_form.tag_query.push(c);
                                    }
                                }
                                KeyCode::Backspace => {
                                    if app.search_form.focused_section == SearchFormSection::Query {
                                        app.search_form.query.pop();
                                    } else if app.search_form.focused_section == SearchFormSection::Tag {
                                        app.search_form.tag_query.pop();
                                    }
                                }
                                _ => {}
                            }
                            continue; // Skip global navigation if form is active
                        }

                        if app.mode == AppMode::SearchBookmarks {
                            match key.code {
                                KeyCode::Esc => {
                                    app.cancel_bookmark_search();
                                    reload_selected_model_cover(app);
                                }
                                KeyCode::Enter => {
                                    app.apply_bookmark_query();
                                }
                                KeyCode::Up => {
                                    if app.bookmark_search_form_draft.mode == SearchFormMode::Builder {
                                        app.bookmark_search_form_draft.focused_section =
                                            match app.bookmark_search_form_draft.focused_section {
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
                                    if app.bookmark_search_form_draft.mode == SearchFormMode::Builder {
                                        app.bookmark_search_form_draft.focused_section =
                                            match app.bookmark_search_form_draft.focused_section {
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
                                    if app.bookmark_search_form_draft.mode == SearchFormMode::Builder {
                                        match app.bookmark_search_form_draft.focused_section {
                                            SearchFormSection::Sort => {
                                                if app.bookmark_search_form_draft.selected_sort > 0 {
                                                    app.bookmark_search_form_draft.selected_sort -= 1;
                                                } else {
                                                    app.bookmark_search_form_draft.selected_sort =
                                                        app.bookmark_search_form_draft.sort_options.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::Period => {
                                                if app.bookmark_search_form_draft.selected_period > 0 {
                                                    app.bookmark_search_form_draft.selected_period -= 1;
                                                } else {
                                                    app.bookmark_search_form_draft.selected_period =
                                                        app.bookmark_search_form_draft.periods.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::Type => {
                                                if app.bookmark_search_form_draft.type_cursor > 0 {
                                                    app.bookmark_search_form_draft.type_cursor -= 1;
                                                } else {
                                                    app.bookmark_search_form_draft.type_cursor =
                                                        app.bookmark_search_form_draft.type_options.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::BaseModel => {
                                                if app.bookmark_search_form_draft.base_cursor > 0 {
                                                    app.bookmark_search_form_draft.base_cursor -= 1;
                                                } else {
                                                    app.bookmark_search_form_draft.base_cursor =
                                                        app.bookmark_search_form_draft.base_options.len().saturating_sub(1);
                                                }
                                            }
                                            SearchFormSection::Tag => {}
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Right => {
                                    if app.bookmark_search_form_draft.mode == SearchFormMode::Builder {
                                        match app.bookmark_search_form_draft.focused_section {
                                            SearchFormSection::Sort => {
                                                app.bookmark_search_form_draft.selected_sort =
                                                    (app.bookmark_search_form_draft.selected_sort + 1)
                                                        % app.bookmark_search_form_draft.sort_options.len();
                                            }
                                            SearchFormSection::Period => {
                                                app.bookmark_search_form_draft.selected_period =
                                                    (app.bookmark_search_form_draft.selected_period + 1)
                                                        % app.bookmark_search_form_draft.periods.len();
                                            }
                                            SearchFormSection::Type => {
                                                app.bookmark_search_form_draft.type_cursor =
                                                    (app.bookmark_search_form_draft.type_cursor + 1)
                                                        % app.bookmark_search_form_draft.type_options.len();
                                            }
                                            SearchFormSection::BaseModel => {
                                                app.bookmark_search_form_draft.base_cursor =
                                                    (app.bookmark_search_form_draft.base_cursor + 1)
                                                        % app.bookmark_search_form_draft.base_options.len();
                                            }
                                            SearchFormSection::Tag => {}
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Char('f') => {
                                    app.bookmark_search_form_draft.begin_builder();
                                }
                                KeyCode::Char(' ') => {
                                    if app.bookmark_search_form_draft.mode == SearchFormMode::Builder {
                                        match app.bookmark_search_form_draft.focused_section {
                                            SearchFormSection::Type => {
                                                if let Some(item) = app
                                                    .bookmark_search_form_draft
                                                    .type_options
                                                    .get(app.bookmark_search_form_draft.type_cursor)
                                                    .cloned()
                                                    && !app.bookmark_search_form_draft.selected_types.insert(item.clone()) {
                                                        app.bookmark_search_form_draft.selected_types.remove(&item);
                                                    }
                                            }
                                            SearchFormSection::BaseModel => {
                                                if let Some(item) = app
                                                    .bookmark_search_form_draft
                                                    .base_options
                                                    .get(app.bookmark_search_form_draft.base_cursor)
                                                    .cloned()
                                                    && !app
                                                        .bookmark_search_form_draft
                                                        .selected_base_models
                                                        .insert(item.clone())
                                                    {
                                                        app.bookmark_search_form_draft
                                                            .selected_base_models
                                                            .remove(&item);
                                                    }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Char(c) => {
                                    if app.bookmark_search_form_draft.focused_section == SearchFormSection::Query {
                                        app.bookmark_search_form_draft.query.push(c);
                                    } else if app.bookmark_search_form_draft.focused_section == SearchFormSection::Tag {
                                        app.bookmark_search_form_draft.tag_query.push(c);
                                    }
                                }
                                KeyCode::Backspace => {
                                    if app.bookmark_search_form_draft.focused_section == SearchFormSection::Query {
                                        app.bookmark_search_form_draft.query.pop();
                                    } else if app.bookmark_search_form_draft.focused_section == SearchFormSection::Tag {
                                        app.bookmark_search_form_draft.tag_query.pop();
                                    }
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.mode == AppMode::SearchImages {
                            match key.code {
                                KeyCode::Esc => {
                                    app.mode = AppMode::Browsing;
                                    reload_selected_image(app);
                                }
                                KeyCode::Up => {
                                    if app.image_search_form.mode == SearchFormMode::Builder {
                                        app.image_search_form.focused_section =
                                            match app.image_search_form.focused_section {
                                                ImageSearchFormSection::Query => ImageSearchFormSection::AspectRatio,
                                                ImageSearchFormSection::Sort => ImageSearchFormSection::Query,
                                                ImageSearchFormSection::Period => ImageSearchFormSection::Sort,
                                                ImageSearchFormSection::MediaType => ImageSearchFormSection::Period,
                                                ImageSearchFormSection::Tag => ImageSearchFormSection::MediaType,
                                                ImageSearchFormSection::BaseModel => ImageSearchFormSection::Tag,
                                                ImageSearchFormSection::AspectRatio => ImageSearchFormSection::BaseModel,
                                            };
                                    }
                                }
                                KeyCode::Down => {
                                    if app.image_search_form.mode == SearchFormMode::Builder {
                                        app.image_search_form.focused_section =
                                            match app.image_search_form.focused_section {
                                                ImageSearchFormSection::Query => ImageSearchFormSection::Sort,
                                                ImageSearchFormSection::Sort => ImageSearchFormSection::Period,
                                                ImageSearchFormSection::Period => ImageSearchFormSection::MediaType,
                                                ImageSearchFormSection::MediaType => ImageSearchFormSection::Tag,
                                                ImageSearchFormSection::Tag => ImageSearchFormSection::BaseModel,
                                                ImageSearchFormSection::BaseModel => ImageSearchFormSection::AspectRatio,
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
                                                    app.image_search_form.media_type_cursor =
                                                        app.image_search_form.media_type_options.len().saturating_sub(1);
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
                                            ImageSearchFormSection::AspectRatio => {
                                                if app.image_search_form.aspect_ratio_cursor > 0 {
                                                    app.image_search_form.aspect_ratio_cursor -= 1;
                                                } else {
                                                    app.image_search_form.aspect_ratio_cursor =
                                                        app.image_search_form.aspect_ratio_options.len().saturating_sub(1);
                                                }
                                            }
                                            ImageSearchFormSection::Tag => {}
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Right => {
                                    if app.image_search_form.mode == SearchFormMode::Builder {
                                        match app.image_search_form.focused_section {
                                            ImageSearchFormSection::Sort => {
                                                app.image_search_form.selected_sort =
                                                    (app.image_search_form.selected_sort + 1)
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
                                                app.image_search_form.base_cursor =
                                                    (app.image_search_form.base_cursor + 1)
                                                        % app.image_search_form.base_options.len();
                                            }
                                            ImageSearchFormSection::AspectRatio => {
                                                app.image_search_form.aspect_ratio_cursor =
                                                    (app.image_search_form.aspect_ratio_cursor + 1)
                                                        % app.image_search_form.aspect_ratio_options.len();
                                            }
                                            ImageSearchFormSection::Tag => {
                                                if app.accept_image_tag_suggestion() {
                                                    app.status =
                                                        "Accepted image tag suggestion.".into();
                                                }
                                            }
                                            _ => {}
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
                                        ));
                                        app.image_feed_loading = true;
                                        app.status = "Searching image feed...".into();
                                    }
                                }
                                KeyCode::Char('f') => {
                                    app.image_search_form.begin_builder();
                                }
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
                                                    if !app.image_search_form.selected_media_types.insert(key.clone()) {
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
                                                    if !app.image_search_form.selected_base_models.insert(key.clone()) {
                                                        app.image_search_form.selected_base_models.remove(&key);
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
                                                    if !app.image_search_form.selected_aspect_ratios.insert(key.clone()) {
                                                        app.image_search_form.selected_aspect_ratios.remove(&key);
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                KeyCode::Char(c) => {
                                    if app.image_search_form.focused_section == ImageSearchFormSection::Query {
                                        app.image_search_form.query.push(c);
                                    } else if app.image_search_form.focused_section == ImageSearchFormSection::Tag {
                                        app.image_search_form.tag_query.push(c);
                                    }
                                }
                                KeyCode::Backspace => {
                                    if app.image_search_form.focused_section == ImageSearchFormSection::Query {
                                        app.image_search_form.query.pop();
                                    } else if app.image_search_form.focused_section == ImageSearchFormSection::Tag {
                                        app.image_search_form.tag_query.pop();
                                    }
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.mode == AppMode::SearchImageBookmarks {
                            match key.code {
                                KeyCode::Esc => {
                                    app.cancel_image_bookmark_search();
                                }
                                KeyCode::Enter => {
                                    app.apply_image_bookmark_query();
                                }
                                KeyCode::Char(c) => {
                                    app.image_bookmark_query_draft.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.image_bookmark_query_draft.pop();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.mode == AppMode::BookmarkPathPrompt {
                            match key.code {
                                KeyCode::Esc => {
                                    app.cancel_bookmark_path_prompt();
                                }
                                KeyCode::Enter => {
                                    app.apply_bookmark_path_prompt();
                                }
                                KeyCode::Char(c) => {
                                    app.bookmark_path_draft.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.bookmark_path_draft.pop();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_status_modal {
                            match key.code {
                                KeyCode::Char('m') | KeyCode::Esc | KeyCode::Enter => {
                                    app.show_status_modal = false;
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_help_modal {
                            match key.code {
                                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Enter => {
                                    app.show_help_modal = false;
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_image_prompt_modal {
                            match key.code {
                                KeyCode::Char('m') | KeyCode::Esc | KeyCode::Enter => {
                                    app.show_image_prompt_modal = false;
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    app.image_prompt_scroll = app.image_prompt_scroll.saturating_add(1);
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    app.image_prompt_scroll = app.image_prompt_scroll.saturating_sub(1);
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_image_model_detail_modal {
                            match key.code {
                                KeyCode::Esc | KeyCode::Enter => {
                                    app.close_image_model_detail_modal();
                                }
                                KeyCode::Char('b') => {
                                    if let Some(model) = app.image_model_detail_model.clone() {
                                        app.toggle_bookmark_for_selected_model(&model);
                                    }
                                }
                                KeyCode::Char('d') => {
                                    if let Some(model) = app.image_model_detail_model.clone() {
                                        app.request_download_for_model(&model);
                                    }
                                }
                                KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('[') => {
                                    if let Some(model) = app.image_model_detail_model.clone() {
                                        app.select_previous_version_for_model(&model);
                                        send_image_model_detail_cover_priority(app);
                                        send_image_model_detail_cover_prefetch(app);
                                    }
                                }
                                KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(']') => {
                                    if let Some(model) = app.image_model_detail_model.clone() {
                                        app.select_next_version_for_model(&model);
                                        send_image_model_detail_cover_priority(app);
                                        send_image_model_detail_cover_prefetch(app);
                                    }
                                }
                                KeyCode::Char('K') => {
                                    if let Some(model) = app.image_model_detail_model.clone() {
                                        app.select_previous_file_for_model(&model);
                                    }
                                }
                                KeyCode::Char('J') => {
                                    if let Some(model) = app.image_model_detail_model.clone() {
                                        app.select_next_file_for_model(&model);
                                    }
                                }
                                KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                                    if let Some(model) = app.image_model_detail_model.clone() {
                                        app.select_previous_file_for_model(&model);
                                    }
                                }
                                KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
                                    if let Some(model) = app.image_model_detail_model.clone() {
                                        app.select_next_file_for_model(&model);
                                    }
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_bookmark_confirm_modal {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    app.confirm_remove_selected_bookmark();
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    app.cancel_bookmark_remove();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_resume_download_modal {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    let sessions = app.interrupted_download_sessions.clone();
                                    for session in sessions {
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::ResumeDownloadModel(
                                                session.model_id,
                                                session.version_id,
                                                session.file_path.clone(),
                                                session.downloaded_bytes,
                                                session.total_bytes,
                                            ));
                                        }
                                    }
                                    app.clear_interrupted_download_sessions();
                                    app.status = "Interrupted downloads resumed.".into();
                                }
                                KeyCode::Char('d') | KeyCode::Char('D') => {
                                    let sessions = app.interrupted_download_sessions.clone();
                                    for session in sessions {
                                        if let Some(path) = session.file_path.clone() {
                                            match fs::remove_file(&path) {
                                                Ok(()) => {}
                                                Err(err) if err.kind() == ErrorKind::NotFound => {}
                                                Err(err) => {
                                                    app.last_error =
                                                        Some(format!("Failed to delete file: {}", err));
                                                    app.show_status_modal = true;
                                                    app.status =
                                                        format!("Failed to delete file for {}", session.filename);
                                                }
                                            }
                                        }
                                        let _ = app.remove_history_for_session(
                                            session.model_id,
                                            session.version_id,
                                        );
                                    }
                                    app.clear_interrupted_download_sessions();
                                    app.status = "Interrupted downloads removed.".into();
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    app.cancel_resume_download_modal();
                                    app.status = "Resume interrupted downloads cancelled.".into();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_exit_confirm_modal {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    let sessions = app.collect_interrupt_sessions_from_active();
                                    for session in &sessions {
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::PauseDownload(session.model_id));
                                        }
                                        app.record_interrupted_session_to_history(session);
                                    }
                                    app.interrupted_download_sessions = sessions;
                                    app.persist_interrupted_downloads();
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::Quit);
                                    }
                                    app.show_exit_confirm_modal = false;
                                    break;
                                }
                                KeyCode::Char('d') | KeyCode::Char('D') => {
                                    let sessions = app.collect_interrupt_sessions_from_active();
                                    for session in sessions.iter() {
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::CancelDownload(session.model_id));
                                        }
                                        if let Some(path) = session.file_path.clone() {
                                            let _ = fs::remove_file(&path);
                                        }
                                        app.remove_history_for_session(session.model_id, session.version_id);
                                    }
                                    app.interrupted_download_sessions.clear();
                                    app.persist_interrupted_downloads();
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::Quit);
                                    }
                                    app.show_exit_confirm_modal = false;
                                    break;
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    app.cancel_exit_confirm_modal();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.active_tab == MainTab::Settings && app.settings_form.editing {
                            match key.code {
                                KeyCode::Esc => {
                                    app.settings_form.editing = false;
                                }
                                KeyCode::Enter => {
                                    if app.settings_form.focused_field == 11 {
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
                                        continue;
                                    }
                                    if app.settings_form.focused_field == 0 {
                                        app.config.api_key = if app.settings_form.input_buffer.is_empty() { None } else { Some(app.settings_form.input_buffer.clone()) };
                                    } else if app.settings_form.focused_field == 1 {
                                        app.config.comfyui_path = if app.settings_form.input_buffer.is_empty() { None } else { Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone())) };
                                    } else if app.settings_form.focused_field == 3 {
                                        app.config.model_search_cache_path = if app.settings_form.input_buffer.is_empty() {
                                            None
                                        } else {
                                            Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone()))
                                        };
                                    } else if app.settings_form.focused_field == 4 {
                                        match app.settings_form.input_buffer.trim().parse::<u64>() {
                                            Ok(value) if value > 0 => {
                                                app.config.model_search_cache_ttl_hours = value;
                                            }
                                            _ => {
                                                app.last_error = Some("Cache TTL must be a positive integer".into());
                                                app.show_status_modal = true;
                                                app.status = "Invalid cache TTL value".into();
                                                continue;
                                            }
                                        }
                                    } else if app.settings_form.focused_field == 5 {
                                        app.config.image_cache_path = if app.settings_form.input_buffer.is_empty() {
                                            None
                                        } else {
                                            Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone()))
                                        };
                                    } else if app.settings_form.focused_field == 6 {
                                        match app.settings_form.input_buffer.trim().parse::<u64>() {
                                            Ok(value) if value > 0 => {
                                                app.config.image_search_cache_ttl_minutes = value;
                                            }
                                            _ => {
                                                app.last_error = Some("Image search cache TTL must be a positive integer".into());
                                                app.show_status_modal = true;
                                                app.status = "Invalid image search cache TTL value".into();
                                                continue;
                                            }
                                        }
                                    } else if app.settings_form.focused_field == 7 {
                                        match app.settings_form.input_buffer.trim().parse::<u64>() {
                                            Ok(value) if value > 0 => {
                                                app.config.image_detail_cache_ttl_minutes = value;
                                            }
                                            _ => {
                                                app.last_error = Some("Image detail cache TTL must be a positive integer".into());
                                                app.show_status_modal = true;
                                                app.status = "Invalid image detail cache TTL value".into();
                                                continue;
                                            }
                                        }
                                    } else if app.settings_form.focused_field == 8 {
                                        match app.settings_form.input_buffer.trim().parse::<u64>() {
                                            Ok(value) => {
                                                app.config.image_cache_ttl_minutes = value;
                                            }
                                            _ => {
                                                app.last_error = Some("Image cache TTL must be a non-negative integer".into());
                                                app.show_status_modal = true;
                                                app.status = "Invalid image cache TTL value".into();
                                                continue;
                                            }
                                        }
                                    } else if app.settings_form.focused_field == 2 {
                                        let path = if app.settings_form.input_buffer.is_empty() {
                                            None
                                        } else {
                                            Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone()))
                                        };
                                        app.config.bookmark_file_path = path.clone();
                                        app.bookmark_file_path = path;
                                    } else if app.settings_form.focused_field == 10 {
                                        let path = if app.settings_form.input_buffer.is_empty() {
                                            None
                                        } else {
                                            Some(std::path::PathBuf::from(app.settings_form.input_buffer.clone()))
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
                                KeyCode::Char(c) => {
                                    app.settings_form.input_buffer.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.settings_form.input_buffer.pop();
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                match key.code {
                                    KeyCode::Char('d') => {
                                        app.move_list_selection_by(10);
                                        send_cover_priority(app);
                                        send_cover_prefetch(app);
                                        continue;
                                    }
                                    KeyCode::Char('u') => {
                                        app.move_list_selection_by(-10);
                                        send_cover_priority(app);
                                        send_cover_prefetch(app);
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            if key.modifiers.contains(KeyModifiers::SHIFT) {
                                match key.code {
                                    KeyCode::Up => {
                                        app.select_previous_file();
                                        continue;
                                    }
                                    KeyCode::Down => {
                                        app.select_next_file();
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                        } else if (app.active_tab == MainTab::Images
                            || app.active_tab == MainTab::ImageBookmarks)
                            && key.modifiers.contains(KeyModifiers::SHIFT) {
                                match key.code {
                                    KeyCode::Up => {
                                        app.select_previous_image_model();
                                        continue;
                                    }
                                    KeyCode::Down => {
                                        app.select_next_image_model();
                                        continue;
                                    }
                                    _ => {}
                                }
                            }

                        match key.code {
                            KeyCode::Char('1') => {
                                app.active_tab = MainTab::Models;
                            }
                            KeyCode::Char('2') => {
                                app.active_tab = MainTab::Bookmarks;
                                app.clamp_bookmark_selection();
                            }
                            KeyCode::Char('3') => {
                                let prev_tab = app.active_tab;
                                app.active_tab = MainTab::Images;
                                if prev_tab != MainTab::Images {
                                    request_image_feed_if_needed(app, None);
                                }
                                ensure_selected_image_loaded(app);
                            }
                            KeyCode::Char('4') => {
                                app.active_tab = MainTab::ImageBookmarks;
                                app.clamp_image_bookmark_selection();
                                ensure_selected_image_loaded(app);
                            }
                            KeyCode::Char('5') => {
                                app.active_tab = MainTab::Downloads;
                            }
                            KeyCode::Char('6') => {
                                app.active_tab = MainTab::Settings;
                            }
                            KeyCode::Char('q') | KeyCode::Esc if !is_ctrl_c_exit => {
                                if app.has_active_download() {
                                    app.begin_exit_confirm_modal();
                                } else if let Some(tx) = &app.tx {
                                    let _ = tx.try_send(WorkerCommand::Quit);
                                    break;
                                }
                            }
                            KeyCode::Char('c') if is_ctrl_c_exit => {
                                if app.has_active_download() {
                                    app.begin_exit_confirm_modal();
                                } else if let Some(tx) = &app.tx {
                                    let _ = tx.try_send(WorkerCommand::Quit);
                                    break;
                                }
                            }
                            KeyCode::Enter => {
                                if app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks {
                                    if let Some(selected_model) = app.selected_image_used_model()
                                        && selected_model.navigable {
                                            if let Some(model_id) = selected_model.model_id {
                                                app.begin_image_model_detail_modal_loading();
                                                if let Some(tx) = &app.tx {
                                                    let _ = tx.try_send(WorkerCommand::FetchModelDetail(
                                                        model_id,
                                                        selected_model.version_id,
                                                        selected_model
                                                            .query_name
                                                            .clone()
                                                            .unwrap_or_else(|| selected_model.label.clone()),
                                                    ));
                                                }
                                                app.status = format!(
                                                    "Opening model details: {}",
                                                    selected_model.label
                                                );
                                            } else {
                                                app.status =
                                                    "Selected model item has no model id.".to_string();
                                            }
                                            continue;
                                        }
                                } else if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    if let Some((_, version_id)) = app.selected_model_version() {
                                        app.image_search_form.set_linked_model_version(Some(version_id));
                                        app.active_tab = MainTab::Images;
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
                                            ));
                                            app.image_feed_loading = true;
                                            app.status =
                                                format!("Opening images for model version {version_id}...");
                                        }
                                        continue;
                                    } else {
                                        app.status =
                                            "Selected model has no model version id to open images.".into();
                                        continue;
                                    }
                                } else if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field == 9 {
                                        app.config.media_quality = app.config.media_quality.next();
                                        refresh_visible_media(app);
                                        if let Err(e) = app.config.save() {
                                            app.last_error = Some(format!("Failed to save config: {}", e));
                                            app.show_status_modal = true;
                                        } else {
                                            app.last_error = None;
                                            if let Some(tx) = &app.tx {
                                                let _ = tx.try_send(WorkerCommand::UpdateConfig(app.config.clone()));
                                            }
                                        }
                                        continue;
                                    }
                                    if app.settings_form.focused_field == 11 {
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
                                        continue;
                                    }
                                    app.settings_form.editing = true;
                                    app.settings_form.input_buffer = if app.settings_form.focused_field == 0 {
                                        app.config.api_key.clone().unwrap_or_default()
                                    } else if app.settings_form.focused_field == 1 {
                                        app.config.comfyui_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
                                    } else if app.settings_form.focused_field == 2 {
                                        app.config.bookmark_file_path.as_ref().map(|path| path.to_string_lossy().to_string()).unwrap_or_default()
                                    } else if app.settings_form.focused_field == 3 {
                                        app.config
                                            .model_search_cache_path
                                            .as_ref()
                                            .map(|path| path.to_string_lossy().to_string())
                                            .unwrap_or_default()
                                    } else if app.settings_form.focused_field == 5 {
                                        app.config
                                            .image_cache_path
                                            .as_ref()
                                            .map(|path| path.to_string_lossy().to_string())
                                            .unwrap_or_default()
                                    } else if app.settings_form.focused_field == 6 {
                                        app.config.image_search_cache_ttl_minutes.to_string()
                                    } else if app.settings_form.focused_field == 7 {
                                        app.config.image_detail_cache_ttl_minutes.to_string()
                                    } else if app.settings_form.focused_field == 8 {
                                        app.config.image_cache_ttl_minutes.to_string()
                                    } else if app.settings_form.focused_field == 10 {
                                        app.config
                                            .download_history_file_path
                                            .as_ref()
                                            .map(|path| path.to_string_lossy().to_string())
                                            .unwrap_or_default()
                                    } else if matches!(app.settings_form.focused_field, 9 | 11) {
                                        String::new()
                                    } else {
                                        app.config.model_search_cache_ttl_hours.to_string()
                                    };
                                }
                            }
                            KeyCode::Left | KeyCode::Char('h') => {
                                if app.active_tab == MainTab::Settings
                                    && !app.settings_form.editing
                                    && app.settings_form.focused_field == 9
                                {
                                    app.config.media_quality = app.config.media_quality.previous();
                                    refresh_visible_media(app);
                                    if let Err(e) = app.config.save() {
                                        app.last_error = Some(format!("Failed to save config: {}", e));
                                        app.show_status_modal = true;
                                    } else if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::UpdateConfig(app.config.clone()));
                                    }
                                    continue;
                                }

                                if app.active_tab == MainTab::Images
                                    || app.active_tab == MainTab::ImageBookmarks
                                {
                                    app.select_previous();
                                    ensure_selected_image_loaded(app);
                                } else if app.active_tab == MainTab::Models
                                    || app.active_tab == MainTab::Bookmarks
                                {
                                    app.select_previous_version();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                }
                            }
                            KeyCode::Right | KeyCode::Char('l') => {
                                if app.active_tab == MainTab::Settings
                                    && !app.settings_form.editing
                                    && app.settings_form.focused_field == 9
                                {
                                    app.config.media_quality = app.config.media_quality.next();
                                    refresh_visible_media(app);
                                    if let Err(e) = app.config.save() {
                                        app.last_error = Some(format!("Failed to save config: {}", e));
                                        app.show_status_modal = true;
                                    } else if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::UpdateConfig(app.config.clone()));
                                    }
                                    continue;
                                }

                                if app.active_tab == MainTab::Images
                                    || app.active_tab == MainTab::ImageBookmarks
                                {
                                    app.select_next();
                                    if app.active_tab == MainTab::Images && app.can_request_more_images(5)
                                        && let Some(next_page) = app.next_image_feed_page() {
                                            request_image_feed_if_needed(app, Some(next_page));
                                        }
                                    ensure_selected_image_loaded(app);
                                } else if app.active_tab == MainTab::Models
                                    || app.active_tab == MainTab::Bookmarks
                                {
                                    app.select_next_version();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                }
                            }
                            KeyCode::Char('b') => {
                                if app.active_tab == MainTab::Models {
                                    if let Some(model) = app.selected_model_in_active_view().cloned() {
                                        app.toggle_bookmark_for_selected_model(&model);
                                    }
                                } else if app.active_tab == MainTab::Bookmarks {
                                    app.request_bookmark_remove_selected();
                                } else if (app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks)
                                    && let Some(image) = app.selected_image_in_active_view().cloned() {
                                        app.toggle_bookmark_for_selected_image(&image);
                                    }
                            }
                            KeyCode::Char('j') => {
                                if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field < 11 { app.settings_form.focused_field += 1; }
                                } else if app.active_tab == MainTab::Downloads {
                                    if app.active_downloads.is_empty() {
                                        app.select_next_history();
                                    } else {
                                        app.select_next_download();
                                    }
                                } else {
                                    app.select_next();
                                    if app.active_tab == MainTab::Models && app.can_request_more_models() {
                                        let prefetch_threshold = 30usize;
                                        let load_more = app
                                            .model_list_state
                                            .selected()
                                            .is_some_and(|selected| {
                                                let trigger_idx = app.models.len().saturating_sub(prefetch_threshold);
                                                selected >= trigger_idx
                                            });
                                        if load_more
                                            && let Some((opts, next_page)) = app.next_model_search_options_if_needed() {
                                                debug_fetch_log(
                                                    &app.config,
                                                    &format!(
                                                    "UI: request more models append=true query=\"{}\" next_page={}",
                                                    opts.query.clone().unwrap_or_default()
                                                    ,
                                                    next_page.is_some()
                                                ),
                                            );
                                            if let Some(tx) = &app.tx {
                                                let _ = tx.try_send(WorkerCommand::SearchModels(
                                                    opts,
                                                    None,
                                                    None,
                                                    false,
                                                    true,
                                                    next_page,
                                                ));
                                                    app.status = "Loading more results...".to_string();
                                                }
                                            }
                                    }
                                    if app.active_tab == MainTab::Images && app.can_request_more_images(5)
                                        && let Some(next_page) = app.next_image_feed_page() {
                                            request_image_feed_if_needed(app, Some(next_page));
                                        }
                                    ensure_selected_image_loaded(app);
                                    if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                        send_cover_priority(app);
                                        send_cover_prefetch(app);
                                    }
                                }
                            }
                            KeyCode::Char('k') => {
                                if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field > 0 { app.settings_form.focused_field -= 1; }
                                } else if app.active_tab == MainTab::Downloads {
                                    if app.active_downloads.is_empty() {
                                        app.select_previous_history();
                                    } else {
                                        app.select_previous_download();
                                    }
                                } else {
                                    app.select_previous();
                                    ensure_selected_image_loaded(app);
                                    if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                        send_cover_priority(app);
                                        send_cover_prefetch(app);
                                    }
                                }
                            }
                            KeyCode::Down => {
                                if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field < 11 {
                                        app.settings_form.focused_field += 1;
                                    }
                                } else if app.active_tab == MainTab::Downloads {
                                    if app.active_downloads.is_empty() {
                                        app.select_next_history();
                                    } else {
                                        app.select_next_download();
                                    }
                                }
                            }
                            KeyCode::Up => {
                                if app.active_tab == MainTab::Settings {
                                    if app.settings_form.focused_field > 0 {
                                        app.settings_form.focused_field -= 1;
                                    }
                                } else if app.active_tab == MainTab::Downloads {
                                    if app.active_downloads.is_empty() {
                                        app.select_previous_history();
                                    } else {
                                        app.select_previous_download();
                                    }
                                }
                            }
                            KeyCode::Char('[') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_previous_version();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                }
                            },
                            KeyCode::Char(']') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_next_version();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                }
                            },
                            KeyCode::Char('J') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_next_file();
                                } else if app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks {
                                    app.select_next_image_model();
                                } else if app.active_tab == MainTab::Downloads {
                                    app.select_next_history();
                                }
                            }
                            KeyCode::Char('K') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_previous_file();
                                } else if app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks {
                                    app.select_previous_image_model();
                                } else if app.active_tab == MainTab::Downloads {
                                    app.select_previous_history();
                                }
                            }
                            KeyCode::Char('d') => {
                                if app.active_tab == MainTab::Downloads {
                                    if let Some(removed) = app.remove_selected_history() {
                                        let was_active = app.active_downloads.contains_key(&removed.model_id);
                                        if was_active
                                            && let Some(tx) = &app.tx {
                                                let _ = tx.try_send(WorkerCommand::CancelDownload(removed.model_id));
                                            }

                                        app.status = if was_active {
                                            format!(
                                                "Deleted history for {} and cancelled active download",
                                                removed.model_name
                                            )
                                        } else {
                                            format!("Deleted history for {}", removed.model_name)
                                        };
                                    } else {
                                        app.status = "No download history selected".into();
                                    }
                                } else {
                                    app.request_download();
                                }
                            }
                            KeyCode::Char('D') => {
                                if app.active_tab == MainTab::Downloads {
                                    if let Some(removed) = app.remove_selected_history() {
                                        let was_active = app.active_downloads.contains_key(&removed.model_id);
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::CancelDownload(removed.model_id));
                                        }

                                        match removed.file_path {
                                        Some(path) => match fs::remove_file(&path) {
                                                Ok(()) => {
                                                    app.last_error = None;
                                                    app.status = format!(
                                                        "Deleted history and file for {}",
                                                        removed.model_name
                                                    );
                                                }
                                                Err(err) => {
                                                    app.last_error = Some(err.to_string());
                                                    app.show_status_modal = true;
                                                    app.status = if err.kind() == ErrorKind::NotFound {
                                                        format!("No file found for {}", removed.model_name)
                                                    } else {
                                                        format!(
                                                            "Failed to delete file for {}: {}",
                                                            removed.model_name,
                                                            err
                                                        )
                                                    };
                                                }
                                            },
                                            None => {
                                                app.last_error = None;
                                                app.status = format!("No file path recorded for {}", removed.model_name);
                                            }
                                        }

                                        if was_active {
                                            app.status = format!(
                                                "{} (and cancelled active download)",
                                                app.status
                                            );
                                        }
                                    } else {
                                        app.status = "No download history selected".into();
                                    }
                                } else {
                                    app.request_download();
                                }
                            }
                            KeyCode::Char('p') => {
                                if app.active_tab == MainTab::Downloads {
                                    if let Some(download_id) = app.selected_download_id()
                                        && let Some(tracker) = app.active_downloads.get(&download_id) {
                                            if tracker.state == DownloadState::Running {
                                                if let Some(tx) = &app.tx {
                                                    let _ = tx.try_send(WorkerCommand::PauseDownload(download_id));
                                                }
                                            } else {
                                                if let Some(tx) = &app.tx {
                                                    let _ = tx.try_send(WorkerCommand::ResumeDownload(download_id));
                                                }
                                            }
                                        }
                                } else {
                                    app.request_download();
                                }
                            }
                            KeyCode::Char('c') => {
                                if app.active_tab == MainTab::Models {
                                    if let Some(tx) = &app.tx {
                                        let _ = tx.try_send(WorkerCommand::ClearSearchCache);
                                        app.status = "Clearing cached search results...".into();
                                    }
                                } else if (app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks)
                                    && let Some(image) = app.selected_image_in_active_view()
                                {
                                    if let Some(json) = comfy_workflow_json(image) {
                                        match copy_to_clipboard(&json) {
                                            Ok(()) => app.status = format!("Copied Comfy workflow for image {}", image.id),
                                            Err(err) => {
                                                app.last_error = Some(err.to_string());
                                                app.show_status_modal = true;
                                                app.status = "Failed to copy workflow".into();
                                            }
                                        }
                                    } else {
                                        app.status = "No Comfy workflow metadata for current image".into();
                                    }
                                } else if app.active_tab == MainTab::Downloads
                                    && let Some(download_id) = app.selected_download_id()
                                        && let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::CancelDownload(download_id));
                                        }
                            }
                            KeyCode::Char('m') => {
                                if app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks {
                                    app.show_image_prompt_modal = true;
                                    app.image_prompt_scroll = 0;
                                    app.status = "Prompt viewer opened".into();
                                } else {
                                    app.show_status_modal = true;
                                }
                            }
                            KeyCode::Char('a') => {
                                if app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks {
                                    app.image_advanced_visible = !app.image_advanced_visible;
                                    app.status = if app.image_advanced_visible {
                                        "Advanced image metadata enabled".into()
                                    } else {
                                        "Advanced image metadata hidden".into()
                                    };
                                }
                            }
                            KeyCode::Char('o') => {
                                if (app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks)
                                    && let Some(image) = app.selected_image_in_active_view()
                                {
                                    match copy_to_clipboard(&image.image_page_url()) {
                                        Ok(()) => {
                                            app.status = format!("Copied image page URL for {}", image.id);
                                        }
                                        Err(err) => {
                                            app.last_error = Some(err.to_string());
                                            app.show_status_modal = true;
                                            app.status = "Failed to copy image page URL".into();
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('w') => {
                                if (app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks)
                                    && let Some(image) = app.selected_image_in_active_view()
                                {
                                    if let Some(json) = comfy_workflow_json(image) {
                                        match copy_to_clipboard(&json) {
                                            Ok(()) => app.status = format!("Copied Comfy workflow for image {}", image.id),
                                            Err(err) => {
                                                app.last_error = Some(err.to_string());
                                                app.show_status_modal = true;
                                                app.status = "Failed to copy workflow".into();
                                            }
                                        }
                                    } else {
                                        app.status = "No Comfy workflow metadata for current image".into();
                                    }
                                }
                            }
                            KeyCode::Char('W') => {
                                if (app.active_tab == MainTab::Images || app.active_tab == MainTab::ImageBookmarks)
                                    && let Some(image) = app.selected_image_in_active_view()
                                {
                                    if let Some(json) = comfy_workflow_json(image) {
                                        match save_text_artifact("comfy-workflow", "json", &json) {
                                            Ok(path) => {
                                                app.status = format!("Saved workflow to {}", path.display());
                                            }
                                            Err(err) => {
                                                app.last_error = Some(err.to_string());
                                                app.show_status_modal = true;
                                                app.status = "Failed to save workflow".into();
                                            }
                                        }
                                    } else {
                                        app.status = "No Comfy workflow metadata for current image".into();
                                    }
                                }
                            }
                            KeyCode::Char('v') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.show_model_details = !app.show_model_details;
                                    app.status = if app.show_model_details {
                                        "Model details panel enabled".into()
                                    } else {
                                        "Model details panel disabled".into()
                                    };
                                }
                            }
                            KeyCode::Char('r') => {
                                if app.active_tab == MainTab::Models {
                                    if let Some(tx) = &app.tx {
                                        let selected_model_id = app.selected_model_version().map(|(model_id, _)| model_id);
                                        let selected_version_id = app.selected_model_version().map(|(_, version_id)| version_id);
                                        debug_fetch_log(
                                            &app.config,
                                            &format!(
                                                "UI: refresh search query=\"{}\" force=true append=false",
                                            app.search_form.query
                                        ),
                                    );
                                        let _ = tx.try_send(WorkerCommand::SearchModels(
                                            app.search_form.build_options(),
                                            selected_model_id,
                                            selected_version_id,
                                            true,
                                            false,
                                            None,
                                        ));
                                        app.model_search_has_more = true;
                                        app.model_search_loading_more = false;
                                        app.status = format!(
                                            "Refreshing search cache for '{}'",
                                            app.search_form.query
                                        );
                                    }
                                } else if app.active_tab == MainTab::Downloads {
                                    if let Some(entry) = app.selected_download_history_entry().cloned() {
                                        if entry.total_bytes > 0 && entry.downloaded_bytes >= entry.total_bytes {
                                            app.status = "Selected item already complete.".into();
                                            continue;
                                        }
                                        if app.active_downloads.contains_key(&entry.model_id) {
                                            app.status = "Download already active for selected model.".into();
                                            continue;
                                        }
                                        if entry.file_path.is_none() {
                                            app.status = "Selected history has no file path.".into();
                                            continue;
                                        }
                                        let has_file = entry
                                            .file_path
                                            .as_deref()
                                            .is_some_and(|path| path.exists());
                                        if !has_file {
                                            app.last_error = Some("Missing partial file".to_string());
                                            app.show_status_modal = true;
                                            app.status = "Cannot resume: partial file not found.".into();
                                            continue;
                                        }
                                        if let Some(tx) = &app.tx {
                                            let _ = tx.try_send(WorkerCommand::ResumeDownloadModel(
                                                entry.model_id,
                                                entry.version_id,
                                                entry.file_path.clone(),
                                                entry.downloaded_bytes,
                                                entry.total_bytes,
                                            ));
                                            app.remove_history_for_session(entry.model_id, entry.version_id);
                                            app.status = format!("Resuming {} (v{})...", entry.model_name, entry.version_id);
                                        }
                                    } else {
                                        app.status = "No download history selected".into();
                                    }
                                }
                            }
                            KeyCode::Char('/') => {
                                if app.active_tab == MainTab::Models {
                                    app.mode = AppMode::SearchForm;
                                    app.search_form.begin_quick_search();
                                    app.status = "Quick search. Type query, Enter apply, Esc cancel.".into();
                                } else if app.active_tab == MainTab::Images {
                                    app.mode = AppMode::SearchImages;
                                    app.image_search_form.begin_quick_search();
                                    app.status = "Quick image search. Type query, Enter apply, Esc cancel.".into();
                                } else if app.active_tab == MainTab::ImageBookmarks {
                                    app.begin_image_bookmark_search();
                                } else if app.active_tab == MainTab::Bookmarks {
                                    app.begin_bookmark_search();
                                    app.bookmark_search_form_draft.begin_quick_search();
                                }
                            }
                            KeyCode::Char('f') => {
                                if app.active_tab == MainTab::Models {
                                    app.mode = AppMode::SearchForm;
                                    app.search_form.begin_builder();
                                    app.status = "Search builder. Up/Down sections, Left/Right options, Space toggle, Enter apply.".into();
                                } else if app.active_tab == MainTab::Images {
                                    app.mode = AppMode::SearchImages;
                                    app.image_search_form.begin_builder();
                                    app.status = "Image filters. Up/Down sections, Left/Right options, Space toggle, Enter apply.".into();
                                } else if app.active_tab == MainTab::Bookmarks {
                                    app.begin_bookmark_search();
                                    app.bookmark_search_form_draft.begin_builder();
                                    app.status = "Bookmark filters. Up/Down sections, Left/Right options, Space toggle, Enter apply.".into();
                                }
                            }
                            KeyCode::Char('g') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_list_first();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                } else if app.active_tab == MainTab::Images {
                                    app.selected_index = 0;
                                } else if app.active_tab == MainTab::ImageBookmarks {
                                    app.selected_image_bookmark_index = 0;
                                }
                            }
                            KeyCode::Char('G') => {
                                if app.active_tab == MainTab::Models || app.active_tab == MainTab::Bookmarks {
                                    app.select_list_last();
                                    send_cover_priority(app);
                                    send_cover_prefetch(app);
                                } else if app.active_tab == MainTab::Images {
                                    if !app.images.is_empty() {
                                        app.selected_index = app.images.len().saturating_sub(1);
                                    }
                                } else if app.active_tab == MainTab::ImageBookmarks {
                                    let visible = app.visible_image_bookmarks();
                                    if !visible.is_empty() {
                                        app.selected_image_bookmark_index = visible.len().saturating_sub(1);
                                    }
                                }
                            }
                            KeyCode::Char('?') => {
                                app.show_help_modal = true;
                            }
                            KeyCode::Char('e') => {
                                    if app.active_tab == MainTab::Bookmarks {
                                        app.begin_bookmark_export_prompt();
                                    }
                                }
                                KeyCode::Char('i') => {
                                    if app.active_tab == MainTab::Bookmarks {
                                        app.begin_bookmark_import_prompt();
                                    }
                                }
                            _ => {}
                        }

                        }
                     }
             }
             // Receiving decoded image bytes and status ticks from worker
             Some(msg) = rx.recv() => {
                 match msg {
                    AppMessage::ImagesLoaded(new_images, append, next_page) => {
                        app.merge_image_tag_catalog_from_hits(&new_images);
                        let loaded_count = new_images.len();
                        if append {
                            let before = app.images.len();
                            app.append_image_feed_results(new_images, next_page);
                            if app.active_tab == MainTab::Images {
                                app.status = format!(
                                    "Loaded {} more images (total {})",
                                    app.images.len().saturating_sub(before),
                                    app.images.len()
                                );
                            }
                        } else {
                            app.set_image_feed_results(new_images, next_page);
                            if app.active_tab == MainTab::Images {
                                app.status = format!("Loaded {} images", app.images.len());
                            }
                        }
                        if app.status.is_empty() && app.active_tab == MainTab::Images {
                            app.status = format!("Loaded {} images", app.images.len());
                        }
                        if loaded_count == 0 {
                            if let Some(next_page) = app.next_image_feed_page() {
                                request_image_feed_if_needed(app, Some(next_page));
                            }
                        } else if app.can_request_more_images(5)
                            && let Some(next_page) = app.next_image_feed_page() {
                                request_image_feed_if_needed(app, Some(next_page));
                            }
                    }
                     AppMessage::ImageDecoded(id, protocol, bytes, request_key) => {
                         app.image_cache.insert(id, protocol);
                         app.image_bytes_cache.insert(id, bytes);
                         if !request_key.is_empty() {
                             app.image_request_keys.insert(id, request_key);
                         }
                     }
                     AppMessage::ModelCoverDecoded(version_id, protocol, bytes, request_key) => {
                         app.model_version_image_cache.insert(version_id, vec![protocol]);
                         app.model_version_image_bytes_cache.insert(version_id, vec![bytes]);
                         if !request_key.is_empty() {
                             app.model_version_image_request_keys.insert(version_id, request_key);
                         }
                         app.model_version_image_failed.remove(&version_id);
                     }
                     AppMessage::ModelCoversDecoded(version_id, protocols, request_key) => {
                         let (protocols, bytes): (Vec<_>, Vec<_>) = protocols.into_iter().unzip();
                         app.model_version_image_cache.insert(version_id, protocols);
                         app.model_version_image_bytes_cache.insert(version_id, bytes);
                         if !request_key.is_empty() {
                             app.model_version_image_request_keys.insert(version_id, request_key);
                         }
                         app.model_version_image_failed.remove(&version_id);
                     }
                     AppMessage::ModelCoverLoadFailed(version_id) => {
                         app.model_version_image_failed.insert(version_id);
                     }
                     AppMessage::ModelsSearchedChunk(results, append, has_more, next_page) => {
                         let before_count = app.models.len();
                         debug_fetch_log(
                             &app.config,
                             &format!(
                                 "UI: received ModelsSearchedChunk append={} incoming={} before={} has_more={} next_page={}",
                                 append,
                                 results.len(),
                                 before_count,
                                 has_more,
                                 next_page.is_some(),
                             ),
                         );
                         if append {
                             let appended_len = results.len();
                             app.append_models_results(results, has_more, next_page);
                             debug_fetch_log(
                                 &app.config,
                                 &format!(
                                     "UI: append done before={} after={} has_more={}",
                                     before_count,
                                     app.models.len(),
                                     app.model_search_has_more
                                 ),
                             );
                             app.status = format!(
                                 "Loaded {} more models (total {})",
                                 appended_len,
                                 app.models.len()
                             );
                         } else {
                             app.set_models_results(results, has_more, next_page);
                             debug_fetch_log(
                                 &app.config,
                                 &format!(
                                     "UI: set models done count={} has_more={}",
                                     app.models.len(),
                                     app.model_search_has_more
                                 ),
                             );
                             send_cover_priority(app);
                             send_cover_prefetch(app);
                             app.status = format!("Found {} models", app.models.len());
                         }
                     }
                    AppMessage::ModelDetailLoaded(model, version_id) => {
                        app.open_image_model_detail_modal(*model, version_id);
                         send_image_model_detail_cover_priority(app);
                         send_image_model_detail_cover_prefetch(app);
                         app.status = if let Some(model) = app.image_model_detail_model.as_ref() {
                             format!("Loaded model details: {}", crate::tui::model::model_name(model))
                         } else {
                             "Loaded model details".to_string()
                         };
                     }
                     AppMessage::StatusUpdate(status) => {
                         app.status = status;
                         if app.status.contains("Error fetching images") {
                            app.image_feed_loading = false;
                         }
                         if is_error_status(&app.status) {
                             app.last_error = Some(app.status.clone());
                             app.show_status_modal = true;
                         } else {
                             app.last_error = None;
                             app.show_status_modal = false;
                         }
                     }
                     AppMessage::DownloadProgress(model_id, filename, progress, downloaded_bytes, total_bytes) => {
                         if let Some(existing) = app.active_downloads.get_mut(&model_id) {
                             existing.filename = filename;
                             existing.progress = progress;
                             existing.downloaded_bytes = downloaded_bytes;
                             existing.total_bytes = total_bytes;
                         }
                     }
                     AppMessage::DownloadStarted(
                        model_id,
                        filename,
                        version_id,
                        model_name,
                        total_bytes,
                        file_path,
                    ) => {
                         if !app.active_download_order.contains(&model_id) {
                             app.active_download_order.push(model_id);
                         }

                         app.active_downloads.insert(
                             model_id,
                             crate::tui::app::DownloadTracker {
                                 filename,
                                 progress: 0.0,
                                 downloaded_bytes: 0,
                                 total_bytes,
                                 file_path,
                                 model_name,
                                 version_id,
                                 state: DownloadState::Running,
                            },
                        );
                         app.status = format!("Download started for model {} ({})", model_id, version_id);
                     }
                     AppMessage::DownloadPaused(model_id) => {
                         if let Some(tracker) = app.active_downloads.get_mut(&model_id) {
                             tracker.state = DownloadState::Paused;
                             app.status = format!("Download paused: {}", tracker.filename);
                         }
                     }
                     AppMessage::DownloadResumed(model_id) => {
                         if let Some(tracker) = app.active_downloads.get_mut(&model_id) {
                             tracker.state = DownloadState::Running;
                             app.status = format!("Download resumed: {}", tracker.filename);
                         }
                     }
                     AppMessage::DownloadCompleted(model_id) => {
                         app.last_error = None;
                         if let Some(tracker) = app.active_downloads.remove(&model_id) {
                            app.push_download_history(NewDownloadHistoryEntry {
                                model_id,
                                version_id: tracker.version_id,
                                filename: tracker.filename,
                                model_name: tracker.model_name,
                                file_path: tracker.file_path,
                                downloaded_bytes: tracker.downloaded_bytes,
                                total_bytes: tracker.total_bytes,
                                status: DownloadHistoryStatus::Completed,
                                progress: tracker.progress,
                            });
                         }
                         app.active_download_order.retain(|id| *id != model_id);
                         app.clamp_selected_download_index();
                         app.clamp_selected_history_index();
                         app.status = format!("Download complete: {}", model_id);
                     }
                     AppMessage::DownloadFailed(model_id, reason) => {
                            if let Some(tracker) = app.active_downloads.remove(&model_id) {
                            app.push_download_history(NewDownloadHistoryEntry {
                                model_id,
                                version_id: tracker.version_id,
                                filename: tracker.filename,
                                model_name: tracker.model_name,
                                file_path: tracker.file_path,
                                downloaded_bytes: tracker.downloaded_bytes,
                                total_bytes: tracker.total_bytes,
                                status: DownloadHistoryStatus::Failed(reason.clone()),
                                progress: tracker.progress,
                            });
                         }
                         app.active_download_order.retain(|id| *id != model_id);
                         app.clamp_selected_download_index();
                         app.clamp_selected_history_index();
                         app.last_error = Some(reason.clone());
                         app.show_status_modal = true;
                         app.status = format!("Download failed: {}", reason);
                     }
                    AppMessage::DownloadCancelled(model_id) => {
                         if let Some(tracker) = app.active_downloads.remove(&model_id) {
                             app.push_download_history(NewDownloadHistoryEntry {
                                 model_id,
                                 version_id: tracker.version_id,
                                 filename: tracker.filename,
                                 model_name: tracker.model_name,
                                 file_path: tracker.file_path,
                                 downloaded_bytes: tracker.downloaded_bytes,
                                 total_bytes: tracker.total_bytes,
                                 status: DownloadHistoryStatus::Cancelled,
                                 progress: tracker.progress,
                             });
                         }
                         app.active_download_order.retain(|id| *id != model_id);
                         app.clamp_selected_download_index();
                         app.clamp_selected_history_index();
                         app.status = format!("Download cancelled: {}", model_id);
                     }
                 }
             }
        }
    }
    Ok(())
}

fn is_error_status(value: &str) -> bool {
    let lowered = value.to_lowercase();
    lowered.contains("error") || lowered.contains("failed") || lowered.contains("fail")
}
