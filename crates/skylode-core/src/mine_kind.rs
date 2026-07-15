//! The canonical mines, and their identity.
//!
//! A [`MineKind`] names one of the game's twelve mines — "the Iron mine", "the
//! End mine" — the notion a bare [`Mine`](crate::mine::Mine) grid lacks. Each
//! kind is a **common cell** and a **cell of value** held in the same grid, and
//! everything else about a mine's identity is *derived* from that pair: the
//! [`World`] it sits in, the [`PickaxeTier`] that gates it, the [`Material`]s it
//! yields. Only the two blocks are stored; the rest is a `match` away, so the
//! identity can never drift out of step with the blocks it is built from.
//!
//! ## Two flavours, one rule
//!
//! - **Same-material mines** pair an ore with its *dense* form — `IronOre` with
//!   `IronBlock`, both dropping [`Material::Iron`]. Enriching such a mine is pure
//!   gain, so its richness dial has one sensible position and the UI hides it.
//! - **Two-material mines** pair a common block with a rarer, *different* one —
//!   `Netherrack` with `QuartzOre`, `Obsidian` with `CryingObsidian`, `Endstone`
//!   with `Amethyst`. There enriching is a substitution, a genuine choice, and
//!   the UI draws the dial.
//!
//! The rules stay uniform across all twelve; only the interface branches, so this
//! module deliberately exposes **no** `has_dial` flag —
//! `common_material() != value_material()` already answers it, and a stored flag
//! would only be a second copy of that fact to keep in sync.
//!
//! ## Why Quartz is Netherrack + Quartz Ore
//!
//! An Overworld ore mine must never have worthless filler as its common cell: the
//! Iron mine is Iron Ore, not Stone with iron veins, or unlocking it would drop
//! the player into breaking mostly-valueless Stone and stall progression at the
//! moment it should accelerate (see `docs/DECISIONS.md`). That rule is about
//! *pace*, and it binds where pace is fragile — the Overworld tier ladder. It does
//! not forbid a themed common cell elsewhere: the Nether's Quartz mine is
//! `Netherrack` (common) plus `QuartzOre` (value). Netherrack, otherwise the one
//! material with no economic function, becomes that mine's own growth currency,
//! and Quartz — the Nether enchant material — finally has a mine to come from
//! without adding a `QuartzBlock` the game has no other use for.

use crate::block::Block;
use crate::material::Material;
use crate::pickaxe::PickaxeTier;
use crate::world::World;

/// One of the game's twelve canonical mines.
///
/// A kind is a pure tag: it owns no data, and answers every question about a
/// mine's identity from its [`common_block`](MineKind::common_block) and
/// [`value_block`](MineKind::value_block) pair. Variants are grouped by world, in
/// progression order, matching the layout of [`Block`] and [`World`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MineKind {
    // --- Overworld ---
    /// The starter mine: `Stone` with `Cobblestone`, both yielding Stone.
    #[default]
    Stone,
    Coal,
    Iron,
    Gold,
    Lapis,
    Redstone,
    Emerald,
    Diamond,
    // --- Nether ---
    /// Netherrack with Quartz Ore — the Nether's entry mine, and the only home of
    /// the Quartz that enchants there.
    Quartz,
    AncientDebris,
    Obsidian,
    // --- End ---
    /// End Stone with Amethyst — the richest mine, and the source of the prestige
    /// material.
    Amethyst,
}

impl MineKind {
    /// The mine's **common cell**: the block that makes up the bulk of a fresh
    /// grid, and whose material funds the mine's own growth.
    ///
    /// On a same-material mine this is the ore (`IronOre`); on a two-material mine
    /// it is the abundant block (`Netherrack`, `Obsidian`, `Endstone`).
    pub fn common_block(self) -> Block {
        match self {
            Self::Stone => Block::Stone,
            Self::Coal => Block::CoalOre,
            Self::Iron => Block::IronOre,
            Self::Gold => Block::GoldOre,
            Self::Lapis => Block::LapisOre,
            Self::Redstone => Block::RedstoneOre,
            Self::Emerald => Block::EmeraldOre,
            Self::Diamond => Block::DiamondOre,
            Self::Quartz => Block::Netherrack,
            Self::AncientDebris => Block::AncientDebris,
            Self::Obsidian => Block::Obsidian,
            Self::Amethyst => Block::Endstone,
        }
    }

