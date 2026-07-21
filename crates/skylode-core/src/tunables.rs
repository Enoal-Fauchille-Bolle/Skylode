//! The game's balance dials that are keyed by nothing.
//!
//! A number belongs here, or does not, by a two-step question. Both steps are
//! mechanical on purpose: a module whose boundary has to be *argued* has no
//! boundary, and this one drifts into a junk drawer the first time someone has to
//! guess.
//!
//! **1. Is it a dial, or a design fact?** A *dial* is a number the balance pass is
//! invited to turn — marked "decided at implementation time" in `docs/ROADMAP.md`.
//! A *design fact* is settled, and settled numbers live with the thing they
//! describe and never come here: `TICKS_PER_HARDNESS` (30) and
//! [`Block::hardness`](crate::block::Block::hardness) are Minecraft's values kept
//! 1:1, `RAW_PER_DENSE_BLOCK` (9) and [`FORTUNE_CAP`](crate::enchant) are settled
//! with their reasons in `docs/MECHANICS.md`. Filing one of those under a module
//! called *tunables* would not describe it — it would **invite someone to turn it**,
//! and the name would be the whole of their justification.
//!
//! **2. If it is a dial: is it keyed by an enum variant?** A value that is "one per
//! [`World`](crate::world::World), per [`PickaxeTier`](crate::pickaxe::PickaxeTier),
//! per [`EnchantType`](crate::enchant::EnchantType)" is a `match` on that enum, in
//! that enum's module — which is also the only shape that turns a *new* variant into
//! a compile error rather than a silent default. Re-exporting its cells as constants
//! here would buy nothing: each would be read exactly once, from the one `match` arm
//! that a jump between two files now stands in front of. Keyed by nothing, and it
//! lives here.
//!
//! Step 2 means this module is **not** the whole tuning surface, so the dials it
//! does not hold are named below rather than left to be found. Whatever step 1
//! rejects is out of scope for balance entirely and is deliberately absent from both
//! lists.
//!
//! ## The dials that live with their enum
//!
//! - [`World::enchant_cap`](crate::world::World::enchant_cap) — the five special
//!   enchants' shared ceiling, per dimension (3 / 6 / 10). Its **order** is load-
//!   bearing where the values are not; see the method.
//! - [`PickaxeTier::efficiency_cap`](crate::pickaxe::PickaxeTier::efficiency_cap) —
//!   Efficiency's ceiling (5, or 15 at Netherite).
//! - [`PickaxeTier::base_power`](crate::pickaxe::PickaxeTier::base_power) — the tier
//!   curve. Strictly monotone, and that much is settled.
//! - `mine::MINE_SIZES` — the mine-size ladder. Keyed by a size level rather than a
//!   variant, but the same reasoning puts it beside the type that walks it.
//!
//! ## Everything else
//!
//! Most of the constants below have no consumer yet: the phases that will read them
//! (5, 6, 7) are not written. They are `pub` regardless — a `pub` item in a library
//! is never flagged `dead_code`, so this module can state its side of the surface up
//! front without a litter of `#[expect(dead_code)]`.
//!
//! One shape rule survives from step 2, for the keyed-by-nothing half: **curve
//! *parameters*, never a price table.** The economy's cost is a curve,
//! `cost(n) = base * growth^n`; what lives here is the two parameters, and the
//! `economy` module (phase 5) generates every price from them. A hundred prices are
//! two numbers, not a hundred constants.

use std::time::Duration;

// --- Enchants (phase 3) ---

