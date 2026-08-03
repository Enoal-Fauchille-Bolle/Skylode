//! The spatial proc flash: which cells a blast just covered, and how bright.
//!
//! `docs/UI.md` §7 settles the *why*: the shape **is** the reward. A Nuke's payout is
//! not the ore — that lands in a toast and in the inventory — it is two hundred cells
//! going at once, and a redraw that simply shows an empty grid one frame later has the
//! player read *"the mine is empty"* without ever seeing it happen. So the cells are
//! painted **before** they are erased, for two beats of ~100 ms, and then they are the
//! holes the model has said they were all along.
//!
//! **This is why the whole feature is front-end.** An animation is nothing *but* an
//! ambient clock, and an ambient clock is the one thing `skylode-core`'s determinism
//! contract keeps on the other side of the boundary. The core produced the geometry —
//! [`GameEvent::SpatialProc`] carries `cells`, the shape holes included — and stops
//! there: no timer, no animation state, and `tick()` stays a pure function of
//! `(state, input)`.
//!
//! ## What this module is, in one sentence
//!
//! A map from a grid cell to **the instant a blast last claimed it**, and a function
//! that turns that instant into a beat when someone is about to draw.
//!
//! ## Why a map, and not a queue of blasts
//!
//! §7's rule for two blasts inside one window is *"the newer overlay wins per cell — no
//! queue, no compositing rules; the last blast to claim a cell owns its colour"*. A
//! queue resolved at paint time would be a compositing rule, which is the thing that
//! sentence refuses. Writing the instant into every cell the blast covered makes
//! *last-write-wins* literal: [`insert`](BTreeMap::insert) is the whole of the rule.
//!
//! [`BTreeMap`] and not a [`HashMap`](std::collections::HashMap), for the reason the
//! save file gives for the same choice: a deterministic order makes the projection
//! assertable in a test. The cost is nothing — the map is bounded by the grid, at most
//! 200 entries.
//!
//! ## Why nothing is ever removed for being old
//!
//! [`Toasts`](crate::toast::Toasts)' rule, for [`Toasts`](crate::toast::Toasts)'
//! reason turned around. There the buffer keeps everything because the Stats history
//! reads it; here there is no second reader — but expiry still belongs in
//! [`resolve`](Flashes::resolve) rather than in a prune, because that keeps **one**
//! place that knows how long a flash lasts. A prune would be a second, and two places
//! that know the same duration are two places that can disagree about it. The map
//! cannot grow past the grid anyway, so there is nothing for a prune to bound.
//!
//! [`GameEvent::SpatialProc`]: skylode_core::game::GameEvent::SpatialProc

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use skylode_core::mine_kind::MineKind;

/// How long the first beat lasts — the cells painted solid in the blast colour.
///
/// **Derived from the redraw budget, not chosen.** `docs/UI.md` §10.1's three clocks
/// leave the real redraw rate at the simulation's 20 fps: every step raises `dirty`,
/// because the auto-miner credits on every one, so the 33 ms frame ceiling never binds.
/// A beat therefore lasts exactly `beat / 50 ms` frames, and **100 ms is two of them** —
/// the floor at which two beats can be distinguished at all. One frame per beat would
/// leave the second one to a single repaint, which a late pass could drop entirely, and
/// the animation would be a single flicker with no fade.
///
/// `the_first_beat_outlasts_two_redraws` pins that against the core's own tick rate, so
/// the constant cannot quietly drift away from the loop it was measured against. What
/// remains open is a *deliberate* retune upward — 150/300 was the alternative looked at
/// on a running grid — and changing this now fails that test rather than passing
/// unnoticed.
pub const BRIGHT: Duration = Duration::from_millis(100);