    /// The mine's **cell of value**: the rarer block that
    /// [richness](crate::mine::Mine) shifts weight toward.
    ///
    /// On a same-material mine this is the dense form (`IronBlock`), worth nine of
    /// the same material; on a two-material mine it is the rare, differently made
    /// block (`QuartzOre`, `CryingObsidian`, `Amethyst`).
    pub fn value_block(self) -> Block {
        match self {
            Self::Stone => Block::Cobblestone,
            Self::Coal => Block::CoalBlock,
            Self::Iron => Block::IronBlock,
            Self::Gold => Block::GoldBlock,
            Self::Lapis => Block::LapisBlock,
            Self::Redstone => Block::RedstoneBlock,
            Self::Emerald => Block::EmeraldBlock,
            Self::Diamond => Block::DiamondBlock,
            Self::Quartz => Block::QuartzOre,
            Self::AncientDebris => Block::NetheriteBlock,
            Self::Obsidian => Block::CryingObsidian,
            Self::Amethyst => Block::Amethyst,
        }
    }

    /// The world the mine sits in.
    ///
    /// Derived from the common cell rather than stored: both cells of a mine live
    /// in the same world (a mine cannot straddle two dimensions), which
    /// [`common_and_value_share_a_world`](self) pins. Storing it would be a second
    /// copy of what [`Block::world`] already knows.
    pub fn world(self) -> World {
        self.common_block().world()
    }

    /// The lowest [`PickaxeTier`] that can open this mine.
    ///
    /// Derived from the common cell, and it is safe to derive from *one* cell only
    /// because both cells of every mine share a minimum tier — an ore and its
    /// dense form always do, and the three two-material mines were built to as
    /// well (Netherrack/Quartz are Wooden, Obsidian/Crying are Diamond,
    /// Endstone/Amethyst are Wooden). If a future mine broke that,
    /// [`common_and_value_share_a_gating_tier`](self) would fail rather than let a
    /// mine unlock with cells its pickaxe cannot break — which would sit in the
    /// grid as permanent holes.
    pub fn gating_tier(self) -> PickaxeTier {
        self.common_block().min_pickaxe_tier()
    }

    /// The material the common cell drops: the mine's own growth currency.
    ///
    /// Size and the low levels of richness are paid in this — Iron for the Iron
    /// mine, Netherrack for the Quartz mine — so every mine funds its early growth
    /// out of what it mostly produces.
    pub fn common_material(self) -> Material {
        self.common_block().material()
    }

    /// The material the value cell drops.
    ///
    /// Equal to [`common_material`](MineKind::common_material) on a same-material
    /// mine, and different on a two-material one — which is exactly the test a
    /// front-end applies to decide whether the richness dial is a real choice
    /// worth drawing.
    pub fn value_material(self) -> Material {
        self.value_block().material()
    }

    /// The mine's display name, e.g. `"Iron"`, `"Quartz"`, `"End"`.
    ///
    /// A method, not a stored field, for the reason the whole module derives
    /// rather than stores: the name belongs to the variant, and a `String` field
    /// would only be a copy that could fall out of step. The label is the mine's
    /// *theme*, which is not always the common material — the Quartz mine is named
    /// for the Quartz it exists to produce, not the Netherrack it is mostly made
    /// of.
    pub fn name(self) -> &'static str {
        match self {
            Self::Stone => "Stone",
            Self::Coal => "Coal",
            Self::Iron => "Iron",
            Self::Gold => "Gold",
            Self::Lapis => "Lapis",
            Self::Redstone => "Redstone",
            Self::Emerald => "Emerald",
            Self::Diamond => "Diamond",
            Self::Quartz => "Quartz",
            Self::AncientDebris => "Ancient Debris",
            Self::Obsidian => "Obsidian",
            Self::Amethyst => "End",
        }
    }
}