/// How much of the pickaxe's speed one level of
/// [`Haste`](crate::enchant::EnchantType::Haste) adds back: level `n` multiplies
/// mining power by `1 + HASTE_PER_LEVEL * n`.
///
/// A curve parameter, like [`COST_GROWTH`] — the factor, not a table of factors —
/// which is why it lives here and not beside [`EnchantType`](crate::enchant::EnchantType)
/// under this module's second rule. That rule is about values that are *one per
/// variant*, and Haste's **cap** is exactly that: it is
/// [`World::enchant_cap`](crate::world::World::enchant_cap), shared with the four
/// other special enchants. The factor is keyed by nothing and needs no variant in
/// scope.
///
/// **Bounded above, and not only below.** The obvious invariant is `> 0` (see the
/// test), but this factor also has a ceiling it must not cross, and the ceiling is
/// the one that is easy to raise by accident. Together with
/// [`World::enchant_cap`](crate::world::World::enchant_cap) it fixes the strongest
/// pickaxe *permanent upgrades alone* can build: Netherite at Efficiency 15 is 235,
/// and the End's cap of 10 takes it to `235 * (1 + 0.2 * 10) = 705`. That must stay
/// **below** Ancient Debris' instamine threshold of `30 * 30 = 900`, because
/// clearing the two hardest blocks is the temporary Redstone boost's job and its
/// only reason to exist. Raise this to `0.3` and the same pickaxe reaches 940,
/// taking Ancient Debris permanently and leaving the boost with nothing to buy;
/// `mine`'s `a_hasted_netherite_instamines_the_dense_blocks_but_not_the_obsidian`
/// is what fails when that happens, and it is the test to read before touching this
/// number.
///
/// **Linear, where Efficiency's `level² + 1` is quadratic**, and that asymmetry is
/// the design, not an open question. The two act on different layers — Efficiency
/// adds into the sum, Haste scales it — so their curves compound rather than add. A
/// second quadratic against the first would grow with the fourth power of
/// investment, clear the hardness table's top end in a few levels, and leave
/// Efficiency — the lever the whole upgrade path is built to climb — a rounding
/// error beside it. Haste is worth buying because it multiplies what Efficiency
/// built, not because it out-races it.
///
/// Provisional; phase 10 balance sets the final value. Must stay strictly above
/// zero: at `0.0` the enchant is sold for nothing, and below it a level the player
/// *paid for* would make them slower. The shape is settled even though the number
/// is not.
pub const HASTE_PER_LEVEL: f32 = 0.2;

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

// --- Boosts (phase 5) ---

/// What a temporary Redstone [`Boost`](crate::boost::Boost) multiplies mining
/// power by while it runs.
///
/// Provisional; phase 10 sets the final value. Unlike most dials here it has a
/// **floor the design fixes rather than balance**: the boost exists to reach the
/// two blocks no permanent upgrade can. A maxed pickaxe is worth 705, so anything
/// at or below `1500 / 705 ≈ 2.13` leaves Obsidian unreachable and the boost with
/// no job — which is the mirror of the ceiling
/// [`HASTE_PER_LEVEL`] is held under for the same reason. Must
/// stay above `1.0`, or a "boost" would slow the player down.
pub const BOOST_MULTIPLIER: f32 = 2.5;

/// How long a bought Redstone boost runs, in phase-7 ticks.
///
/// Ticks rather than a [`Duration`] because the tick loop is what counts it down,
/// and a wall-clock read inside the core would break determinism. At the fixed
/// 20 tps this is 30 seconds. Provisional; must stay above zero, or a boost would
/// lapse on the tick it was bought.
pub const BOOST_DURATION_TICKS: u32 = 600;

/// The raw Redstone one boost costs.
///
/// **Flat, not a step on [`COST_BASE`]'s curve.** The geometric curve is indexed by
/// *how far up a permanent ladder* the player already is; a boost is a consumable
/// with no level held, so there is no `n` to read it at, and pricing it off the
/// count already bought would make it dearer with no design reason. 500 raw quotes
/// as `5 Compressed Redstone`, which is what the player sees. Provisional.
pub const BOOST_COST: u32 = 500;

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

    /// A Haste level the player bought must be worth something. At `0.0` the
    /// enchant multiplies by 1 and is sold for nothing; negative, it is an upgrade
    /// that slows the pickaxe down.
    #[test]
    fn a_level_of_haste_is_always_worth_buying() {
        const { assert!(HASTE_PER_LEVEL > 0.0) }
    }

    /// A boost must speed the player up and must last long enough to be used. The
    /// *design* floor — high enough to reach Obsidian — is not asserted here, since
    /// it is a claim about the hardness table and the tier curve, not about this
    /// number alone; `mine` pins it against the real threshold instead.
    #[test]
    fn a_boost_is_faster_than_no_boost_and_lasts_a_while() {
        const {
            assert!(BOOST_MULTIPLIER > 1.0);
            assert!(BOOST_DURATION_TICKS > 0);
            assert!(BOOST_COST > 0);
        }
    }
}
