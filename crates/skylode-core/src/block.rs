//! Mineable blocks.
//!
//! A [`Block`] is a single cell of a [`Mine`](crate::mine::Mine). Each block
//! carries the four properties the mining loop needs: the [`Material`] it
//! drops, its [`hardness`](Block::hardness) (mining time), the
//! [`World`] it belongs to, and the minimum
//! [`PickaxeTier`] required to break it.
//!
//! Many resources come in two forms: an *ore* variant (drops a single item) and
//! a *dense* variant (`IronBlock`, `Cobblestone`, …) which is tougher and drops
//! nine, mirroring Minecraft's nine-ingots-per-block convention.
//!
//! A dense block is **not** a Compressed unit. This module deals in cells you
//! swing a pickaxe at; a Compressed unit is a denomination the player mints in
//! their inventory, worth a hundred raw, that no block in the ground contains.
//! Nine versus a hundred, mined versus minted — see [`Item`].

use crate::material::{Item, Material};
use crate::pickaxe::PickaxeTier;
use crate::world::World;

/// Number of raw items a *dense* block yields when mined.
///
/// Matches Minecraft's crafting ratio: nine ingots or gems make one block, so
/// breaking that block returns nine. Private, and it stays that way: this is a
/// balance number, and callers who want to know what a block is worth should ask
/// the block, via [`Block::drops`]. Unrelated to
/// [`RAW_PER_COMPRESSED`](crate::tunables::RAW_PER_COMPRESSED) (100), which *is*
/// public, because it is a denomination the UI must know in order to render a
/// price — different ratio, different concept, different audience.
const RAW_PER_DENSE_BLOCK: u32 = 9;

/// A single mineable block.
///
/// Variants are grouped as `<Resource>Ore` / `<Resource>Block` pairs where a
/// dense form exists, plus standalone blocks (Netherrack, Obsidian, …) that have
/// no dual form. The whole enum is `Copy` because a block is just a lightweight
/// tag with no owned data.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    // --- Overworld ---
    #[default]
    Stone,
    Cobblestone,
    CoalOre,
    CoalBlock,
    IronOre,
    IronBlock,
    GoldOre,
    GoldBlock,
    LapisOre,
    LapisBlock,
    RedstoneOre,
    RedstoneBlock,
    EmeraldOre,
    EmeraldBlock,
    DiamondOre,
    DiamondBlock,
    // --- Nether ---
    Netherrack,
    QuartzOre,
    AncientDebris,
    NetheriteBlock,
    Obsidian,
    CryingObsidian,
    // --- End ---
    Endstone,
    Amethyst,
}

impl Block {
    /// Returns the material the block is made of.
    ///
    /// Total, not partial: *every* block yields something, fillers included.
    /// Netherrack and End Stone drop their own material, exactly as Stone does in
    /// the Overworld — a filler is the block the player breaks most often, and one
    /// that paid nothing would be pure spent time. An `Option` here would hand
    /// every caller a `None` branch that can never be taken.
    pub fn material(self) -> Material {
        match self {
            Self::Stone | Self::Cobblestone => Material::Stone,
            Self::CoalOre | Self::CoalBlock => Material::Coal,
            Self::IronOre | Self::IronBlock => Material::Iron,
            Self::GoldOre | Self::GoldBlock => Material::Gold,
            Self::LapisOre | Self::LapisBlock => Material::Lapis,
            Self::RedstoneOre | Self::RedstoneBlock => Material::Redstone,
            Self::EmeraldOre | Self::EmeraldBlock => Material::Emerald,
            Self::DiamondOre | Self::DiamondBlock => Material::Diamond,
            Self::Netherrack => Material::Netherrack,
            Self::QuartzOre => Material::Quartz,
            Self::AncientDebris | Self::NetheriteBlock => Material::AncientDebris,
            Self::Obsidian => Material::Obsidian,
            Self::CryingObsidian => Material::CryingObsidian,
            Self::Endstone => Material::Endstone,
            Self::Amethyst => Material::Amethyst,
        }
    }

