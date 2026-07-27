//! The six tabs of the ring, and the dispatch to each.
//!
//! The ring is UI-EN.md §6.1: `Tab` cycles forward, `Shift+Tab` back, `1`..`6`
//! jump. Everything else in the design hangs off one of these six.
//!
//! **The counted widths are the `Fill` weights.** Every side-by-side split on these
//! screens used to be `Length(N) + Min(0)`, which froze one pane at the width
//! UI-EN.md §4 counted and handed the other every column the terminal had to spare.
//! They are now `Fill(a) + Fill(b)`, and `a` and `b` are *those same counted
//! numbers* — `38 : 42` on Mines, `30 : 50` on Stats. `Fill` divides the row in the
//! ratio of its arguments, so at 80 columns the solver reproduces the count exactly
//! and above it the same proportion holds. Reducing the pair (`19 : 21`) would work
//! identically and would cost the one property that makes this readable: the number
//! in the code is still the number in the wireframe.
//!
//! **Why an `enum` and not `Box<dyn Screen>`.** The set of screens is fixed and
//! known at compile time, so a trait object would buy dynamic dispatch we never
//! use and cost an allocation and a vtable hop per frame. More importantly the
//! `match` below is *exhaustive*: adding a seventh screen breaks the build in
//! every place that must learn about it. The compiler keeps the inventory, which
//! is exactly the job a design doc with fifteen states should not be doing by hand.

mod inventory;
mod levels;
mod mine;
mod mines;
mod stats;
mod upgrades;

use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::Rect,
    style::Style,
    widgets::{Block, BorderType, Borders, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::{action::Action, theme, view::View};

/// A rounded, fully-bordered panel with the given title — the box shape every
/// real screen shares, spelled once so a title style change lands everywhere.
///
/// That last clause is now load-bearing rather than aspirational: the border and
/// the title take their colours from [`crate::theme`] *here*, so every bordered box
/// on all six screens is one edit away from a different look, and none of them can
/// drift from the others by being styled at its own call site.
///
/// Returns `Block<'static>`, not `Block<'_>`: the block **owns** its title (that
/// `to_owned` is why), so tying the output's lifetime to the borrowed `title`
/// would force callers to keep a temporary format string alive for nothing. It is
/// `pub(super)` — the child screen modules reach it, nothing outside `screen` can.
pub(super) fn panel(title: &str) -> Block<'static> {
    Block::default()
        .title(title.to_owned())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::MUTED))
        .title_style(theme::TITLE)
}

/// Draws a vertical scrollbar of `content_length` items with the thumb at
/// `position`, using the design's `░` track and `█` thumb (UI.md §5.6).
///
/// Shared because both the Levels roadmap and the two scrolling Upgrades sub-tabs
/// draw the same bar. `content_length` is the whole list's length — the *ladder*,
/// not the window — so the thumb's size reflects how much of it is off-screen; a
/// zero length renders nothing, which is ratatui's own guard, not ours.
///
/// The thumb takes [`theme::ACCENT`] and the track [`theme::MUTED`], which is the
/// same pairing the status gauges use for their filled and unfilled halves — a
/// scrollbar and a gauge are both "how far along this are you", so they should not
/// answer that in two different colours.
pub(super) fn scrollbar(frame: &mut Frame, area: Rect, content_length: usize, position: usize) {
    let mut state = ScrollbarState::new(content_length).position(position);
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("░"))
        .thumb_symbol("█")
        .track_style(Style::default().fg(theme::MUTED))
        .thumb_style(Style::default().fg(theme::ACCENT));
    frame.render_stateful_widget(bar, area, &mut state);
}

/// One tab of the ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Active mining — the grid and the status gauges.
    Mine,
    /// Pick a world and a mine; slide the richness dial.
    Mines,
    /// Ores held, in both denominations; compress and decompress.
    Inventory,
    /// Pickaxe roadmap, enchants, and both mine tracks.
    Upgrades,
    /// Progression, prestige, run progress, and the event history.
    Stats,
    /// The level roadmap and what each level grants.
    Levels,
}

