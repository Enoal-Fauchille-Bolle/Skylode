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
//! 1:1, `RAW_PER_DENSE_BLOCK` (9) and Efficiency's `level² + 1` are settled
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
//! - [`World::unlock_level`](crate::world::World::unlock_level) — the mining level
//!   each dimension opens at. The **one entry on this list that is a `match` over
//!   constants declared below**, and deliberately so: the two thresholds are dials
//!   ([`NETHER_UNLOCK_LEVEL`], [`END_UNLOCK_LEVEL`]) whose *ordering* is asserted at
//!   compile time here, where both are in scope, while the lookup keyed by the
//!   variant belongs with the enum — so that a fourth dimension is a compile error
//!   rather than a world unlocking at level 0. Splitting them buys both guarantees;
//!   neither half alone gives the other.
//!
//! ## Everything else
//!
//! Most of the constants below have no consumer yet: the phases that will read them
//! (5, 6, 7) are not written. They are `pub` regardless — a `pub` item in a library
//! is never flagged `dead_code`, so this module can state its side of the surface up
//! front without a litter of `#[expect(dead_code)]`.
//!
//! One shape rule survives from step 2, for the keyed-by-nothing half: **curve
//! *parameters*, never a price table.** Every price in the economy is a step on some
//! curve `cost(n) = base * growth^n`; what lives here is the *parameters*, and the
//! `economy` module generates the prices from them. Several hundred prices are a
//! handful of numbers, not several hundred constants.
//!
//! There is more than one such curve — one per upgrade track — and that is not a
//! breach of the rule but the reason it needs restating. A slope compounds over
//! however many steps a track has, and it is only meaningful against **that track's**
//! production growth, so a nine-step track and a fifteen-step one cannot share one
//! number without the longer one running away. What the rule forbids is a *table of
//! prices*; four tracks with four slopes is still four numbers generating everything.
//! Which curve a track reads is the track's own business, expressed in `economy` by
//! the helper it calls.

use std::time::Duration;

// --- Enchants (phase 3) ---

/// How much of the pickaxe's speed one level of
/// [`Haste`](crate::enchant::EnchantType::Haste) adds back: level `n` multiplies
/// mining power by `1 + HASTE_PER_LEVEL * n`.
///
/// A curve parameter, like the economy's `*_COST_GROWTH` slopes — the factor, not a
/// table of factors —
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
/// makes mining level, not just pickaxe tier, a real axis of progression. Read
/// through [`World::unlock_level`](crate::world::World::unlock_level), never
/// compared against by hand.
pub const NETHER_UNLOCK_LEVEL: u32 = 15;

/// Mining level that unlocks the End.
///
/// Strictly above [`NETHER_UNLOCK_LEVEL`] and at or below [`LEVEL_CAP`]: the three
/// together must stay ordered, or a world would unlock after the level cap has
/// frozen the player short of it. The invariant is tested here over the two
/// constants, and again in [`world`](crate::world) over the whole enum — which is
/// where a *fourth* dimension slotted in out of order would be caught.
pub const END_UNLOCK_LEVEL: u32 = 30;

/// The highest mining level the player can reach.
///
/// Enforced in exactly one place,
/// [`Player::xp_for_level`](crate::player::Player::xp_for_level), which stops quoting
/// a price past this level; every loop that walks the curve then terminates on the
/// missing price rather than on a guard of its own. Must stay at or above
/// [`END_UNLOCK_LEVEL`] so the last world is actually reachable.
pub const LEVEL_CAP: u32 = 50;

// --- Level-up rewards (phase 6) ---

/// Raw items a level-up's ore bundle is worth **per level**: the whole budget is
/// `LEVEL_REWARD_BASE * level`.
///
/// **Linear, and emphatically not on the [cost curve](crate::economy).** That curve is
/// indexed by a *track's step* — 0 to 15 at the very most — and reading it at a mining
/// level instead would run its exponent to 50: the enchant slope at step 50 is six
/// million raw, where the dearest single purchase in the game costs 16 527. The reward
/// would stop being an opening hand and become the economy.
///
/// At this value the bundles total ~3 % of everything a run must buy, and the two
/// erasures compose: against prices the reward falls from 20 % of the first purchase to
/// 3 % of the last, and against production from a full grid to 8 % of one. That fade is
/// the intent. Should the reward ever need to be a standing income instead, the fix is
/// a geometric curve of about 1.11 — not a bigger number here, which only lifts the
/// early game.
pub const LEVEL_REWARD_BASE: u32 = 10;