/// How long the flash lasts in total; the second beat is everything after [`BRIGHT`].
///
/// ~200 ms is `docs/UI.md` §7's *"long enough to register, short enough not to feel like
/// a cutscene"*, and the second half of it is the fade. Twice [`BRIGHT`] rather than an
/// independent number: two beats of equal length is what the stage table draws, and a
/// lopsided pair would need an argument this design does not have.
pub const TOTAL: Duration = Duration::from_millis(200);

/// Which beat of the flash a cell is on, when it is on one at all.
///
/// Two variants and no `Gone`: a cell past [`TOTAL`] is simply absent from
/// [`resolve`](Flashes::resolve)'s answer, which is the same shape the grid itself uses
/// for a hole. An `Option` at the edge beats a third variant every caller would have to
/// remember means "draw nothing".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlashStage {
    /// 0..[`BRIGHT`] — the cells painted solid in the blast colour, still *there*.
    Bright,
    /// [`BRIGHT`]..[`TOTAL`] — the same cells at half the ink, becoming the hole they
    /// already are.
    Fading,
}

/// The cells a blast has claimed, and when.
///
/// **Stamped with the mine the coordinates are about**, which is the one piece of
/// bookkeeping that is not obvious. A cell is a bare `(u8, u8)` and says nothing about
/// which of the twelve grids it indexes, so a flash left running across a change of mine
/// would paint a blast that never happened onto somewhere the player has just walked
/// into. Clearing the buffer at every site that changes mine —
/// [`enter_selected_mine`](crate::app::App), a prestige, whatever phase 8 adds — would
/// work until the first site that forgot. Carrying the answer *inside* the buffer means
/// no site has to remember: [`push`](Flashes::push) clears on a change and
/// [`resolve`](Flashes::resolve) refuses a mine that does not match. Total by
/// construction, the same move [`Mine::grid`] makes by fusing its hole mask into an
/// `Option<Block>`.
///
/// [`None`] until the first blast, so an empty buffer is about no mine at all rather
/// than about whichever one a constructor happened to be handed.
///
/// [`Mine::grid`]: skylode_core::mine::Mine
#[derive(Clone, Debug, Default)]
pub struct Flashes {
    /// Which grid the coordinates below index into.
    mine: Option<MineKind>,
    /// Cell to the instant a blast last covered it. Last write wins, literally.
    cells: BTreeMap<(u8, u8), Instant>,
}

impl Flashes {
    /// A buffer with nothing flashing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that a blast covered `cells` of `mine` at `now`.
    ///
    /// **Every cell of the shape, holes included**, because that is what the core sends
    /// and what §7 asks to be drawn: a blast the player watches must look like a blast,
    /// not like the four cells that happened to still be standing.
    ///
    /// `now` is a parameter for the reason every clock in this crate is one — the caller
    /// is [`App::advance`](crate::app::App), which already holds the instant its
    /// simulation steps ran against, and a second reading here could disagree with it.
    /// Three procs can fire in one swing, and they arrive stamped with that same instant;
    /// the last one pushed owns any cell they share, which is the pinned `PROC_ORDER`
    /// deciding a question the player cannot see, since all three paint one colour.
    pub fn push(&mut self, mine: MineKind, cells: &[(u8, u8)], now: Instant) {
        if self.mine != Some(mine) {
            self.cells.clear();
            self.mine = Some(mine);
        }
        for &cell in cells {
            self.cells.insert(cell, now);
        }
    }

    /// Which beat each still-flashing cell of `mine` is on, at `now`.
    ///
    /// **This is where a flash expires, and expiring is not something that happens to
    /// the buffer** — the module header's argument, and [`Toasts::render`]'s precedent.
    /// The comparison lives here, so the same instant answers "is this still showing"
    /// without anything being thrown away and without a second constant to keep in step.
    ///
    /// A mine that is not the stamped one yields nothing rather than an error: this is
    /// read on the way to a frame, and a renderer that could refuse to draw is worse
    /// than one that draws slightly less.
    ///
    /// [`Toasts::render`]: crate::toast::Toasts::render
    pub fn resolve(&self, mine: MineKind, now: Instant) -> BTreeMap<(u8, u8), FlashStage> {
        if self.mine != Some(mine) {
            return BTreeMap::new();
        }
        self.cells
            .iter()
            .filter_map(|(&cell, &at)| {
                // `saturating_duration_since` and not the subtraction: an instant in the
                // future is unreachable through the loop, but it panics rather than
                // reporting anything useful, and a panic is never how a frame should
                // report that two clocks disagreed by a microsecond.
                stage(now.saturating_duration_since(at)).map(|stage| (cell, stage))
            })
            .collect()
    }
}

