use super::*;

impl App {
    pub fn visible_image_bookmarks(&self) -> &[ImageItem] {
        &self.visible_image_bookmarks_cache
    }

    pub(super) fn refresh_visible_image_bookmarks_cache(&mut self) {
        let query = self.image_bookmark_query.trim().to_ascii_lowercase();
        self.visible_image_bookmarks_cache = if query.is_empty() {
            self.image_bookmarks.clone()
        } else {
            self.image_bookmarks
                .iter()
                .filter(|image| {
                    let username = image
                        .user
                        .as_ref()
                        .and_then(|user| user.username.as_deref())
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    let base_model = image
                        .base_model
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    image.id.to_string().contains(&query)
                        || username.contains(&query)
                        || base_model.contains(&query)
                        || image
                            .metadata
                            .as_ref()
                            .map(|meta| meta.to_string().to_ascii_lowercase().contains(&query))
                            .unwrap_or(false)
                })
                .cloned()
                .collect()
        };
    }

    pub fn clamp_image_bookmark_selection(&mut self) {
        let visible = self.visible_image_bookmarks();
        if visible.is_empty() {
            self.selected_image_bookmark_index = 0;
            self.image_bookmark_list_state.select(None);
            return;
        }

        if self.selected_image_bookmark_index >= visible.len() {
            self.selected_image_bookmark_index = visible.len() - 1;
        }
        self.image_bookmark_list_state
            .select(Some(self.selected_image_bookmark_index));
    }

    pub fn selected_image_in_active_view(&self) -> Option<&ImageItem> {
        match self.active_tab {
            MainTab::Images => self.images.get(self.selected_index),
            MainTab::SavedImages => self
                .visible_image_bookmarks()
                .get(self.selected_image_bookmark_index),
            _ => None,
        }
    }

    pub fn active_image_items(&self) -> &[ImageItem] {
        match self.active_tab {
            MainTab::Images => &self.images,
            MainTab::SavedImages => self.visible_image_bookmarks(),
            _ => &[],
        }
    }

    pub fn active_image_selected_index(&self) -> usize {
        match self.active_tab {
            MainTab::Images => self.selected_index,
            MainTab::SavedImages => self.selected_image_bookmark_index,
            _ => 0,
        }
    }

    pub fn set_image_feed_results(&mut self, mut images: Vec<ImageItem>, next_page: Option<u32>) {
        let new_ids = images.iter().map(|item| item.id).collect::<HashSet<_>>();
        self.image_cache.retain(|id, _| new_ids.contains(id));
        self.image_bytes_cache.retain(|id, _| new_ids.contains(id));
        self.image_request_keys.retain(|id, _| new_ids.contains(id));
        self.selected_image_model_index
            .retain(|id, _| new_ids.contains(id));
        self.image_feed_next_page = next_page;
        self.image_feed_has_more = self.image_feed_next_page.is_some();
        self.image_feed_loaded = true;
        self.image_feed_loading = false;
        self.images = std::mem::take(&mut images);
        if self.images.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.images.len() {
            self.selected_index = self.images.len() - 1;
        }
    }

    pub fn append_image_feed_results(
        &mut self,
        mut images: Vec<ImageItem>,
        next_page: Option<u32>,
    ) {
        if !self.images.is_empty() && !images.is_empty() {
            let known_ids: HashSet<u64> = self.images.iter().map(|item| item.id).collect();
            images.retain(|item| !known_ids.contains(&item.id));
        }

        self.images.append(&mut images);
        self.image_feed_next_page = next_page;
        self.image_feed_has_more = self.image_feed_next_page.is_some();
        self.image_feed_loading = false;
        if !self.images.is_empty() && self.selected_index >= self.images.len() {
            self.selected_index = self.images.len() - 1;
        }
    }

    pub fn has_cached_image_request(&self, image_id: u64, request_key: &str) -> bool {
        self.image_cache.contains_key(&image_id)
            && self
                .image_request_keys
                .get(&image_id)
                .is_some_and(|key| key == request_key)
    }

    pub fn merge_image_tag_catalog_from_hits(&mut self, images: &[ImageItem]) {
        let mut existing = self
            .image_tag_catalog
            .iter()
            .map(|tag| tag.to_lowercase())
            .collect::<HashSet<_>>();
        let mut changed = false;

        for tag in images
            .iter()
            .flat_map(image_tags)
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
        {
            if existing.insert(tag.to_lowercase()) {
                self.image_tag_catalog.push(tag);
                changed = true;
            }
        }

        if changed {
            self.image_tag_catalog.sort_by_key(|tag| tag.to_lowercase());
            self.persist_image_tag_catalog();
        }
    }

