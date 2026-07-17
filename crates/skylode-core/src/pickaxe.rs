//! The player's pickaxe.
//!
//! A [`Pickaxe`] is defined by its [`PickaxeTier`] (the base material) and its
//! [`Enchants`]. Together they determine the
//! [`mining_power`](Pickaxe::mining_power) applied against block
//! [`hardness`](crate::block::Block::hardness) each tick, how much the block that
//! falls is worth ([`fortune_multiplier`](Pickaxe::fortune_multiplier)), and —
//! through the tier alone — which blocks the player is allowed to mine at all
//! (see [`can_mine`](Pickaxe::can_mine)).
//!
//! Speed, yield, permission: the three are deliberately separate answers. Nothing
//! here composes them into a swing — that is the phase-7 tick's job, which is why
//! each is handed out as its own primitive.

use crate::block::Block;
use crate::enchant::{EnchantType, Enchants};
use crate::error::CoreError;
use crate::tunables::HASTE_PER_LEVEL;

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
    tier: PickaxeTier,
    /// The enchantments on the pickaxe, which modify its mining power and other
    /// properties.
    enchants: Enchants,
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

    /// Returns the tier of the pickaxe.
    pub fn get_tier(&self) -> PickaxeTier {
        self.tier
    }

    /// Whether this pickaxe is allowed to break `block` at all: its tier must
    /// reach the block's [`min_pickaxe_tier`](Block::min_pickaxe_tier).
    ///
    /// **The comparison is `>=`, and the boundary is the rule, not an edge case.**
    /// A block's required tier is the one that *opens* it, not the one that is
    /// still too weak for it: a Stone pickaxe exists to earn the Iron that asks for
    /// Stone. A `>` would lock every block to the tier above its own and leave each
    /// rung of the ladder unable to mine the material it is named for.
    ///
    /// **Reads the tier and nothing else.** Efficiency and Haste buy *speed*; they
    /// never buy *permission*, however far they are pushed — which is what keeps
    /// tier a progression axis rather than a number the other levers can drown out.
    /// This is one half of the two-axis gate: tier opens mines, mining level opens
    /// worlds (phase 6), and neither alone advances the player.
    ///
    /// **A refusal is total, never a slowdown.** Minecraft lets the wrong tool chip
    /// away regardless, swapping its 30 divisor for 100; Skylode has no second
    /// regime, which is why [`TICKS_PER_HARDNESS`](crate::mine) is the mine's only
    /// conversion constant. This predicate is the other side of that decision: below
    /// the tier the answer is no, not "slowly". See `docs/DECISIONS.md`.
    ///
    /// `pub`, and it needs no gatekeeping: a predicate changes nothing, so unlike
    /// [`upgrade`](Pickaxe::upgrade) there is no free transaction to protect. A
    /// front-end is owed this one — it is what greys out a mine the player cannot
    /// open yet.
    ///
    /// Nothing enforces it yet. The gate that bites is the *mine's*, and it lands
    /// with the phase-7 mine selection as
    /// `pickaxe.can_mine(kind.common_block())` — one cell answers for both, because
    /// a mine's two cells always share a tier (see
    /// [`MineKind::gating_tier`](crate::mine_kind::MineKind::gating_tier)). Inside a
    /// mine the player was let into, no standing cell can fail this, which is why
    /// [`dig`](crate::mine::Mine) does not re-ask the question per swing.
    pub fn can_mine(&self, block: Block) -> bool {
        self.tier >= block.min_pickaxe_tier()
    }

    /// What one broken block is worth in loot: the count in
    /// [`Block::drops`](Block::drops) times `1 + level` of
    /// [`Fortune`](EnchantType::Fortune), so an un-fortuned pickaxe multiplies by
    /// exactly 1 and takes what the block contained.
    ///
    /// **The `1 +` is the whole deliverable, and it is not a rounding detail.**
    /// `docs/MECHANICS.md` says a block "yields its drop times Fortune"; read
    /// literally, level 0 multiplies the loot by *zero* and the game has no drops
    /// at all. The identity at level 0 is what leaves `Block::drops` true for a
    /// player who never enchanted — the same load-bearing role
    /// [`haste_multiplier`](Pickaxe::haste_multiplier)'s `1.0` plays for the
    /// hardness table.
    ///
    /// **A `u32`, where the speed levers are `f32`.** Loot is a count of items, so
    /// an exact multiplier is what keeps "what do I round, and which way?" from
    /// ever becoming a question. The cap of 10 makes the largest product `9 * 11`,
    /// which is why no overflow check is needed here rather than merely absent.
    ///
    /// **It reads the block's material through no argument at all**, and that is
    /// the design: Fortune applies to every cell equally. Minecraft exempts
    /// anything that drops itself — every dense block, Obsidian, Ancient Debris —
    /// which would leave the enchant inert across the whole endgame and make mine
    /// richness *shrink* Fortune's reach instead of scaling it. `docs/DECISIONS.md`
    /// settles the other way on both counts, so the fidelity this crate keeps 1:1
    /// for hardness stops here.
    ///
    /// **Loot only, never XP.** Experience is granted per item the block
    /// *contained* — `Block::drops`, before this multiplier ever runs. If Fortune
    /// paid both, one purchase would advance the pickaxe axis and the level axis at
    /// once, and the two-axis gate the whole progression rests on would collapse to
    /// one. See `docs/DECISIONS.md`.
    ///
    /// Nothing multiplies anything yet: the phase-7 tick is what puts this beside a
    /// broken block, as `inventory.add(item, n * pickaxe.fortune_multiplier())`.
    /// `pub` and ungated — a pure function guards neither freeness nor cadence, and
    /// a front-end needs it to quote the yield of an upgrade before it is bought.
    pub fn fortune_multiplier(&self) -> u32 {
        1 + u32::from(self.enchants.get_level(EnchantType::Fortune))
    }

    /// The multiplier the permanent [`Haste`](EnchantType::Haste) enchant applies
    /// to the pickaxe's speed: `1 + HASTE_PER_LEVEL * level`, so an un-hasted
    /// pickaxe multiplies by exactly `1.0` and is left alone.
    ///
    /// That identity at level 0 is load-bearing, not a happy accident: every break
    /// time this game inherits 1:1 from Minecraft is measured on a pickaxe with no
    /// Haste, so a multiplier that missed `1.0` would silently re-time the whole
    /// hardness table.
    ///
    /// **Only the permanent source.** The spec's `haste_multiplier` is a *product*
    /// of two things (`docs/MECHANICS.md`), and the other one — the temporary
    /// Redstone boost — is deliberately not here: it is run state carrying a
    /// tick-based timer, not a property of the tool, and it would drag a countdown
    /// into a struct the save serialises as an inert `(tier, enchants)` pair. Phase
    /// 5 keeps it on the game state and folds it in where the two meet, in the
    /// tick: `pickaxe.mining_power() * boost`.
    fn haste_multiplier(&self) -> f32 {
        let level = self.enchants.get_level(EnchantType::Haste);
        1.0 + HASTE_PER_LEVEL * level as f32
    }

    /// Computes the total mining power applied against block hardness.
    ///
    /// Combines the tier's [`base_power`](PickaxeTier::base_power) with an
    /// Efficiency bonus of `level² + 1`, and the squaring is what makes high
    /// Efficiency the main long-term power lever. The sum is then scaled by
    /// [`haste_multiplier`](Pickaxe::haste_multiplier).
    ///
    /// **The bonus is earned from level 1, not level 0**, which is Minecraft's
    /// `if (i > 0)` guard in `Player.getDigSpeed`. The `+ 1` is therefore not a
    /// flat bonus every pickaxe collects: it is what makes the *first* level a
    /// discrete jump of `+2` rather than the `+1` a bare `level²` would give — a
    /// deliberate kick that stops rank 1 from feeling like nothing. Reading it as
    /// unconditional costs a fresh Wooden pickaxe 50% of its speed (3 instead of
    /// 2), and with it the 1:1 break times `Mine`'s conversion exists to preserve.
    ///
    /// **Haste multiplies the whole sum, Efficiency included** — the parentheses
    /// are the design, not formatting. `base * haste + eff_bonus` type-checks just
    /// as well and quietly turns the two enchants into rivals: Efficiency's square
    /// would then be the one term Haste cannot touch, so past a point the player
    /// would be choosing between them instead of compounding them. Multiplying the
    /// sum is what lets an additive lever and a multiplicative one sit on different
    /// layers and stack, which is the reason the game has both.
    pub fn mining_power(&self) -> f32 {
        let base = self.tier.base_power();
        let level = self.enchants.get_level(EnchantType::Efficiency);
        let eff_bonus = if level > 0 {
            (level as f32).powi(2) + 1.0
        } else {
            0.0
        };
        (base + eff_bonus) * self.haste_multiplier()
    }

    /// Advances the pickaxe one step along its upgrade path.
    ///
    /// The upgrade curve is two-phase:
    /// 1. While Efficiency is below its tier-dependent cap
    ///    ([`efficiency_cap`](PickaxeTier::efficiency_cap)), each call bumps
    ///    Efficiency.
    /// 2. Once Efficiency is maxed, it is reset to 0 and the pickaxe advances to
    ///    the next tier — so the player re-climbs Efficiency on a stronger base.
    ///
    /// At [`Netherite`](PickaxeTier::Netherite) Efficiency climbs to its raised
    /// cap of 15 and the pickaxe is then fully upgraded: further calls are
    /// **refused** with [`CoreError::PickaxeFullyUpgraded`]. They must not fall
    /// through to phase 2, which would reset Efficiency with no tier left to gain
    /// in exchange — a permanent downgrade from 235 mining power back to 9. The
    /// refusal is what lets the paid path (phase 5) decline the sale instead of
    /// charging for that downgrade.
    ///
    /// `pub(crate)` because it is **free**: it consults no inventory and debits
    /// nothing. A front-end that could call it could walk a fresh player to a
    /// maxed Netherite pickaxe in thirty calls. The paid path will check solvency,
    /// debit, and then delegate here.
    // Dead outside the tests until the phase-5 purchase path calls it. `expect`
    // rather than `allow`, so the lint fires again — and this line can go — the
    // moment a real caller appears; `cfg_attr` because the tests *do* call it, and
    // an unconditional expectation would go unfulfilled in the test build.
    #[cfg_attr(not(test), expect(dead_code, reason = "awaiting the phase-5 economy"))]
    pub(crate) fn upgrade(&mut self) -> Result<(), CoreError> {
        let efficiency = EnchantType::Efficiency;

        if self.enchants.get_level(efficiency) < self.tier.efficiency_cap() {
            // The guard is the same one `Enchants::upgrade_efficiency` re-checks,
            // so this can only be `Ok` — propagating rather than discarding keeps
            // that a fact the compiler enforces instead of a comment.
            self.enchants.upgrade_efficiency(self.tier)?;
        } else if let Some(next_tier) = self.tier.next() {
            self.enchants.reset_level(efficiency);
            self.tier = next_tier;
        } else {
            return Err(CoreError::PickaxeFullyUpgraded);
        }
        Ok(())
    }
}