    /// Returns the hardness of the block,
    /// which determines how long it takes to mine.
    ///
    /// Hardness is **not** in the same units as a pickaxe's
    /// [`mining_power`](crate::pickaxe::Pickaxe::mining_power); the two scales are
    /// Minecraft's two scales, and `mine`'s `TICKS_PER_HARDNESS` is the conversion
    /// between them. A block yields after `30 * hardness` of accumulated power, so
    /// it takes `ceil(30 * hardness / mining_power)` ticks. Reading the two as one
    /// scale makes every block in the game an instamine.
    ///
    /// The values are Minecraft's, exactly (Stone `1.5`, Obsidian `50.0`) — this is
    /// the one table `DECISIONS.md` keeps 1:1, which is what lets the break times
    /// come out 1:1 too and spares us re-deriving a balance pass Mojang already did.
    /// The *dense* forms are the exception, tougher than their ore counterparts —
    /// that toughness is what they cost you for the nine items they give back.
    pub fn hardness(self) -> f32 {
        match self {
            Self::Stone => 1.5,
            Self::Cobblestone => 2.0,
            Self::CoalOre => 3.0,
            Self::CoalBlock => 5.0,
            Self::IronOre => 3.0,
            Self::IronBlock => 5.0,
            Self::GoldOre => 3.0,
            Self::GoldBlock => 5.0,
            Self::LapisOre => 3.0,
            Self::LapisBlock => 5.0,
            Self::RedstoneOre => 3.0,
            Self::RedstoneBlock => 5.0,
            Self::EmeraldOre => 3.0,
            Self::EmeraldBlock => 5.0,
            Self::DiamondOre => 3.0,
            Self::DiamondBlock => 5.0,
            Self::Netherrack => 0.4,
            Self::QuartzOre => 3.0,
            Self::AncientDebris => 30.0,
            Self::NetheriteBlock => 50.0,
            Self::Obsidian => 50.0,
            Self::CryingObsidian => 50.0,
            Self::Endstone => 3.0,
            Self::Amethyst => 1.5,
        }
    }

    /// Returns the world in which the block can be found.
    pub fn world(self) -> World {
        match self {
            Self::Stone
            | Self::Cobblestone
            | Self::CoalOre
            | Self::CoalBlock
            | Self::IronOre
            | Self::IronBlock
            | Self::GoldOre
            | Self::GoldBlock
            | Self::LapisOre
            | Self::LapisBlock
            | Self::RedstoneOre
            | Self::RedstoneBlock
            | Self::DiamondOre
            | Self::DiamondBlock
            | Self::EmeraldOre
            | Self::EmeraldBlock => World::Overworld,
            Self::Netherrack
            | Self::QuartzOre
            | Self::AncientDebris
            | Self::NetheriteBlock
            | Self::Obsidian
            | Self::CryingObsidian => World::Nether,
            Self::Endstone | Self::Amethyst => World::End,
        }
    }

    /// Returns the minimum pickaxe tier required to mine the block.
    pub fn min_pickaxe_tier(self) -> PickaxeTier {
        match self {
            Self::Stone
            | Self::Cobblestone
            | Self::CoalOre
            | Self::CoalBlock
            | Self::Netherrack
            | Self::QuartzOre
            | Self::Endstone
            | Self::Amethyst => PickaxeTier::Wooden,
            Self::IronOre | Self::IronBlock | Self::LapisOre | Self::LapisBlock => {
                PickaxeTier::Stone
            }
            Self::GoldOre
            | Self::GoldBlock
            | Self::RedstoneOre
            | Self::RedstoneBlock
            | Self::DiamondOre
            | Self::DiamondBlock
            | Self::EmeraldOre
            | Self::EmeraldBlock => PickaxeTier::Iron,
            Self::AncientDebris | Self::NetheriteBlock | Self::Obsidian | Self::CryingObsidian => {
                PickaxeTier::Diamond
            }
        }
    }

