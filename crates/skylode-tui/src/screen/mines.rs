//! The Mines screen — pick a world and a mine, slide the richness dial
//! (UI-EN.md §5.3).
//!
//! Placeholder. The real screen is master-detail: a `List` of twelve mines under
//! three world headers on the left, and on the right the selected mine's gate,
//! size, and — on the three two-material mines only — the richness dial moved
//! with `←`/`→`.

use ratatui::{Frame, crossterm::event::KeyEvent, layout::Rect};

use crate::{action::Action, screen::placeholder, view::View};

/// Draws the placeholder panel.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    placeholder(
        frame,
        area,
        " Mines ",
        &[
            format!("Current   {}", view.mine_name),
            "List + detail pane land here.".to_owned(),
        ],
    );
}

/// No contextual bindings yet; `↑↓`, `Enter` and the dial's `←→` come later.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}