impl PickaxeTier {
    /// Returns the flat mining power contributed by the tier alone, before
    /// enchantments.
    ///
    /// The curve is **strictly monotone**, and deliberately not Minecraft's,
    /// where Gold out-mines Diamond. A tier jump already costs the player their
    /// Efficiency, so it is meant to be a *dip*: slower for a while, then better
    /// than ever. If the base power itself went backwards — Gold above Diamond —
    /// that dip would become a permanent regression, and the player would be
    /// right to refuse the upgrade. The 1:1 fidelity to Minecraft is kept for
    /// hardness only. See `docs/DECISIONS.md`.
    ///
    /// Tier also gates *which* blocks are mineable, via
    /// [`Block::min_pickaxe_tier`](crate::block::Block::min_pickaxe_tier).
    pub fn base_power(&self) -> f32 {
        match self {
            PickaxeTier::Wooden => 2.0,
            PickaxeTier::Stone => 4.0,
            PickaxeTier::Iron => 6.0,
            PickaxeTier::Gold => 7.0,
            PickaxeTier::Diamond => 8.0,
            PickaxeTier::Netherite => 9.0,
        }
    }

    /// Returns the level ceiling this tier grants to
    /// [`Efficiency`](crate::enchant::EnchantType::Efficiency): 5 everywhere, 15
    /// at [`Netherite`](PickaxeTier::Netherite).
    ///
    /// That raised cap is the whole reward for reaching the final tier: past
    /// Netherite there is no [`next`](PickaxeTier::next) to climb, so the ten
    /// extra Efficiency levels are what the upgrade path spends itself into.
    ///
    /// Efficiency is the one enchant keyed by the **tier**, which is why its
    /// ceiling lives here and not beside the other caps. The five special
    /// enchants are keyed by the world instead
    /// ([`World::enchant_cap`](crate::world::World::enchant_cap)), and Fortune by
    /// nothing at all. [`EnchantType::max_level`](crate::enchant::EnchantType::max_level)
    /// is the generic front door that dispatches to whichever of the three
    /// applies; each of them stays the single source of its own number.
    ///
    /// Keeping this one here is what lets [`Pickaxe::upgrade`] ask its question —
    /// "how far can Efficiency go on *this tier*" — without being handed a
    /// [`World`](crate::world::World) it would provably never read.
    ///
    /// [`Pickaxe::upgrade`]: Pickaxe::upgrade
    pub fn efficiency_cap(self) -> u8 {
        match self {
            PickaxeTier::Netherite => 15,
            _ => 5,
        }
    }

