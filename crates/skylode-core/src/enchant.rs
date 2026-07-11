//! Pickaxe enchantments.
//!
//! Enchantments modify how a [`Pickaxe`](crate::pickaxe::Pickaxe) mines. This
//! module provides:
//! - [`EnchantType`]: the kind of enchantment (Efficiency, Fortune, …) plus its
//!   per-tier level caps.
//! - [`Enchants`]: a compact per-pickaxe store mapping each active enchantment
//!   to its current level.
//! - [`Enchant`]: a standalone `(type, level)` pair used when an enchantment
//!   needs to be passed around on its own.

use crate::pickaxe::PickaxeTier;
use std::collections::HashMap;

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
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum EnchantType {
    /// Increases mining speed.
    /// Ranges from 0 to 5, except on Netherite where it can reach 15.
    Efficiency,
    /// Increases the drop rate of ores.
    /// Ranges from 0 to 10.
    Fortune,
    /// Clears a 3x3 area, and can expand further.
    /// Ranges from 0 to (TODO: determine max level).
    Explosive,
    /// Clears a whole row of blocks.
    /// Ranges from 0 to (TODO: determine max level).
    Jackhammer,
    /// Clears an entire mine at once.
    /// Ranges from 0 to (TODO: determine max level).
    Nuke,
    /// Grants a chance to drop a Compressed Ore.
    /// Ranges from 0 to (TODO: determine max level).
    Excavator,
    /// Increases mining speed permanently.
    /// Ranges from 0 to (TODO: determine max level).
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
    /// Only [`Efficiency`](EnchantType::Efficiency) depends on the pickaxe
    /// tier: it caps at 5 for every tier except
    /// [`Netherite`](PickaxeTier::Netherite), which raises the cap to 15.
    /// Passing `None` falls back to the default cap of 5. All other
    /// enchantments ignore the tier and return a fixed cap (several of which are
    /// still placeholder values pending balancing — see the `EnchantType`
    /// variant docs).
    pub fn max_level(self, pickaxe_tier: Option<PickaxeTier>) -> u8 {
        match self {
            EnchantType::Efficiency => {
                match pickaxe_tier {
                    Some(PickaxeTier::Wooden) => 5,
                    Some(PickaxeTier::Stone) => 5,
                    Some(PickaxeTier::Iron) => 5,
                    Some(PickaxeTier::Gold) => 5,
                    Some(PickaxeTier::Diamond) => 5,
                    Some(PickaxeTier::Netherite) => 15,
                    None => 5, // Default max level if no tier is provided
                }
            }
            EnchantType::Fortune => 10,
            EnchantType::Explosive => 3,  // Example max level
            EnchantType::Jackhammer => 3, // Example max level
            EnchantType::Nuke => 1,       // Example max level
            EnchantType::Excavator => 3,  // Example max level
            EnchantType::Haste => 5,      // Example max level
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
    /// level 1. Calls beyond [`max_level`](EnchantType::max_level) are no-ops:
    /// the cap is enforced here rather than left to each caller, so no code path
    /// can hand the player a level the game has no rules for.
    ///
    /// `pickaxe_tier` is needed because
    /// [`Efficiency`](EnchantType::Efficiency)'s cap depends on it; pass `None`
    /// for the tier-less default of 5.
    pub fn upgrade(&mut self, kind: EnchantType, pickaxe_tier: Option<PickaxeTier>) {
        let level = self.get_level(kind);
        if level < kind.max_level(pickaxe_tier) {
            self.levels.insert(kind, level + 1);
        }
    }

    /// Resets the level of the specified enchantment to 0.
    ///
    /// Removes the entry outright rather than storing a 0, which keeps the
    /// "absent == level 0" invariant true: [`iter`](Enchants::iter) must only
    /// ever yield enchantments the pickaxe actually has.
    pub fn reset_level(&mut self, kind: EnchantType) {
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

    /// The enchantments whose cap does not depend on the pickaxe tier — that is,
    /// every one but [`Efficiency`](EnchantType::Efficiency).
    const TIER_INDEPENDENT: [EnchantType; 6] = [
        EnchantType::Fortune,
        EnchantType::Explosive,
        EnchantType::Jackhammer,
        EnchantType::Nuke,
        EnchantType::Excavator,
        EnchantType::Haste,
    ];

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
            installed.upgrade(detached.enchant_type, None);
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
        enchants.upgrade(EnchantType::Fortune, None);
        assert_eq!(enchants.get_level(EnchantType::Fortune), 1);
    }

    #[test]
    fn reset_level_leaves_the_other_enchants_alone() {
        let mut enchants = Enchants::new();
        enchants.upgrade(EnchantType::Fortune, None);
        enchants.upgrade(EnchantType::Haste, None);

        enchants.reset_level(EnchantType::Fortune);

        assert_eq!(enchants.get_level(EnchantType::Fortune), 0);
        assert_eq!(enchants.get_level(EnchantType::Haste), 1);
    }

    #[test]
    fn reset_clears_every_enchant() {
        let mut enchants = Enchants::new();
        enchants.upgrade(EnchantType::Fortune, None);
        enchants.upgrade(EnchantType::Haste, None);

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
        enchants.upgrade(EnchantType::Fortune, None);
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
            assert_eq!(EnchantType::Efficiency.max_level(Some(tier)), 5);
        }
        assert_eq!(
            EnchantType::Efficiency.max_level(Some(PickaxeTier::Netherite)),
            15
        );
        assert_eq!(
            EnchantType::Efficiency.max_level(None),
            5,
            "the tier-less fallback must match the cap shared by every non-Netherite tier"
        );
    }

    #[test]
    fn other_enchants_ignore_the_pickaxe_tier() {
        for kind in TIER_INDEPENDENT {
            assert_eq!(
                kind.max_level(None),
                kind.max_level(Some(PickaxeTier::Netherite)),
                "{} must not depend on the pickaxe tier",
                kind.name()
            );
        }
    }

    #[test]
    fn every_enchant_has_a_reachable_cap() {
        for kind in TIER_INDEPENDENT {
            assert!(
                kind.max_level(None) > 0,
                "{} caps at 0, so it can never be earned",
                kind.name()
            );
        }
    }

    /// A level above the cap is a level the game has no rules for: `max_level`
    /// would no longer bound what the player can hold.
    #[test]
    fn upgrade_stops_at_the_enchant_cap() {
        let cap = EnchantType::Fortune.max_level(None);
        let mut enchants = Enchants::new();
        for _ in 0..(u32::from(cap) + 5) {
            enchants.upgrade(EnchantType::Fortune, None);
        }

        assert_eq!(
            enchants.get_level(EnchantType::Fortune),
            cap,
            "Enchants::upgrade let Fortune climb past its cap of {cap}"
        );
    }

    /// Efficiency is the one enchant whose ceiling moves with the tier, so the
    /// cap `upgrade` enforces has to follow the tier it is handed.
    #[test]
    fn the_cap_upgrade_enforces_follows_the_tier() {
        let mut wooden = Enchants::new();
        let mut netherite = Enchants::new();
        for _ in 0..15 {
            wooden.upgrade(EnchantType::Efficiency, Some(PickaxeTier::Wooden));
            netherite.upgrade(EnchantType::Efficiency, Some(PickaxeTier::Netherite));
        }

        assert_eq!(wooden.get_level(EnchantType::Efficiency), 5);
        assert_eq!(netherite.get_level(EnchantType::Efficiency), 15);
    }
}
