use crossterm::terminal;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::{AppConfig, MediaQualityPreference};
use crate::tui::app::MediaRenderRequest;

pub fn debug_fetch_log_path(config: &AppConfig) -> Option<PathBuf> {
    AppConfig::config_dir()
        .or_else(|| config.search_cache_path())
        .map(|dir| dir.join("fetch_debug.log"))
}

pub fn debug_fetch_log(config: &AppConfig, message: &str) {
    if !config.debug_logging {
        return;
    }
    let Some(path) = debug_fetch_log_path(config) else {
        return;
    };

    write_debug_fetch_log(&path, message);
}

pub fn current_image_render_request() -> MediaRenderRequest {
    let (cols, rows) = terminal::size().unwrap_or((120, 40));
    image_render_request_for_size(cols, rows)
}

pub fn current_image_protocol_area() -> Rect {
    let (cols, rows) = terminal::size().unwrap_or((120, 40));
    image_protocol_area_for_size(cols, rows)
}

pub fn current_model_cover_render_request() -> MediaRenderRequest {
    let (cols, rows) = terminal::size().unwrap_or((120, 40));
    model_cover_render_request_for_size(cols, rows)
}

pub fn image_render_request_for_size(cols: u16, rows: u16) -> MediaRenderRequest {
    let panel_width = ((cols as f32) * 0.46).round().max(24.0) as u32;
    let panel_height = ((rows as f32) * 0.72).round().max(12.0) as u32;
    MediaRenderRequest {
        width: panel_width.saturating_mul(14),
        height: panel_height.saturating_mul(28),
    }
}

pub fn model_cover_render_request_for_size(cols: u16, rows: u16) -> MediaRenderRequest {
    let panel_width = ((cols as f32) * 0.38).round().max(18.0) as u32;
    let panel_height = ((rows as f32) * 0.38).round().max(10.0) as u32;
    MediaRenderRequest {
        width: panel_width.saturating_mul(12),
        height: panel_height.saturating_mul(24),
    }
}

pub fn image_protocol_area_for_size(cols: u16, rows: u16) -> Rect {
    let app_area = Rect::new(0, 0, cols, rows);
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(app_area);
    let image_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(outer[1]);
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(image_area[1]);
    let panel = main[0];

    Rect::new(
        panel.x.saturating_add(1),
        panel.y.saturating_add(1),
        panel.width.saturating_sub(2),
        panel.height.saturating_sub(2),
    )
}

pub fn render_request_key(request: MediaRenderRequest, quality: MediaQualityPreference) -> String {
    format!("{}:{}x{}", quality.label(), request.width, request.height)
}

pub fn image_render_cache_key(
    request: MediaRenderRequest,
    quality: MediaQualityPreference,
    area: Rect,
) -> String {
    format!(
        "{}:{}x{}@{}x{}",
        quality.label(),
        request.width,
        request.height,
        area.width,
        area.height
    )
}

fn write_debug_fetch_log(path: &Path, message: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs();
        let _ = writeln!(file, "[{}] {}", ts, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_render_request_respects_expected_scaling() {
        let request = image_render_request_for_size(100, 50);

        assert_eq!(request.width, 644);
        assert_eq!(request.height, 1008);
    }

    #[test]
    fn model_cover_request_applies_minimum_size() {
        let request = model_cover_render_request_for_size(10, 5);

        assert_eq!(
            request,
            MediaRenderRequest {
                width: 216,
                height: 240,
            }
        );
    }

    #[test]
    fn render_request_key_includes_quality_and_dimensions() {
        let key = render_request_key(
            MediaRenderRequest {
                width: 320,
                height: 240,
            },
            MediaQualityPreference::High,
        );

        assert_eq!(key, "High:320x240");
    }

    #[test]
    fn image_protocol_area_matches_expected_layout_shape() {
        let area = image_protocol_area_for_size(100, 50);

        assert_eq!(area.width, 48);
        assert_eq!(area.height, 40);
    }

    #[test]
    fn image_render_cache_key_includes_protocol_area() {
        let key = image_render_cache_key(
            MediaRenderRequest {
                width: 320,
                height: 240,
            },
            MediaQualityPreference::High,
            Rect::new(0, 0, 48, 20),
        );

        assert_eq!(key, "High:320x240@48x20");
    }
}