    /// Returns the tier one step up the upgrade ladder, or `None` at
    /// [`Netherite`](PickaxeTier::Netherite).
    ///
    /// The ladder is deliberately a *partial* function. Modelling it as a total
    /// one — with Netherite mapping to itself — reads as "there is always a next
    /// tier", which let `Pickaxe::upgrade` treat the top of the ladder as an
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

    use crate::world::World;

    /// The world these helpers buy Haste in.
    ///
    /// The End, whose [`enchant_cap`](World::enchant_cap) is the game's highest, so
    /// a test asking for a Haste level is never throttled by a ceiling it is not
    /// about. Efficiency needs no world here — that is the whole point of
    /// [`Enchants::upgrade_efficiency`].
    const HASTE_WORLD: World = World::End;

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

    /// A pickaxe of `tier` carrying explicit Efficiency and Haste levels.
    ///
    /// Goes through `Enchants::upgrade` rather than seeding the map directly, so a
    /// level this helper hands out is one the game would actually sell — a test
    /// built on a level past the cap would be measuring a pickaxe no player can
    /// hold.
    fn enchanted(tier: PickaxeTier, efficiency: u8, haste: u8) -> Pickaxe {
        let mut enchants = Enchants::new();
        for _ in 0..efficiency {
            assert!(enchants.upgrade_efficiency(tier).is_ok());
        }
        for _ in 0..haste {
            assert!(
                enchants
                    .upgrade(EnchantType::Haste, tier, HASTE_WORLD)
                    .is_ok()
            );
        }
        Pickaxe::new(tier, enchants)
    }

