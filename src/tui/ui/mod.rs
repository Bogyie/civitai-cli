mod downloads;
mod footer;
mod helpers;
mod images;
mod modals;
mod models;
mod tabs;

use anyhow::{Context, Result};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Tabs},
};
use std::io::{self, Stdout};

use crate::tui::app::{App, AppMode, MainTab};

const TAB_COUNT: usize = 6;
const FULL_TAB_LAYOUT_BUFFER: usize = 10;
const MEDIUM_TAB_LAYOUT_BUFFER: usize = 10;

fn tab_layout_for_width(width: u16) -> (Vec<&'static str>, &'static str, &'static str) {
    let inner_width = width.saturating_sub(2) as usize;
    let full = [
        "Models (1)",
        "Liked Models (2)",
        "Images (3)",
        "Liked Images (4)",
        "Downloads (5)",
        "Settings (6)",
    ];
    let medium = [
        "Models(1)",
        "Liked Models(2)",
        "Images(3)",
        "Liked Images(4)",
        "Down(5)",
        "Settings(6)",
    ];
    let compact = ["M1", "LM2", "I3", "LI4", "D5", "Set6"];

    let full_divider = " | ";
    let compact_divider = "|";

    let full_width =
        full.iter().map(|label| label.len()).sum::<usize>() + full_divider.len() * (TAB_COUNT - 1);
    if full_width + FULL_TAB_LAYOUT_BUFFER <= inner_width {
        return (
            full.into_iter().collect(),
            full_divider,
            " Civitai CLI | [1-6] Switch tab ",
        );
    }

    let medium_width = medium.iter().map(|label| label.len()).sum::<usize>()
        + compact_divider.len() * (TAB_COUNT - 1);
    if medium_width + MEDIUM_TAB_LAYOUT_BUFFER <= inner_width {
        return (
            medium.into_iter().collect(),
            compact_divider,
            " Civitai | [1-6] Tabs ",
        );
    }

    (compact.into_iter().collect(), compact_divider, " Tabs ")
}

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    let mut stdout = io::stdout();
    enable_raw_mode().context("failed to enable raw mode")?;
    execute!(stdout, EnterAlternateScreen).context("unable to enter alternate screen")?;
    Terminal::new(CrosstermBackend::new(stdout)).context("creating terminal failed")
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("unable to switch to main screen")?;
    terminal.show_cursor().context("unable to show cursor")
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    let (titles, divider, block_title) = tab_layout_for_width(chunks[0].width);
    let active_idx = match app.active_tab {
        MainTab::Models => 0,
        MainTab::LikedModels => 1,
        MainTab::Images => 2,
        MainTab::LikedImages => 3,
        MainTab::Downloads => 4,
        MainTab::Settings => 5,
    };
    let enable_name_rolling = !matches!(
        app.mode,
        AppMode::SearchForm
            | AppMode::SearchImages
            | AppMode::SearchLikedModels
            | AppMode::SearchLikedImages
            | AppMode::LikedPathPrompt
    );

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(block_title))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .select(active_idx)
        .divider(divider);

    f.render_widget(tabs, chunks[0]);

    tabs::draw_active_tab(f, app, chunks[1], enable_name_rolling);
    footer::draw_footer_section(f, app, chunks[2]);
    modals::draw_active_modals(f, app);
}

#[cfg(test)]
mod tests {
    use super::tab_layout_for_width;

    #[test]
    fn uses_full_tabs_when_space_allows() {
        let (titles, divider, title) = tab_layout_for_width(110);
        assert_eq!(divider, " | ");
        assert_eq!(titles[3], "Liked Images (4)");
        assert_eq!(title, " Civitai CLI | [1-6] Switch tab ");
    }

    #[test]
    fn falls_back_to_medium_tabs_for_tighter_widths() {
        let (titles, divider, title) = tab_layout_for_width(90);
        assert_eq!(divider, "|");
        assert_eq!(titles[1], "Liked Models(2)");
        assert_eq!(titles[4], "Down(5)");
        assert_eq!(title, " Civitai | [1-6] Tabs ");
    }

    #[test]
    fn leaves_full_layout_earlier_before_labels_start_feeling_cramped() {
        let (titles, divider, title) = tab_layout_for_width(102);
        assert_eq!(divider, "|");
        assert_eq!(titles[1], "Liked Models(2)");
        assert_eq!(title, " Civitai | [1-6] Tabs ");
    }

    #[test]
    fn falls_back_to_compact_tabs_for_narrow_widths() {
        let (titles, divider, title) = tab_layout_for_width(30);
        assert_eq!(divider, "|");
        assert_eq!(titles[1], "LM2");
        assert_eq!(titles[4], "D5");
        assert_eq!(title, " Tabs ");
    }

    #[test]
    fn leaves_medium_layout_earlier_before_labels_start_feeling_cramped() {
        let (titles, divider, title) = tab_layout_for_width(74);
        assert_eq!(divider, "|");
        assert_eq!(titles[1], "LM2");
        assert_eq!(titles[4], "D5");
        assert_eq!(title, " Tabs ");
    }
}
