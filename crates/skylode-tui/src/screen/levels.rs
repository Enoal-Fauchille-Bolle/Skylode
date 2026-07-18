//! The Levels screen — the level roadmap (UI-EN.md §5.7.5).
//!
//! Placeholder. The real screen is a scrolling roadmap of levels 1..50 showing
//! what each grants, with two marks: the list cursor and the player's current
//! level. It exists because level-ups get no modal — the loot is a toast, and
//! this is the screen you open yourself to look at the ladder.

use ratatui::{Frame, crossterm::event::KeyEvent, layout::Rect};

use crate::{action::Action, screen::placeholder, view::View};

/// Draws the placeholder panel.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    placeholder(
        frame,
        area,
        " Levels ",
        &[
            format!(
                "Lv {} · {} / {} XP to Lv {}",
                view.player_level,
                view.xp,
                view.xp_to_next,
                view.player_level + 1
            ),
            "The 1..50 roadmap lands here.".to_owned(),
        ],
    );
}

/// No contextual bindings yet; `↑↓` scrolls, `Home` jumps to the current level.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}
