use super::*;

impl App {
    pub fn visible_liked_models(&self) -> &[Model] {
        &self.visible_liked_models_cache
    }

    pub(super) fn refresh_visible_liked_models_cache(&mut self) {
        let query = self
            .liked_model_search_form
            .query
            .trim()
            .to_ascii_lowercase();
        let mut items = self
            .liked_models
            .iter()
            .filter(|model| {
                liked_model_matches_query(model, &query)
                    && liked_model_matches_type(model, &self.liked_model_search_form.selected_types)
                    && liked_model_matches_base_model(
                        model,
                        &self.liked_model_search_form.selected_base_models,
                    )
                    && liked_model_matches_period(
                        model,
                        self.liked_model_search_form
                            .periods
                            .get(self.liked_model_search_form.selected_period),
                    )
            })
            .cloned()
            .collect::<Vec<_>>();

        sort_liked_models(
            &mut items,
            self.liked_model_search_form
                .sort_options
                .get(self.liked_model_search_form.selected_sort)
                .unwrap_or(&ModelSearchSortBy::Relevance),
        );
        self.visible_liked_models_cache = items;
    }

    pub fn clamp_liked_model_selection(&mut self) {
        let visible = self.visible_liked_models();
        if visible.is_empty() {
            self.liked_model_list_state.select(None);
            return;
        }

        let selected = self.liked_model_list_state.selected().unwrap_or(0);
        if selected >= visible.len() {
            self.liked_model_list_state.select(Some(visible.len() - 1));
        }
    }

    pub fn selected_model_in_active_view(&self) -> Option<&Model> {
        match self.active_tab {
            MainTab::Models => self
                .models
                .get(self.model_list_state.selected().unwrap_or(0)),
            MainTab::LikedModels => self
                .visible_liked_models()
                .get(self.liked_model_list_state.selected().unwrap_or(0)),
            _ => None,
        }
    }

    pub fn is_model_liked(&self, model_id: u64) -> bool {
        self.liked_models.iter().any(|model| model.id == model_id)
    }

    pub fn toggle_like_for_selected_model(&mut self, model: &Model) {
        if self.is_model_liked(model.id) {
            self.liked_models.retain(|item| item.id != model.id);
            self.set_status(format!("Removed liked: {}", model_name(model)));
        } else {
            self.liked_models.push(model.clone());
            self.set_status(format!("Added liked: {}", model_name(model)));
        }
        self.deduplicate_liked_models();
        self.rebuild_parsed_model_cache();
        self.refresh_visible_liked_models_cache();
        if self.active_tab == MainTab::LikedModels {
            self.clamp_liked_model_selection();
        }
        self.persist_liked_models();
    }

    pub fn confirm_remove_selected_like(&mut self) {
        let Some(model_id) = self.pending_liked_model_remove_id.take() else {
            self.show_like_confirm_modal = false;
            return;
        };

        if let Some(pos) = self
            .liked_models
            .iter()
            .position(|model| model.id == model_id)
        {
            let name = model_name(&self.liked_models[pos]);
            self.liked_models.remove(pos);
            self.rebuild_parsed_model_cache();
            self.refresh_visible_liked_models_cache();
            self.persist_liked_models();
            self.clamp_liked_model_selection();
            self.set_status(format!("Removed liked: {}", name));
        } else {
            self.set_warn("Liked already removed.");
        }

        self.show_like_confirm_modal = false;
        self.pending_liked_model_remove_id = None;
    }

    pub fn cancel_like_remove(&mut self) {
        self.show_like_confirm_modal = false;
        self.pending_liked_model_remove_id = None;
    }

    pub fn request_liked_model_remove_selected(&mut self) {
        if self.active_tab != MainTab::LikedModels {
            return;
        }

        if let Some(model) = self.selected_model_in_active_view() {
            self.pending_liked_model_remove_id = Some(model.id);
            self.show_like_confirm_modal = true;
        } else {
            self.set_warn("No liked model selected");
        }
    }

    pub fn begin_liked_model_search(&mut self) {
        self.liked_model_search_form_draft = self.liked_model_search_form.clone();
        self.liked_model_query_draft = self.liked_model_search_form.query.clone();
        self.mode = AppMode::SearchLikedModels;
        self.set_status("Filter liked models. Enter/Esc apply and close");
    }