    /// The experience breaking this block grants, before
    /// [Fortune](crate::pickaxe::Pickaxe::fortune_multiplier) — which never multiplies
    /// it (see [`drops`](Block::drops)).
    ///
    /// **Not a function of the drop count, and that is the correction it exists to
    /// make.** XP once *was* the drop count — one per ore cell, nine per dense one —
    /// which tied the level curve to a number chosen for Minecraft's crafting
    /// fidelity, and had a consequence nobody picked: the three endgame mines are
    /// exactly the three with no dense form, so their cells granted one apiece while
    /// an Iron Block granted nine. The Iron mine out-levelled the End, 4 968 XP a full
    /// grid against 3 820. Loot and experience are now two tables, and
    /// [`RAW_PER_DENSE_BLOCK`] governs only the first.
    ///
    /// **A cell of value is worth three times its mine's common cell**, everywhere —
    /// including on the two-material mines, where the rare cell drops a single item.
    /// Three rather than nine so the richness dial still moves the level bar without
    /// the loot ratio dominating it.
    ///
    /// **The mines are ordered, and the ordering is a property rather than a balance
    /// pass.** A full grid is worth `base * (1 + 2w)` for a dial weight `w`, so it is
    /// proportional to the mine's base at *every* dial setting — rising bases order the
    /// twelve mines at all settings at once, which is what
    /// `xp_rises_with_the_progression_at_every_dial_setting` pins. The ordering holds
    /// per grid and per second *with* a boost; it does not hold per second without
    /// one, because Ancient Debris and Obsidian take 67 and 70 seconds a grid where
    /// every other mine takes ten. That is the gap the boost exists to close
    /// (`docs/DECISIONS.md`), so the one regime where the order breaks is the one the
    /// player is meant to spend Redstone on.
    ///
    /// Read by [`Player::grant_break_experience`](crate::player::Player), which is
    /// where the "before Fortune" above stops being a convention and becomes a
    /// signature — it is handed the blocks a swing broke and no pickaxe at all.
    ///
    /// Provisional; phase 10 balances the twelve bases.
    pub fn xp_value(self) -> u32 {
        match self {
            Self::Stone => 1,
            Self::Cobblestone => 3,
            Self::CoalOre => 2,
            Self::CoalBlock => 6,
            Self::IronOre => 3,
            Self::IronBlock => 9,
            Self::LapisOre => 4,
            Self::LapisBlock => 12,
            Self::GoldOre => 4,
            Self::GoldBlock => 12,
            Self::RedstoneOre => 4,
            Self::RedstoneBlock => 12,
            Self::EmeraldOre => 6,
            Self::EmeraldBlock => 18,
            Self::DiamondOre => 8,
            Self::DiamondBlock => 24,
            Self::Netherrack => 10,
            Self::QuartzOre => 30,
            Self::AncientDebris => 14,
            Self::NetheriteBlock => 42,
            Self::Obsidian => 18,
            Self::CryingObsidian => 54,
            Self::Endstone => 24,
            Self::Amethyst => 72,
        }
    }

    /// Amount of raw material dropped, before
    /// [Fortune](crate::pickaxe::Pickaxe::fortune_multiplier). Dense forms yield
    /// [`RAW_PER_DENSE_BLOCK`]; everything else yields 1.
    ///
    /// Private: it is half an answer. [`drops`](Block::drops) is the whole one,
    /// and pairing the count with the item is what keeps a caller from asking for
    /// the amount and forgetting to ask what it is an amount *of*.
    ///
    /// **Flat across materials, unlike Minecraft**, where Lapis drops 4–9 and
    /// Redstone 4–5. A per-ore count would duplicate the phase-5 cost curve rather
    /// than add to it: prices are quoted in each mine's *own* material, so dropping
    /// four times as much Lapis and charging four times as much for it is the same
    /// game with longer numbers. What distinguishes an ore here is what it buys —
    /// Redstone speed, Emerald Fortune, Lapis enchants — and the ore/dense split
    /// below, which mine richness turns into a dial. Should phase-10 balance want the
    /// variance back, it is a `match` arm here and nothing else: no caller reads a
    /// count it did not get from [`drops`](Block::drops).
    fn drop_amount(self) -> u32 {
        match self {
            Self::Cobblestone
            | Self::CoalBlock
            | Self::IronBlock
            | Self::GoldBlock
            | Self::LapisBlock
            | Self::RedstoneBlock
            | Self::EmeraldBlock
            | Self::DiamondBlock
            | Self::NetheriteBlock => RAW_PER_DENSE_BLOCK,
            _ => 1,
        }
    }

