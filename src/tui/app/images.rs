use super::*;

const IMAGE_FEED_PAGE_SIZE: usize = 50;

impl App {
    fn image_tag_modal_selectable_indices(&self) -> Vec<usize> {
        let tags = self.selected_image_tag_list();
        match self.image_tag_modal_column {
            TagViewerColumn::Current => (0..tags.len()).collect(),
            TagViewerColumn::Include => tags
                .iter()
                .enumerate()
                .filter(|(_, tag)| {
                    self.image_tag_modal_include_pending
                        .contains(&tag.to_lowercase())
                })
                .map(|(idx, _)| idx)
                .collect(),
            TagViewerColumn::Exclude => tags
                .iter()
                .enumerate()
                .filter(|(_, tag)| {
                    self.image_tag_modal_exclude_pending
                        .contains(&tag.to_lowercase())
                })
                .map(|(idx, _)| idx)
                .collect(),
        }
    }

    fn clamp_image_tag_modal_selection(&mut self) {
        let selectable = self.image_tag_modal_selectable_indices();
        if selectable.is_empty() {
            self.image_tag_modal_selected_index = 0;
            return;
        }

        if selectable.contains(&self.image_tag_modal_selected_index) {
            return;
        }

        if let Some(next_idx) = selectable
            .iter()
            .copied()
            .find(|idx| *idx >= self.image_tag_modal_selected_index)
            .or_else(|| selectable.last().copied())
        {
            self.image_tag_modal_selected_index = next_idx;
        }
    }

    fn parse_tag_filter_values(value: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        value
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .filter_map(|tag| {
                let normalized = tag.to_lowercase();
                seen.insert(normalized).then(|| tag.to_string())
            })
            .collect()
    }

    fn join_tag_filter_values(values: &[String]) -> String {
        values.join(", ")
    }

    fn selected_image_tag_list(&self) -> Vec<String> {
        self.selected_image_in_active_view()
            .map(image_tags)
            .unwrap_or_default()
    }

    fn image_tag_suggestions_for(&self, query: &str, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }

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

    pub fn visible_liked_images(&self) -> &[ImageItem] {
        &self.visible_liked_images_cache
    }

