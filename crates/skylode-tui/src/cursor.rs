//! Where the player is *pointing*, as opposed to where they are.
//!
//! A cursor is front-end state and nothing else: which row of a list is
//! highlighted, which sub-tab is showing. None of it belongs in `skylode-core` —
//! a list selection has no business reaching a save file, and the rules must stay
//! answerable without a screen.
//!
//! **A module of its own, rather than a field on [`App`](crate::app::App)**, and the
//! reason is a dependency and not tidiness. Both `app` and [`view`](crate::view)
//! need this type: `app` owns it and moves it, `view` reads it to project the run
//! into what the screens draw. Declaring it in `app` would make `view` import the
//! application — legal in Rust, since modules of one crate may refer to each other
//! in a cycle, but it points the dependency backwards: the read model would then
//! know about the loop that drives it.
//!
//! Phases 5 to 7 add their own fields here (the Inventory row, the Upgrades sub-tab
//! and its row) rather than growing
//! [`View::from_state`](crate::view::View::from_state) one parameter per phase.

use skylode_core::mine_kind::MineKind;

/// Every list cursor the front-end owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursors {
    /// Which of the twelve mines the Mines screen highlights and describes.
    ///
    /// A [`MineKind`] and not an index into a row list, so it cannot point at a row
    /// that no longer exists — the twelve are a fixed set, and the screen's own
    /// grouping into worlds is a rendering detail this must not depend on.
    ///
    /// **Distinct from the mine the player is standing in**, which is the run's and
    /// lives in `GameState`. The two start equal and part company the moment `↑` is
    /// pressed; the list marks them `▸` and `●` precisely so the difference is
    /// visible.
    pub mine: MineKind,
}

impl Cursors {
    /// The cursors a session opens with, given the mine the run is standing in.
    ///
    /// Seeded from the run rather than defaulted to [`MineKind::Stone`], so opening
    /// the Mines screen highlights where the player actually is. There is no
    /// [`Default`] for that reason: a default here would have to invent an answer
    /// the caller already holds.
    pub fn new(mine: MineKind) -> Self {
        Self { mine }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_opens_pointing_at_the_mine_it_is_standing_in() {
        assert_eq!(Cursors::new(MineKind::Obsidian).mine, MineKind::Obsidian);
    }
}