    /// What this block *contains*, before [Fortune]: an item, and how many of it.
    ///
    /// Always an [`Item::Raw`], and that is the point of stating it as an `Item`
    /// at all: nothing in the ground is worth a hundred. A Compressed unit is
    /// minted, a hundred raw at a time, and never dug up.
    ///
    /// This is the block's own drop table, not the outcome of a mining tick. The
    /// [`Excavator`] enchant can *substitute* a Compressed unit for what came out
    /// of the ground, but that is a property of the pickaxe swinging, not of the
    /// rock being swung at — it belongs to the mining loop, which starts here and
    /// then applies the enchants.
    ///
    /// "Contains" is also the word phase 6 grants XP by, and the reason this stays
    /// the pre-Fortune number: experience is paid on what the rock held, loot on
    /// what the pickaxe pulled out. [Fortune] multiplies the second and never the
    /// first, which is what keeps the level axis and the pickaxe axis from becoming
    /// one axis bought twice.
    ///
    /// [`Excavator`]: crate::enchant::EnchantType::Excavator
    /// [Fortune]: crate::pickaxe::Pickaxe::fortune_multiplier
    pub fn drops(self) -> (Item, u32) {
        (Item::Raw(self.material()), self.drop_amount())
    }
}

/// Every [`Block`] variant, for tests that must cover the whole enum.
///
/// Test-only: an enum has no built-in way to enumerate its variants, and the
/// table-consistency tests (here and in [`world`](crate::world)) need to walk
/// all of them. The `match`es above are exhaustive, so adding a variant already
/// breaks the build; the length assertion in `all_blocks_covers_every_variant`
/// is what reminds you to extend this list too.
#[cfg(test)]
pub(crate) const ALL_BLOCKS: &[Block] = &[
    Block::Stone,
    Block::Cobblestone,
    Block::CoalOre,
    Block::CoalBlock,
    Block::IronOre,
    Block::IronBlock,
    Block::GoldOre,
    Block::GoldBlock,
    Block::LapisOre,
    Block::LapisBlock,
    Block::RedstoneOre,
    Block::RedstoneBlock,
    Block::EmeraldOre,
    Block::EmeraldBlock,
    Block::DiamondOre,
    Block::DiamondBlock,
    Block::Netherrack,
    Block::QuartzOre,
    Block::AncientDebris,
    Block::NetheriteBlock,
    Block::Obsidian,
    Block::CryingObsidian,
    Block::Endstone,
    Block::Amethyst,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The `(ore, dense)` pairs the enum's grouping promises.
    ///
    /// Cobblestone is the odd one out: it is not "a block of Stone" the way
    /// `IronBlock` is a block of Iron. But mechanically it plays exactly that
    /// role — a tougher cell that returns nine Stone — which is why the concept
    /// is named *dense* rather than *block form*. The name has to be true of
    /// every row in this table.
    const DENSE_FORMS: &[(Block, Block)] = &[
        (Block::Stone, Block::Cobblestone),
        (Block::CoalOre, Block::CoalBlock),
        (Block::IronOre, Block::IronBlock),
        (Block::GoldOre, Block::GoldBlock),
        (Block::LapisOre, Block::LapisBlock),
        (Block::RedstoneOre, Block::RedstoneBlock),
        (Block::EmeraldOre, Block::EmeraldBlock),
        (Block::DiamondOre, Block::DiamondBlock),
        (Block::AncientDebris, Block::NetheriteBlock),
    ];

    #[test]
    fn all_blocks_covers_every_variant() {
        assert_eq!(
            ALL_BLOCKS.len(),
            24,
            "a Block variant was added or removed: update ALL_BLOCKS"
        );
    }

    /// An ore and its dense form are two shapes of one resource, so they must
    /// agree on everything that identifies the resource. Only the two quantities
    /// that express "dense" — hardness and yield — may differ.
    #[test]
    fn ore_and_dense_forms_describe_the_same_resource() {
        for &(ore, dense) in DENSE_FORMS {
            assert_eq!(
                ore.material(),
                dense.material(),
                "{ore:?} and {dense:?} drop different materials"
            );
            assert_eq!(
                ore.world(),
                dense.world(),
                "{ore:?} and {dense:?} live in different worlds"
            );
            assert_eq!(
                ore.min_pickaxe_tier(),
                dense.min_pickaxe_tier(),
                "{ore:?} and {dense:?} need different pickaxe tiers"
            );
        }
    }

    /// The whole point of the dense form: harder to break, but worth nine times
    /// as much. Either half alone would make it pointless or free.
    #[test]
    fn dense_forms_are_tougher_and_drop_nine() {
        for &(ore, dense) in DENSE_FORMS {
            assert!(
                dense.hardness() > ore.hardness(),
                "{dense:?} ({}) is not tougher than {ore:?} ({})",
                dense.hardness(),
                ore.hardness()
            );
            assert_eq!(ore.drop_amount(), 1, "{ore:?} should drop a single item");
            assert_eq!(
                dense.drop_amount(),
                RAW_PER_DENSE_BLOCK,
                "{dense:?} should drop a full block's worth"
            );
        }
    }

    /// The separation this module exists to keep: a *dense block* sits in the
    /// ground and pays out raw items; a *Compressed unit* is minted by the player
    /// and is worth a hundred. If a block ever *contained* one, the two would have
    /// merged back into a single muddled concept, and the ground would be paying
    /// out a hundred to one where it means to pay nine.
    ///
    /// This constrains the drop *table*, not the mining loop. The Excavator
    /// enchant is allowed to hand the player a Compressed unit — it substitutes
    /// the drop after the fact, which is the pickaxe's doing. What it may not do
    /// is come from the rock.
    #[test]
    fn no_block_contains_a_compressed_unit() {
        for &block in ALL_BLOCKS {
            let (item, amount) = block.drops();
            assert!(
                matches!(item, Item::Raw(_)),
                "{block:?} contains {item}; blocks hold raw items and nothing else"
            );
            assert!(
                amount <= RAW_PER_DENSE_BLOCK,
                "{block:?} drops {amount} items, more than a dense block's worth"
            );
        }
    }

    /// Every block pays. The mining loop has no branch for a swing that lands on
    /// nothing, and this is what entitles it to have none.
    #[test]
    fn every_block_drops_at_least_one_item() {
        for &block in ALL_BLOCKS {
            let (item, amount) = block.drops();
            assert_eq!(item, Item::Raw(block.material()));
            assert!(
                amount >= 1,
                "{block:?} drops nothing, so mining it is a tax"
            );
        }
    }

    /// A cell of value is worth exactly three times its mine's common cell, on all
    /// twelve mines — including the three whose value cell drops a single item, which
    /// is the whole reason XP stopped being the drop count.
    #[test]
    fn a_cell_of_value_is_worth_three_of_its_mines_common_cell() {
        for &mine in crate::mine_kind::ALL_MINES {
            assert_eq!(
                mine.value_block().xp_value(),
                3 * mine.common_block().xp_value(),
                "{mine:?}: the value cell is not worth three of the common one"
            );
        }
    }

    /// The mines grant XP in progression order, and — the load-bearing part — they do
    /// so at **every** richness setting, not merely at the top.
    ///
    /// A full grid is worth `base * (1 + 2w)` for a dial weight `w`, so it stays
    /// proportional to the mine's base whatever the dial does; rising bases therefore
    /// order the twelve mines everywhere at once. That is why this walks three
    /// settings rather than asserting one table: it is testing the *property*, and a
    /// single-point check would pass on a table that inverted somewhere in between.
    #[test]
    fn xp_rises_with_the_progression_at_every_dial_setting() {
        use crate::mine_kind::MineKind;

        // Mines that share a rung are grouped, since ties are deliberate.
        const ORDER: &[&[MineKind]] = &[
            &[MineKind::Stone],
            &[MineKind::Coal],
            &[MineKind::Iron],
            &[MineKind::Lapis, MineKind::Gold, MineKind::Redstone],
            &[MineKind::Emerald],
            &[MineKind::Diamond],
            &[MineKind::Quartz],
            &[MineKind::AncientDebris],
            &[MineKind::Obsidian],
            &[MineKind::Amethyst],
        ];

        // `weight` is the value cell's share of a grid, in percent: the two ends of
        // the dial and a point in between.
        for weight in [10u32, 50, 91] {
            let grid = |mine: MineKind| -> u32 {
                (100 - weight) * mine.common_block().xp_value()
                    + weight * mine.value_block().xp_value()
            };

            for pair in ORDER.windows(2) {
                let (earlier, later) = (pair[0], pair[1]);
                for &a in earlier {
                    for &b in later {
                        assert!(
                            grid(b) > grid(a),
                            "at dial weight {weight}%, {b:?} ({}) grants no more \
                             than {a:?} ({})",
                            grid(b),
                            grid(a)
                        );
                    }
                }
                // Mines sharing a rung must grant exactly the same, or the tie is a
                // typo rather than a decision.
                for &a in earlier {
                    assert_eq!(grid(a), grid(earlier[0]), "{a:?} breaks its rung's tie");
                }
            }
        }
    }

    /// XP and loot are two tables now, and the separation is the point: a dense block
    /// still drops nine but grants three times its ore, so the richness dial moves the
    /// level bar without the crafting ratio deciding the level curve.
    #[test]
    fn xp_is_not_the_drop_count() {
        for &(ore, dense) in DENSE_FORMS {
            assert_eq!(dense.drop_amount(), RAW_PER_DENSE_BLOCK);
            assert_eq!(
                dense.xp_value(),
                3 * ore.xp_value(),
                "{dense:?} grants XP in proportion to its loot rather than its table"
            );
        }
    }

    /// Every block is worth some experience. A cell granting none would make part of
    /// a grid pure spent time on the level axis, the same fault
    /// `every_block_drops_at_least_one_item` rules out on the loot axis.
    #[test]
    fn every_block_grants_some_experience() {
        for &block in ALL_BLOCKS {
            assert!(
                block.xp_value() > 0,
                "{block:?} grants no experience, so mining it is a tax on levelling"
            );
        }
    }

    /// Each world's filler pays out its own material, and the three worlds agree
    /// on that rule. The filler is the block the player breaks most often — most
    /// of a fresh grid *is* filler — so one that yielded nothing would make the
    /// bulk of the game's swings pay nothing.
    #[test]
    fn every_world_filler_drops_its_own_material() {
        assert_eq!(Block::Stone.material(), Material::Stone);
        assert_eq!(Block::Netherrack.material(), Material::Netherrack);
        assert_eq!(Block::Endstone.material(), Material::Endstone);
    }

    #[test]
    fn every_block_has_positive_hardness() {
        // Hardness is the divisor of the mining loop; a zero or negative value
        // would make a block break instantly or never.
        for &block in ALL_BLOCKS {
            assert!(
                block.hardness() > 0.0,
                "{block:?} has a non-positive hardness of {}",
                block.hardness()
            );
        }
    }

    /// A fresh player holds a Wooden pickaxe, so the filler block of every
    /// world must be breakable with it — otherwise arriving in that world
    /// would soft-lock the run.
    #[test]
    fn every_world_filler_is_breakable_with_a_wooden_pickaxe() {
        for block in [Block::Stone, Block::Netherrack, Block::Endstone] {
            assert_eq!(block.min_pickaxe_tier(), PickaxeTier::Wooden);
        }
    }

    #[test]
    fn obsidian_class_blocks_gate_behind_diamond() {
        for block in [
            Block::Obsidian,
            Block::CryingObsidian,
            Block::AncientDebris,
            Block::NetheriteBlock,
        ] {
            assert_eq!(block.min_pickaxe_tier(), PickaxeTier::Diamond);
        }
    }
}