/// Every [`MineKind`] variant, for tests that must cover the whole enum.
///
/// Test-only; see [`ALL_BLOCKS`](crate::block::ALL_BLOCKS) for the rationale. The
/// `match`es above are exhaustive, so adding a variant already breaks the build;
/// the length assertion in `all_mines_covers_every_variant` is what reminds you to
/// extend this list too.
#[cfg(test)]
pub(crate) const ALL_MINES: &[MineKind] = &[
    MineKind::Stone,
    MineKind::Coal,
    MineKind::Iron,
    MineKind::Gold,
    MineKind::Lapis,
    MineKind::Redstone,
    MineKind::Emerald,
    MineKind::Diamond,
    MineKind::Quartz,
    MineKind::AncientDebris,
    MineKind::Obsidian,
    MineKind::Amethyst,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_mines_covers_every_variant() {
        assert_eq!(
            ALL_MINES.len(),
            12,
            "a MineKind variant was added or removed: update ALL_MINES"
        );
    }

    /// A mine is one grid, and a grid is in one world. Both cells must therefore
    /// agree on which — and [`MineKind::world`] derives itself from the common
    /// cell, so the value cell has to match it or the derivation would be a lie
    /// for that mine.
    #[test]
    fn common_and_value_share_a_world() {
        for &mine in ALL_MINES {
            assert_eq!(
                mine.common_block().world(),
                mine.value_block().world(),
                "{mine:?}: common and value cells live in different worlds"
            );
            assert_eq!(
                mine.world(),
                mine.common_block().world(),
                "{mine:?}: derived world disagrees with its cells"
            );
        }
    }

    /// The licence to derive the gate from a single cell. A mine unlocks as a
    /// whole, so if its two cells needed different tiers, the player could open it
    /// with a pickaxe that cannot break one of them — and that cell would sit in
    /// the grid forever as an unbreakable hole. Equal tiers is what makes
    /// [`MineKind::gating_tier`] reading only the common cell correct.
    #[test]
    fn common_and_value_share_a_gating_tier() {
        for &mine in ALL_MINES {
            assert_eq!(
                mine.common_block().min_pickaxe_tier(),
                mine.value_block().min_pickaxe_tier(),
                "{mine:?}: its two cells need different pickaxe tiers"
            );
            assert_eq!(
                mine.gating_tier(),
                mine.common_block().min_pickaxe_tier(),
                "{mine:?}: derived gating tier disagrees with its cells"
            );
        }
    }

    /// A mine may only be built from blocks its world actually contains, or mine
    /// generation would place a cell the world's own registry does not list.
    #[test]
    fn both_cells_belong_to_the_mines_world() {
        for &mine in ALL_MINES {
            let world_blocks = mine.world().blocks();
            assert!(
                world_blocks.contains(&mine.common_block()),
                "{mine:?}: {:?} is not a block of {}",
                mine.common_block(),
                mine.world().name()
            );
            assert!(
                world_blocks.contains(&mine.value_block()),
                "{mine:?}: {:?} is not a block of {}",
                mine.value_block(),
                mine.world().name()
            );
        }
    }

    /// The decision this module is built to keep: an Overworld ore mine's common
    /// cell is the ore itself, never filler. Encoded as "both cells drop the same
    /// material" — which is exactly true of an ore/dense pair, and false of a
    /// filler-plus-veins mine. If someone ever made the Iron mine Stone-common,
    /// this test is what would stop it, because breaking mostly-valueless Stone on
    /// unlock is the stall the rule exists to prevent.
    #[test]
    fn overworld_ore_mines_are_pure_same_material() {
        for &mine in ALL_MINES {
            if mine.world() == World::Overworld {
                assert_eq!(
                    mine.common_material(),
                    mine.value_material(),
                    "{mine:?}: an Overworld mine with a filler common cell would \
                     stall progression on unlock"
                );
            }
        }
    }

    /// Only the three later-world mines mix materials, and they are precisely the
    /// ones where a richness dial is a real choice. This pins the partition the UI
    /// branches on, without a stored flag: same-material means the dial is hidden,
    /// two-material means it is drawn.
    #[test]
    fn only_the_endgame_mines_are_two_material() {
        let two_material: Vec<MineKind> = ALL_MINES
            .iter()
            .copied()
            .filter(|mine| mine.common_material() != mine.value_material())
            .collect();
        assert_eq!(
            two_material,
            vec![MineKind::Quartz, MineKind::Obsidian, MineKind::Amethyst]
        );
    }

    /// Names go straight to the UI, so two mines sharing one would be two entries
    /// the player cannot tell apart.
    #[test]
    fn mine_names_are_unique() {
        for (i, &a) in ALL_MINES.iter().enumerate() {
            for &b in &ALL_MINES[i + 1..] {
                assert_ne!(
                    a.name(),
                    b.name(),
                    "{a:?} and {b:?} share the display name {:?}",
                    a.name()
                );
            }
        }
    }
}
