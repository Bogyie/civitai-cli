use super::*;

impl App {
    pub fn set_download_history_file_path(&mut self, path: PathBuf) {
        self.download_history_file_path = Some(path.clone());
        self.config.download_history_file_path = Some(path);
        self.persist_download_history();
    }

    pub fn has_active_download(&self) -> bool {
        !self.active_downloads.is_empty()
    }

    pub fn collect_interrupt_sessions_from_active(&self) -> Vec<InterruptedDownloadSession> {
        self.active_download_order
            .iter()
            .filter_map(|model_id| {
                self.active_downloads
                    .get(model_id)
                    .map(|tracker| (*model_id, tracker))
            })
            .map(|(model_id, tracker)| InterruptedDownloadSession {
                model_id,
                version_id: tracker.version_id,
                filename: tracker.filename.clone(),
                model_name: tracker.model_name.clone(),
                file_path: tracker.file_path.clone(),
                downloaded_bytes: tracker.downloaded_bytes,
                total_bytes: tracker.total_bytes,
                created_at: std::time::SystemTime::now(),
            })
            .collect()
    }

    pub fn select_next_download(&mut self) {
        if !self.active_download_order.is_empty()
            && self.selected_download_index + 1 < self.active_download_order.len()
        {
            self.selected_download_index += 1;
        }
    }

    pub fn select_previous_download(&mut self) {
        if self.selected_download_index > 0 {
            self.selected_download_index -= 1;
        }
    }

    pub fn select_next_history(&mut self) {
        if self.selected_history_index + 1 < self.download_history.len() {
            self.selected_history_index += 1;
        }
    }

    pub fn select_previous_history(&mut self) {
        if self.selected_history_index > 0 {
            self.selected_history_index -= 1;
        }
    }

    pub fn selected_download_id(&mut self) -> Option<u64> {
        if self.active_download_order.is_empty() {
            self.selected_download_index = 0;
            return None;
        }
        if self.selected_download_index >= self.active_download_order.len() {
            self.selected_download_index = self.active_download_order.len() - 1;
        }
        self.active_download_order
            .get(self.selected_download_index)
            .copied()
    }

    pub fn push_download_history(&mut self, entry: NewDownloadHistoryEntry) {
        self.download_history.push(DownloadHistoryEntry {
            model_id: entry.model_id,
            version_id: entry.version_id,
            filename: entry.filename,
            model_name: entry.model_name,
            file_path: entry.file_path,
            downloaded_bytes: entry.downloaded_bytes,
            total_bytes: entry.total_bytes,
            status: entry.status,
            progress: entry.progress,
            created_at: std::time::SystemTime::now(),
        });
        if self.download_history.len() > 200 {
            let extra = self.download_history.len() - 200;
            self.download_history.drain(0..extra);
            self.clamp_selected_history_index();
        }
        self.persist_download_history();
    }

    pub fn selected_history_entry_index(&self) -> Option<usize> {
        if self.download_history.is_empty() {
            return None;
        }
        let idx = self
            .selected_history_index
            .min(self.download_history.len().saturating_sub(1));
        Some(self.download_history.len() - 1 - idx)
    }

    pub fn remove_selected_history(&mut self) -> Option<DownloadHistoryEntry> {
        let idx = self.selected_history_entry_index()?;
        let removed = self.download_history.remove(idx);
        self.clamp_selected_history_index();
        self.persist_download_history();
        Some(removed)
    }

    pub fn clamp_selected_download_index(&mut self) {
        if self.active_download_order.is_empty() {
            self.selected_download_index = 0;
            return;
        }
        if self.selected_download_index >= self.active_download_order.len() {
            self.selected_download_index = self.active_download_order.len() - 1;
        }
    }

    pub fn clamp_selected_history_index(&mut self) {
        if self.download_history.is_empty() {
            self.selected_history_index = 0;
            return;
        }
        if self.selected_history_index >= self.download_history.len() {
            self.selected_history_index = self.download_history.len() - 1;
        }
    }

    pub fn selected_download_history_entry(&self) -> Option<&DownloadHistoryEntry> {
        let idx = self.selected_history_entry_index()?;
        self.download_history.get(idx)
    }

    pub fn upsert_download_history(&mut self, entry: DownloadHistoryEntry) {
        self.download_history.retain(|existing| {
            !(existing.model_id == entry.model_id
                && existing.version_id == entry.version_id
                && matches!(existing.status, DownloadHistoryStatus::Paused))
        });

        self.download_history.push(entry);

        if self.download_history.len() > 200 {
            let extra = self.download_history.len() - 200;
            self.download_history.drain(0..extra);
            self.clamp_selected_history_index();
        }

        self.persist_download_history();
    }

    pub fn record_interrupted_session_to_history(&mut self, session: &InterruptedDownloadSession) {
        let progress = if session.total_bytes > 0 {
            (session.downloaded_bytes as f64 / session.total_bytes as f64) * 100.0
        } else {
            0.0
        };
        self.upsert_download_history(DownloadHistoryEntry {
            model_id: session.model_id,
            version_id: session.version_id,
            filename: session.filename.clone(),
            model_name: session.model_name.clone(),
            file_path: session.file_path.clone(),
            downloaded_bytes: session.downloaded_bytes,
            total_bytes: session.total_bytes,
            status: DownloadHistoryStatus::Paused,
            progress,
            created_at: session.created_at,
        });
    }

    pub fn cancel_resume_download_modal(&mut self) {
        self.show_resume_download_modal = false;
    }

    pub fn clear_interrupted_download_sessions(&mut self) {
        self.interrupted_download_sessions.clear();
        self.show_resume_download_modal = false;
        self.persist_interrupted_downloads();
    }

    pub fn remove_history_for_session(&mut self, model_id: u64, version_id: u64) -> usize {
        let before = self.download_history.len();
        self.download_history
            .retain(|entry| !(entry.model_id == model_id && entry.version_id == version_id));
        if before != self.download_history.len() {
            self.persist_download_history();
            self.clamp_selected_history_index();
        }
        before.saturating_sub(self.download_history.len())
    }

    pub fn persist_interrupted_downloads(&mut self) {
        if let Some(path) = &self.interrupted_download_file_path {
            if let Err(err) =
                save_interrupted_downloads_to_file(path, &self.interrupted_download_sessions)
            {
                self.last_error = Some(err.to_string());
            } else {
                self.last_error = None;
            }
        }
    }

    pub fn begin_exit_confirm_modal(&mut self) {
        self.show_exit_confirm_modal = true;
        self.status = format!(
            "Active downloads detected ({}). Confirm exit: [Y] Save and exit, [D] Delete and exit, [N] Cancel.",
            self.active_downloads.len()
        );
    }

    pub fn cancel_exit_confirm_modal(&mut self) {
        self.show_exit_confirm_modal = false;
    }

    pub fn persist_download_history(&mut self) {
        if let Some(path) = &self.download_history_file_path {
            if let Err(err) = save_download_history_to_file(path, &self.download_history) {
                self.last_error = Some(err.to_string());
            } else {
                self.last_error = None;
            }
        }
    }
}
