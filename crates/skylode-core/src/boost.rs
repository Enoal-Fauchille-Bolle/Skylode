//! Temporary mining-speed boosts: the half of the haste product the pickaxe does
//! not carry.
//!
//! `docs/MECHANICS.md` specifies mining power as
//! `(base_tier + efficiency_bonus) * haste_multiplier`, where the multiplier is a
//! **product of two sources**: the permanent Haste enchant, which
//! [`Pickaxe::mining_power`] already folds in, and a temporary Redstone boost,
//! which is this module. Phase 7's tick composes them where they actually meet —
//! `pickaxe.mining_power() * boost.multiplier()`.
//!
//! ## Why a boost is not a property of the pickaxe
//!
//! A [`Boost`] carries a countdown, so it is **run state**, not tool state. The
//! save serialises a pickaxe as an inert `(tier, enchants)` pair; a timer on it
//! would be a value that changes twenty times a second inside a struct that
//! otherwise changes only when the player buys something. `docs/SYSTEMS.md` files
//! the active boosts on the game state for that reason, and phase 7 owns that
//! aggregate.
//!
//! ## Why this module holds no *active* boost, and no reserve either
//!
//! Nothing here **produces** a `Boost`. A boost is *earned* as a **charge** — bought
//! from [`buy_boost`](crate::economy::buy_boost), or granted every fifth level-up —
//! and a charge is not a `Boost`: every boost in the game is identical, so one
//! sitting in reserve carries no information beyond *how many*. The reserve is
//! therefore a plain count on phase 7's game state, and the type below is what a
//! charge becomes when the player **fires** it.
//!
//! Splitting the two that way is what removes the stacking question rather than
//! answering it: two charges never overlap unless the player chooses to overlap
//! them, and what happens if they do — refresh the timer, extend it, multiply the
//! two — belongs to the phase that owns the collection. This is the same split
//! phases 3 and 4 took with `can_mine`, `fortune_multiplier` and
//! `resolve_excavator`: ship the primitive, let the phase that owns the composition
//! compose.
//!
//! [`Pickaxe::mining_power`]: crate::pickaxe::Pickaxe::mining_power

/// A temporary multiplier on mining speed, and how many ticks it has left.
///
/// The one job the design gives it is closing a gap permanent upgrades cannot:
/// Netherite at Efficiency 15 with Haste at the End's cap is worth 705, while
/// Obsidian needs 1500 and Ancient Debris 900 (`docs/MECHANICS.md`). A ceiling the
/// player cannot buy past is what leaves the boost a role at all, so
/// [`BOOST_MULTIPLIER`](crate::tunables::BOOST_MULTIPLIER) has a *floor* it must
/// clear, not just a value.
///
/// **The timer is in ticks, not a `Duration`.** What decrements it is phase 7's
/// fixed 20 tps step, and a wall-clock duration there would reintroduce exactly
/// the ambient clock read the core's determinism contract forbids. Ticks are also
/// what the save can carry without a unit conversion: a reloaded run resumes the
/// countdown on the integer it left off at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Boost {
    /// The factor mining power is multiplied by while this boost runs.
    multiplier: f32,
    /// Ticks left before it lapses; `0` means expired.
    remaining_ticks: u32,
}

impl Boost {
    /// A boost of `multiplier`, running for `ticks`.
    ///
    /// **`pub(crate)`, and that is a gameplay guard, not caution.** This is the
    /// *activation* step: the player spends a charge — one bought with Redstone
    /// through [`buy_boost`](crate::economy::buy_boost), or granted every fifth
    /// level-up — and gets back a boost that starts counting down. A public
    /// constructor would let a front-end mint the game's strongest speed multiplier
    /// for free, without holding a charge to spend, which is the same reason
    /// [`Mine::take`](crate::mine::Mine::take) and
    /// [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe::upgrade) are not public either.
    ///
    /// **No `Result`, and no validation of `multiplier`.** A boost below `1.0`
    /// would be a *slow*, and a non-finite one would poison every later
    /// `break_progress` — but neither can be constructed, because the only caller
    /// this is waiting on reads
    /// [`BOOST_MULTIPLIER`](crate::tunables::BOOST_MULTIPLIER), whose invariants are
    /// asserted at *compile time* in `tunables`. Checking here would hand every
    /// caller an error case that no input can produce. That caller now exists:
    /// [`GameState::fire_boost`](crate::game::GameState::fire_boost), the one door
    /// that charges a charge for the privilege.
    pub(crate) fn new(multiplier: f32, ticks: u32) -> Self {
        Self {
            multiplier,
            remaining_ticks: ticks,
        }
    }

    /// The factor to multiply mining power by — and **exactly `1.0` once the timer
    /// has run out**.
    ///
    /// That identity is the load-bearing part, and it mirrors the one
    /// [`haste_multiplier`](crate::pickaxe::Pickaxe) keeps at Haste 0: every break
    /// time this game inherits 1:1 from Minecraft is measured unboosted, so a
    /// lapsed boost that still multiplied would silently re-time the whole hardness
    /// table.
    ///
    /// Making expiry *structural* here rather than leaving it to the caller is what
    /// keeps that true. Phase 7 will sweep expired boosts off the game state, but a
    /// sweep that runs a tick late — or a `#[test]` that never sweeps at all — must
    /// not be able to hand out a multiplier the player no longer owns.
    pub fn multiplier(&self) -> f32 {
        if self.is_expired() {
            1.0
        } else {
            self.multiplier
        }
    }

    /// How many ticks the boost still has, for the HUD readout `docs/MECHANICS.md`
    /// puts beside the pickaxe's power.
    pub fn remaining_ticks(&self) -> u32 {
        self.remaining_ticks
    }

