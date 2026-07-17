//! Pickaxe enchantments.
//!
//! Enchantments modify how a [`Pickaxe`](crate::pickaxe::Pickaxe) mines. This
//! module provides:
//! - [`EnchantType`]: the kind of enchantment (Efficiency, Fortune, …) plus the
//!   dispatch to its level cap, which is keyed by the pickaxe tier, by the world,
//!   or by nothing, depending on the enchant.
//! - [`Enchants`]: a compact per-pickaxe store mapping each active enchantment
//!   to its current level.
//! - [`Enchant`]: a standalone `(type, level)` pair used when an enchantment
//!   needs to be passed around on its own.

use crate::error::CoreError;
use crate::pickaxe::PickaxeTier;
use crate::world::World;
use std::collections::HashMap;

/// The level ceiling of [`Fortune`](EnchantType::Fortune) — the one enchant keyed
/// by neither the pickaxe tier nor the world.
///
/// Settled by the design rather than left open, which is why it is a constant here
/// and not in [`tunables`](crate::tunables): past 10 the ore is abundant enough
/// that further Fortune buys nothing, so the player is meant to move to another
/// lever. See `docs/MECHANICS.md`.
const FORTUNE_CAP: u8 = 10;

/// A single enchantment together with its level.
///
/// This is a detached value type; the enchantments actually installed on a
/// pickaxe live in [`Enchants`], which stores them more compactly as a map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enchant {
    /// The kind of enchantment (Efficiency, Fortune, …).
    enchant_type: EnchantType,
    /// The level of the enchantment, which must not exceed
    level: u8,
}

/// The kinds of enchantment a pickaxe can have.
///
/// Derives [`Hash`]/[`Eq`] so it can be used as a [`HashMap`] key inside
/// [`Enchants`]. Each variant's effective level cap comes from
/// [`max_level`](EnchantType::max_level).
///
/// The seven split into three groups by **what their cap is keyed on**, and that
/// split is the shape of the whole module:
///
/// - [`Efficiency`](EnchantType::Efficiency) is keyed by the **pickaxe tier**
///   ([`PickaxeTier::efficiency_cap`]).
/// - [`Fortune`](EnchantType::Fortune) is keyed by **nothing** ([`FORTUNE_CAP`]).
/// - The other five — the *special* enchants — are keyed by the **world**
///   ([`World::enchant_cap`]). All five are on sale from the Overworld onwards;
///   a new dimension unlocks none of them and only raises their shared ceiling.
///
/// That is also why the two progression axes stay independent: the tier moves
/// Efficiency, the mining level moves the world and with it the five specials, and
/// neither axis alone advances the player.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum EnchantType {
    /// Increases mining speed, by `level² + 1` added to the pickaxe's base power.
    /// Capped by the pickaxe tier: 5, or 15 at Netherite.
    Efficiency,
    /// Increases the drop rate of ores, multiplying the loot by `1 + level`.
    /// Capped at [`FORTUNE_CAP`], whatever the tier and whatever the world.
    Fortune,
    /// Clears a compact blob around the impact cell, growing with level.
    /// Capped by the world; see [`World::enchant_cap`].
    Explosive,
    /// Clears a whole row of blocks, then a band of `k` rows at high levels.
    /// Capped by the world; see [`World::enchant_cap`].
    Jackhammer,
    /// Clears an entire mine at once, on a cooldown that shortens with level.
    /// Capped by the world; see [`World::enchant_cap`].
    Nuke,
    /// Grants a chance to drop a [`Compressed`] unit, or an Emerald, in place of
    /// the raw ore.
    ///
    /// The only thing in the game that mints a Compressed unit without paying its
    /// 100 raw, which is what makes it a windfall rather than a prettier drop. It
    /// substitutes the drop *after* it leaves the block, so no block gains a
    /// Compressed unit of its own — see [`Block::drops`].
    /// Capped by the world; see [`World::enchant_cap`].
    ///
    /// [`Compressed`]: crate::material::Item::Compressed
    /// [`Block::drops`]: crate::block::Block::drops
    Excavator,
    /// Multiplies mining speed permanently, by
    /// `1 + HASTE_PER_LEVEL * level` — linear, where
    /// [`Efficiency`](EnchantType::Efficiency) is quadratic, so the two compound
    /// instead of racing. See [`HASTE_PER_LEVEL`](crate::tunables::HASTE_PER_LEVEL).
    /// Capped by the world; see [`World::enchant_cap`].
    Haste,
}

