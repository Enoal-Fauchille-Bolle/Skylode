//! The game's open balance constants, in one place.
//!
//! Every value here is one the design left *open* — marked "decided at
//! implementation time" in `docs/ROADMAP.md` — as opposed to the numbers that are
//! already settled and live with the thing they describe: the `MINE_SIZES` table,
//! `RAW_PER_DENSE_BLOCK`, the pickaxe tier curve. Those stay put. This module is
//! the home for the ones still in flux, so a later phase consumes a named constant
//! instead of re-inventing a magic number on the spot.
//!
//! Two rules keep this module from swelling into a junk drawer:
//!
//! - **Scalars and curve *parameters* only — never a price table.** The economy's
//!   cost is a curve, `cost(n) = base * growth^n`; what lives here is the two
//!   parameters, and the [`economy`](crate) module (phase 5) generates every price
//!   from them. A hundred prices are two numbers, not a hundred constants.
//! - **Anything keyed by an enum variant stays with its enum.** A value that is
//!   "one per [`PickaxeTier`](crate::pickaxe::PickaxeTier), per
//!   [`EnchantType`](crate::enchant::EnchantType), per [`World`](crate::world::World)"
//!   belongs in a table beside that type, where its variants are in scope — moving
//!   it here would only invert a dependency and scatter the enum's own truth.
//!
//! Most of these have no consumer yet: the phases that will read them (2, 5, 6, 7)
//! are not written. They are `pub` regardless — a `pub` item in a library is never
//! flagged `dead_code`, so the module can state the game's full tuning surface up
//! front without a litter of `#[expect(dead_code)]`.

use std::time::Duration;

// --- Progression (phase 6) ---

/// Mining level that unlocks the Nether.
///
/// The lower of the two world gates; paired with [`END_UNLOCK_LEVEL`] it is what
/// makes mining level, not just pickaxe tier, a real axis of progression.
pub const NETHER_UNLOCK_LEVEL: u32 = 15;

/// Mining level that unlocks the End.
///
/// Strictly above [`NETHER_UNLOCK_LEVEL`] and at or below [`LEVEL_CAP`]: the three
/// together must stay ordered, or a world would unlock after the level cap has
/// frozen the player short of it. The invariant is tested.
pub const END_UNLOCK_LEVEL: u32 = 30;

/// The highest mining level the player can reach.
///
/// `Player::add_experience` climbs without bound today; this is the ceiling phase 6
/// will clamp it to. Must stay at or above [`END_UNLOCK_LEVEL`] so the last world is
/// actually reachable.
pub const LEVEL_CAP: u32 = 50;

// --- Offline accrual (phase 7) ---

/// The most offline time the auto-miner is ever credited for at once.
///
/// A [`Duration`], not a raw seconds count, because that is what phase 7's
/// closed-form accrual clamps the injected `elapsed` against. It carries no clock
/// read of its own — the caller injects `now`, so the core stays deterministic.
pub const OFFLINE_CAP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

// --- Compression ---

/// How many raw items one Compressed unit is worth.
///
/// 100 is round enough that the player can do the arithmetic in their head: an
/// upgrade priced at 650 Iron is quoted as `6 Compressed Iron + 50 Iron`, which
/// reads better than the single large number. See `docs/DECISIONS.md`.
///
/// Not to be confused with `RAW_PER_DENSE_BLOCK` (9), which is what *mining* a
/// dense block yields. The two are unrelated ratios for unrelated things; see
/// [`Item`](crate::material::Item).
pub const RAW_PER_COMPRESSED: u32 = 100;

// --- Mine (phase 2) ---

/// Remaining-block count at which a mine refills its whole grid at once.
///
/// Zero: the batch reset fires only on a fully cleared mine. A higher threshold
/// would refill early and hand out blocks the player never broke — the same free
/// batch reset the richness dial is carefully built to forbid.
pub const BATCH_RESET_THRESHOLD: u32 = 0;

// --- Economy (phase 5) ---

/// Base term of the geometric cost curve `cost(n) = base * growth^n`.
///
/// Provisional; phase 10 balance sets the final value. Must stay above zero, or the
/// whole curve collapses to zero and nothing ever costs anything.
pub const COST_BASE: u32 = 10;

/// Growth factor of the geometric cost curve `cost(n) = base * growth^n`.
///
/// Provisional; phase 10 balance sets the final value. Must stay strictly above
/// `1.0`, or successive upgrades would cost the same or *less*, and the sink the
/// economy exists to be would leak.
pub const COST_GROWTH: f64 = 1.15;

#[cfg(test)]
mod tests {
    use super::*;

    // These verify at *compile time* — a `const` block over `const` operands: an
    // invariant broken by a future re-balance stops the crate compiling, not a
    // test run. `Duration` goes through `is_zero`, since its `>` is not a `const fn`.

    #[test]
    fn world_unlocks_are_ordered_and_within_the_cap() {
        const {
            assert!(NETHER_UNLOCK_LEVEL < END_UNLOCK_LEVEL);
            assert!(END_UNLOCK_LEVEL <= LEVEL_CAP);
        }
    }

    #[test]
    fn the_cost_curve_actually_grows() {
        const {
            assert!(COST_BASE > 0);
            assert!(COST_GROWTH > 1.0);
        }
    }

    #[test]
    fn the_compression_ratio_is_meaningful() {
        const { assert!(RAW_PER_COMPRESSED > 1) }
    }

    #[test]
    fn the_offline_cap_is_positive() {
        const { assert!(!OFFLINE_CAP.is_zero()) }
    }
}