    pub(super) fn refresh_visible_liked_images_cache(&mut self) {
        let query = self.liked_image_query.trim().to_ascii_lowercase();
        self.visible_liked_images_cache = if query.is_empty() {
            self.liked_images.clone()
        } else {
            self.liked_images
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

    pub fn clamp_liked_image_selection(&mut self) {
        let visible = self.visible_liked_images();
        if visible.is_empty() {
            self.selected_liked_image_index = 0;
            self.liked_image_list_state.select(None);
            return;
        }

        if self.selected_liked_image_index >= visible.len() {
            self.selected_liked_image_index = visible.len() - 1;
        }
        self.liked_image_list_state
            .select(Some(self.selected_liked_image_index));
    }

    pub fn selected_image_in_active_view(&self) -> Option<&ImageItem> {
        match self.active_tab {
            MainTab::Images => self.images.get(self.selected_index),
            MainTab::LikedImages => self
                .visible_liked_images()
                .get(self.selected_liked_image_index),
            _ => None,
        }
    }

    pub fn active_image_items(&self) -> &[ImageItem] {
        match self.active_tab {
            MainTab::Images => &self.images,
            MainTab::LikedImages => self.visible_liked_images(),
            _ => &[],
        }
    }

    pub fn active_image_selected_index(&self) -> usize {
        match self.active_tab {
            MainTab::Images => self.selected_index,
            MainTab::LikedImages => self.selected_liked_image_index,
            _ => 0,
        }
    }

    pub fn open_image_jump_modal(&mut self) {
        self.show_image_jump_modal = true;
        self.image_jump_input.clear();
        self.set_status("Jump to image index. Type a number, Enter apply, Esc cancel.");
    }

    pub fn close_image_jump_modal(&mut self) {
        self.show_image_jump_modal = false;
        self.image_jump_input.clear();
    }

    pub fn append_image_jump_digit(&mut self, c: char) {
        if c.is_ascii_digit() {
            self.image_jump_input.push(c);
        }
    }

    pub fn backspace_image_jump_input(&mut self) {
        self.image_jump_input.pop();
    }

    fn normalized_jump_target(&self) -> Option<usize> {
        let raw = self.image_jump_input.trim().parse::<usize>().ok()?;
        Some(raw.saturating_sub(1))
    }

    pub fn apply_image_jump(&mut self) -> bool {
        let Some(mut target) = self.normalized_jump_target() else {
            self.set_warn("Enter an image index to jump to.");
            return false;
        };

        match self.active_tab {
            MainTab::LikedImages => {
                let visible_len = self.visible_liked_images().len();
                if visible_len == 0 {
                    self.set_warn("No liked images available to jump to.");
                    return false;
                }

                target = target.min(visible_len.saturating_sub(1));
                self.selected_liked_image_index = target;
                self.liked_image_list_state.select(Some(target));
                self.set_status(format!("Jumped to liked image {}", target + 1));
                true
            }
            MainTab::Images => {
                if let Some(total_hits) = self.image_feed_total_hits
                    && total_hits > 0
                {
                    target = target.min(total_hits.saturating_sub(1) as usize);
                }

                self.pending_image_jump_target = Some(target);
                let completed = self.resolve_pending_image_jump();
                if !completed {
                    let page = target / IMAGE_FEED_PAGE_SIZE + 1;
                    self.set_status(format!(
                        "Jumping to image {}... loading page {}",
                        target + 1,
                        page
                    ));
                }
                completed
            }
            _ => false,
        }
    }

    pub fn resolve_pending_image_jump(&mut self) -> bool {
        let Some(mut target) = self.pending_image_jump_target else {
            return false;
        };

        if let Some(total_hits) = self.image_feed_total_hits
            && total_hits > 0
        {
            target = target.min(total_hits.saturating_sub(1) as usize);
            self.pending_image_jump_target = Some(target);
        }

        if target < self.images.len() {
            self.selected_index = target;
            self.pending_image_jump_target = None;
            self.set_status(format!("Jumped to image {}", target + 1));
            return true;
        }

        if !self.image_feed_has_more {
            self.pending_image_jump_target = None;
            if self.images.is_empty() {
                self.selected_index = 0;
                self.set_warn("No images available to jump to.");
                return false;
            }

            self.selected_index = self.images.len().saturating_sub(1);
            self.set_status(format!(
                "Jumped to last available image ({})",
                self.selected_index + 1
            ));
            return true;
        }

        false
    }

    pub fn pending_image_jump_request(&self) -> Option<u32> {
        self.pending_image_jump_target?;
        self.image_feed_next_page
    }

    pub fn pending_image_jump_status(&self) -> Option<String> {
        let target = self.pending_image_jump_target?;
        let page = target / IMAGE_FEED_PAGE_SIZE + 1;
        Some(format!(
            "Jumping to image {}... loading page {}",
            target + 1,
            page
        ))
    }

    pub fn reset_image_feed_for_search(&mut self) {
        self.images.clear();
        self.image_cache.clear();
        self.image_bytes_cache.clear();
        self.selected_index = 0;
        self.image_feed_loaded = false;
        self.image_feed_loading = false;
        self.image_feed_next_page = None;
        self.image_feed_total_hits = None;
        self.image_feed_has_more = true;
        self.pending_image_jump_target = None;
        self.close_image_jump_modal();
    }

    pub fn set_image_feed_results(
        &mut self,
        mut images: Vec<ImageItem>,
        next_page: Option<u32>,
        total_hits: Option<u64>,
    ) {
        let new_ids = images.iter().map(|item| item.id).collect::<HashSet<_>>();
        self.image_cache.retain(|id, _| new_ids.contains(id));
        self.image_bytes_cache.retain(|id, _| new_ids.contains(id));
        self.image_request_keys.retain(|id, _| new_ids.contains(id));
        self.selected_image_model_index
            .retain(|id, _| new_ids.contains(id));
        self.image_feed_next_page = next_page;
        self.image_feed_total_hits = total_hits;
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
        total_hits: Option<u64>,
    ) {
        if !self.images.is_empty() && !images.is_empty() {
            let known_ids: HashSet<u64> = self.images.iter().map(|item| item.id).collect();
            images.retain(|item| !known_ids.contains(&item.id));
        }

        self.images.append(&mut images);
        self.image_feed_next_page = next_page;
        if total_hits.is_some() {
            self.image_feed_total_hits = total_hits;
        }
        self.image_feed_has_more = self.image_feed_next_page.is_some();
        self.image_feed_loading = false;
        if !self.images.is_empty() && self.selected_index >= self.images.len() {
            self.selected_index = self.images.len() - 1;
        }
    }

    pub fn update_image_detail(&mut self, image: &ImageItem) {
        if let Some(existing) = self.images.iter_mut().find(|item| item.id == image.id) {
            *existing = image.clone();
        }

        if let Some(existing) = self
            .liked_images
            .iter_mut()
            .find(|item| item.id == image.id)
        {
            *existing = image.clone();
            self.refresh_visible_liked_images_cache();
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
        self.image_tag_suggestions_for(&self.image_search_form.tag_query, limit)
    }

    pub fn accept_image_tag_suggestion(&mut self) -> bool {
        let suggestions = self.image_tag_suggestions(1);
        Self::accept_tag_suggestion(&mut self.image_search_form.tag_query, suggestions)
    }

    pub fn image_excluded_tag_suggestions(&self, limit: usize) -> Vec<String> {
        self.image_tag_suggestions_for(&self.image_search_form.excluded_tag_query, limit)
    }

    pub fn accept_image_excluded_tag_suggestion(&mut self) -> bool {
        let suggestions = self.image_excluded_tag_suggestions(1);
        Self::accept_tag_suggestion(&mut self.image_search_form.excluded_tag_query, suggestions)
    }

    fn accept_tag_suggestion(target: &mut String, suggestions: Vec<String>) -> bool {
        let Some(suggestion) = suggestions.into_iter().next() else {
            return false;
        };

        let mut tags = target
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect::<Vec<_>>();

        if target.trim().is_empty() || target.trim_end().ends_with(',') {
            tags.push(suggestion);
        } else if let Some(last) = tags.last_mut() {
            *last = suggestion;
        } else {
            tags.push(suggestion);
        }

        let mut seen = HashSet::new();
        tags.retain(|tag| seen.insert(tag.to_lowercase()));
        *target = tags.join(", ");
        true
    }

    pub fn request_download(&mut self) {
        if matches!(self.active_tab, MainTab::Images | MainTab::LikedImages) {
            if let Some(img) = self.selected_image_in_active_view().cloned()
                && let Some(tx) = &self.tx
            {
                let _ = tx.try_send(WorkerCommand::DownloadImage(img.clone()));
                self.set_status(format!("Downloading image {}...", img.id));
            }
        } else if (self.active_tab == MainTab::Models || self.active_tab == MainTab::LikedModels)
            && let Some(model) = self.selected_model_in_active_view().cloned()
        {
            self.request_download_for_model(&model);
        }
    }

    pub fn begin_image_model_detail_modal_loading(&mut self) {
        self.show_image_model_detail_modal = true;
        self.image_model_detail_model = None;
    }

    pub fn open_image_tags_modal(&mut self) {
        let tags = self.selected_image_tag_list();
        self.image_tag_modal_include_pending.clear();
        self.image_tag_modal_exclude_pending.clear();

        let selected_include = Self::parse_tag_filter_values(&self.image_search_form.tag_query)
            .into_iter()
            .map(|tag| tag.to_lowercase())
            .collect::<HashSet<_>>();
        let selected_exclude =
            Self::parse_tag_filter_values(&self.image_search_form.excluded_tag_query)
                .into_iter()
                .map(|tag| tag.to_lowercase())
                .collect::<HashSet<_>>();

        for tag in tags {
            let key = tag.to_lowercase();
            if selected_include.contains(&key) {
                self.image_tag_modal_include_pending.insert(key.clone());
            }
            if selected_exclude.contains(&key) {
                self.image_tag_modal_exclude_pending.insert(key);
            }
        }

        self.show_image_tags_modal = true;
        self.image_tags_scroll = 0;
        self.image_tag_modal_column = TagViewerColumn::Current;
        self.image_tag_modal_selected_index = 0;
        self.clamp_image_tag_modal_selection();
        self.set_status("Tag viewer opened");
    }

    pub fn close_image_tags_modal(&mut self) {
        self.show_image_tags_modal = false;
        self.image_tags_scroll = 0;
        self.image_tag_modal_column = TagViewerColumn::Current;
        self.image_tag_modal_selected_index = 0;
        self.image_tag_modal_include_pending.clear();
        self.image_tag_modal_exclude_pending.clear();
    }

    pub fn select_next_image_tag_modal_row(&mut self) {
        let selectable = self.image_tag_modal_selectable_indices();
        let Some(position) = selectable
            .iter()
            .position(|idx| *idx == self.image_tag_modal_selected_index)
        else {
            self.clamp_image_tag_modal_selection();
            return;
        };
        if position + 1 < selectable.len() {
            self.image_tag_modal_selected_index = selectable[position + 1];
        }
    }

    pub fn select_previous_image_tag_modal_row(&mut self) {
        let selectable = self.image_tag_modal_selectable_indices();
        let Some(position) = selectable
            .iter()
            .position(|idx| *idx == self.image_tag_modal_selected_index)
        else {
            self.clamp_image_tag_modal_selection();
            return;
        };
        if position > 0 {
            self.image_tag_modal_selected_index = selectable[position - 1];
        }
    }

    pub fn cycle_image_tag_modal_column_forward(&mut self) {
        self.image_tag_modal_column = match self.image_tag_modal_column {
            TagViewerColumn::Include => TagViewerColumn::Current,
            TagViewerColumn::Current => TagViewerColumn::Exclude,
            TagViewerColumn::Exclude => TagViewerColumn::Include,
        };
        self.clamp_image_tag_modal_selection();
    }

    pub fn cycle_image_tag_modal_column_backward(&mut self) {
        self.image_tag_modal_column = match self.image_tag_modal_column {
            TagViewerColumn::Include => TagViewerColumn::Exclude,
            TagViewerColumn::Current => TagViewerColumn::Include,
            TagViewerColumn::Exclude => TagViewerColumn::Current,
        };
        self.clamp_image_tag_modal_selection();
    }

    pub fn toggle_image_tag_modal_left(&mut self) {
        let tags = self.selected_image_tag_list();
        let Some(tag) = tags.get(self.image_tag_modal_selected_index) else {
            return;
        };
        let key = tag.to_lowercase();

        match self.image_tag_modal_column {
            TagViewerColumn::Current => {
                if self.image_tag_modal_exclude_pending.contains(&key) {
                    self.image_tag_modal_exclude_pending.remove(&key);
                } else {
                    self.image_tag_modal_include_pending.insert(key.clone());
                    self.image_tag_modal_exclude_pending.remove(&key);
                }
            }
            TagViewerColumn::Exclude => {
                self.image_tag_modal_exclude_pending.remove(&key);
                self.clamp_image_tag_modal_selection();
            }
            TagViewerColumn::Include => {}
        }
    }

    pub fn toggle_image_tag_modal_right(&mut self) {
        let tags = self.selected_image_tag_list();
        let Some(tag) = tags.get(self.image_tag_modal_selected_index) else {
            return;
        };
        let key = tag.to_lowercase();

        match self.image_tag_modal_column {
            TagViewerColumn::Current => {
                if self.image_tag_modal_include_pending.contains(&key) {
                    self.image_tag_modal_include_pending.remove(&key);
                } else {
                    self.image_tag_modal_exclude_pending.insert(key.clone());
                    self.image_tag_modal_include_pending.remove(&key);
                }
            }
            TagViewerColumn::Include => {
                self.image_tag_modal_include_pending.remove(&key);
                self.clamp_image_tag_modal_selection();
            }
            TagViewerColumn::Exclude => {}
        }
    }

    pub fn apply_image_tag_modal_filters(&mut self) -> bool {
        let tags = self.selected_image_tag_list();
        if tags.is_empty() {
            return false;
        }

        let mut include_values = Self::parse_tag_filter_values(&self.image_search_form.tag_query);
        let mut exclude_values =
            Self::parse_tag_filter_values(&self.image_search_form.excluded_tag_query);

        let mut include_keys = include_values
            .iter()
            .map(|tag| tag.to_lowercase())
            .collect::<HashSet<_>>();
        let mut exclude_keys = exclude_values
            .iter()
            .map(|tag| tag.to_lowercase())
            .collect::<HashSet<_>>();

        for tag in &tags {
            let key = tag.to_lowercase();
            let should_include = self.image_tag_modal_include_pending.contains(&key);
            let should_exclude = self.image_tag_modal_exclude_pending.contains(&key);

            if should_include {
                if include_keys.insert(key.clone()) {
                    include_values.push(tag.clone());
                }
                if exclude_keys.remove(&key) {
                    exclude_values.retain(|value| !value.eq_ignore_ascii_case(tag));
                }
            } else if should_exclude {
                if exclude_keys.insert(key.clone()) {
                    exclude_values.push(tag.clone());
                }
                if include_keys.remove(&key) {
                    include_values.retain(|value| !value.eq_ignore_ascii_case(tag));
                }
            } else {
                if include_keys.remove(&key) {
                    include_values.retain(|value| !value.eq_ignore_ascii_case(tag));
                }
                if exclude_keys.remove(&key) {
                    exclude_values.retain(|value| !value.eq_ignore_ascii_case(tag));
                }
            }
        }

        let next_include = Self::join_tag_filter_values(&include_values);
        let next_exclude = Self::join_tag_filter_values(&exclude_values);
        let changed = self.image_search_form.tag_query != next_include
            || self.image_search_form.excluded_tag_query != next_exclude;

        self.image_search_form.tag_query = next_include;
        self.image_search_form.excluded_tag_query = next_exclude;
        changed
    }

    pub fn select_next_image_model(&mut self) {
        if !(self.active_tab == MainTab::Images || self.active_tab == MainTab::LikedImages) {
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
        if !(self.active_tab == MainTab::Images || self.active_tab == MainTab::LikedImages) {
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

    pub fn is_image_liked(&self, image_id: u64) -> bool {
        self.liked_images.iter().any(|image| image.id == image_id)
    }

    pub fn toggle_like_for_selected_image(&mut self, image: &ImageItem) {
        if self.is_image_liked(image.id) {
            self.liked_images.retain(|item| item.id != image.id);
            self.set_status(format!("Removed image liked: {}", image.id));
        } else {
            self.liked_images.push(image.clone());
            self.set_status(format!("Added image liked: {}", image.id));
        }
        self.deduplicate_liked_images();
        self.refresh_visible_liked_images_cache();
        if self.active_tab == MainTab::LikedImages {
            self.clamp_liked_image_selection();
        }
        self.persist_liked_images();
    }

    pub fn begin_liked_image_search(&mut self) {
        self.liked_image_query_draft = self.liked_image_query.clone();
        self.mode = AppMode::SearchLikedImages;
        self.set_status("Search liked images. Enter/Esc apply and close");
    }

    pub fn apply_liked_image_query(&mut self) {
        self.liked_image_query = self.liked_image_query_draft.clone();
        self.refresh_visible_liked_images_cache();
        self.mode = AppMode::Browsing;
        self.clamp_liked_image_selection();
        self.set_status(format!(
            "Liked image filter applied: {}",
            if self.liked_image_query.is_empty() {
                "<all>".to_string()
            } else {
                self.liked_image_query.clone()
            }
        ));
    }

    pub(super) fn deduplicate_liked_images(&mut self) {
        let mut seen = HashSet::new();
        self.liked_images.retain(|image| seen.insert(image.id));
    }

    pub fn persist_liked_images(&mut self) {
        if let Some(path) = &self.liked_image_file_path
            && let Err(err) = save_liked_images_to_file(path, &self.liked_images)
        {
            self.set_error("Failed to persist liked images", err.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn isolated_config() -> AppConfig {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("civitai-cli-image-tests-{unique}"));
        AppConfig {
            liked_model_file_path: Some(root.join("liked_models.json")),
            liked_image_file_path: Some(root.join("liked_images.json")),
            download_history_file_path: Some(root.join("download_history.json")),
            interrupted_download_file_path: Some(root.join("interrupted_downloads.json")),
            ..AppConfig::default()
        }
    }

    fn sample_image_with_tags(tags: &[&str]) -> ImageItem {
        ImageItem {
            id: 42,
            url: None,
            width: None,
            height: None,
            r#type: None,
            created_at: None,
            prompt: None,
            base_model: None,
            hash: None,
            hide_meta: None,
            user: None,
            stats: None,
            tag_names: tags.iter().map(|tag| Some((*tag).to_string())).collect(),
            model_version_ids: Vec::new(),
            nsfw_level: None,
            browsing_level: None,
            sort_at: None,
            sort_at_unix: None,
            metadata: None,
            generation_process: None,
            ai_nsfw_level: None,
            combined_nsfw_level: None,
            thumbnail_url: None,
        }
    }

    #[test]
    fn image_tag_modal_syncs_selected_image_tags_back_to_filters() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Images;
        app.images = vec![sample_image_with_tags(&["foo", "bar", "baz"])];
        app.image_search_form.tag_query = "foo, keep".into();
        app.image_search_form.excluded_tag_query = "bar, keep-out".into();

        app.open_image_tags_modal();
        app.select_next_image_tag_modal_row();
        app.toggle_image_tag_modal_right();
        app.select_previous_image_tag_modal_row();
        app.toggle_image_tag_modal_right();
        app.toggle_image_tag_modal_right();

        let changed = app.apply_image_tag_modal_filters();

        assert!(changed);
        assert_eq!(app.image_search_form.tag_query, "keep");
        assert_eq!(
            app.image_search_form.excluded_tag_query,
            "bar, keep-out, foo"
        );
    }

    #[test]
    fn image_tag_modal_adds_new_include_without_touching_other_filters() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Images;
        app.images = vec![sample_image_with_tags(&["foo", "bar"])];
        app.image_search_form.tag_query = "keep".into();
        app.image_search_form.excluded_tag_query = "keep-out".into();

        app.open_image_tags_modal();
        app.toggle_image_tag_modal_left();

        let changed = app.apply_image_tag_modal_filters();

        assert!(changed);
        assert_eq!(app.image_search_form.tag_query, "keep, foo");
        assert_eq!(app.image_search_form.excluded_tag_query, "keep-out");
    }

    #[test]
    fn image_tag_modal_keeps_focus_in_current_column_when_adding() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Images;
        app.images = vec![sample_image_with_tags(&["foo", "bar", "baz"])];

        app.open_image_tags_modal();
        app.toggle_image_tag_modal_left();

        assert_eq!(app.image_tag_modal_column, TagViewerColumn::Current);
        assert_eq!(app.image_tag_modal_selected_index, 0);
        assert!(
            app.image_tag_modal_include_pending.contains("foo"),
            "expected foo to be queued for include"
        );
    }

    #[test]
    fn image_tag_modal_moves_only_across_tags_present_in_focused_column() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Images;
        app.images = vec![sample_image_with_tags(&["foo", "bar", "baz"])];

        app.open_image_tags_modal();
        app.toggle_image_tag_modal_left();
        app.select_next_image_tag_modal_row();
        app.toggle_image_tag_modal_left();

        app.cycle_image_tag_modal_column_backward();
        assert_eq!(app.image_tag_modal_column, TagViewerColumn::Include);
        assert_eq!(app.image_tag_modal_selected_index, 1);

        app.select_previous_image_tag_modal_row();
        assert_eq!(app.image_tag_modal_selected_index, 0);

        app.select_next_image_tag_modal_row();
        assert_eq!(app.image_tag_modal_selected_index, 1);
    }

    #[test]
    fn image_tag_modal_opposite_direction_cancels_before_switching_sides() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Images;
        app.images = vec![sample_image_with_tags(&["foo"])];

        app.open_image_tags_modal();
        app.toggle_image_tag_modal_left();
        assert!(app.image_tag_modal_include_pending.contains("foo"));
        assert!(!app.image_tag_modal_exclude_pending.contains("foo"));

        app.toggle_image_tag_modal_right();
        assert!(!app.image_tag_modal_include_pending.contains("foo"));
        assert!(!app.image_tag_modal_exclude_pending.contains("foo"));

        app.toggle_image_tag_modal_right();
        assert!(!app.image_tag_modal_include_pending.contains("foo"));
        assert!(app.image_tag_modal_exclude_pending.contains("foo"));

        app.toggle_image_tag_modal_left();
        assert!(!app.image_tag_modal_include_pending.contains("foo"));
        assert!(!app.image_tag_modal_exclude_pending.contains("foo"));
    }

    fn sample_feed_image(id: u64) -> ImageItem {
        ImageItem {
            id,
            ..sample_image_with_tags(&[])
        }
    }

    #[test]
    fn image_jump_digits_and_backspace_edit_modal_input() {
        let mut app = App::new(isolated_config());

        app.open_image_jump_modal();
        app.append_image_jump_digit('1');
        app.append_image_jump_digit('2');
        app.append_image_jump_digit('a');
        app.backspace_image_jump_input();

        assert!(app.show_image_jump_modal);
        assert_eq!(app.image_jump_input, "1");
    }

    #[test]
    fn image_jump_within_loaded_images_selects_immediately() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Images;
        app.images = vec![
            sample_feed_image(1),
            sample_feed_image(2),
            sample_feed_image(3),
        ];
        app.image_feed_loaded = true;
        app.image_feed_has_more = true;
        app.image_jump_input = "2".into();

        let completed = app.apply_image_jump();

        assert!(completed);
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.pending_image_jump_target, None);
    }

    #[test]
    fn image_jump_beyond_loaded_images_sets_pending_target() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Images;
        app.images = vec![sample_feed_image(1), sample_feed_image(2)];
        app.image_feed_loaded = true;
        app.image_feed_has_more = true;
        app.image_feed_next_page = Some(2);
        app.image_jump_input = "55".into();

        let completed = app.apply_image_jump();

        assert!(!completed);
        assert_eq!(app.pending_image_jump_target, Some(54));
        assert_eq!(app.pending_image_jump_request(), Some(2));
    }