    /// Whether the boost has lapsed — the predicate phase 7's sweep drops it on.
    pub fn is_expired(&self) -> bool {
        self.remaining_ticks == 0
    }

    /// Burns one tick off the countdown.
    ///
    /// **`pub(crate)` for the reason [`Mine::dig`](crate::mine::Mine::dig) is: this
    /// is cadence.** Nothing in the core bounds how often a caller may invoke it,
    /// so a front-end that called it from its redraw loop would expire the player's
    /// boost at thirty frames a second instead of twenty ticks. Cadence belongs to
    /// the tick, and the tick is phase 7's.
    ///
    /// **`saturating_sub`, not `- 1`.** A `u32` at zero underflows: that panics in
    /// debug and *wraps to 4.29 billion* in release — a lapsed boost silently
    /// becoming a permanent one. It is the same class of bug the spatial enchants'
    /// `u8` coordinates had to clip for, and the same answer: make the boundary
    /// arithmetic total instead of trusting every caller to check first.
    pub(crate) fn tick(&mut self) {
        self.remaining_ticks = self.remaining_ticks.saturating_sub(1);
    }

    /// Adds `ticks` to the countdown: firing a charge onto a boost already running.
    ///
    /// **Charges stack by extending, and never by replacing.** Every boost in the
    /// game is identical, so there is no stronger one to swap in — the only thing a
    /// second charge can buy is more time. Refreshing the timer instead would let a
    /// player fire a charge with 25 of 30 seconds left and receive 5 seconds for it,
    /// which is a purchase that loses the buyer something.
    ///
    /// **Extending is not the same as refusing, and the difference is whose call it
    /// is.** A refusal in the core would make "fire while one runs" unrepresentable
    /// everywhere, front-end included; extending leaves the *rule* permissive and
    /// lets the interface put a confirmation in front of it — the same split
    /// `organization/UI-EN.md` §5.9 draws for the proc flash, where the core hands
    /// over the data and the wall-clock experience lives outside.
    ///
    /// `saturating_add` for [`tick`](Boost::tick)'s reason run the other way: a
    /// `u32` overflow would wrap a very long stack back to a very short boost, so a
    /// player who banked charges all run would fire them and get nothing.
    pub(crate) fn extend(&mut self, ticks: u32) {
        self.remaining_ticks = self.remaining_ticks.saturating_add(ticks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunables::{BOOST_DURATION_TICKS, BOOST_MULTIPLIER};

    /// A boost the player paid for must actually speed them up — the whole reason
    /// the tunable has a floor and not just a value.
    #[test]
    fn a_running_boost_multiplies_by_more_than_one() {
        let boost = Boost::new(BOOST_MULTIPLIER, BOOST_DURATION_TICKS);
        assert!(boost.multiplier() > 1.0);
        assert!(!boost.is_expired());
    }

    /// The load-bearing identity: a lapsed boost multiplies by *exactly* 1.0, so
    /// every unboosted break time stays the one Minecraft's hardness table implies.
    /// Asserted on a boost that still carries its factor, which is the case a
    /// late sweep would leak.
    #[test]
    fn an_expired_boost_multiplies_by_exactly_one() {
        let boost = Boost::new(BOOST_MULTIPLIER, 0);
        assert_eq!(boost.multiplier(), 1.0);
        assert!(boost.is_expired());
    }

    /// The countdown burns one tick per call, and lapses on the tick that takes it
    /// to zero — not one before, not one after.
    #[test]
    fn a_boost_lapses_on_the_tick_that_empties_it() {
        let mut boost = Boost::new(BOOST_MULTIPLIER, 2);

        boost.tick();
        assert_eq!(boost.remaining_ticks(), 1);
        assert!(!boost.is_expired(), "one tick left is still one tick");

        boost.tick();
        assert_eq!(boost.remaining_ticks(), 0);
        assert!(boost.is_expired());
    }

    /// Ticking past zero must stay at zero. Without the saturating subtraction this
    /// wraps to `u32::MAX` in a release build, turning a lapsed boost into a
    /// permanent one — a bug no debug-mode test run would ever show.
    #[test]
    fn ticking_an_expired_boost_stays_at_zero() {
        let mut boost = Boost::new(BOOST_MULTIPLIER, 0);
        boost.tick();
        boost.tick();
        assert_eq!(boost.remaining_ticks(), 0);
        assert_eq!(boost.multiplier(), 1.0);
    }

    /// A second charge **adds** its time rather than restarting the clock. Under a
    /// refresh, firing one with most of a boost still running would hand the player
    /// less than the charge is worth — a purchase that takes something away.
    #[test]
    fn a_second_charge_adds_its_time_instead_of_restarting_the_clock() {
        let mut boost = Boost::new(BOOST_MULTIPLIER, BOOST_DURATION_TICKS);
        boost.tick();

        boost.extend(BOOST_DURATION_TICKS);

        assert_eq!(boost.remaining_ticks(), 2 * BOOST_DURATION_TICKS - 1);
        assert_eq!(
            boost.multiplier(),
            BOOST_MULTIPLIER,
            "stacking must buy time, never a stronger multiplier"
        );
    }

    /// Extending past `u32::MAX` must stay at `u32::MAX`. Wrapping would turn a
    /// hoard of charges into a boost shorter than one of them — the release-only
    /// mirror of the underflow `tick` guards against.
    #[test]
    fn extending_past_the_counters_ceiling_stays_at_the_ceiling() {
        let mut boost = Boost::new(BOOST_MULTIPLIER, u32::MAX - 1);

        boost.extend(BOOST_DURATION_TICKS);

        assert_eq!(boost.remaining_ticks(), u32::MAX);
    }
}
