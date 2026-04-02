use std::time::{Duration, SystemTime};

use time::{OffsetDateTime, UtcOffset, format_description::FormatItem, macros::format_description};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Warn,
    Debug,
    Error,
}

impl StatusLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Debug => "DEBUG",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusHistoryFilter {
    All,
    Info,
    Warn,
    Debug,
    Error,
}

impl StatusHistoryFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Info => "Info",
            Self::Warn => "Warn",
            Self::Debug => "Debug",
            Self::Error => "Error",
        }
    }

    pub fn matches(self, level: StatusLevel) -> bool {
        match self {
            Self::All => true,
            Self::Info => level == StatusLevel::Info,
            Self::Warn => level == StatusLevel::Warn,
            Self::Debug => level == StatusLevel::Debug,
            Self::Error => level == StatusLevel::Error,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StatusEvent {
    pub level: StatusLevel,
    pub summary: String,
    pub detail: Option<String>,
    pub recorded_at: SystemTime,
    pub show_modal: bool,
}

impl StatusEvent {
    pub fn info(summary: impl Into<String>) -> Self {
        Self::new(StatusLevel::Info, summary, None, false)
    }

    pub fn warn(summary: impl Into<String>) -> Self {
        Self::new(StatusLevel::Warn, summary, None, false)
    }

    pub fn info_detail(summary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(StatusLevel::Info, summary, Some(detail.into()), false)
    }

    pub fn debug(summary: impl Into<String>) -> Self {
        Self::new(StatusLevel::Debug, summary, None, false)
    }

    pub fn error_detail(summary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(StatusLevel::Error, summary, Some(detail.into()), true)
    }

    pub fn full_text(&self) -> String {
        let mut lines = vec![
            format!("Time: {}", format_status_timestamp(self.recorded_at)),
            format!("Level: {}", self.level.label()),
            format!("Summary: {}", self.summary),
        ];

        if let Some(detail) = self.detail.as_deref().filter(|detail| !detail.trim().is_empty()) {
            lines.push(String::new());
            lines.push("Detail:".to_string());
            lines.push(detail.to_string());
        }

        lines.join("\n")
    }

    pub fn history_preview(&self) -> String {
        match self.detail.as_deref().filter(|detail| !detail.trim().is_empty()) {
            Some(detail) => format!(
                "{} [{}] {} | {}",
                format_status_time_only(self.recorded_at),
                self.level.label(),
                self.summary,
                detail.replace('\n', " "),
            ),
            None => format!(
                "{} [{}] {}",
                format_status_time_only(self.recorded_at),
                self.level.label(),
                self.summary,
            ),
        }
    }

    fn new(
        level: StatusLevel,
        summary: impl Into<String>,
        detail: Option<String>,
        show_modal: bool,
    ) -> Self {
        Self {
            level,
            summary: summary.into(),
            detail,
            recorded_at: SystemTime::now(),
            show_modal,
        }
    }
}

impl From<String> for StatusEvent {
    fn from(value: String) -> Self {
        StatusEvent::info(value)
    }
}

impl From<&str> for StatusEvent {
    fn from(value: &str) -> Self {
        StatusEvent::info(value)
    }
}

const TIME_ONLY_FORMAT: &[FormatItem<'static>] = format_description!("[hour]:[minute]:[second]");
const TIMESTAMP_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

pub fn format_status_time_only(value: SystemTime) -> String {
    format_time(value, TIME_ONLY_FORMAT)
}

pub fn format_status_timestamp(value: SystemTime) -> String {
    format_time(value, TIMESTAMP_FORMAT)
}

pub fn is_status_stale(value: SystemTime) -> bool {
    SystemTime::now()
        .duration_since(value)
        .unwrap_or(Duration::ZERO)
        >= Duration::from_secs(3)
}

fn format_time(value: SystemTime, format: &[FormatItem<'static>]) -> String {
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let dt = OffsetDateTime::from(value).to_offset(offset);
    dt.format(format)
        .unwrap_or_else(|_| OffsetDateTime::from(value).format(format).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_old_status_as_stale() {
        let old = SystemTime::now() - Duration::from_secs(4);
        assert!(is_status_stale(old));
    }

    #[test]
    fn keeps_recent_status_as_fresh() {
        let recent = SystemTime::now() - Duration::from_secs(2);
        assert!(!is_status_stale(recent));
    }
}
