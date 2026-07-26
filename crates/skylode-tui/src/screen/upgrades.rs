//! The Upgrades screen — pickaxe, enchants, and both mine tracks
//! (UI-EN.md §5.5).
//!
//! Placeholder. The real screen is the one that does not fit flat: ~96 rows of
//! content cut into three sub-tabs, each a scrolling list with a detail pane that
//! carries the tier-jump dip warning before the purchase is committed.

use ratatui::{Frame, crossterm::event::KeyEvent, layout::Rect};

use crate::{action::Action, screen::placeholder, view::View};

/// Draws the placeholder panel.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    placeholder(
        frame,
        area,
        " Upgrades ",
        &[
            format!("Pickaxe   {}", view.pickaxe.summary),
            "Sub-tabs: Pickaxe / Enchants / Mines.".to_owned(),
        ],
    );
}

/// No contextual bindings yet; the sub-tab binding is configurable (§9).
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}