    pub fn image_tag_suggestions(&self, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }

        let query = self.image_search_form.tag_query.as_str();
        let prefix = query
            .rsplit(',')
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .to_lowercase();
        let selected = query
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_lowercase())
            .collect::<HashSet<_>>();

        let mut suggestions = self
            .image_tag_catalog
            .iter()
            .filter(|tag| !selected.contains(&tag.to_lowercase()))
            .filter(|tag| prefix.is_empty() || tag.to_lowercase().starts_with(&prefix))
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();

        if suggestions.len() >= limit || prefix.is_empty() {
            return suggestions;
        }

        for tag in self
            .image_tag_catalog
            .iter()
            .filter(|tag| !selected.contains(&tag.to_lowercase()))
            .filter(|tag| tag.to_lowercase().contains(&prefix))
        {
            if suggestions.len() >= limit {
                break;
            }
            if !suggestions
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(tag))
            {
                suggestions.push(tag.clone());
            }
        }

        suggestions
    }

    pub fn accept_image_tag_suggestion(&mut self) -> bool {
        let Some(suggestion) = self.image_tag_suggestions(1).into_iter().next() else {
            return false;
        };

        let mut tags = self
            .image_search_form
            .tag_query
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect::<Vec<_>>();

        if self.image_search_form.tag_query.trim().is_empty()
            || self.image_search_form.tag_query.trim_end().ends_with(',')
        {
            tags.push(suggestion);
        } else if let Some(last) = tags.last_mut() {
            *last = suggestion;
        } else {
            tags.push(suggestion);
        }

        let mut seen = HashSet::new();
        tags.retain(|tag| seen.insert(tag.to_lowercase()));
        self.image_search_form.tag_query = tags.join(", ");
        true
    }

    pub fn request_download(&mut self) {
        if self.active_tab == MainTab::Images {
            if let Some(img) = self.images.get(self.selected_index)
                && let Some(tx) = &self.tx
            {
                let _ = tx.try_send(WorkerCommand::DownloadImage(img.clone()));
                self.set_status(format!("Downloading image {}...", img.id));
            }
        } else if (self.active_tab == MainTab::Models || self.active_tab == MainTab::SavedModels)
            && let Some(model) = self.selected_model_in_active_view().cloned()
        {
            self.request_download_for_model(&model);
        }
    }

    pub fn begin_image_model_detail_modal_loading(&mut self) {
        self.show_image_model_detail_modal = true;
        self.image_model_detail_model = None;
    }

    pub fn select_next_image_model(&mut self) {
        if !(self.active_tab == MainTab::Images || self.active_tab == MainTab::SavedImages) {
            return;
        }
        if let Some(image) = self.selected_image_in_active_view() {
            let items = image_used_models(image);
            if items.is_empty() {
                return;
            }
            let index = self.selected_image_model_index.entry(image.id).or_insert(0);
            if *index < items.len().saturating_sub(1) {
                *index += 1;
            }
        }
    }

    pub fn select_previous_image_model(&mut self) {
        if !(self.active_tab == MainTab::Images || self.active_tab == MainTab::SavedImages) {
            return;
        }
        if let Some(image) = self.selected_image_in_active_view() {
            let index = self.selected_image_model_index.entry(image.id).or_insert(0);
            if *index > 0 {
                *index -= 1;
            }
        }
    }

    pub fn selected_image_used_model(&self) -> Option<ParsedUsedModel> {
        let image = self.selected_image_in_active_view()?;
        let entries = image_used_model_entries(image);
        let index = self
            .selected_image_model_index
            .get(&image.id)
            .copied()
            .unwrap_or(0)
            .min(entries.len().saturating_sub(1));
        entries.get(index).cloned()
    }

    pub fn open_image_model_detail_modal(&mut self, model: Model, version_id: Option<u64>) {
        if let Some(version_id) = version_id
            && let Some(index) = self
                .parsed_model_versions(&model)
                .iter()
                .position(|version| version.id == version_id)
        {
            self.selected_version_index.insert(model.id, index);
        } else if let Some(version_id) = version_id
            && let Some(index) = model_versions(&model)
                .iter()
                .position(|version| version.id == version_id)
        {
            self.selected_version_index.insert(model.id, index);
        }
        self.image_model_detail_model = Some(model);
        self.rebuild_parsed_model_cache();
        self.show_image_model_detail_modal = true;
    }

    pub fn close_image_model_detail_modal(&mut self) {
        self.show_image_model_detail_modal = false;
        self.image_model_detail_model = None;
        self.rebuild_parsed_model_cache();
    }

    pub fn image_model_detail_selected_cover(&self) -> Option<SelectedVersionCover> {
        let model = self.image_model_detail_model.as_ref()?;
        let versions = self.parsed_model_versions(model);
        if versions.is_empty() {
            return None;
        }

        let selected_index = self
            .selected_version_index
            .get(&model.id)
            .copied()
            .unwrap_or(0)
            .min(versions.len().saturating_sub(1));
        let version = versions.get(selected_index)?;
        if self.model_version_image_failed.contains(&version.id) {
            return None;
        }

        let preview = version.images.first();
        Some((
            version.id,
            preview.map(|image| image.url.clone()),
            preview.and_then(|image| image.width.zip(image.height)),
        ))
    }

    pub fn image_model_detail_neighbor_cover_urls(&self, radius: usize) -> Vec<VersionCoverJob> {
        let Some(model) = self.image_model_detail_model.as_ref() else {
            return Vec::new();
        };
        let versions = self.parsed_model_versions(model);
        if versions.is_empty() {
            return Vec::new();
        }

        let selected_index = self
            .selected_version_index
            .get(&model.id)
            .copied()
            .unwrap_or(0)
            .min(versions.len().saturating_sub(1));

        let requestable_range = selected_index.saturating_sub(radius)
            ..=((selected_index + radius).min(versions.len().saturating_sub(1)));

        versions
            .iter()
            .enumerate()
            .filter(|(idx, version)| {
                *idx != selected_index
                    && requestable_range.contains(idx)
                    && !self.model_version_image_cache.contains_key(&version.id)
                    && !self.model_version_image_failed.contains(&version.id)
            })
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

    pub fn is_image_bookmarked(&self, image_id: u64) -> bool {
        self.image_bookmarks
            .iter()
            .any(|image| image.id == image_id)
    }

    pub fn toggle_bookmark_for_selected_image(&mut self, image: &ImageItem) {
        if self.is_image_bookmarked(image.id) {
            self.image_bookmarks.retain(|item| item.id != image.id);
            self.set_status(format!("Removed image bookmark: {}", image.id));
        } else {
            self.image_bookmarks.push(image.clone());
            self.set_status(format!("Added image bookmark: {}", image.id));
        }
        self.deduplicate_image_bookmarks();
        self.refresh_visible_image_bookmarks_cache();
        if self.active_tab == MainTab::SavedImages {
            self.clamp_image_bookmark_selection();
        }
        self.persist_image_bookmarks();
    }

    pub fn begin_image_bookmark_search(&mut self) {
        self.image_bookmark_query_draft = self.image_bookmark_query.clone();
        self.mode = AppMode::SearchSavedImages;
        self.set_status("Search image bookmarks. Enter apply, Esc cancel");
    }

    pub fn apply_image_bookmark_query(&mut self) {
        self.image_bookmark_query = self.image_bookmark_query_draft.clone();
        self.refresh_visible_image_bookmarks_cache();
        self.mode = AppMode::Browsing;
        self.clamp_image_bookmark_selection();
        self.set_status(format!(
            "Image bookmark query applied: {}",
            if self.image_bookmark_query.is_empty() {
                "<all>".to_string()
            } else {
                self.image_bookmark_query.clone()
            }
        ));
    }

    pub fn cancel_image_bookmark_search(&mut self) {
        self.image_bookmark_query_draft = self.image_bookmark_query.clone();
        self.mode = AppMode::Browsing;
        self.set_status("Image bookmark search cancelled.");
    }

    pub(super) fn deduplicate_image_bookmarks(&mut self) {
        let mut seen = HashSet::new();
        self.image_bookmarks.retain(|image| seen.insert(image.id));
    }

    pub fn persist_image_bookmarks(&mut self) {
        if let Some(path) = &self.image_bookmark_file_path
            && let Err(err) = save_image_bookmarks_to_file(path, &self.image_bookmarks)
        {
            self.set_error("Failed to persist image bookmarks", err.to_string());
        }
    }

    pub fn persist_image_tag_catalog(&mut self) {
        let Some(path) = self.config.image_tag_catalog_path() else {
            return;
        };

        if let Err(err) = save_image_tag_catalog_to_file(path.as_path(), &self.image_tag_catalog) {
            self.set_error("Failed to persist image tag catalog", err.to_string());
        }
    }
}