impl Screen {
    /// Every screen, in ring order.
    ///
    /// Declaration order *is* the tab order and the `1`..`6` mapping, so the
    /// array is the single place that decides all three.
    pub const ALL: [Self; 6] = [
        Self::Mine,
        Self::Mines,
        Self::Inventory,
        Self::Upgrades,
        Self::Stats,
        Self::Levels,
    ];

    /// The label shown in the tab bar, without its digit prefix.
    pub fn title(self) -> &'static str {
        match self {
            Self::Mine => "Mine",
            Self::Mines => "Mines",
            Self::Inventory => "Inventory",
            Self::Upgrades => "Upgrades",
            Self::Stats => "Stats",
            Self::Levels => "Levels",
        }
    }

    /// This screen's zero-based position in [`Self::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// The screen at `index`, or `None` if the index is past the end.
    ///
    /// Returns an `Option` rather than clamping because a caller asking for a
    /// seventh tab has a bug, and silently landing them on `Levels` would hide it.
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// The next screen along the ring, wrapping from the last back to the first.
    ///
    /// It wraps because the design calls the tabs a *ring* — `Tab` from `Levels`
    /// must reach `Mine` without a `Shift+Tab` marathon back through five tabs.
    pub fn next(self) -> Self {
        let next = (self.index() + 1) % Self::ALL.len();
        Self::ALL[next]
    }

    /// The previous screen, wrapping the other way.
    pub fn prev(self) -> Self {
        let prev = (self.index() + Self::ALL.len() - 1) % Self::ALL.len();
        Self::ALL[prev]
    }

    /// Draws this screen into `area`.
    pub fn render(self, frame: &mut Frame, area: Rect, view: &View) {
        match self {
            Self::Mine => mine::render(frame, area, view),
            Self::Mines => mines::render(frame, area, view),
            Self::Inventory => inventory::render(frame, area, view),
            Self::Upgrades => upgrades::render(frame, area, view),
            Self::Stats => stats::render(frame, area, view),
            Self::Levels => levels::render(frame, area, view),
        }
    }

    /// Decodes a key that the global bindings did not claim.
    ///
    /// Every screen currently declines every key: the contextual bindings
    /// (`↑↓` selection, `←→` value-adjust, `Enter`, `c`/`C`) arrive with the
    /// screens themselves. Returning `None` means "not mine", which lets
    /// [`crate::keymap`] fall through cleanly instead of swallowing the key.
    pub fn map_key(self, key: KeyEvent) -> Option<Action> {
        match self {
            Self::Mine => mine::map_key(key),
            Self::Mines => mines::map_key(key),
            Self::Inventory => inventory::map_key(key),
            Self::Upgrades => upgrades::map_key(key),
            Self::Stats => stats::map_key(key),
            Self::Levels => levels::map_key(key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_holds_every_screen_once() {
        assert_eq!(Screen::ALL.len(), 6);
        for (position, screen) in Screen::ALL.iter().enumerate() {
            assert_eq!(screen.index(), position);
        }
    }

    #[test]
    fn index_and_from_index_are_inverses() {
        for screen in Screen::ALL {
            assert_eq!(Screen::from_index(screen.index()), Some(screen));
        }
    }

    #[test]
    fn an_index_past_the_last_tab_is_refused() {
        assert_eq!(Screen::from_index(6), None);
    }

    #[test]
    fn next_wraps_from_the_last_screen_to_the_first() {
        assert_eq!(Screen::Mine.next(), Screen::Mines);
        assert_eq!(Screen::Levels.next(), Screen::Mine);
    }

    #[test]
    fn prev_wraps_from_the_first_screen_to_the_last() {
        assert_eq!(Screen::Mines.prev(), Screen::Mine);
        assert_eq!(Screen::Mine.prev(), Screen::Levels);
    }

    #[test]
    fn a_full_lap_of_the_ring_returns_to_the_start() {
        let mut screen = Screen::Mine;
        for _ in 0..Screen::ALL.len() {
            screen = screen.next();
        }
        assert_eq!(screen, Screen::Mine);
    }
}
