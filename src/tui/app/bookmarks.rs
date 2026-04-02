use super::*;

impl App {
    pub fn visible_bookmarks(&self) -> &[Model] {
        &self.visible_bookmarks_cache
    }

    pub(super) fn refresh_visible_bookmarks_cache(&mut self) {
        let query = self.bookmark_search_form.query.trim().to_ascii_lowercase();
        let mut items = self
            .bookmarks
            .iter()
            .filter(|model| {
                bookmark_matches_query(model, &query)
                    && bookmark_matches_type(model, &self.bookmark_search_form.selected_types)
                    && bookmark_matches_base_model(
                        model,
                        &self.bookmark_search_form.selected_base_models,
                    )
                    && bookmark_matches_period(
                        model,
                        self.bookmark_search_form
                            .periods
                            .get(self.bookmark_search_form.selected_period),
                    )
            })
            .cloned()
            .collect::<Vec<_>>();

        sort_bookmarks(
            &mut items,
            self.bookmark_search_form
                .sort_options
                .get(self.bookmark_search_form.selected_sort)
                .unwrap_or(&ModelSearchSortBy::Relevance),
        );
        self.visible_bookmarks_cache = items;
    }

    pub fn clamp_bookmark_selection(&mut self) {
        let visible = self.visible_bookmarks();
        if visible.is_empty() {
            self.bookmark_list_state.select(None);
            return;
        }

        let selected = self.bookmark_list_state.selected().unwrap_or(0);
        if selected >= visible.len() {
            self.bookmark_list_state.select(Some(visible.len() - 1));
        }
    }

    pub fn selected_model_in_active_view(&self) -> Option<&Model> {
        match self.active_tab {
            MainTab::Models => self
                .models
                .get(self.model_list_state.selected().unwrap_or(0)),
            MainTab::SavedModels => self
                .visible_bookmarks()
                .get(self.bookmark_list_state.selected().unwrap_or(0)),
            _ => None,
        }
    }

    pub fn is_model_bookmarked(&self, model_id: u64) -> bool {
        self.bookmarks.iter().any(|model| model.id == model_id)
    }

    pub fn toggle_bookmark_for_selected_model(&mut self, model: &Model) {
        if self.is_model_bookmarked(model.id) {
            self.bookmarks.retain(|item| item.id != model.id);
            self.set_status(format!("Removed bookmark: {}", model_name(model)));
        } else {
            self.bookmarks.push(model.clone());
            self.set_status(format!("Added bookmark: {}", model_name(model)));
        }
        self.deduplicate_bookmarks();
        self.rebuild_parsed_model_cache();
        self.refresh_visible_bookmarks_cache();
        if self.active_tab == MainTab::SavedModels {
            self.clamp_bookmark_selection();
        }
        self.persist_bookmarks();
    }

    pub fn confirm_remove_selected_bookmark(&mut self) {
        let Some(model_id) = self.pending_bookmark_remove_id.take() else {
            self.show_bookmark_confirm_modal = false;
            return;
        };

        if let Some(pos) = self.bookmarks.iter().position(|model| model.id == model_id) {
            let name = model_name(&self.bookmarks[pos]);
            self.bookmarks.remove(pos);
            self.rebuild_parsed_model_cache();
            self.refresh_visible_bookmarks_cache();
            self.persist_bookmarks();
            self.clamp_bookmark_selection();
            self.set_status(format!("Removed bookmark: {}", name));
        } else {
            self.set_warn("Bookmark already removed.");
        }

        self.show_bookmark_confirm_modal = false;
        self.pending_bookmark_remove_id = None;
    }

    pub fn cancel_bookmark_remove(&mut self) {
        self.show_bookmark_confirm_modal = false;
        self.pending_bookmark_remove_id = None;
    }

    pub fn request_bookmark_remove_selected(&mut self) {
        if self.active_tab != MainTab::SavedModels {
            return;
        }

        if let Some(model) = self.selected_model_in_active_view() {
            self.pending_bookmark_remove_id = Some(model.id);
            self.show_bookmark_confirm_modal = true;
        } else {
            self.set_warn("No bookmark selected");
        }
    }

    pub fn begin_bookmark_search(&mut self) {
        self.bookmark_search_form_draft = self.bookmark_search_form.clone();
        self.bookmark_query_draft = self.bookmark_search_form.query.clone();
        self.mode = AppMode::SearchSavedModels;
        self.set_status("Filter bookmarks. Enter apply, Esc cancel");
    }

    pub fn begin_bookmark_export_prompt(&mut self) {
        self.bookmark_path_draft = self
            .effective_bookmark_file_path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        self.bookmark_path_prompt_action = Some(BookmarkPathAction::Export);
        self.mode = AppMode::BookmarkPathPrompt;
        self.set_status("Bookmark export path. Enter to confirm, Esc to cancel.");
    }

