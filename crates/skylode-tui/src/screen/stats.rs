//! The Stats screen — progression, prestige, run progress, history
//! (UI-EN.md §5.6).
//!
//! Placeholder. The real screen holds three panels, one of which is the full
//! event history — the same buffer the toasts show the tail of, which is why
//! the tick must eventually *return* what happened rather than only mutate.

use ratatui::{Frame, crossterm::event::KeyEvent, layout::Rect};

use crate::{action::Action, screen::placeholder, view::View};

/// Draws the placeholder panel.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    placeholder(
        frame,
        area,
        " Stats ",
        &[
            format!("Mining level   {}", view.player_level),
            format!("XP             {} / {}", view.xp, view.xp_to_next),
        ],
    );
}

/// No contextual bindings yet; `↑↓` scrolls the history, `p` opens prestige.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}
