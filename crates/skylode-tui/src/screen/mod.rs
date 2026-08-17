//! The six tabs of the ring, and the dispatch to each.
//!
//! The ring is `docs/UI.md` §2.1: `Tab` cycles forward, `Shift+Tab` back, `1`..`6`
//! jump. Everything else in the design hangs off one of these six.
//!
//! **The counted widths are the `Fill` weights.** Every side-by-side split on these
//! screens used to be `Length(N) + Min(0)`, which froze one pane at the width the
//! counted frames of `docs/UI.md` §5 gave it and handed the other every column the
//! terminal had to spare. They are now `Fill(a) + Fill(b)`, and `a` and `b` are
//! *those same counted numbers* — `38 : 42` on Mines, `30 : 50` on Stats. `Fill`
//! divides the row in the ratio of its arguments, so at 80 columns the solver
//! reproduces the count exactly and above it the same proportion holds. Reducing the
//! pair (`19 : 21`) would work identically and would cost the one property that
//! makes this readable: the number in the code is still the number in the wireframe.
//!
//! **Unreduced with no exceptions**, including where the split is even: Help writes
//! `40 : 40` rather than `1 : 1`. The reduced form is not wrong there and is arguably
//! clearer read alone — but a rule with one exception is a rule every reader has to
//! check a file against before trusting a number in it, which costs more than the
//! one line it saves.
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

use std::ops::Range;

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

/// Draws a vertical scrollbar for a `total`-long list showing `visible` rows, with
/// the thumb at scroll `position`, using the design's `░` track and `█` thumb
/// (UI.md §5.6).
///
/// Shared because both the Levels roadmap and the two scrolling Upgrades sub-tabs
/// draw the same bar.
///
/// **The translation into ratatui's contract lives here, and it is not the obvious
/// one.** [`ScrollbarState::content_length`] is *not* the number of rows: it is the
/// number of distinct scroll **positions**, which for a windowed list is
/// `total - visible + 1`. Ratatui places the thumb at
/// `position * track / (content_length - 1 + viewport)` and gives it a length of
/// `viewport * track / (content_length - 1 + viewport)`, so the two sum to the whole
/// track — the thumb touches the bottom — exactly when `position == content_length - 1`.
/// Handing it the row count instead reserves track for `visible` positions the list
/// does not have, and the thumb stops short of the end by that fraction: on the
/// 51-rung pickaxe ladder at 43 visible rows it stopped at a quarter.
///
/// [`ScrollbarState::viewport_content_length`] is set for the same reason rather than
/// left to default: ratatui falls back to the *track's* height, which happens to equal
/// `visible` at both call sites today and would silently stop doing so the day a bar is
/// drawn beside a header it does not span.
///
/// A list that fits draws **nothing**, and the guard is ours rather than ratatui's
/// `content_length == 0` one: the `+ 1` below would turn a fitting list into a
/// full-height thumb, which reads as a scrollbar that is stuck rather than as one that
/// is absent.
///
/// The thumb takes [`theme::ACCENT`] and the track [`theme::MUTED`], which is the
/// same pairing the status gauges use for their filled and unfilled halves — a
/// scrollbar and a gauge are both "how far along this are you", so they should not
/// answer that in two different colours.
pub(super) fn scrollbar(
    frame: &mut Frame,
    area: Rect,
    total: usize,
    visible: usize,
    position: usize,
) {
    let scrolled = total.saturating_sub(visible);
    if scrolled == 0 {
        return;
    }
    let mut state = ScrollbarState::new(scrolled + 1)
        .position(position)
        .viewport_content_length(visible);
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("░"))
        .thumb_symbol("█")
        .track_style(Style::default().fg(theme::MUTED))
        .thumb_style(Style::default().fg(theme::ACCENT));
    frame.render_stateful_widget(bar, area, &mut state);
}

