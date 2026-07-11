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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tier, weakest first.
    const ALL_TIERS: [PickaxeTier; 6] = [
        PickaxeTier::Wooden,
        PickaxeTier::Stone,
        PickaxeTier::Iron,
        PickaxeTier::Gold,
        PickaxeTier::Diamond,
        PickaxeTier::Netherite,
    ];

    /// Upgrades needed to move one tier: fill Efficiency to its cap of 5, then
    /// one more to spend the maxed enchant on the next tier.
    const UPGRADES_PER_TIER: usize = 6;

    /// Drives a fresh pickaxe all the way to Netherite with no Efficiency.
    fn maxed_tier_pickaxe() -> Pickaxe {
        let mut pickaxe = Pickaxe::default();
        for _ in 0..(UPGRADES_PER_TIER * (ALL_TIERS.len() - 1)) {
            pickaxe.upgrade();
        }
        pickaxe
    }

    #[test]
    fn a_fresh_pickaxe_is_wooden_and_unenchanted() {
        let pickaxe = Pickaxe::default();
        assert_eq!(pickaxe.tier, PickaxeTier::Wooden);
        assert_eq!(pickaxe.enchants.get_level(EnchantType::Efficiency), 0);
    }

    /// The `+ 1` in the Efficiency formula means level 0 is still worth
    /// something: a fresh pickaxe mines at 2 (Wooden) + 0² + 1 = 3, not 2.
    #[test]
    fn an_unenchanted_pickaxe_still_gets_the_flat_bonus() {
        assert_eq!(Pickaxe::default().mining_power(), 3);
    }

    /// Efficiency squares, so it is the long-term lever: on the same Wooden
    /// base, level 5 is worth 26 power where level 1 is worth 2.
    #[test]
    fn efficiency_scales_quadratically() {
        let tier = Some(PickaxeTier::Wooden);
        let mut enchants = Enchants::new();
        enchants.upgrade(EnchantType::Efficiency, tier);
        let level_one = Pickaxe::new(PickaxeTier::Wooden, enchants.clone());
        assert_eq!(level_one.mining_power(), 2 + 1 + 1);

        for _ in 0..4 {
            enchants.upgrade(EnchantType::Efficiency, tier);
        }
        let level_five = Pickaxe::new(PickaxeTier::Wooden, enchants);
        assert_eq!(level_five.mining_power(), 2 + 25 + 1);
    }

    /// The ladder must be walkable end to end, and stop exactly once.
    #[test]
    fn the_tier_ladder_ends_at_netherite() {
        for pair in ALL_TIERS.windows(2) {
            assert_eq!(pair[0].next(), Some(pair[1]));
        }
        assert_eq!(
            PickaxeTier::Netherite.next(),
            None,
            "Netherite is the final tier and must not report a successor"
        );
    }

    /// Deliberate, and easy to "fix" by accident: Gold mines faster than both
    /// Diamond and Netherite, mirroring Minecraft. Tier gates *access*, not raw
    /// speed.
    #[test]
    fn gold_out_powers_diamond_and_netherite() {
        assert!(PickaxeTier::Gold.base_power() > PickaxeTier::Diamond.base_power());
        assert!(PickaxeTier::Gold.base_power() > PickaxeTier::Netherite.base_power());
    }

    /// `min_pickaxe_tier` gating relies on `Ord` following declaration order.
    #[test]
    fn tiers_compare_in_progression_order() {
        for pair in ALL_TIERS.windows(2) {
            assert!(
                pair[0] < pair[1],
                "{:?} should rank below {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    /// Phase 1 of the upgrade curve: spend upgrades on Efficiency while the cap
    /// allows, keeping the tier put.
    #[test]
    fn upgrades_fill_efficiency_before_touching_the_tier() {
        let mut pickaxe = Pickaxe::default();
        for expected_level in 1..=5 {
            pickaxe.upgrade();
            assert_eq!(pickaxe.tier, PickaxeTier::Wooden);
            assert_eq!(
                pickaxe.enchants.get_level(EnchantType::Efficiency),
                expected_level
            );
        }
    }

    /// Phase 2: once Efficiency is capped, the next upgrade cashes it in for a
    /// tier, and the climb restarts on a stronger base.
    #[test]
    fn a_capped_efficiency_is_cashed_in_for_the_next_tier() {
        let mut pickaxe = Pickaxe::default();
        for _ in 0..UPGRADES_PER_TIER {
            pickaxe.upgrade();
        }
        assert_eq!(pickaxe.tier, PickaxeTier::Stone);
        assert_eq!(pickaxe.enchants.get_level(EnchantType::Efficiency), 0);
    }

    #[test]
    fn thirty_upgrades_walk_the_whole_tier_ladder() {
        let pickaxe = maxed_tier_pickaxe();
        assert_eq!(pickaxe.tier, PickaxeTier::Netherite);
        assert_eq!(pickaxe.enchants.get_level(EnchantType::Efficiency), 0);
    }

    /// Netherite raises the Efficiency cap from 5 to 15, which is what makes the
    /// final tier worth reaching despite its middling base power.
    #[test]
    fn netherite_efficiency_climbs_to_fifteen() {
        let mut pickaxe = maxed_tier_pickaxe();
        for expected_level in 1..=15 {
            pickaxe.upgrade();
            assert_eq!(pickaxe.tier, PickaxeTier::Netherite);
            assert_eq!(
                pickaxe.enchants.get_level(EnchantType::Efficiency),
                expected_level
            );
        }
        // 9 (Netherite) + 15² + 1
        assert_eq!(pickaxe.mining_power(), 235);
    }

    /// The final tier has nowhere left to advance to, so an upgrade there must
    /// never be a *downgrade*. Wiping Efficiency at the ceiling would drop the
    /// player from 235 mining power back to 10 — permanently, since the tier can
    /// no longer climb to compensate.
    #[test]
    fn upgrading_a_fully_maxed_pickaxe_never_reduces_its_power() {
        let mut pickaxe = maxed_tier_pickaxe();
        for _ in 0..15 {
            pickaxe.upgrade();
        }
        let maxed_power = pickaxe.mining_power();

        pickaxe.upgrade();

        assert_eq!(pickaxe.tier, PickaxeTier::Netherite);
        assert!(
            pickaxe.mining_power() >= maxed_power,
            "upgrading a maxed Netherite pickaxe dropped its mining power from {maxed_power} to {}",
            pickaxe.mining_power()
        );
    }
}
