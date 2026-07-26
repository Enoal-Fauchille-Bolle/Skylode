//! The splash / main menu (UI.md §6.1).
//!
//! The first screen, and per the bootstrap rule (§2.3) its chrome is **hardcoded**
//! while its summary is **save-derived**: `Continue` and the two summary lines are
//! absent on a fresh install. Here both are present, with placeholder figures, so
//! the layout can be judged before a save exists to fill it.
//!
//! `#[allow(dead_code)]` until the session state machine opens on it (phase 7).

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use super::centered_rect;

/// The block-letter wordmark; the value cell's own art, five rows tall.
const LOGO: [&str; 5] = [
    "███████ ██   ██ ██    ██",
    "██      ██  ██   ██  ██",
    "███████ █████     ████",
    "     ██ ██  ██     ██",
    "███████ ██   ██    ██     L O D E",
];

/// Draws the splash screen.
#[allow(dead_code, reason = "awaiting the phase-7 session state machine")]
pub fn render(frame: &mut Frame, area: Rect) {
    let [_, logo_area, menu_band, summary_area, fill, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(5),
        Constraint::Length(6),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    // Left-aligned inside a centred rect, so the block art's columns stay lined up
    // — a per-line centre would jag them.
    let logo_width = LOGO.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
    frame.render_widget(
        Paragraph::new(LOGO.join("\n")),
        centered_rect(logo_area, logo_width, 5),
    );

    let menu = [
        "  ▸  Continue",
        "     New game",
        "     Settings",
        "     Quit",
    ];
    let menu_box = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain);
    frame.render_widget(
        Paragraph::new(menu.join("\n")).block(menu_box),
        centered_rect(menu_band, 30, 6),
    );

    let summary = "Lv 23  ·  Diamond Pickaxe\nlast played 3 hours ago";
    frame.render_widget(
        Paragraph::new(summary).alignment(Alignment::Center),
        summary_area,
    );

    // The version sits bottom-right, above the key hints on the last row.
    frame.render_widget(
        Paragraph::new("skylode 0.1.0")
            .alignment(Alignment::Right)
            .block(Block::default().padding(ratatui::widgets::Padding::right(2))),
        fill,
    );
    frame.render_widget(
        Paragraph::new(" ↑↓  select     Enter  confirm     q  quit"),
        footer_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_shows_the_menu_the_summary_and_the_version() {
        let frame = crate::overlay::render_to_string(render);
        assert!(frame.contains("Continue"), "{frame}");
        assert!(
            frame.contains("New game") && frame.contains("Quit"),
            "{frame}"
        );
        assert!(frame.contains("Lv 23  ·  Diamond Pickaxe"), "{frame}");
        assert!(frame.contains("last played 3 hours ago"), "{frame}");
        assert!(frame.contains("skylode 0.1.0"), "{frame}");
    }

    #[test]
    fn it_draws_the_wordmark() {
        let frame = crate::overlay::render_to_string(render);
        // The block art renders as full blocks; at least one row must survive.
        assert!(frame.contains('█'), "the wordmark did not draw: {frame}");
        assert!(frame.contains("L O D E"), "{frame}");
    }
}