/// The slice of a `total`-long list that a `visible`-row viewport should draw,
/// given the stored scroll `offset` and where the `cursor` sits.
///
/// **Why the offset is an argument and not a return value.** The obvious helper
/// derives the window from the cursor alone — centre it, clamp to the ends — and
/// that would be wrong twice over. It would jump the list under the player every
/// time the selection moved by one, and, because the window would then be a
/// function of the terminal's height, it would silently redraw the 80×24 frame the
/// wireframes were counted against. Carrying the offset makes scrolling *minimal*:
/// the window only moves when it has to, which is [`ratatui::widgets::ListState`]'s
/// own contract and the reason this is not a novel design.
///
/// The three clamps, in the order they must apply:
///
/// 1. against `total - visible`, for a viewport that just grew or a list that shrank;
/// 2. down to `cursor`, when the selection has scrolled off the **top**;
/// 3. up to `cursor + 1 - visible`, when it has scrolled off the **bottom**.
///
/// Everything is `saturating_*` because all three are `usize` and every one of them
/// underflows in a reachable case — an empty list, a viewport taller than the
/// content, a cursor at row zero. A wrapped subtraction here would ask for a slice
/// starting near `usize::MAX` and take the process down over a scroll position.
///
/// A `visible` of zero yields an empty range rather than a panic: a screen squeezed
/// to no rows at all still has to render something, and "nothing" is the answer.
/// The cursor is clamped into the list for the same reason — an out-of-range one
/// would push the offset past the end and hand back an **inverted** range, which is
/// the one shape that panics the moment a caller slices with it.
pub(super) fn window(total: usize, cursor: usize, offset: usize, visible: usize) -> Range<usize> {
    if visible == 0 || total == 0 {
        return 0..0;
    }
    let cursor = cursor.min(total - 1);
    let offset = offset
        .min(total.saturating_sub(visible))
        .min(cursor)
        .max((cursor + 1).saturating_sub(visible));
    offset..(offset + visible).min(total)
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
    ///
    /// Through [`cursor::step_in`](crate::cursor::step_in), which is now the crate's
    /// one step: it could not be while the lists clamped, since the helper *was* the
    /// clamp and a ring had to spell its own `%` out. The dependency points the right
    /// way — `cursor` knows nothing of `screen`.
    pub fn next(self) -> Self {
        crate::cursor::step_in(&Self::ALL, self, 1)
    }

    /// The previous screen, wrapping the other way.
    pub fn prev(self) -> Self {
        crate::cursor::step_in(&Self::ALL, self, -1)
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
    use ratatui::{
        Terminal,
        backend::TestBackend,
        crossterm::event::{KeyCode, KeyModifiers},
    };

    use super::*;

    /// The bar a `total`-long list of `visible` rows draws at `position`, as one
    /// string of `height` glyphs.
    ///
    /// Rendered through a real [`TestBackend`] rather than by calling ratatui's
    /// arithmetic directly: the thing under test *is* the translation into ratatui's
    /// contract, so a test that reimplemented the contract could only agree with
    /// itself.
    fn bar(total: usize, visible: usize, position: usize, height: u16) -> String {
        let mut terminal = match Terminal::new(TestBackend::new(1, height)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        if let Err(infallible) = terminal.draw(|frame| {
            scrollbar(frame, frame.area(), total, visible, position);
        }) {
            match infallible {}
        }
        let buffer = terminal.backend().buffer().clone();
        (0..height).map(|y| buffer[(0, y)].symbol()).collect()
    }

    #[test]
    fn the_thumb_reaches_both_ends_of_the_track() {
        // The screenshot's own numbers: 51 rungs, 43 visible, so the last offset is 8.
        // At that offset the thumb must be flush with the *bottom* — the bug this
        // helper was rewritten for left it at roughly a quarter, because it was handed
        // 51 scroll positions when the list has 9.
        let top = bar(51, 43, 0, 43);
        assert!(top.starts_with('█'), "the thumb is not at the top: {top}");
        assert!(top.ends_with('░'), "the track has no tail: {top}");

        let bottom = bar(51, 43, 8, 43);
        assert!(
            bottom.ends_with('█'),
            "the thumb does not reach the bottom: {bottom}"
        );
        assert!(bottom.starts_with('░'), "the track has no head: {bottom}");
    }

    #[test]
    fn the_thumb_is_the_visible_share_of_the_list() {
        // Half the list on screen is half a track of thumb, which is the reading a
        // scrollbar exists to give: how much of this am I looking at.
        let half = bar(20, 10, 0, 10);
        assert_eq!(half.matches('█').count(), 5, "{half}");
    }

    #[test]
    fn a_list_that_fits_draws_no_bar_at_all() {
        // Not a full-height thumb, and not a bare track: a list with nowhere to
        // scroll has nothing to report, and the reserved column stays blank. Both the
        // equal case and the taller-viewport one, since only the first is reachable
        // through `window`.
        assert_eq!(bar(10, 10, 0, 10), " ".repeat(10));
        assert_eq!(bar(4, 10, 0, 10), " ".repeat(10));
    }

    #[test]
    fn a_position_past_the_last_offset_pins_the_thumb_rather_than_overflowing() {
        // Unreachable through `window`, which clamps — but a scroll position is an
        // untyped `usize`, and a renderer is the wrong place to discover a caller was
        // wrong. Ratatui clamps it for us; this pins that it does.
        let over = bar(51, 43, 999, 43);
        assert_eq!(over, bar(51, 43, 8, 43));
    }

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
    fn a_list_that_fits_is_never_windowed() {
        // The whole list, and the offset is ignored rather than obeyed: a stored
        // offset outlives the resize that made it meaningless.
        assert_eq!(window(6, 3, 4, 20), 0..6);
        assert_eq!(window(6, 0, 0, 6), 0..6);
    }

    #[test]
    fn a_stored_offset_survives_a_redraw_that_did_not_need_to_move_it() {
        // The property the counted 80×24 frames depend on: with the cursor already
        // inside the window, `window` hands back exactly the slice the view asked
        // for. If this ever fails, every wireframe in `docs/UI.md` §5 has moved.
        assert_eq!(window(46, 30, 27, 19), 27..46);
    }

    #[test]
    fn a_taller_viewport_scrolls_back_rather_than_running_off_the_end() {
        // 45 rows of room in a 46-row list: the offset cannot stay at 27 or the
        // window would ask for rows 27..72. It gives ground to the *first* clamp.
        assert_eq!(window(46, 30, 27, 45), 1..46);
    }

    #[test]
    fn the_window_follows_the_cursor_off_either_edge_by_the_minimum() {
        // Off the top: the offset drops to the cursor, not to zero and not to a
        // recentred guess — one row of scroll for one row of movement.
        assert_eq!(window(46, 26, 27, 19), 26..45);
        // Off the bottom, from an offset that really is above it: the window slides
        // just far enough for the cursor to land on the *last* visible row.
        let range = window(46, 40, 5, 19);
        assert_eq!(range, 22..41);
        assert_eq!(range.end - 1, 40);
        // And a cursor already inside the window moves nothing at all — the clamps
        // are corrections, not a recentring.
        assert_eq!(window(46, 40, 27, 19), 27..46);
    }

    #[test]
    fn the_degenerate_inputs_yield_an_empty_range_rather_than_a_panic() {
        // No rows to draw into, no rows to draw, and a cursor past the end — the
        // last is the one that would invert the range and panic a slicing caller.
        assert_eq!(window(46, 3, 0, 0), 0..0);
        assert_eq!(window(0, 0, 0, 20), 0..0);
        let range = window(46, 999, 0, 19);
        assert!(range.start <= range.end, "an inverted range: {range:?}");
        assert_eq!(range, 27..46);
    }

    #[test]
    fn only_the_screens_with_bindings_claim_a_key() {
        // Asserted through the dispatch rather than on each module's `map_key`
        // directly, because the dispatch is the part that can go wrong: a seventh
        // screen wired to the wrong arm would return another screen's answer, and
        // `match` exhaustiveness cannot catch a swap.
        //
        // This test used to read `every_screen_declines_every_key_it_is_offered`, and
        // it was written to fail on the first screen that claimed one — which is what
        // it did when Mines grew its list bindings. The table below is what replaced
        // it: every screen still has a row, so the next screen to be wired fails here
        // in the same way rather than silently joining a blanket exception.
        let keys = [
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Enter,
            KeyCode::Char('c'),
            KeyCode::Char('\u{1}'),
            KeyCode::Char('M'),
            KeyCode::Home,
            KeyCode::Char('A'),
            // A global rather than any screen's, and in the table for exactly that
            // reason: `Esc` returns to the Mine screen from all six, so the column
            // that matters is the one full of `None`. A screen that claimed it would
            // shadow the global in `keymap`'s rule 6 and lose the way back, on that
            // screen only — the quietest way this binding could break.
            KeyCode::Esc,
        ];
        // What each screen answers, in the order of `keys` above.
        let claimed = |screen: Screen| -> [Option<Action>; 11] {
            match screen {
                Screen::Mines => [
                    Some(Action::CursorUp),
                    Some(Action::CursorDown),
                    Some(Action::AdjustLeft),
                    Some(Action::AdjustRight),
                    Some(Action::Confirm),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
                // The table walks its rows and opens the compression dialog on `c`,
                // but adjusts nothing and confirms nothing itself: `←/→` and `Enter`
                // belong to the spinner inside that dialog, which is a modal and is
                // resolved before any screen is asked.
                Screen::Inventory => [
                    Some(Action::CursorUp),
                    Some(Action::CursorDown),
                    None,
                    None,
                    None,
                    Some(Action::Compress),
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
                // **The one screen that answers nothing lateral**, deliberately: UI.md
                // §9 leaves `←/→` free here so the configurable sub-tab binding can own
                // it, and that binding is resolved in `keymap` — the only place the
                // config is visible — before this function is ever reached.
                //
                // `c` is claimed here *and* on the Inventory, to two different
                // actions: the walk out and the dialog it walks to. One letter for one
                // idea across the two screens §8.4's loop runs between — which this
                // table is the place to notice, since it is the only view of the
                // bindings laid side by side.
                Screen::Upgrades => [
                    Some(Action::CursorUp),
                    Some(Action::CursorDown),
                    None,
                    None,
                    Some(Action::Confirm),
                    Some(Action::GoCompress),
                    None,
                    Some(Action::BuyMax),
                    None,
                    None,
                    None,
                ],
                // **`A` is the Levels screen's alone, and `Home` is now shared with
                // Stats** — which is what these two columns are here to pin. `A` collects
                // a whole ladder of rewards and would be nonsense claimed globally;
                // `Home` means *back to where you actually are*, and the two screens that
                // have such a place both answer it. The column below is where that
                // sharing is visible, since it is the only view of the bindings laid
                // side by side.
                Screen::Levels => [
                    Some(Action::CursorUp),
                    Some(Action::CursorDown),
                    None,
                    None,
                    Some(Action::Confirm),
                    None,
                    None,
                    None,
                    Some(Action::JumpToCurrent),
                    Some(Action::ClaimAll),
                    None,
                ],
                // **The row this table was missing, and the bug it cost.** Stats used to
                // fall into the catch-all below, commented "phase 7's remaining screen"
                // — so the trou was *documented here* rather than detected, and `↑↓`
                // stayed unbound on a screen whose footer had been advertising them
                // since the panel was drawn. Every other link existed: the cursor, the
                // reducer's arm, the projection, the accented row. Only the decode was
                // absent, and nothing in the suite crossed that seam.
                //
                // `p` is not in `keys` above, so the prestige preview does not show
                // here; `keymap`'s own tests carry it.
                //
                // `Home` is the one column this screen shares with the Levels roadmap,
                // and it decodes to the *same* action: the log is capped at five hundred
                // entries, so "back to the newest" is the same need the roadmap's fifty
                // rungs raised.
                Screen::Stats => [
                    Some(Action::CursorUp),
                    Some(Action::CursorDown),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(Action::JumpToCurrent),
                    None,
                    None,
                ],
                // Mine takes only `Space`, which `keymap` resolves above this table, so
                // it is the one screen left in the catch-all. `None` is "not mine",
                // which lets `keymap` fall through instead of swallowing the key.
                _ => [const { None }; 11],
            }
        };
        for screen in Screen::ALL {
            for (code, expected) in keys.into_iter().zip(claimed(screen)) {
                let key = KeyEvent::new(code, KeyModifiers::NONE);
                assert_eq!(
                    screen.map_key(key),
                    expected,
                    "{screen:?} answered {code:?} differently from the table here"
                );
            }
        }
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
