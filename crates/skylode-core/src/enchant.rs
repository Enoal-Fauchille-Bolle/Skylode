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
    pub enchant_type: EnchantType,
    pub level: u8,
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

    /// Increases the level of the specified enchantment type by 1.
    /// If the enchantment is not present, it will be added with a level of 1.
    pub fn upgrade(&mut self, kind: EnchantType) {
        let level = self.levels.entry(kind).or_insert(0);
        *level += 1;
    }

    /// Resets the level of the specified enchantment type to 0.
    /// If the enchantment is not present, it will be added with a level of 0.
    pub fn reset_level(&mut self, kind: EnchantType) {
        self.levels.insert(kind, 0);
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