    /// A pickaxe carrying `level` of Fortune and nothing else.
    ///
    /// Goes through `Enchants::upgrade` for the reason [`enchanted`] does — a level
    /// no shop would sell measures a pickaxe no player can hold. The tier and world
    /// it passes are both arbitrary: Fortune is keyed by neither, which
    /// `the_five_special_enchants_and_fortune_ignore_the_pickaxe_tier` and
    /// `efficiency_and_fortune_ignore_the_world` in `enchant` are what establish.
    ///
    /// Deliberately *not* a fourth parameter on [`enchanted`]: that would make every
    /// existing call site carry a `, 0` for an enchant it does not test, and turn a
    /// readable helper into four positional integers. The tier is Wooden and
    /// arbitrary — [`Pickaxe::fortune_multiplier`] takes no tier and no block, so
    /// varying either would assert the absence of a term the signature cannot even
    /// express.
    fn fortuned(level: u8) -> Pickaxe {
        let mut enchants = Enchants::new();
        for _ in 0..level {
            assert!(
                enchants
                    .upgrade(EnchantType::Fortune, PickaxeTier::Wooden, HASTE_WORLD)
                    .is_ok()
            );
        }
        Pickaxe::new(PickaxeTier::Wooden, enchants)
    }

    /// Drives a fresh pickaxe all the way to Netherite with no Efficiency.
    fn maxed_tier_pickaxe() -> Pickaxe {
        let mut pickaxe = Pickaxe::default();
        for _ in 0..(UPGRADES_PER_TIER * (ALL_TIERS.len() - 1)) {
            assert!(pickaxe.upgrade().is_ok());
        }
        pickaxe
    }