    pub fn begin_liked_model_export_prompt(&mut self) {
        self.liked_model_path_draft = self
            .effective_liked_model_file_path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        self.liked_model_path_prompt_action = Some(LikedPathAction::Export);
        self.mode = AppMode::LikedPathPrompt;
        self.set_status("Liked export path. Enter to confirm, Esc to cancel.");
    }

    pub fn begin_liked_model_import_prompt(&mut self) {
        self.liked_model_path_draft = self
            .effective_liked_model_file_path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        self.liked_model_path_prompt_action = Some(LikedPathAction::Import);
        self.mode = AppMode::LikedPathPrompt;
        self.set_status("Liked import path. Enter to confirm, Esc to cancel.");
    }

    pub fn cancel_liked_model_path_prompt(&mut self) {
        self.liked_model_path_prompt_action = None;
        self.mode = AppMode::Browsing;
        self.set_status("Liked path input cancelled.");
    }

    pub fn apply_liked_model_path_prompt(&mut self) {
        let action = self.liked_model_path_prompt_action.take();
        if action.is_none() {
            self.mode = AppMode::Browsing;
            return;
        }

        self.mode = AppMode::Browsing;

        let path = {
            let trimmed = self.liked_model_path_draft.trim();
            if trimmed.is_empty() {
                self.effective_liked_model_file_path()
            } else {
                Some(PathBuf::from(trimmed))
            }
        };

        let Some(path) = path else {
            self.set_warn("No liked model file path configured.");
            return;
        };

        self.set_liked_model_file_path(path.clone());

        match action {
            Some(LikedPathAction::Export) => self.export_liked_models_to_path(path),
            Some(LikedPathAction::Import) => self.import_liked_models_from_path(path),
            None => {}
        }
    }

    pub fn effective_liked_model_file_path(&self) -> Option<PathBuf> {
        self.liked_model_file_path
            .clone()
            .or_else(crate::config::AppConfig::liked_model_path)
    }

    pub fn set_liked_model_file_path(&mut self, path: PathBuf) {
        self.liked_model_file_path = Some(path.clone());
        self.config.liked_model_file_path = Some(path);
    }

    pub fn is_liked_model_export_prompt(&self) -> bool {
        matches!(
            self.liked_model_path_prompt_action,
            Some(LikedPathAction::Export)
        )
    }

    pub fn apply_liked_model_query(&mut self) {
        self.liked_model_search_form = self.liked_model_search_form_draft.clone();
        self.liked_model_query = self.liked_model_search_form.query.clone();
        self.liked_model_query_draft = self.liked_model_query.clone();
        self.refresh_visible_liked_models_cache();
        self.mode = AppMode::Browsing;
        self.clamp_liked_model_selection();
        self.set_status(format!(
            "Liked filter applied: {}",
            if self.liked_model_search_form.query.is_empty() {
                "<all>".to_string()
            } else {
                self.liked_model_search_form.query.clone()
            }
        ));
    }

    pub fn export_liked_models_to_path(&mut self, path: PathBuf) {
        self.set_liked_model_file_path(path.clone());

        if let Err(err) = save_liked_models_to_file(&path, &self.liked_models) {
            self.set_error("Failed to export liked models", err.to_string());
            return;
        }

        self.set_status_detail(
            format!("Exported {} liked models", self.liked_models.len()),
            format!("Destination: {}", path.display()),
        );
    }

    pub fn import_liked_models_from_path(&mut self, path: PathBuf) {
        self.set_liked_model_file_path(path.clone());
        let mut imported = load_liked_models(Some(path.as_path()));
        if imported.is_empty() {
            self.set_warn("No liked models found in import file.");
            return;
        }

        let before = self.liked_models.len();
        self.liked_models.append(&mut imported);
        self.deduplicate_liked_models();
        self.rebuild_parsed_model_cache();
        self.refresh_visible_liked_models_cache();
        self.clamp_liked_model_selection();
        self.persist_liked_models();

        if self.liked_models.len() > before {
            self.set_status(format!(
                "Imported {} new liked model(s).",
                self.liked_models.len() - before
            ));
        } else {
            self.set_status("Import completed, no new liked models.");
        }
    }

    pub(super) fn deduplicate_liked_models(&mut self) {
        let mut seen = HashSet::new();
        self.liked_models.retain(|model| seen.insert(model.id));
    }

    pub fn persist_liked_models(&mut self) {
        if let Some(path) = &self.liked_model_file_path
            && let Err(err) = save_liked_models_to_file(path, &self.liked_models)
        {
            self.set_error("Failed to persist liked models", err.to_string());
        }
    }
}
