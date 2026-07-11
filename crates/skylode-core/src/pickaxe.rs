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
    /// At [`Netherite`](PickaxeTier::Netherite) Efficiency climbs to its raised
    /// cap of 15 and the pickaxe is then fully upgraded: further calls do
    /// nothing. They must not fall through to phase 2, which would reset
    /// Efficiency with no tier left to gain in exchange — a permanent downgrade
    /// from 235 mining power back to 10.
    pub fn upgrade(&mut self) {
        let efficiency = EnchantType::Efficiency;

        if self.enchants.get_level(efficiency) < efficiency.max_level(Some(self.tier)) {
            self.enchants.upgrade(efficiency, Some(self.tier));
        } else if let Some(next_tier) = self.tier.next() {
            self.enchants.reset_level(efficiency);
            self.tier = next_tier;
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

    /// Returns the tier one step up the upgrade ladder, or `None` at
    /// [`Netherite`](PickaxeTier::Netherite).
    ///
    /// The ladder is deliberately a *partial* function. Modelling it as a total
    /// one — with Netherite mapping to itself — reads as "there is always a next
    /// tier", which let [`Pickaxe::upgrade`] treat the top of the ladder as an
    /// ordinary step and wipe the player's Efficiency for nothing. `None` forces
    /// every caller to say what happens when there is no next tier.
    pub fn next(self) -> Option<PickaxeTier> {
        match self {
            PickaxeTier::Wooden => Some(PickaxeTier::Stone),
            PickaxeTier::Stone => Some(PickaxeTier::Iron),
            PickaxeTier::Iron => Some(PickaxeTier::Gold),
            PickaxeTier::Gold => Some(PickaxeTier::Diamond),
            PickaxeTier::Diamond => Some(PickaxeTier::Netherite),
            PickaxeTier::Netherite => None,
        }
    }
}
