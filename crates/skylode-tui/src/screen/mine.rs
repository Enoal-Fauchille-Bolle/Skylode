//! The Mine screen — active mining (UI-EN.md §5.2).
//!
//! Placeholder. The real screen is a fixed 42-column grid panel with a `Haul`
//! header strip above it and Pickaxe/Mine panels beside it, over three
//! `LineGauge` rows. The grid itself becomes a custom widget in
//! [`crate::widget`], because the core owns the geometry and the TUI only paints it.

use ratatui::{Frame, crossterm::event::KeyEvent, layout::Rect};

use crate::{action::Action, screen::placeholder, view::View};

/// Draws the placeholder panel.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    placeholder(
        frame,
        area,
        " Mine ",
        &[
            format!("Mine      {}", view.mine_name),
            format!("Pickaxe   {}", view.pickaxe),
            format!("Level     {}", view.player_level),
        ],
    );
}

/// No contextual bindings yet; `Space` (mine) arrives with the real screen.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}
