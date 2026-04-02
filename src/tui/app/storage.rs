use civitai_cli::sdk::{SearchImageHit as ImageItem, SearchModelHit as Model};
use serde_json;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::tui::app::{DownloadHistoryEntry, DownloadHistoryStatus, InterruptedDownloadSession};

pub(super) fn load_bookmarks(path: Option<&Path>) -> Vec<Model> {
    let Some(path) = path else {
        return Vec::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut models: Vec<Model> = serde_json::from_str(&content).unwrap_or_default();

    let mut seen = HashSet::new();
    models.retain(|model| seen.insert(model.id));
    models
}

pub(super) fn load_image_bookmarks(path: Option<&Path>) -> Vec<ImageItem> {
    let Some(path) = path else {
        return Vec::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut images: Vec<ImageItem> = serde_json::from_str(&content).unwrap_or_default();
    let mut seen = HashSet::new();
    images.retain(|image| seen.insert(image.id));
    images
}

pub(super) fn load_image_tag_catalog(path: Option<&Path>) -> Vec<String> {
    let Some(path) = path else {
        return Vec::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut tags: Vec<String> = serde_json::from_str(&content).unwrap_or_default();
    let mut seen = HashSet::new();
    tags.retain(|tag| {
        let normalized = tag.trim().to_lowercase();
        !normalized.is_empty() && seen.insert(normalized)
    });
    tags.sort_by_key(|tag| tag.to_lowercase());
    tags
}

pub(super) fn save_bookmarks_to_file(path: &Path, bookmarks: &[Model]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let mut normalized = bookmarks.to_vec();
    let mut seen = HashSet::new();
    normalized.retain(|model| seen.insert(model.id));

    let json = serde_json::to_string_pretty(&normalized).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}

pub(super) fn save_image_bookmarks_to_file(
    path: &Path,
    bookmarks: &[ImageItem],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let mut normalized = bookmarks.to_vec();
    let mut seen = HashSet::new();
    normalized.retain(|image| seen.insert(image.id));

    let json = serde_json::to_string_pretty(&normalized).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}

pub(super) fn save_image_tag_catalog_to_file(path: &Path, tags: &[String]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let mut normalized = tags
        .iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    normalized.retain(|tag| seen.insert(tag.to_lowercase()));
    normalized.sort_by_key(|tag| tag.to_lowercase());

    let json = serde_json::to_string_pretty(&normalized).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}

pub(super) fn load_download_history(path: Option<&Path>) -> Vec<DownloadHistoryEntry> {
    let Some(path) = path else {
        return Vec::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut history: Vec<DownloadHistoryEntry> = serde_json::from_str(&content).unwrap_or_default();

    if history.len() > 200 {
        let extra = history.len() - 200;
        history.drain(0..extra);
    }

    history
}

pub(super) fn collect_paused_sessions_from_history(
    history: &[DownloadHistoryEntry],
) -> Vec<InterruptedDownloadSession> {
    let mut sessions = Vec::new();
    let mut seen = HashSet::new();

    for entry in history.iter().rev() {
        if !matches!(entry.status, DownloadHistoryStatus::Paused) {
            continue;
        }

        if entry.downloaded_bytes == 0 {
            continue;
        }

        if let Some(total_bytes) = if entry.total_bytes == 0 {
            None
        } else {
            Some(entry.total_bytes)
        } && entry.downloaded_bytes >= total_bytes
        {
            continue;
        }

        if let Some(file_path) = &entry.file_path {
            if !file_path.exists() {
                continue;
            }
        } else {
            continue;
        }

        let session_key = (
            entry.model_id,
            entry.version_id,
            entry.file_path
                .clone()
                .unwrap_or_else(|| PathBuf::from(entry.filename.clone())),
        );
        if seen.contains(&session_key) {
            continue;
        }
        seen.insert(session_key);

        sessions.push(InterruptedDownloadSession {
            model_id: entry.model_id,
            version_id: entry.version_id,
            filename: entry.filename.clone(),
            model_name: entry.model_name.clone(),
            file_path: entry.file_path.clone(),
            downloaded_bytes: entry.downloaded_bytes,
            total_bytes: entry.total_bytes,
            created_at: entry.created_at,
        });
    }

    sessions.reverse();
    sessions
}

pub(super) fn load_interrupted_downloads(path: Option<&Path>) -> Vec<InterruptedDownloadSession> {
    let Some(path) = path else {
        return Vec::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    serde_json::from_str::<Vec<InterruptedDownloadSession>>(&content).unwrap_or_default()
}

pub(super) fn save_download_history_to_file(
    path: &Path,
    history: &[DownloadHistoryEntry],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let json = serde_json::to_string_pretty(history).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}

pub(super) fn save_interrupted_downloads_to_file(
    path: &Path,
    sessions: &[InterruptedDownloadSession],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    if sessions.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }

    let json = serde_json::to_string_pretty(sessions).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn test_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("civitai-cli-storage-{unique}"));
        fs::create_dir_all(&dir).expect("create dir");
        dir
    }

    fn write_json(path: &Path, value: serde_json::Value) {
        fs::write(path, serde_json::to_vec(&value).expect("json")).expect("write json");
    }

    #[test]
    fn loads_bookmarks_without_duplicates() {
        let dir = test_dir();
        let path = dir.join("bookmarks.json");
        write_json(
            &path,
            json!([
                { "id": 1, "name": "One" },
                { "id": 1, "name": "Duplicate" },
                { "id": 2, "name": "Two" }
            ]),
        );

        let bookmarks = load_bookmarks(Some(&path));

        assert_eq!(
            bookmarks.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn normalizes_image_tag_catalog_before_saving() {
        let dir = test_dir();
        let path = dir.join("tags.json");
        let tags = vec![
            "  Flux ".to_string(),
            "flux".to_string(),
            "".to_string(),
            "Anime".to_string(),
        ];

        save_image_tag_catalog_to_file(&path, &tags).expect("save tags");

        let saved = fs::read_to_string(&path).expect("saved file");
        let parsed: Vec<String> = serde_json::from_str(&saved).expect("parsed tags");
        assert_eq!(parsed, vec!["Anime".to_string(), "Flux".to_string()]);
    }

    #[test]
    fn collects_only_resumable_paused_sessions() {
        let dir = test_dir();
        let file_path = dir.join("partial.safetensors");
        fs::write(&file_path, b"partial").expect("partial file");
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let history = vec![
            DownloadHistoryEntry {
                model_id: 1,
                version_id: 10,
                filename: "a.safetensors".to_string(),
                model_name: "A".to_string(),
                file_path: Some(file_path.clone()),
                downloaded_bytes: 50,
                total_bytes: 100,
                status: DownloadHistoryStatus::Paused,
                progress: 50.0,
                created_at: now,
            },
            DownloadHistoryEntry {
                model_id: 1,
                version_id: 10,
                filename: "a-new.safetensors".to_string(),
                model_name: "A".to_string(),
                file_path: Some(file_path),
                downloaded_bytes: 75,
                total_bytes: 100,
                status: DownloadHistoryStatus::Paused,
                progress: 75.0,
                created_at: now + Duration::from_secs(1),
            },
            DownloadHistoryEntry {
                model_id: 2,
                version_id: 20,
                filename: "b.safetensors".to_string(),
                model_name: "B".to_string(),
                file_path: None,
                downloaded_bytes: 25,
                total_bytes: 100,
                status: DownloadHistoryStatus::Paused,
                progress: 25.0,
                created_at: now,
            },
        ];

        let sessions = collect_paused_sessions_from_history(&history);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].model_id, 1);
        assert_eq!(sessions[0].downloaded_bytes, 75);
    }
}