    pub fn begin_bookmark_import_prompt(&mut self) {
        self.bookmark_path_draft = self
            .effective_bookmark_file_path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        self.bookmark_path_prompt_action = Some(BookmarkPathAction::Import);
        self.mode = AppMode::BookmarkPathPrompt;
        self.set_status("Bookmark import path. Enter to confirm, Esc to cancel.");
    }

    pub fn cancel_bookmark_path_prompt(&mut self) {
        self.bookmark_path_prompt_action = None;
        self.mode = AppMode::Browsing;
        self.set_status("Bookmark path input cancelled.");
    }

    pub fn apply_bookmark_path_prompt(&mut self) {
        let action = self.bookmark_path_prompt_action.take();
        if action.is_none() {
            self.mode = AppMode::Browsing;
            return;
        }

        self.mode = AppMode::Browsing;

        let path = {
            let trimmed = self.bookmark_path_draft.trim();
            if trimmed.is_empty() {
                self.effective_bookmark_file_path()
            } else {
                Some(PathBuf::from(trimmed))
            }
        };

        let Some(path) = path else {
            self.set_warn("No bookmark file path configured.");
            return;
        };

        self.set_bookmark_file_path(path.clone());

        match action {
            Some(BookmarkPathAction::Export) => self.export_bookmarks_to_path(path),
            Some(BookmarkPathAction::Import) => self.import_bookmarks_from_path(path),
            None => {}
        }
    }

    pub fn effective_bookmark_file_path(&self) -> Option<PathBuf> {
        self.bookmark_file_path
            .clone()
            .or_else(crate::config::AppConfig::bookmark_path)
    }

    pub fn set_bookmark_file_path(&mut self, path: PathBuf) {
        self.bookmark_file_path = Some(path.clone());
        self.config.bookmark_file_path = Some(path);
    }

    pub fn is_bookmark_export_prompt(&self) -> bool {
        matches!(
            self.bookmark_path_prompt_action,
            Some(BookmarkPathAction::Export)
        )
    }

    pub fn apply_bookmark_query(&mut self) {
        self.bookmark_search_form = self.bookmark_search_form_draft.clone();
        self.bookmark_query = self.bookmark_search_form.query.clone();
        self.bookmark_query_draft = self.bookmark_query.clone();
        self.refresh_visible_bookmarks_cache();
        self.mode = AppMode::Browsing;
        self.clamp_bookmark_selection();
        self.set_status(format!(
            "Bookmark filter applied: {}",
            if self.bookmark_search_form.query.is_empty() {
                "<all>".to_string()
            } else {
                self.bookmark_search_form.query.clone()
            }
        ));
    }

    pub fn cancel_bookmark_search(&mut self) {
        self.bookmark_search_form_draft = self.bookmark_search_form.clone();
        self.bookmark_query_draft = self.bookmark_search_form.query.clone();
        self.mode = AppMode::Browsing;
        self.set_status("Bookmark filter cancelled.");
    }

    pub fn export_bookmarks_to_path(&mut self, path: PathBuf) {
        self.set_bookmark_file_path(path.clone());

        if let Err(err) = save_bookmarks_to_file(&path, &self.bookmarks) {
            self.set_error("Failed to export bookmarks", err.to_string());
            return;
        }

        self.set_status_detail(
            format!("Exported {} bookmarks", self.bookmarks.len()),
            format!("Destination: {}", path.display()),
        );
    }

    pub fn import_bookmarks_from_path(&mut self, path: PathBuf) {
        self.set_bookmark_file_path(path.clone());
        let mut imported = load_bookmarks(Some(path.as_path()));
        if imported.is_empty() {
            self.set_warn("No bookmarks found in import file.");
            return;
        }

        let before = self.bookmarks.len();
        self.bookmarks.append(&mut imported);
        self.deduplicate_bookmarks();
        self.rebuild_parsed_model_cache();
        self.refresh_visible_bookmarks_cache();
        self.clamp_bookmark_selection();
        self.persist_bookmarks();

        if self.bookmarks.len() > before {
            self.set_status(format!(
                "Imported {} new bookmark(s).",
                self.bookmarks.len() - before
            ));
        } else {
            self.set_status("Import completed, no new bookmarks.");
        }
    }

    pub(super) fn deduplicate_bookmarks(&mut self) {
        let mut seen = HashSet::new();
        self.bookmarks.retain(|model| seen.insert(model.id));
    }

    pub fn persist_bookmarks(&mut self) {
        if let Some(path) = &self.bookmark_file_path
            && let Err(err) = save_bookmarks_to_file(path, &self.bookmarks)
        {
            self.set_error("Failed to persist bookmarks", err.to_string());
        }
    }
}