/// The set of enchantments installed on a pickaxe.
///
/// Stored as a sparse map: an enchantment absent from the map is treated as
/// level 0, so only active enchantments consume memory. The `levels` field is
/// private — callers go through the methods below to keep the "absent == 0"
/// invariant intact.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Enchants {
    levels: HashMap<EnchantType, u8>,
}

impl Enchant {
    /// Constructs a standalone `(type, level)` enchantment pair.
    pub fn new(enchant_type: EnchantType, level: u8) -> Self {
        Self {
            enchant_type,
            level,
        }
    }
}

impl EnchantType {
    /// Returns the human-readable display name of the enchantment.
    pub fn name(self) -> &'static str {
        match self {
            Self::Efficiency => "Efficiency",
            Self::Fortune => "Fortune",
            Self::Explosive => "Explosive",
            Self::Jackhammer => "Jackhammer",
            Self::Nuke => "Nuke",
            Self::Excavator => "Excavator",
            Self::Haste => "Haste",
        }
    }

    /// Returns the maximum level this enchantment can reach.
    ///
    /// The generic front door to a cap: **pure dispatch, holding no number of its
    /// own**. Each of the three groups above keeps the single source of its own
    /// ceiling — [`PickaxeTier::efficiency_cap`], [`FORTUNE_CAP`],
    /// [`World::enchant_cap`] — and this method only picks which one applies. A
    /// table here would fork that knowledge in two and let the copies drift.
    ///
    /// Both arguments are **required, and neither is an `Option`**. Every enchant
    /// ignores at least one of them, so it is tempting to let a caller skip the one
    /// it believes irrelevant; a defaulted cap, though, is not refused but *wrong
    /// and plausible*. The paid path (phase 5) would debit Amethyst and hand back a
    /// level ceilinged at the Overworld's, with nothing to say so — the same silent
    /// failure `Enchants::upgrade` returns [`CoreError::EnchantAtCap`] to avoid. In
    /// play there is always a tier and always a world, so requiring both costs a
    /// caller nothing and makes the omission a compile error.
    ///
    /// `world` is the **highest world the player has unlocked**, not the one whose
    /// mine they happen to be standing in: reaching a dimension raises the ceiling
    /// for good, so a pickaxe never weakens by going back to farm an early mine.
    /// Phase 6 owns that set and will pass it.
    pub fn max_level(self, pickaxe_tier: PickaxeTier, world: World) -> u8 {
        match self {
            Self::Efficiency => pickaxe_tier.efficiency_cap(),
            Self::Fortune => FORTUNE_CAP,
            Self::Explosive | Self::Jackhammer | Self::Nuke | Self::Excavator | Self::Haste => {
                world.enchant_cap()
            }
        }
    }
}

impl Enchants {
    /// Creates a new instance of [`Enchants`].
    pub fn new() -> Self {
        Self {
            levels: HashMap::new(),
        }
    }

    /// Gets the level of the specified enchantment type.
    /// Returns 0 if the enchantment is not present.
    pub fn get_level(&self, kind: EnchantType) -> u8 {
        self.levels.get(&kind).copied().unwrap_or(0)
    }

    /// Increases the level of the specified enchantment by 1, up to its cap.
    ///
    /// Absent enchantments start from 0, so the first call installs them at
    /// level 1. Calls beyond [`max_level`](EnchantType::max_level) are
    /// **refused**, not quietly dropped: the cap is enforced here rather than
    /// left to each caller, so no code path can hand the player a level the game
    /// has no rules for — and the paid path can tell "bought a level" from "paid
    /// for nothing".
    ///
    /// Both `pickaxe_tier` and `world` are needed because the cap is keyed by one
    /// or the other depending on `kind`; see
    /// [`max_level`](EnchantType::max_level), which this defers to.
    ///
    /// `pub(crate)` for the same reason as
    /// [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe::upgrade): it is free. An
    /// enchant level is bought with the world's enchant material plus a mix of
    /// earlier mines' ore, and none of that is checked here.
    // Dead outside the tests until the phase-5 purchase path calls it: the one
    // in-crate caller, `Pickaxe::upgrade`, goes through `upgrade_efficiency`
    // instead, precisely because it has no world to hand over.
    #[cfg_attr(not(test), expect(dead_code, reason = "awaiting the phase-5 economy"))]
    pub(crate) fn upgrade(
        &mut self,
        kind: EnchantType,
        pickaxe_tier: PickaxeTier,
        world: World,
    ) -> Result<(), CoreError> {
        self.upgrade_to_cap(kind, kind.max_level(pickaxe_tier, world))
    }

