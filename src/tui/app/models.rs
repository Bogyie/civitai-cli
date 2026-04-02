use super::*;

impl App {
    pub fn request_selected_model_detail_sidebar(&mut self) {
        if !self.show_model_details {
            return;
        }
        let Some(model) = self.selected_model_in_active_view().cloned() else {
            return;
        };
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(WorkerCommand::FetchModelDetail(
                model.id,
                None,
                model_name(&model),
            ));
        }
    }

    pub fn apply_sidebar_model_detail(&mut self, model: Model) {
        if let Some(existing) = self.models.iter_mut().find(|item| item.id == model.id) {
            *existing = model.clone();
        }
        if let Some(existing) = self.bookmarks.iter_mut().find(|item| item.id == model.id) {
            *existing = model;
            self.refresh_visible_bookmarks_cache();
        }
        self.rebuild_parsed_model_cache();
    }

    pub fn can_request_more_models(&self) -> bool {
        self.active_tab == MainTab::Models
            && !self.models.is_empty()
            && self.model_search_has_more
            && !self.model_search_loading_more
    }

    pub fn next_model_search_options_if_needed(
        &mut self,
    ) -> Option<(ModelSearchState, Option<u32>)> {
        if !self.can_request_more_models() {
            return None;
        }

        self.model_search_loading_more = true;
        Some((
            self.search_form.build_options(),
            self.model_search_next_page,
        ))
    }

    pub fn select_next(&mut self) {
        if self.active_tab == MainTab::Images {
            if !self.images.is_empty() && self.selected_index < self.images.len() - 1 {
                self.selected_index += 1;
            }
        } else if self.active_tab == MainTab::SavedImages {
            let visible = self.visible_image_bookmarks();
            if !visible.is_empty() && self.selected_image_bookmark_index < visible.len() - 1 {
                self.selected_image_bookmark_index += 1;
                self.image_bookmark_list_state
                    .select(Some(self.selected_image_bookmark_index));
            }
        } else if self.active_tab == MainTab::Models {
            if !self.models.is_empty() {
                let current = self.model_list_state.selected().unwrap_or(0);
                if current < self.models.len() - 1 {
                    self.model_list_state.select(Some(current + 1));
                    self.request_selected_model_detail_sidebar();
                }
            }
        } else if self.active_tab == MainTab::SavedModels {
            let visible = self.visible_bookmarks();
            if let Some(current) = self.bookmark_list_state.selected() {
                if current < visible.len().saturating_sub(1) {
                    self.bookmark_list_state.select(Some(current + 1));
                    self.request_selected_model_detail_sidebar();
                }
            } else if !visible.is_empty() {
                self.bookmark_list_state.select(Some(0));
                self.request_selected_model_detail_sidebar();
            }
        }
    }

    pub fn select_previous(&mut self) {
        if self.active_tab == MainTab::Images {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            }
        } else if self.active_tab == MainTab::SavedImages {
            if self.selected_image_bookmark_index > 0 {
                self.selected_image_bookmark_index -= 1;
                self.image_bookmark_list_state
                    .select(Some(self.selected_image_bookmark_index));
            }
        } else if self.active_tab == MainTab::Models {
            let current = self.model_list_state.selected().unwrap_or(0);
            if current > 0 {
                self.model_list_state.select(Some(current - 1));
                self.request_selected_model_detail_sidebar();
            }
        } else if self.active_tab == MainTab::SavedModels {
            let current = self.bookmark_list_state.selected().unwrap_or(0);
            if current > 0 {
                self.bookmark_list_state.select(Some(current - 1));
                self.request_selected_model_detail_sidebar();
            }
        }
    }

    pub fn move_list_selection_by(&mut self, delta: isize) {
        match self.active_tab {
            MainTab::Models => {
                if self.models.is_empty() {
                    self.model_list_state.select(None);
                    return;
                }
                let current = self.model_list_state.selected().unwrap_or(0) as isize;
                let max = self.models.len().saturating_sub(1) as isize;
                let next = (current + delta).clamp(0, max) as usize;
                self.model_list_state.select(Some(next));
                self.request_selected_model_detail_sidebar();
            }
            MainTab::SavedModels => {
                let visible = self.visible_bookmarks();
                if visible.is_empty() {
                    self.bookmark_list_state.select(None);
                    return;
                }
                let current = self.bookmark_list_state.selected().unwrap_or(0) as isize;
                let max = visible.len().saturating_sub(1) as isize;
                let next = (current + delta).clamp(0, max) as usize;
                self.bookmark_list_state.select(Some(next));
                self.request_selected_model_detail_sidebar();
            }
            _ => {}
        }
    }

    pub fn select_list_first(&mut self) {
        match self.active_tab {
            MainTab::Models => {
                if self.models.is_empty() {
                    self.model_list_state.select(None);
                } else {
                    self.model_list_state.select(Some(0));
                    self.request_selected_model_detail_sidebar();
                }
            }
            MainTab::SavedModels => {
                if self.visible_bookmarks().is_empty() {
                    self.bookmark_list_state.select(None);
                } else {
                    self.bookmark_list_state.select(Some(0));
                    self.request_selected_model_detail_sidebar();
                }
            }
            _ => {}
        }
    }

    pub fn select_list_last(&mut self) {
        match self.active_tab {
            MainTab::Models => {
                if self.models.is_empty() {
                    self.model_list_state.select(None);
                } else {
                    self.model_list_state
                        .select(Some(self.models.len().saturating_sub(1)));
                    self.request_selected_model_detail_sidebar();
                }
            }
            MainTab::SavedModels => {
                let visible = self.visible_bookmarks();
                if visible.is_empty() {
                    self.bookmark_list_state.select(None);
                } else {
                    self.bookmark_list_state
                        .select(Some(visible.len().saturating_sub(1)));
                    self.request_selected_model_detail_sidebar();
                }
            }
            _ => {}
        }
    }

    pub fn append_models_results(
        &mut self,
        mut new_models: Vec<Model>,
        has_more: bool,
        next_page: Option<u32>,
    ) {
        if new_models.is_empty() {
            self.model_search_has_more = has_more;
            self.model_search_loading_more = false;
            self.model_search_next_page = next_page;
            return;
        }

        new_models.sort_by_key(|model| !has_displayable_model_version(model));
        let mut seen_ids = self
            .models
            .iter()
            .map(|model| model.id)
            .collect::<HashSet<_>>();
        for model in new_models {
            if seen_ids.insert(model.id) {
                self.models.push(model);
            }
        }
        self.rebuild_parsed_model_cache();
        self.model_search_has_more = has_more;
        self.model_search_loading_more = false;
        self.model_search_next_page = next_page;
    }

    pub fn set_models_results(
        &mut self,
        mut models: Vec<Model>,
        has_more: bool,
        next_page: Option<u32>,
    ) {
        models.sort_by_key(|model| !has_displayable_model_version(model));
        let known_version_ids = models
            .iter()
            .flat_map(model_versions)
            .map(|version| version.id)
            .collect::<HashSet<_>>();
        self.model_version_image_cache
            .retain(|version_id, _| known_version_ids.contains(version_id));
        self.model_version_image_bytes_cache
            .retain(|version_id, _| known_version_ids.contains(version_id));
        self.model_version_image_request_keys
            .retain(|version_id, _| known_version_ids.contains(version_id));
        self.model_version_image_failed
            .retain(|version_id| known_version_ids.contains(version_id));
        self.models = models;
        self.rebuild_parsed_model_cache();
        self.model_search_has_more = has_more;
        self.model_search_loading_more = false;
        self.model_search_next_page = next_page;
        self.model_list_state.select(Some(0));
    }

    pub fn select_next_version(&mut self) {
        if (self.active_tab == MainTab::Models || self.active_tab == MainTab::SavedModels)
            && let Some(model) = self.selected_model_in_active_view().cloned()
        {
            self.select_next_version_for_model(&model);
        }
    }

    pub fn select_previous_version(&mut self) {
        if (self.active_tab == MainTab::Models || self.active_tab == MainTab::SavedModels)
            && let Some(model) = self.selected_model_in_active_view().cloned()
        {
            self.select_previous_version_for_model(&model);
        }
    }

    pub fn select_next_file(&mut self) {
        if (self.active_tab == MainTab::Models || self.active_tab == MainTab::SavedModels)
            && let Some(model) = self.selected_model_in_active_view().cloned()
        {
            self.select_next_file_for_model(&model);
        }
    }

    pub fn select_previous_file(&mut self) {
        if (self.active_tab == MainTab::Models || self.active_tab == MainTab::SavedModels)
            && let Some(model) = self.selected_model_in_active_view().cloned()
        {
            self.select_previous_file_for_model(&model);
        }
    }

    pub fn select_next_version_for_model(&mut self, model: &Model) {
        let model_id = model.id;
        let version_len = self.parsed_model_versions(model).len();
        let next_index = self
            .selected_version_index
            .get(&model_id)
            .copied()
            .unwrap_or(0);
        if next_index < version_len.saturating_sub(1) {
            let next_index = next_index + 1;
            self.selected_version_index.insert(model_id, next_index);
            if let Some(version) = self.selected_parsed_version(model, next_index) {
                self.selected_file_index.entry(version.id).or_insert(0);
            }
        }
    }

    pub fn select_previous_version_for_model(&mut self, model: &Model) {
        let model_id = model.id;
        let current_index = self
            .selected_version_index
            .get(&model_id)
            .copied()
            .unwrap_or(0);
        if current_index > 0 {
            let next_index = current_index - 1;
            self.selected_version_index.insert(model_id, next_index);
            if let Some(version) = self.selected_parsed_version(model, next_index) {
                self.selected_file_index.entry(version.id).or_insert(0);
            }
        }
    }

    pub fn select_next_file_for_model(&mut self, model: &Model) {
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        if let Some(version) = self.selected_parsed_version(model, version_index) {
            let file_len = version.files.len();
            if file_len > 0 {
                let file_idx = self.selected_file_index.entry(version.id).or_insert(0);
                if *file_idx < file_len.saturating_sub(1) {
                    *file_idx += 1;
                }
            }
        }
    }

    pub fn select_previous_file_for_model(&mut self, model: &Model) {
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        if let Some(version) = self.selected_parsed_version(model, version_index) {
            let file_idx = self.selected_file_index.entry(version.id).or_insert(0);
            if *file_idx > 0 {
                *file_idx -= 1;
            }
        }
    }

    pub fn request_download_for_model(&mut self, model: &Model) {
        let v_idx = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        if let Some(version) = self.selected_parsed_version(model, v_idx) {
            let file_idx = *self.selected_file_index.get(&version.id).unwrap_or(&0);
            if let Some(tx) = &self.tx {
                let _ = tx.try_send(WorkerCommand::DownloadModel(
                    Box::new(model.clone()),
                    version.id,
                    file_idx,
                ));
                self.status = format!(
                    "Initiated download for {} (v: {}, file: {})",
                    model_name(model),
                    version.name,
                    file_idx + 1
                );
            }
        }
    }

    pub fn has_cached_model_cover_request(&self, version_id: u64, request_key: &str) -> bool {
        self.model_version_image_cache.contains_key(&version_id)
            && self
                .model_version_image_request_keys
                .get(&version_id)
                .is_some_and(|key| key == request_key)
    }

    pub fn selected_model_version(&self) -> Option<(u64, u64)> {
        let model = self.selected_model_in_active_view()?;
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        if let Some(version) = self.selected_parsed_version(model, version_index) {
            Some((model.id, version.id))
        } else {
            model
                .primary_model_version_id()
                .map(|version_id| (model.id, version_id))
        }
    }

    pub fn selected_model_version_with_cover_url(&self) -> Option<SelectedModelCover> {
        let model = self.selected_model_in_active_view()?;
        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        let version = self.selected_parsed_version(model, version_index)?;
        if self.model_version_image_failed.contains(&version.id) {
            return None;
        }
        let preview = version.images.first();
        Some((
            model.id,
            version.id,
            preview.map(|image| image.url.clone()),
            preview.and_then(|image| image.width.zip(image.height)),
        ))
    }

    pub fn selected_model_neighbor_cover_urls(&self, radius: usize) -> Vec<VersionCoverJob> {
        let Some(model) = self.selected_model_in_active_view() else {
            return Vec::new();
        };
        let versions = self.parsed_model_versions(model);
        if versions.is_empty() {
            return Vec::new();
        }

        let version_index = *self.selected_version_index.get(&model.id).unwrap_or(&0);
        let center = version_index.min(versions.len().saturating_sub(1));
        let start = center.saturating_sub(radius);
        let end = (center + radius).min(versions.len().saturating_sub(1));

        versions
            .iter()
            .enumerate()
            .filter(|(_, version)| {
                !self.model_version_image_cache.contains_key(&version.id)
                    && !self.model_version_image_failed.contains(&version.id)
            })
            .filter(|(idx, _)| *idx != center && *idx >= start && *idx <= end)
            .map(|(_, version)| {
                let preview = version.images.first();
                (
                    version.id,
                    preview.map(|image| image.url.clone()),
                    preview.and_then(|image| image.width.zip(image.height)),
                )
            })
            .collect()
    }

    pub fn parsed_model_metrics(&self, model: &Model) -> ParsedModelMetrics {
        self.parsed_model_cache
            .get(&model.id)
            .map(|entry| entry.metrics.clone())
            .unwrap_or_else(|| model_metrics(model))
    }

    pub fn parsed_model_versions(&self, model: &Model) -> &[ParsedModelVersion] {
        self.parsed_model_cache
            .get(&model.id)
            .map(|entry| entry.versions.as_slice())
            .unwrap_or(&[])
    }

    pub fn parsed_default_base_model(&self, model: &Model) -> Option<&str> {
        self.parsed_model_cache
            .get(&model.id)
            .and_then(|entry| entry.default_base_model.as_deref())
    }

    pub fn selected_parsed_version(
        &self,
        model: &Model,
        index: usize,
    ) -> Option<&ParsedModelVersion> {
        let versions = self.parsed_model_versions(model);
        if versions.is_empty() {
            return None;
        }
        versions.get(index.min(versions.len().saturating_sub(1)))
    }

    pub(super) fn rebuild_parsed_model_cache(&mut self) {
        self.parsed_model_cache.clear();

        for model in self
            .models
            .iter()
            .chain(self.bookmarks.iter())
            .chain(self.image_model_detail_model.iter())
        {
            self.parsed_model_cache
                .entry(model.id)
                .or_insert_with(|| ParsedModelCacheEntry {
                    metrics: model_metrics(model),
                    versions: model_versions(model),
                    default_base_model: default_base_model(model),
                });
        }
    }
}