    /// A Netherite pickaxe with Efficiency at its raised cap of 15: the end of
    /// the upgrade path.
    fn fully_upgraded_pickaxe() -> Pickaxe {
        let mut pickaxe = maxed_tier_pickaxe();
        for _ in 0..15 {
            assert!(pickaxe.upgrade().is_ok());
        }
        pickaxe
    }

    #[test]
    fn a_fresh_pickaxe_is_wooden_and_unenchanted() {
        let pickaxe = Pickaxe::default();
        assert_eq!(pickaxe.get_tier(), PickaxeTier::Wooden);
        assert_eq!(pickaxe.enchants.get_level(EnchantType::Efficiency), 0);
    }

    /// An unenchanted pickaxe is its tier and nothing else: a fresh one mines at
    /// 2 (Wooden), not 3. Minecraft gates the whole `level² + 1` bonus behind
    /// `if (i > 0)`, and reading the `+ 1` as unconditional hands every pickaxe a
    /// free Efficiency-worth of speed it never enchanted for.
    ///
    /// This is the floor the rest of the game's pacing is measured from — it is
    /// what makes `a_diamond_pickaxe_clears_obsidian_in_the_ticks_minecraft_charges`
    /// land on the wiki's number — so it is pinned here rather than left implied
    /// by [`efficiency_scales_quadratically`], which only ever looks above 0.
    #[test]
    fn an_unenchanted_pickaxe_earns_no_efficiency_bonus() {
        assert_eq!(Pickaxe::default().mining_power(), 2.0);
    }

    /// The first level of Efficiency is a *step*, not the start of a ramp: the
    /// `+ 1` rides along with it, so level 1 is worth `1² + 1 = 2` power where a
    /// bare `level²` would be worth 1.
    ///
    /// Worth its own test because the shape is easy to lose in either direction —
    /// dropping the `+ 1` flattens the kick that stops rank 1 from feeling like
    /// nothing, and paying it at level 0 dissolves the step entirely.
    #[test]
    fn the_first_level_of_efficiency_is_a_discrete_jump() {
        let unenchanted = Pickaxe::default();

        let mut enchants = Enchants::new();
        assert!(enchants.upgrade_efficiency(PickaxeTier::Wooden).is_ok());
        let level_one = Pickaxe::new(PickaxeTier::Wooden, enchants);

        assert_eq!(unenchanted.mining_power(), 2.0);
        assert_eq!(level_one.mining_power(), 4.0);
        assert_eq!(
            level_one.mining_power() - unenchanted.mining_power(),
            2.0,
            "the first Efficiency level must jump by 1² + 1, not by 1"
        );
    }

    /// Efficiency squares, so it is the long-term lever: on the same Wooden
    /// base, level 5 is worth 26 power where level 1 is worth 2.
    #[test]
    fn efficiency_scales_quadratically() {
        let tier = PickaxeTier::Wooden;
        let mut enchants = Enchants::new();
        assert!(enchants.upgrade_efficiency(tier).is_ok());
        let level_one = Pickaxe::new(PickaxeTier::Wooden, enchants.clone());
        assert_eq!(level_one.mining_power(), 2.0 + 1.0 + 1.0);

        for _ in 0..4 {
            assert!(enchants.upgrade_efficiency(tier).is_ok());
        }
        let level_five = Pickaxe::new(PickaxeTier::Wooden, enchants);
        assert_eq!(level_five.mining_power(), 2.0 + 25.0 + 1.0);
    }