    #[test]
    fn image_jump_clamps_to_known_total_hits() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Images;
        app.images = vec![sample_feed_image(1), sample_feed_image(2)];
        app.image_feed_loaded = true;
        app.image_feed_has_more = true;
        app.image_feed_next_page = Some(2);
        app.image_feed_total_hits = Some(4);
        app.image_jump_input = "99".into();

        let completed = app.apply_image_jump();

        assert!(!completed);
        assert_eq!(app.pending_image_jump_target, Some(3));
    }

    #[test]
    fn image_jump_resolves_to_last_loaded_when_feed_exhausted() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::Images;
        app.images = vec![
            sample_feed_image(1),
            sample_feed_image(2),
            sample_feed_image(3),
        ];
        app.image_feed_loaded = true;
        app.image_feed_has_more = false;
        app.pending_image_jump_target = Some(99);

        let completed = app.resolve_pending_image_jump();

        assert!(completed);
        assert_eq!(app.selected_index, 2);
        assert_eq!(app.pending_image_jump_target, None);
    }

    #[test]
    fn liked_image_jump_clamps_to_visible_results() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::LikedImages;
        app.liked_images = vec![
            sample_feed_image(10),
            sample_feed_image(20),
            sample_feed_image(30),
        ];
        app.refresh_visible_liked_images_cache();
        app.image_jump_input = "99".into();

        let completed = app.apply_image_jump();

        assert!(completed);
        assert_eq!(app.selected_liked_image_index, 2);
        assert_eq!(app.liked_image_list_state.selected(), Some(2));
    }

    #[test]
    fn liked_image_jump_warns_when_no_visible_results() {
        let mut app = App::new(isolated_config());
        app.active_tab = MainTab::LikedImages;
        app.image_jump_input = "1".into();

        let completed = app.apply_image_jump();

        assert!(!completed);
        assert_eq!(app.selected_liked_image_index, 0);
    }

    #[test]
    fn request_download_sends_selected_liked_image_to_worker() {
        let mut app = App::new(isolated_config());
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        app.tx = Some(tx);
        app.active_tab = MainTab::LikedImages;
        app.liked_images = vec![sample_feed_image(10), sample_feed_image(20)];
        app.refresh_visible_liked_images_cache();
        app.selected_liked_image_index = 1;
        app.liked_image_list_state.select(Some(1));

        app.request_download();

        let command = rx.try_recv().expect("expected image download command");
        match command {
            WorkerCommand::DownloadImage(image) => assert_eq!(image.id, 20),
            _ => panic!("expected liked image download command"),
        }
        assert_eq!(app.status, "Downloading image 20...");
    }
}
