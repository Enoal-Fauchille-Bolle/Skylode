//! The player's pickaxe.
//!
//! A [`Pickaxe`] is defined by its [`PickaxeTier`] (the base material) and its
//! [`Enchants`]. Together they determine the
//! [`mining_power`](Pickaxe::mining_power) applied against block
//! [`hardness`](crate::block::Block::hardness) each tick, and which blocks the
//! player is allowed to mine at all (see
//! [`Block::min_pickaxe_tier`](crate::block::Block::min_pickaxe_tier)).

use crate::enchant::{EnchantType, Enchants};

/// The material tier of a pickaxe.
///
/// Derives [`PartialOrd`]/[`Ord`] so tiers can be compared directly (e.g.
/// `pickaxe.tier >= block.min_pickaxe_tier()`); the ordering follows
/// declaration order, from weakest ([`Wooden`](PickaxeTier::Wooden)) to
/// strongest ([`Netherite`](PickaxeTier::Netherite)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PickaxeTier {
    Wooden,
    Stone,
    Iron,
    Gold,
    Diamond,
    Netherite,
}

/// The player's mining tool: a tier plus its enchantments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pickaxe {
    /// The tier of the pickaxe, which determines its base mining power.
    pub tier: PickaxeTier,
    /// The enchantments on the pickaxe, which modify its mining power and other
    /// properties.
    pub enchants: Enchants,
}

impl Default for Pickaxe {
    fn default() -> Self {
        Self {
            tier: PickaxeTier::Wooden,
            enchants: Enchants::new(),
        }
    }
}

impl Pickaxe {
    /// Constructs a pickaxe from an explicit tier and enchantment set.
    ///
    /// Use [`Pickaxe::default`] for a fresh Wooden pickaxe with no enchants.
    pub fn new(tier: PickaxeTier, enchants: Enchants) -> Self {
        Self { tier, enchants }
    }

    /// Computes the total mining power applied against block hardness.
    ///
    /// Combines the tier's [`base_power`](PickaxeTier::base_power) with an
    /// Efficiency bonus of `level² + 1`. The `+ 1` means even an unenchanted
    /// pickaxe (Efficiency 0) gets a small bonus, while the squaring makes high
    /// Efficiency levels scale sharply — the main long-term power lever.
    pub fn mining_power(&self) -> u32 {
        let base = self.tier.base_power();
        let eff_bonus = (self.enchants.get_level(EnchantType::Efficiency) as u32).pow(2) + 1;
        base + eff_bonus
    }

    /// Advances the pickaxe one step along its upgrade path.
    ///
    /// The upgrade curve is two-phase:
    /// 1. While Efficiency is below its tier-dependent cap
    ///    ([`max_level`](EnchantType::max_level)), each call bumps Efficiency.
    /// 2. Once Efficiency is maxed, it is reset to 0 and the pickaxe advances to
    ///    the next tier — so the player re-climbs Efficiency on a stronger base.
    ///
    /// At [`Netherite`](PickaxeTier::Netherite) (the final tier) the tier stops
    /// advancing, though Efficiency there can still climb to a higher cap.
    pub fn upgrade(&mut self) {
        if self.enchants.get_level(EnchantType::Efficiency)
            < EnchantType::Efficiency.max_level(Some(self.tier))
        {
            self.enchants.upgrade(EnchantType::Efficiency);
        } else {
            self.enchants.reset_level(EnchantType::Efficiency);
            self.tier = match self.tier {
                PickaxeTier::Wooden => PickaxeTier::Stone,
                PickaxeTier::Stone => PickaxeTier::Iron,
                PickaxeTier::Iron => PickaxeTier::Gold,
                PickaxeTier::Gold => PickaxeTier::Diamond,
                PickaxeTier::Diamond => PickaxeTier::Netherite,
                PickaxeTier::Netherite => PickaxeTier::Netherite, // Max tier
            };
        }
    }
}

impl PickaxeTier {
    /// Returns the flat mining power contributed by the tier alone, before
    /// enchantments.
    ///
    /// Note the curve is not strictly monotonic: `Gold` (12) out-powers
    /// `Diamond` (8) and `Netherite` (9), mirroring Minecraft, where Gold mines
    /// fast but is otherwise weak. Tier still gates *which* blocks are
    /// mineable via [`Block::min_pickaxe_tier`](crate::block::Block::min_pickaxe_tier).
    pub fn base_power(&self) -> u32 {
        match self {
            PickaxeTier::Wooden => 2,
            PickaxeTier::Stone => 4,
            PickaxeTier::Iron => 6,
            PickaxeTier::Gold => 12,
            PickaxeTier::Diamond => 8,
            PickaxeTier::Netherite => 9,
        }
    }
}