    /// **The shape of the whole formula, in one assertion**: Haste scales the
    /// Efficiency bonus as well as the tier, because it multiplies the sum.
    ///
    /// The rival reading — `base * haste + eff_bonus`, scaling only the tier —
    /// type-checks identically and is wrong in a way no other test here would
    /// catch: it makes Efficiency's square the one term Haste cannot reach, so the
    /// two enchants stop compounding and start competing for the same upgrade.
    /// Levels are picked where the two formulas *disagree* (Diamond 8 + Efficiency
    /// 5 = 34, doubled = 68; the rival gives 8×2 + 26 = 42). At Efficiency 0 or
    /// Haste 0 they coincide, and the test would pass while proving nothing.
    #[test]
    fn haste_multiplies_the_efficiency_bonus_and_not_just_the_tier() {
        let tier = PickaxeTier::Diamond;
        let unhasted = enchanted(tier, 5, 0);
        let hasted = enchanted(tier, 5, 5);

        assert_eq!(unhasted.mining_power(), 34.0, "8 (Diamond) + 5² + 1");
        assert_eq!(
            hasted.mining_power(),
            68.0,
            "Haste 5 must double the whole 34, not just the tier — 42.0 is the \
             reading that leaves the Efficiency bonus behind"
        );
        assert_eq!(
            hasted.mining_power(),
            unhasted.mining_power() * 2.0,
            "hasting a pickaxe scales what it already had"
        );
    }

    /// Haste 0 must multiply by *exactly* 1.0, at every tier and every Efficiency.
    ///
    /// This is the test that says adding a multiplicative layer changed nothing for
    /// a pickaxe that has not bought into it. Every break time Skylode inherits
    /// 1:1 from Minecraft — `mine`'s golden Obsidian-in-188-ticks among them — is
    /// quoted on an un-hasted pickaxe, so a multiplier that missed 1.0 by a hair
    /// would re-time the entire hardness table and every save with it.
    #[test]
    fn an_unhasted_pickaxe_keeps_exactly_its_additive_power() {
        for &tier in &ALL_TIERS {
            assert_eq!(
                enchanted(tier, 0, 0).mining_power(),
                tier.base_power(),
                "{tier:?} unenchanted must be worth its base power and nothing else"
            );
            assert_eq!(
                enchanted(tier, 5, 0).mining_power(),
                tier.base_power() + 26.0,
                "{tier:?} at Efficiency 5 must be worth base + 5² + 1"
            );
        }
    }

    /// Haste's rungs are evenly spaced 20% steps — `2.0, 2.4, 2.8` on a bare
    /// Wooden pickaxe — and that is the contrast worth pinning against
    /// `the_first_level_of_efficiency_is_a_discrete_jump`: the two speed enchants
    /// deliberately have different first rungs. Efficiency opens with a `+2` kick
    /// so rank 1 does not feel like nothing; Haste needs no kick, because a
    /// multiplier is felt in proportion to the power it multiplies, and the pickaxe
    /// that can afford Haste already has power to multiply.
    ///
    /// Pins the three values rather than asserting the *differences* are equal.
    /// They are not, quite: `0.2` has no exact binary form, so the measured steps
    /// come out `0.4000001` and `0.39999986`. Nothing in the game compares one
    /// increment against another — the formula is affine in `level` and is read
    /// whole — so that spread is a fact about `f32`, not about the design. Three
    /// exact numbers state the linearity without asking the arithmetic for a
    /// promise it cannot keep.
    #[test]
    fn hastes_rungs_are_evenly_spaced_linear_steps() {
        let tier = PickaxeTier::Wooden;

        assert_eq!(enchanted(tier, 0, 0).mining_power(), 2.0);
        assert_eq!(enchanted(tier, 0, 1).mining_power(), 2.4);
        assert_eq!(enchanted(tier, 0, 2).mining_power(), 2.8);
    }

