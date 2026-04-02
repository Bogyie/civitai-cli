use super::*;
use crate::tui::runtime::current_image_render_request;

impl App {
    pub fn open_search_template_modal(&mut self, kind: SearchTemplateKind) {
        self.show_search_template_modal = true;
        self.search_template_kind = kind;
        self.search_template_name_editing = false;
        self.search_template_name_draft.clear();
        self.clamp_search_template_selection();
        self.set_status(match kind {
            SearchTemplateKind::Model => "Model search templates opened",
            SearchTemplateKind::Image => "Image search templates opened",
        });
    }

    pub fn close_search_template_modal(&mut self) {
        self.show_search_template_modal = false;
        self.search_template_name_editing = false;
        self.search_template_name_draft.clear();
    }

    pub fn begin_search_template_save(&mut self) {
        self.search_template_name_editing = true;
        self.search_template_name_draft.clear();
        self.set_status("Type a template name and press Enter to save");
    }

    pub fn clamp_search_template_selection(&mut self) {
        let len = match self.search_template_kind {
            SearchTemplateKind::Model => self.model_search_templates.len(),
            SearchTemplateKind::Image => self.image_search_templates.len(),
        };
        if len == 0 {
            self.selected_search_template_index = 0;
        } else if self.selected_search_template_index >= len {
            self.selected_search_template_index = len - 1;
        }
    }

    pub fn select_next_search_template(&mut self) {
        let len = match self.search_template_kind {
            SearchTemplateKind::Model => self.model_search_templates.len(),
            SearchTemplateKind::Image => self.image_search_templates.len(),
        };
        if len > 0 && self.selected_search_template_index + 1 < len {
            self.selected_search_template_index += 1;
        }
    }

    pub fn select_previous_search_template(&mut self) {
        if self.selected_search_template_index > 0 {
            self.selected_search_template_index -= 1;
        }
    }

    pub fn search_template_names(&self) -> Vec<String> {
        match self.search_template_kind {
            SearchTemplateKind::Model => self
                .model_search_templates
                .iter()
                .map(|item| item.name.clone())
                .collect(),
            SearchTemplateKind::Image => self
                .image_search_templates
                .iter()
                .map(|item| item.name.clone())
                .collect(),
        }
    }

    pub fn save_current_search_template(&mut self) {
        let name = self.search_template_name_draft.trim().to_string();
        if name.is_empty() {
            self.set_warn("Template name cannot be empty.");
            return;
        }

        match self.search_template_kind {
            SearchTemplateKind::Model => {
                let state = self.search_form.persisted_state();
                if let Some(existing) = self
                    .model_search_templates
                    .iter_mut()
                    .find(|item| item.name.eq_ignore_ascii_case(&name))
                {
                    existing.name = name.clone();
                    existing.state = state;
                } else {
                    self.model_search_templates.push(ModelSearchTemplate {
                        name: name.clone(),
                        state,
                    });
                }
                self.model_search_templates
                    .sort_by_key(|item| item.name.to_lowercase());
            }
            SearchTemplateKind::Image => {
                let state = self.image_search_form.persisted_state();
                if let Some(existing) = self
                    .image_search_templates
                    .iter_mut()
                    .find(|item| item.name.eq_ignore_ascii_case(&name))
                {
                    existing.name = name.clone();
                    existing.state = state;
                } else {
                    self.image_search_templates.push(ImageSearchTemplate {
                        name: name.clone(),
                        state,
                    });
                }
                self.image_search_templates
                    .sort_by_key(|item| item.name.to_lowercase());
            }
        }

        self.persist_search_templates();
        self.search_template_name_editing = false;
        self.search_template_name_draft.clear();
        self.clamp_search_template_selection();
        self.set_status(format!("Saved template \"{}\"", name));
    }

    pub fn delete_selected_search_template(&mut self) {
        let removed = match self.search_template_kind {
            SearchTemplateKind::Model => {
                if self.model_search_templates.is_empty() {
                    None
                } else {
                    Some(
                        self.model_search_templates
                            .remove(self.selected_search_template_index)
                            .name,
                    )
                }
            }
            SearchTemplateKind::Image => {
                if self.image_search_templates.is_empty() {
                    None
                } else {
                    Some(
                        self.image_search_templates
                            .remove(self.selected_search_template_index)
                            .name,
                    )
                }
            }
        };

        if let Some(name) = removed {
            self.clamp_search_template_selection();
            self.persist_search_templates();
            self.set_status(format!("Deleted template \"{}\"", name));
        } else {
            self.set_warn("No template selected.");
        }
    }

    pub fn load_selected_search_template(&mut self) {
        match self.search_template_kind {
            SearchTemplateKind::Model => {
                let Some(template) = self
                    .model_search_templates
                    .get(self.selected_search_template_index)
                    .cloned()
                else {
                    self.set_warn("No model template selected.");
                    return;
                };
                self.search_form.apply_persisted_state(&template.state);
                self.close_search_template_modal();
                self.mode = AppMode::Browsing;
                self.execute_model_search();
                self.set_status(format!("Loaded model template \"{}\"", template.name));
            }
            SearchTemplateKind::Image => {
                let Some(template) = self
                    .image_search_templates
                    .get(self.selected_search_template_index)
                    .cloned()
                else {
                    self.set_warn("No image template selected.");
                    return;
                };
                self.image_search_form
                    .apply_persisted_state(&template.state);
                self.close_search_template_modal();
                self.mode = AppMode::Browsing;
                self.execute_image_search();
                self.set_status(format!("Loaded image template \"{}\"", template.name));
            }
        }
    }

    pub fn persist_search_templates(&mut self) {
        let Some(path) = self.search_template_file_path.clone() else {
            self.set_warn("No search template path configured.");
            return;
        };

        let store = SearchTemplateStore {
            model_templates: self.model_search_templates.clone(),
            image_templates: self.image_search_templates.clone(),
        };
        if let Err(err) = save_search_templates_to_file(path.as_path(), &store) {
            self.set_error("Failed to persist search templates", err);
        }
    }

    pub fn execute_model_search(&mut self) {
        let selected_model_id = self.selected_model_version().map(|(model_id, _)| model_id);
        let selected_version_id = self
            .selected_model_version()
            .map(|(_, version_id)| version_id);
        let search_options = self.search_form.build_options();
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(WorkerCommand::SearchModels(
                search_options,
                selected_model_id,
                selected_version_id,
                false,
                false,
                None,
            ));
            self.model_search_has_more = true;
            self.model_search_loading_more = false;
            self.status = format!("Searching for models: '{}'...", self.search_form.query);
        }
    }

    pub fn execute_image_search(&mut self) {
        self.images.clear();
        self.image_cache.clear();
        self.image_bytes_cache.clear();
        self.selected_index = 0;
        self.image_feed_loaded = false;
        self.image_feed_loading = false;
        self.image_feed_next_page = None;
        self.image_feed_has_more = true;
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(WorkerCommand::FetchImages(
                self.image_search_form.build_options(),
                None,
                current_image_render_request(),
            ));
            self.image_feed_loading = true;
            self.status = "Searching image feed...".into();
        }
    }
}