    /// Increases [`Efficiency`](EnchantType::Efficiency) by 1, up to the cap its
    /// `tier` allows.
    ///
    /// The world-free door into [`upgrade`](Enchants::upgrade), and the only one
    /// [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe::upgrade) needs: Efficiency is
    /// keyed by the tier alone, and a `Pickaxe` knows no world. Routing it through
    /// the generic path would mean threading a [`World`] the length of the tier
    /// ladder for a value provably never read — and the only way to supply one
    /// would be to invent it at the call site, which is how a cap stops meaning
    /// anything.
    ///
    /// Enforces the ceiling exactly as `upgrade` does; both delegate to the same
    /// [`upgrade_to_cap`](Enchants::upgrade_to_cap), so neither can drift into
    /// being the lenient one.
    pub(crate) fn upgrade_efficiency(&mut self, tier: PickaxeTier) -> Result<(), CoreError> {
        self.upgrade_to_cap(EnchantType::Efficiency, tier.efficiency_cap())
    }

    /// Bumps `kind` by one level, refusing at `cap`.
    ///
    /// The single place the ceiling is actually applied. Both public doors resolve
    /// a cap and hand it here, so "which cap" and "enforce it" stay separate
    /// questions with one answer each.
    fn upgrade_to_cap(&mut self, kind: EnchantType, cap: u8) -> Result<(), CoreError> {
        let level = self.get_level(kind);
        if level >= cap {
            return Err(CoreError::EnchantAtCap { kind, cap });
        }
        self.levels.insert(kind, level + 1);
        Ok(())
    }

    /// Resets the level of the specified enchantment to 0.
    ///
    /// Removes the entry outright rather than storing a 0, which keeps the
    /// "absent == level 0" invariant true: [`iter`](Enchants::iter) must only
    /// ever yield enchantments the pickaxe actually has.
    ///
    /// `pub(crate)`: this exists to serve the tier jump, which cashes a maxed
    /// Efficiency in for the next tier. Wiping an enchant outside that trade is
    /// a pure loss to the player, and nothing outside the core should be able to
    /// inflict it.
    pub(crate) fn reset_level(&mut self, kind: EnchantType) {
        self.levels.remove(&kind);
    }

    /// Resets all enchantments to level 0.
    /// This will clear the internal HashMap of enchantments.
    pub fn reset(&mut self) {
        self.levels.clear();
    }