    /// A level of Haste must never be a downgrade — the same guarantee
    /// `base_power_never_goes_backwards_up_the_ladder` makes for the tier ladder,
    /// and for the same reason: a lever the player can rationally refuse is a lever
    /// that does not belong in the upgrade path. Swept from `max_level` — now the
    /// End's [`World::enchant_cap`](crate::world::World::enchant_cap) — so a
    /// re-balance of that ceiling keeps the guarantee under test rather than
    /// quietly shrinking its range.
    #[test]
    fn haste_never_slows_the_pickaxe() {
        let tier = PickaxeTier::Netherite;
        let cap = EnchantType::Haste.max_level(tier, HASTE_WORLD);
        assert!(cap > 0, "a Haste that cannot be earned proves nothing here");

        let mut previous = enchanted(tier, 15, 0).mining_power();
        for haste in 1..=cap {
            let power = enchanted(tier, 15, haste).mining_power();
            assert!(
                power > previous,
                "Haste {haste} ({power}) mines no faster than Haste {} ({previous})",
                haste - 1
            );
            previous = power;
        }
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

    /// Pins the balance table. These six numbers set the pace of the entire
    /// run, so a change to any of them should be a deliberate one that also
    /// updates this test.
    #[test]
    fn each_tier_has_its_balanced_base_power() {
        assert_eq!(PickaxeTier::Wooden.base_power(), 2.0);
        assert_eq!(PickaxeTier::Stone.base_power(), 4.0);
        assert_eq!(PickaxeTier::Iron.base_power(), 6.0);
        assert_eq!(PickaxeTier::Gold.base_power(), 7.0);
        assert_eq!(PickaxeTier::Diamond.base_power(), 8.0);
        assert_eq!(PickaxeTier::Netherite.base_power(), 9.0);
    }

    /// The load-bearing property of the whole tier ladder, and the reason we
    /// broke with Minecraft: a stronger tier must never mine slower than a weaker
    /// one. Minecraft's Gold out-speeds Diamond, and porting that here would turn
    /// the tier-jump dip — which is meant to be temporary, paid back once
    /// Efficiency is re-climbed — into a permanent downgrade the player should
    /// rationally refuse.
    ///
    /// Nothing else guards this. `each_tier_has_its_balanced_base_power` pins the
    /// six numbers, but a rebalance that edits all six could quietly reintroduce a
    /// dent and still pass it.
    #[test]
    fn base_power_never_goes_backwards_up_the_ladder() {
        for pair in ALL_TIERS.windows(2) {
            let (tier, next) = (pair[0], pair[1]);
            assert!(
                next.base_power() > tier.base_power(),
                "{next:?} ({}) mines slower than {tier:?} ({}), so taking the upgrade would be a mistake",
                next.base_power(),
                tier.base_power()
            );
        }
    }

    /// [`can_mine`](Pickaxe::can_mine) is a single `>=` on this `Ord`, so the whole
    /// gate rests on it following declaration order.
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

    /// The boundary *is* the rule: a block's required tier is the one that opens
    /// it. `>` instead of `>=` type-checks, passes any test that only looks at
    /// tiers far apart, and quietly locks every block to the rung above its own —
    /// so the Stone pickaxe could never mine the Iron it exists to earn, and the
    /// ladder would have no first step.
    ///
    /// Both sides are asserted, because either alone is satisfiable by a constant:
    /// a `can_mine` that always says yes passes the first, always-no passes the
    /// second.
    #[test]
    fn the_gate_admits_a_block_at_exactly_its_own_tier() {
        assert_eq!(Block::IronOre.min_pickaxe_tier(), PickaxeTier::Stone);

        assert!(
            enchanted(PickaxeTier::Stone, 0, 0).can_mine(Block::IronOre),
            "a Stone pickaxe must mine the Iron that asks for exactly Stone"
        );
        assert!(
            !enchanted(PickaxeTier::Wooden, 0, 0).can_mine(Block::IronOre),
            "a Wooden pickaxe must not reach Iron"
        );
    }

    /// Fortune 0 multiplies by exactly 1: a player who never enchanted walks away
    /// with what the block contained, and `Block::drops` is the whole answer.
    ///
    /// The identity is the entire reason the formula reads `1 + level`. `level`
    /// alone type-checks just as well, and hands every unenchanted block in the game
    /// a drop of zero — the game's opening minute, wiped by an off-by-one.
    #[test]
    fn an_unfortuned_pickaxe_multiplies_its_loot_by_exactly_one() {
        assert_eq!(Pickaxe::default().fortune_multiplier(), 1);
        assert_eq!(fortuned(0).fortune_multiplier(), 1);
    }

    /// At its cap Fortune is worth **eleven** times the block, not ten: the last
    /// level bought is `1 + 10`.
    ///
    /// The cap is read from `max_level` to *build* the pickaxe but the expectation
    /// is the literal 11, on purpose. Computing `cap + 1` here would restate the
    /// implementation and assert `1 + level == level + 1` — true by construction,
    /// the trap `the_gate_admits_a_block_at_exactly_its_own_tier` avoids by naming
    /// its tiers. Reading the cap still means phase 4's per-world caps re-ask the
    /// question here instead of drifting past it: move the cap and this fails, which
    /// is the point.
    ///
    /// With the level-0 identity above, two points pin the affine map exactly — a
    /// third level would measure nothing a line does not already say.
    #[test]
    fn fortune_at_its_cap_is_worth_eleven_times_the_block() {
        let cap = EnchantType::Fortune.max_level(PickaxeTier::Wooden, HASTE_WORLD);
        assert_eq!(fortuned(cap).fortune_multiplier(), 11);
    }

    /// Phase 1 of the upgrade curve: spend upgrades on Efficiency while the cap
    /// allows, keeping the tier put.
    #[test]
    fn upgrades_fill_efficiency_before_touching_the_tier() {
        let mut pickaxe = Pickaxe::default();
        for expected_level in 1..=5 {
            assert!(pickaxe.upgrade().is_ok());
            assert_eq!(pickaxe.get_tier(), PickaxeTier::Wooden);
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
            assert!(pickaxe.upgrade().is_ok());
        }
        assert_eq!(pickaxe.get_tier(), PickaxeTier::Stone);
        assert_eq!(pickaxe.enchants.get_level(EnchantType::Efficiency), 0);
    }

    #[test]
    fn thirty_upgrades_walk_the_whole_tier_ladder() {
        let pickaxe = maxed_tier_pickaxe();
        assert_eq!(pickaxe.get_tier(), PickaxeTier::Netherite);
        assert_eq!(pickaxe.enchants.get_level(EnchantType::Efficiency), 0);
    }

    /// Netherite raises the Efficiency cap from 5 to 15, which is what makes the
    /// final tier worth reaching despite its middling base power.
    #[test]
    fn netherite_efficiency_climbs_to_fifteen() {
        let mut pickaxe = maxed_tier_pickaxe();
        for expected_level in 1..=15 {
            assert!(pickaxe.upgrade().is_ok());
            assert_eq!(pickaxe.get_tier(), PickaxeTier::Netherite);
            assert_eq!(
                pickaxe.enchants.get_level(EnchantType::Efficiency),
                expected_level
            );
        }
        // 9 (Netherite) + 15² + 1
        assert_eq!(pickaxe.mining_power(), 235.0);
    }

    /// The final tier has nowhere left to advance to, so an upgrade there must
    /// never be a *downgrade*. Wiping Efficiency at the ceiling would drop the
    /// player from 235 mining power back to 9 — permanently, since the tier can
    /// no longer climb to compensate.
    ///
    /// The refusal is the other half: an upgrade that silently did nothing would
    /// look, from the till, exactly like one that worked, so the paid path would
    /// take the player's ore and hand back the pickaxe they already had.
    #[test]
    fn upgrading_a_fully_maxed_pickaxe_is_refused_rather_than_silently_ignored() {
        let mut pickaxe = fully_upgraded_pickaxe();
        let maxed_power = pickaxe.mining_power();

        assert_eq!(pickaxe.upgrade(), Err(CoreError::PickaxeFullyUpgraded));

        assert_eq!(pickaxe.get_tier(), PickaxeTier::Netherite);
        assert_eq!(
            pickaxe.mining_power(),
            maxed_power,
            "the refused upgrade moved the pickaxe anyway"
        );
    }
}