/// How often a level-up hands over a boost charge: every fifth level, ten times in a
/// run, **including** the two world levels.
///
/// A charge is not a running boost — it is held until the player fires it — so it
/// announces nothing a world unlock could dilute, which is why it ignores the
/// payout's exclusive rule. It is also what makes crossing several levels at once, on
/// a lump of offline experience, safe: charges accumulate instead of burning down in a
/// window nobody is watching.
///
/// Must stay strictly above zero, and the failure it guards against is a *quiet* one:
/// `u32::is_multiple_of(0)` is true only of zero itself, so a cadence of zero does not
/// crash — it simply stops the charge landing, at every level, forever. A number that
/// deletes a mechanic without raising anything is exactly what a compile-time
/// assertion is for.
pub const LEVEL_REWARD_BOOST_EVERY: u32 = 5;

/// How often a level-up adds an Emerald line: every third level, and never on the two
/// world levels — Emerald is ore, so it obeys the payout rule the charge escapes.
///
/// Emerald earns a rhythm of its own because
/// [`Fortune`](crate::enchant::EnchantType::Fortune) is the one permanent purchase
/// whose currency stops being mined once the Overworld is behind the player.
///
/// Must stay strictly above zero, for the reason [`LEVEL_REWARD_BOOST_EVERY`] must.
pub const LEVEL_REWARD_EMERALD_EVERY: u32 = 3;

/// What the Emerald line is worth, in permille of the bundle's budget — and it is paid
/// **on top of** that budget, not carved out of it.
///
/// On top, because the point is that those levels are *visibly better*, not differently
/// split: a share taken out of the budget would move the same total around and leave
/// the player unable to tell a third level from any other.
pub const LEVEL_REWARD_EMERALD_PERMILLE: u32 = 250;

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

/// Base term of the geometric cost curve `cost(n) = base * growth^n`, shared by
/// every track except the enchant ladder ([`ENCHANT_COST_BASE`]).
///
/// **The base governs the early game; the slope governs the late one.** This is the
/// single most useful thing to know before turning either, and it follows from the
/// curve itself: step zero costs `base * growth^0`, which is the base *whatever the
/// slope*. So raising a `*_GROWTH` to make the game harder leaves the opening
/// untouched and inflates only the endgame — and raising the base lifts the whole
/// curve without changing how steeply it climbs. A balance pass that wants "a harder
/// start" reaches here; one that wants "a longer endgame" reaches for a slope.
///
/// Also the unit the boost is quoted in ([`BOOST_COST`]), so a re-balance moves the
/// consumable with the ladders instead of leaving it behind.
///
/// Provisional; phase 10 balance sets the final value. Must stay above zero, or the
/// whole curve collapses to zero and nothing ever costs anything.
pub const COST_BASE: u32 = 100;

/// Growth factor of the **mine size** track's cost curve.
///
/// The steepest slope in the game, and deliberately so: size is the track that
/// multiplies production. Its nine steps take a mine from 9 cells to 200 — a factor
/// of **22** — so a slope whose nine steps climb by less than that makes the track
/// *easier* to fund the further it is climbed, and the sink leaks exactly where it
/// should bite. At `1.55` the price climbs by 33 across the track, which keeps every
/// step costing between one and two and a half full grids instead of a fifth of one.
///
/// Provisional; phase 10 balance sets the final value. Must stay strictly above
/// `1.0`, or successive upgrades would cost the same or *less*.
pub const SIZE_COST_GROWTH: f64 = 1.55;

/// Growth factor of the **mine richness** track's cost curve.
///
/// Gentler than [`SIZE_COST_GROWTH`] because it is measured against a smaller gain:
/// richness multiplies a same-material mine's yield by 4.6 across its ten rungs,
/// where size multiplies cell count by 22. Pricing the two on one slope would make
/// whichever of them was mis-matched either free or unbuyable — the reason this crate
/// carries a slope per track rather than the single `COST_GROWTH` it once had.
///
/// Provisional; phase 10 balance sets the final value. Must stay strictly above `1.0`.
pub const RICHNESS_COST_GROWTH: f64 = 1.35;

/// Growth factor shared by the two **pickaxe** tracks: Efficiency within a tier, and
/// the tier jump itself.
///
/// The one slope that must survive a **fifteen-step** track (Netherite's Efficiency
/// climb) as well as a five-step one, which is what holds it below the size slope. At
/// `1.55` the fifteenth step alone would cost 643 full Obsidian grids — over five
/// hours of mining for a single Efficiency level — because a slope compounds over
/// however many steps a track has, and two tracks of different lengths cannot share
/// one without the longer one running away.
///
/// Provisional; phase 10 balance sets the final value. Must stay strictly above `1.0`.
pub const UPGRADE_COST_GROWTH: f64 = 1.45;

