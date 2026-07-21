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
//! ## Why this module holds no *active* boost
//!
//! There is deliberately no `ActiveBoosts` collection here, and no stacking rule.
//! A boost is **produced** by [`buy_boost`](crate::economy::buy_boost) today and by
//! phase 6's level-up reward later, and what happens when a second one arrives
//! while the first still runs — refresh the timer, extend it, multiply the two —
//! is a question about a *collection* that phase 7 owns. Answering it here would
//! mean inventing that collection a phase early, and the type below constrains
//! none of the three answers. This is the same split phases 3 and 4 took with
//! `can_mine`, `fortune_multiplier` and `resolve_excavator`: ship the primitive,
//! let the phase that owns the composition compose.
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
    /// **`pub(crate)`, and that is a gameplay guard, not caution.** A boost is
    /// *bought* — with Redstone, through [`buy_boost`](crate::economy::buy_boost) —
    /// or granted by phase 6's level-up reward. A public constructor would let a
    /// front-end mint the game's strongest speed multiplier for free, which is the
    /// same reason [`Mine::take`](crate::mine::Mine::take) and
    /// [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe::upgrade) are not public
    /// either. It carries no `#[expect(dead_code)]` because it already has a live
    /// caller in `economy`.
    ///
    /// **No `Result`, and no validation of `multiplier`.** A boost below `1.0`
    /// would be a *slow*, and a non-finite one would poison every later
    /// `break_progress` — but neither can be constructed, because the only two call
    /// sites read [`BOOST_MULTIPLIER`](crate::tunables::BOOST_MULTIPLIER), whose
    /// invariants are asserted at *compile time* in `tunables`. Checking here would
    /// hand every caller an error case that no input can produce.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "awaiting the phase-7 tick, which activates a bought charge"
        )
    )]
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
    #[cfg_attr(not(test), expect(dead_code, reason = "awaiting the phase-7 tick"))]
    pub(crate) fn tick(&mut self) {
        self.remaining_ticks = self.remaining_ticks.saturating_sub(1);
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
}
