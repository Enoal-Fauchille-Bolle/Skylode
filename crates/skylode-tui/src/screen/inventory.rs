//! The Inventory screen — ores held, and manual compression (UI-EN.md §5.4).
//!
//! Placeholder. The real screen is a fifteen-row `Table` of materials in both
//! denominations beside a Compress panel, and it is where the "compress first"
//! refusal is turned into one keypress rather than a walk to another screen.

use ratatui::{Frame, crossterm::event::KeyEvent, layout::Rect};

use crate::{action::Action, screen::placeholder, view::View};

/// Draws the placeholder panel.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    placeholder(
        frame,
        area,
        " Inventory ",
        &[
            format!("Raw          {}", view.raw_held),
            format!("Compressed   {}", view.compressed_held),
        ],
    );
}

/// No contextual bindings yet; `c` / `C` arrive with the compression dialog.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}