    /// Returns an iterator over the enchantments and their levels.
    /// Each item in the iterator is a tuple of (EnchantType, level).
    pub fn iter(&self) -> impl Iterator<Item = (EnchantType, u8)> + '_ {
        self.levels.iter().map(|(&k, &v)| (k, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::world::ALL_WORLDS;

    /// Every [`EnchantType`] variant.
    const ALL_ENCHANTS: [EnchantType; 7] = [
        EnchantType::Efficiency,
        EnchantType::Fortune,
        EnchantType::Explosive,
        EnchantType::Jackhammer,
        EnchantType::Nuke,
        EnchantType::Excavator,
        EnchantType::Haste,
    ];

    /// The five *special* enchants: the ones whose cap is keyed by the world.
    ///
    /// Together with [`Efficiency`](EnchantType::Efficiency) (keyed by the tier)
    /// and [`Fortune`](EnchantType::Fortune) (keyed by nothing) this partitions
    /// [`ALL_ENCHANTS`], which `the_three_cap_groups_partition_every_enchant`
    /// checks — an enchant that fell out of all three would have a cap no test
    /// here ever looks at.
    const SPECIAL_ENCHANTS: [EnchantType; 5] = [
        EnchantType::Explosive,
        EnchantType::Jackhammer,
        EnchantType::Nuke,
        EnchantType::Excavator,
        EnchantType::Haste,
    ];

    /// A tier for tests whose subject is not the tier.
    ///
    /// Named rather than picked inline so the claim is visible: these tests assert
    /// something the tier does not change, and
    /// `the_five_special_enchants_and_fortune_ignore_the_pickaxe_tier` is what
    /// holds that claim up.
    const ANY_TIER: PickaxeTier = PickaxeTier::Wooden;

    /// A world for tests whose subject is not the world; the counterpart to
    /// [`ANY_TIER`], backed by `efficiency_and_fortune_ignore_the_world`.
    ///
    /// The End specifically, because it is the loosest: a test that needs a few
    /// levels of a special enchant must not trip over a ceiling it never meant to
    /// exercise.
    const ANY_WORLD: World = World::End;

    /// Names are what the pickaxe screen shows, so a blank or duplicated one
    /// would leave the player unable to tell two enchantments apart.
    #[test]
    fn enchant_names_are_present_and_unique() {
        for (i, &a) in ALL_ENCHANTS.iter().enumerate() {
            assert!(!a.name().is_empty(), "{a:?} has no display name");
            for &b in &ALL_ENCHANTS[i + 1..] {
                assert_ne!(
                    a.name(),
                    b.name(),
                    "{a:?} and {b:?} share the display name {:?}",
                    a.name()
                );
            }
        }
    }

    /// `Enchant` and `Enchants` are two shapes of the same `(type, level)`
    /// fact — a detached pair on one side, a sparse map on the other. Anything
    /// that hands an `Enchant` around must be able to round-trip it through the
    /// set actually installed on a pickaxe.
    #[test]
    fn a_detached_enchant_round_trips_through_the_installed_set() {
        let detached = Enchant::new(EnchantType::Fortune, 3);
        assert_eq!(detached.enchant_type, EnchantType::Fortune);
        assert_eq!(detached.level, 3);

        let mut installed = Enchants::new();
        for _ in 0..detached.level {
            assert!(
                installed
                    .upgrade(detached.enchant_type, ANY_TIER, ANY_WORLD)
                    .is_ok()
            );
        }

        let read_back = Enchant::new(
            detached.enchant_type,
            installed.get_level(detached.enchant_type),
        );
        assert_eq!(read_back, detached);
    }

    #[test]
    fn an_absent_enchant_reads_as_level_zero() {
        // The core of the sparse-map design: nothing is stored until it is
        // earned, and callers still see a level for every enchant.
        assert_eq!(Enchants::new().get_level(EnchantType::Fortune), 0);
        assert_eq!(Enchants::new().iter().count(), 0);
    }

    #[test]
    fn upgrading_an_absent_enchant_installs_it_at_level_one() {
        let mut enchants = Enchants::new();
        assert!(
            enchants
                .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                .is_ok()
        );
        assert_eq!(enchants.get_level(EnchantType::Fortune), 1);
    }

    #[test]
    fn reset_level_leaves_the_other_enchants_alone() {
        let mut enchants = Enchants::new();
        assert!(
            enchants
                .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                .is_ok()
        );
        assert!(
            enchants
                .upgrade(EnchantType::Haste, ANY_TIER, ANY_WORLD)
                .is_ok()
        );

        enchants.reset_level(EnchantType::Fortune);

        assert_eq!(enchants.get_level(EnchantType::Fortune), 0);
        assert_eq!(enchants.get_level(EnchantType::Haste), 1);
    }

    #[test]
    fn reset_clears_every_enchant() {
        let mut enchants = Enchants::new();
        assert!(
            enchants
                .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                .is_ok()
        );
        assert!(
            enchants
                .upgrade(EnchantType::Haste, ANY_TIER, ANY_WORLD)
                .is_ok()
        );

        enchants.reset();

        assert_eq!(enchants.get_level(EnchantType::Fortune), 0);
        assert_eq!(enchants.get_level(EnchantType::Haste), 0);
        assert_eq!(enchants.iter().count(), 0);
    }

    /// `iter()` is what a UI walks to list "what is on this pickaxe", so it must
    /// yield only enchants the player actually has. The struct promises
    /// "absent == level 0" and that only active enchants consume memory —
    /// a level-0 entry left in the map breaks both.
    #[test]
    fn iter_yields_only_active_enchants() {
        let mut enchants = Enchants::new();
        assert!(
            enchants
                .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                .is_ok()
        );
        enchants.reset_level(EnchantType::Fortune);

        assert_eq!(
            enchants.iter().count(),
            0,
            "a level-0 entry survived reset_level, so the UI would list an enchant the player does not have"
        );
    }

    /// Only Efficiency reads the tier, and only Netherite changes the answer.
    /// That raised cap is the whole reward for reaching the final tier.
    #[test]
    fn efficiency_caps_at_five_everywhere_but_netherite() {
        for tier in [
            PickaxeTier::Wooden,
            PickaxeTier::Stone,
            PickaxeTier::Iron,
            PickaxeTier::Gold,
            PickaxeTier::Diamond,
        ] {
            assert_eq!(EnchantType::Efficiency.max_level(tier, ANY_WORLD), 5);
        }
        assert_eq!(
            EnchantType::Efficiency.max_level(PickaxeTier::Netherite, ANY_WORLD),
            15
        );
    }

    #[test]
    fn the_five_special_enchants_and_fortune_ignore_the_pickaxe_tier() {
        for kind in SPECIAL_ENCHANTS.iter().chain(&[EnchantType::Fortune]) {
            assert_eq!(
                kind.max_level(PickaxeTier::Wooden, ANY_WORLD),
                kind.max_level(PickaxeTier::Netherite, ANY_WORLD),
                "{} must not depend on the pickaxe tier",
                kind.name()
            );
        }
    }

    /// The mirror of the test above, and together they are what keeps the two
    /// progression axes from collapsing into one. The tier moves Efficiency; the
    /// world moves the five specials. If a world also raised Efficiency's ceiling,
    /// one investment would advance both axes at once.
    #[test]
    fn efficiency_and_fortune_ignore_the_world() {
        for kind in [EnchantType::Efficiency, EnchantType::Fortune] {
            for world in ALL_WORLDS {
                assert_eq!(
                    kind.max_level(ANY_TIER, world),
                    kind.max_level(ANY_TIER, World::Overworld),
                    "{} must not depend on the world, but {} changes it",
                    kind.name(),
                    world.name()
                );
            }
        }
    }

    /// `docs/MECHANICS.md`: "Overworld enchants use Lapis (lowest level cap),
    /// Nether uses Quartz (higher), End uses Amethyst (maximum)". Strictly
    /// increasing, not merely non-decreasing: a world that raised no ceiling would
    /// leave its enchant material buying nothing, and the ladder Lapis → Quartz →
    /// Amethyst is the entire reason the three materials are distinct.
    #[test]
    fn enchant_caps_grow_strictly_with_the_world() {
        for pair in ALL_WORLDS.windows(2) {
            let (lower, higher) = (pair[0], pair[1]);
            assert!(
                lower.enchant_cap() < higher.enchant_cap(),
                "{} caps enchants at {} and {} at {}: reaching {} must raise the ceiling",
                lower.name(),
                lower.enchant_cap(),
                higher.name(),
                higher.enchant_cap(),
                higher.name()
            );
        }
    }

    /// One ceiling per world, shared by all five — not a cap per
    /// `(enchant, world)` pair. `max_level` is dispatch, so this is really a test
    /// that no special enchant has quietly grown a table of its own.
    #[test]
    fn the_five_special_enchants_share_their_world_cap() {
        for world in ALL_WORLDS {
            for kind in SPECIAL_ENCHANTS {
                assert_eq!(
                    kind.max_level(ANY_TIER, world),
                    world.enchant_cap(),
                    "{} in {} must read the world's shared cap",
                    kind.name(),
                    world.name()
                );
            }
        }
    }

    /// `docs/MECHANICS.md`: "All five enchants are available as soon as you can
    /// enchant (Overworld); progressing to a new world only raises the level cap."
    /// A special enchant capped at 0 in the Overworld would be *unlocked* by the
    /// Nether instead — a different design, and not this one.
    #[test]
    fn every_special_enchant_is_reachable_in_the_overworld() {
        for kind in SPECIAL_ENCHANTS {
            assert!(
                kind.max_level(ANY_TIER, World::Overworld) > 0,
                "{} caps at 0 in the Overworld, so reaching the Nether would unlock it \
                 rather than merely raise its ceiling",
                kind.name()
            );
        }
    }

    /// Every enchant must fall in exactly one cap group. One that fell out of all
    /// three would still answer `max_level` — via whichever arm of the `match`
    /// caught it — but no test here would be watching that answer.
    #[test]
    fn the_three_cap_groups_partition_every_enchant() {
        for kind in ALL_ENCHANTS {
            let groups = [
                kind == EnchantType::Efficiency,
                kind == EnchantType::Fortune,
                SPECIAL_ENCHANTS.contains(&kind),
            ];
            assert_eq!(
                groups.iter().filter(|&&in_group| in_group).count(),
                1,
                "{} must be keyed by the tier, by the world, or by nothing — exactly one",
                kind.name()
            );
        }
    }

    #[test]
    fn every_enchant_has_a_reachable_cap() {
        for kind in ALL_ENCHANTS {
            assert!(
                kind.max_level(ANY_TIER, ANY_WORLD) > 0,
                "{} caps at 0, so it can never be earned",
                kind.name()
            );
        }
    }

    /// A level above the cap is a level the game has no rules for: `max_level`
    /// would no longer bound what the player can hold.
    #[test]
    fn upgrade_stops_at_the_enchant_cap() {
        let cap = EnchantType::Fortune.max_level(ANY_TIER, ANY_WORLD);
        let mut enchants = Enchants::new();
        for _ in 0..cap {
            assert!(
                enchants
                    .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                    .is_ok()
            );
        }

        for _ in 0..5 {
            let _ = enchants.upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD);
        }

        assert_eq!(
            enchants.get_level(EnchantType::Fortune),
            cap,
            "Enchants::upgrade let Fortune climb past its cap of {cap}"
        );
    }

    /// The cap must *refuse*, not shrug. A silent no-op is indistinguishable from
    /// a successful upgrade at the call site, so the paid path (phase 5) would
    /// happily debit the player's ore for a level they never received — the same
    /// hole `Inventory::remove` used to have when it clamped an over-large
    /// withdrawal to zero.
    #[test]
    fn upgrading_a_capped_enchant_is_refused_and_changes_nothing() {
        let cap = EnchantType::Fortune.max_level(ANY_TIER, ANY_WORLD);
        let mut enchants = Enchants::new();
        for _ in 0..cap {
            assert!(
                enchants
                    .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                    .is_ok()
            );
        }

        assert_eq!(
            enchants.upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD),
            Err(CoreError::EnchantAtCap {
                kind: EnchantType::Fortune,
                cap,
            })
        );
        assert_eq!(enchants.get_level(EnchantType::Fortune), cap);
    }