/// The beat a cell is on `elapsed` after the blast claimed it, or [`None`] once it is
/// over.
///
/// Half-open bands, both of them: exactly [`BRIGHT`] is already the fade, exactly
/// [`TOTAL`] is already gone. Which side a boundary falls on matters more here than the
/// arithmetic suggests — at two frames a beat, a boundary drawn the other way would give
/// the first beat three frames on a run whose ticks happen to land on it and two on one
/// whose do not.
///
/// A free function rather than a method: it is about a duration and knows nothing about
/// a buffer, which is also what lets the boundary tests name it directly.
fn stage(elapsed: Duration) -> Option<FlashStage> {
    if elapsed < BRIGHT {
        Some(FlashStage::Bright)
    } else if elapsed < TOTAL {
        Some(FlashStage::Fading)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use skylode_core::tunables::TICKS_PER_SECOND;

    use super::*;

    /// A blast shape, small enough to write out and big enough to overlap another.
    const SQUARE: [(u8, u8); 4] = [(1, 1), (1, 2), (2, 1), (2, 2)];

    #[test]
    fn a_fresh_buffer_is_about_no_mine_at_all() {
        let flashes = Flashes::new();
        assert_eq!(flashes.mine, None);
        assert!(flashes.resolve(MineKind::Iron, Instant::now()).is_empty());
    }

    #[test]
    fn the_two_beats_meet_at_the_bright_boundary() {
        // The bands are half-open on both edges, which is what keeps a beat exactly two
        // frames wide however the tick happens to land on it.
        assert_eq!(stage(Duration::ZERO), Some(FlashStage::Bright));
        assert_eq!(
            stage(BRIGHT - Duration::from_millis(1)),
            Some(FlashStage::Bright)
        );
        assert_eq!(stage(BRIGHT), Some(FlashStage::Fading));
        assert_eq!(
            stage(TOTAL - Duration::from_millis(1)),
            Some(FlashStage::Fading)
        );
        assert_eq!(stage(TOTAL), None);
        assert_eq!(stage(Duration::from_secs(60)), None);
    }

    /// The constant that is *derived* rather than chosen, asserted against the rate it
    /// was derived from.
    ///
    /// Every simulation step raises `dirty`, so the redraw rate is the simulation's, and
    /// a beat has to outlast two of them or the fade gets a single repaint that a late
    /// pass can drop. Read off `TICKS_PER_SECOND` and not off a literal `50`: the front
    /// end already refuses to write the tick rate down twice (`SIM_PERIOD` derives it
    /// too), and a balance pass that changed it must move this number or fail here.
    #[test]
    fn the_first_beat_outlasts_two_redraws() {
        let redraw = Duration::from_nanos(1_000_000_000 / TICKS_PER_SECOND);
        assert!(
            BRIGHT >= 2 * redraw,
            "a beat of {BRIGHT:?} is under two redraws of {redraw:?}"
        );
        // Two beats of equal length is what the stage table draws.
        assert_eq!(TOTAL, 2 * BRIGHT);
    }

    #[test]
    fn a_blast_paints_every_cell_of_its_shape_holes_included() {
        let now = Instant::now();
        let mut flashes = Flashes::new();
        flashes.push(MineKind::Iron, &SQUARE, now);

        let painted = flashes.resolve(MineKind::Iron, now);
        assert_eq!(painted.len(), SQUARE.len());
        for cell in SQUARE {
            assert_eq!(painted.get(&cell), Some(&FlashStage::Bright), "{cell:?}");
        }
    }

    /// **The rule §7 states and this module exists to make literal**: two blasts inside
    /// one window, and the newer one owns the cells they share.
    ///
    /// The assertion that matters is the *mixed* frame — one resolve carrying both beats
    /// at once. That is the only picture that proves the rule is per cell rather than
    /// per flash, which a queue keyed on the newest blast would also have passed.
    #[test]
    fn the_last_blast_owns_a_shared_cell_and_the_others_keep_their_own_beat() {
        let first = Instant::now();
        let second = first + Duration::from_millis(120);
        let mut flashes = Flashes::new();

        flashes.push(MineKind::Iron, &SQUARE, first);
        // Overlaps the square at (2, 2) only.
        flashes.push(MineKind::Iron, &[(2, 2), (3, 3)], second);

        let painted = flashes.resolve(MineKind::Iron, second);
        // 120 ms on: the first blast's own cells are fading…
        assert_eq!(painted.get(&(1, 1)), Some(&FlashStage::Fading));
        // …the shared one was re-stamped and is bright again…
        assert_eq!(painted.get(&(2, 2)), Some(&FlashStage::Bright));
        // …and the second blast's own cell is bright with it.
        assert_eq!(painted.get(&(3, 3)), Some(&FlashStage::Bright));
    }

    /// An expired entry leaves the frame and stays in the buffer — the toast's rule,
    /// which here is about having exactly one place that knows how long a flash lasts.
    #[test]
    fn an_expired_cell_leaves_the_frame_and_stays_in_the_buffer() {
        let now = Instant::now();
        let mut flashes = Flashes::new();
        flashes.push(MineKind::Iron, &SQUARE, now);

        assert!(flashes.resolve(MineKind::Iron, now + TOTAL).is_empty());
        assert_eq!(flashes.cells.len(), SQUARE.len());
    }

    /// A flash never crosses a change of mine, and the buffer is what refuses — not the
    /// several call sites that change one.
    #[test]
    fn a_flash_does_not_paint_a_mine_it_did_not_happen_in() {
        let now = Instant::now();
        let mut flashes = Flashes::new();
        flashes.push(MineKind::Iron, &SQUARE, now);

        assert!(flashes.resolve(MineKind::Gold, now).is_empty());
        // And it is still there for the mine it *did* happen in: refusing is a question
        // asked at the draw, not an edit made to the buffer.
        assert!(!flashes.resolve(MineKind::Iron, now).is_empty());
    }

    #[test]
    fn a_blast_in_a_new_mine_clears_what_the_old_one_left() {
        let now = Instant::now();
        let mut flashes = Flashes::new();
        flashes.push(MineKind::Iron, &SQUARE, now);
        flashes.push(MineKind::Gold, &[(0, 0)], now);

        let painted = flashes.resolve(MineKind::Gold, now);
        assert_eq!(painted.len(), 1, "the Iron cells survived the walk");
        assert_eq!(painted.get(&(0, 0)), Some(&FlashStage::Bright));
    }

    /// A `now` before the instant a cell was stamped with saturates instead of panicking.
    ///
    /// Unreachable through the loop — `advance` stamps with an instant the following
    /// frame is drawn after — but `Instant` subtraction panics on the reversal, and this
    /// crate forbids a panic in a path a frame passes through.
    #[test]
    fn an_instant_from_before_the_blast_does_not_panic() {
        let now = Instant::now();
        let mut flashes = Flashes::new();
        flashes.push(MineKind::Iron, &SQUARE, now);

        let painted = flashes.resolve(MineKind::Iron, now - Duration::from_secs(1));
        assert_eq!(painted.get(&(1, 1)), Some(&FlashStage::Bright));
    }
}