/// Base term of the **enchant** ladder's cost curve — an order of magnitude above
/// [`COST_BASE`], and paired with the game's gentlest slope.
///
/// **A high base with a low slope is what spreads a budget across the worlds.** An
/// enchant's ten levels are split 3 / 3 / 4 between the Overworld, the Nether and the
/// End, so with a steep slope the Overworld's three levels are a rounding error —
/// 2.3 % of the enchant's lifetime cost — and the ores that fuel them are never
/// really demanded. Since the base governs the early steps (see [`COST_BASE`]),
/// flattening the curve and lifting its floor moves the Overworld's share to 11.5 %
/// without making the End's levels cheap in absolute terms. This is the whole reason
/// the enchant ladder does not read off [`COST_BASE`] like everything else.
///
/// Provisional; phase 10 balance sets the final value. Must stay above zero.
pub const ENCHANT_COST_BASE: u32 = 1_000;

/// Growth factor of the **enchant** ladder's cost curve — the gentlest in the game,
/// for the reason [`ENCHANT_COST_BASE`] is the highest.
///
/// Provisional; phase 10 balance sets the final value. Must stay strictly above `1.0`.
pub const ENCHANT_COST_GROWTH: f64 = 1.25;

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

/// The raw Redstone one boost costs: three times [`COST_BASE`].
///
/// **Flat on the curve, but quoted in its base — the two are not the same claim.**
/// Flat means there is no `n`: the geometric curve is indexed by how far up a
/// permanent ladder the player already is, and a consumable holds no level, so
/// pricing it off the count already bought would make it dearer for no design reason.
/// That is unchanged. What the multiple of [`COST_BASE`] adds is a *unit*: the boost
/// is worth three opening upgrades, and it stays worth three of them after a
/// re-balance instead of drifting into irrelevance while every ladder around it
/// moves. A hard-coded number surviving a re-balance of the curve is precisely how
/// this economy came to have a boost that cost fifty opening upgrades.
///
/// 300 raw quotes as `3 Compressed Redstone`, which is what the player sees.
/// Provisional.
pub const BOOST_COST: u32 = 3 * COST_BASE;

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

    /// The two reward cadences divide the level in
    /// [`reward_for_level`](crate::reward::reward_for_level). A zero there is not a
    /// mis-balanced game but a **deleted** one: `is_multiple_of(0)` holds only of zero,
    /// so every level would silently stop granting its garnish, with nothing raised and
    /// nothing to notice. The budget is asserted alongside for the same reason — at zero
    /// every bundle is empty and every line dropped.
    #[test]
    fn the_reward_cadences_can_be_divided_by() {
        const {
            assert!(LEVEL_REWARD_BASE > 0);
            assert!(LEVEL_REWARD_BOOST_EVERY > 0);
            assert!(LEVEL_REWARD_EMERALD_EVERY > 0);
        }
    }

    /// Every curve in the economy must actually be a *growing* curve: a base above
    /// zero, and a slope strictly above one. A slope at or below `1.0` makes each
    /// step cost the same or *less* than the last, so the sink the economy exists to
    /// be leaks — and it leaks silently, since every price still looks plausible.
    ///
    /// Walked over all four slopes rather than one, because they are independent
    /// dials now: a re-balance that flattened only the richness track would slip past
    /// an assertion that named the size track.
    #[test]
    fn every_cost_curve_actually_grows() {
        const {
            assert!(COST_BASE > 0);
            assert!(ENCHANT_COST_BASE > 0);
            assert!(SIZE_COST_GROWTH > 1.0);
            assert!(RICHNESS_COST_GROWTH > 1.0);
            assert!(UPGRADE_COST_GROWTH > 1.0);
            assert!(ENCHANT_COST_GROWTH > 1.0);
        }
    }

    /// The size track is the steepest, and that ordering is the design rather than an
    /// accident of tuning: size is what multiplies *production* (9 cells to 200
    /// across its nine steps), so its price has to climb faster than any track that
    /// multiplies less. If a re-balance ever puts richness or the pickaxe tracks
    /// above it, the mine that grows fastest also becomes the cheapest to grow.
    #[test]
    fn the_size_track_is_the_steepest() {
        const {
            assert!(SIZE_COST_GROWTH > RICHNESS_COST_GROWTH);
            assert!(SIZE_COST_GROWTH > UPGRADE_COST_GROWTH);
            assert!(SIZE_COST_GROWTH > ENCHANT_COST_GROWTH);
        }
    }

    /// The enchant ladder trades slope for base, and both halves of that trade have
    /// to hold or it buys nothing. A high base with a *steep* slope would be the
    /// worst of both; a low base with a gentle one would make the whole ladder cheap.
    /// The pairing is what spreads an enchant's budget across the three worlds
    /// instead of concentrating it in the End — see [`ENCHANT_COST_BASE`].
    #[test]
    fn the_enchant_ladder_trades_slope_for_base() {
        const {
            assert!(ENCHANT_COST_BASE > COST_BASE);
            assert!(ENCHANT_COST_GROWTH < UPGRADE_COST_GROWTH);
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