    /// Efficiency is the one enchant whose ceiling moves with the tier, so the
    /// cap `upgrade_efficiency` enforces has to follow the tier it is handed —
    /// including which of the two calls is the one that gets refused.
    #[test]
    fn the_cap_upgrade_enforces_follows_the_tier() {
        let mut wooden = Enchants::new();
        let mut netherite = Enchants::new();
        for _ in 0..15 {
            let _ = wooden.upgrade_efficiency(PickaxeTier::Wooden);
            assert!(netherite.upgrade_efficiency(PickaxeTier::Netherite).is_ok());
        }

        assert_eq!(wooden.get_level(EnchantType::Efficiency), 5);
        assert_eq!(netherite.get_level(EnchantType::Efficiency), 15);
    }

    /// The two doors must enforce the *same* ceiling. They resolve it by different
    /// routes — one through `max_level`'s dispatch, one straight from the tier —
    /// and if those ever disagreed, `Pickaxe::upgrade` would be selling Efficiency
    /// levels under a rule of its own.
    #[test]
    fn both_upgrade_doors_stop_efficiency_at_the_same_level() {
        for tier in [PickaxeTier::Wooden, PickaxeTier::Netherite] {
            for world in ALL_WORLDS {
                let mut generic = Enchants::new();
                let mut direct = Enchants::new();
                for _ in 0..20 {
                    let _ = generic.upgrade(EnchantType::Efficiency, tier, world);
                    let _ = direct.upgrade_efficiency(tier);
                }
                assert_eq!(
                    generic.get_level(EnchantType::Efficiency),
                    direct.get_level(EnchantType::Efficiency),
                    "the generic and world-free doors disagree on {tier:?} in {}",
                    world.name()
                );
            }
        }
    }
}
