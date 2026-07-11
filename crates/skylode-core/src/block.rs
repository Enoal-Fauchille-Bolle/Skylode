//! Mineable blocks.
//!
//! A [`Block`] is a single cell of a [`Mine`](crate::mine::Mine). Each block
//! carries the four properties the mining loop needs: the [`Material`] it
//! drops, its [`hardness`](Block::hardness) (mining time), the
//! [`World`] it belongs to, and the minimum
//! [`PickaxeTier`] required to break it.
//!
//! Many resources come in two forms: an *ore* variant (drops a single item)
//! and a compressed *block* variant (drops `ITEMS_PER_BLOCK` items), mirroring
//! Minecraft's 9-ingots-per-block convention.

use crate::material::Material;
use crate::pickaxe::PickaxeTier;
use crate::world::World;

/// Number of raw items a compressed *block* form yields when mined.
///
/// Matches Minecraft's crafting ratio: 9 ingots/gems compress into one block,
/// so breaking that block returns 9.
const ITEMS_PER_BLOCK: u32 = 9;

/// A single mineable block.
///
/// Variants are grouped as `<Resource>Ore` / `<Resource>Block` pairs where a
/// compressed form exists, plus standalone blocks (Netherrack, Obsidian, …)
/// that have no dual form. The whole enum is `Copy` because a block is just a
/// lightweight tag with no owned data.
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
    /// Returns the material of the block, if it has one.
    /// Some blocks, like Netherrack, do not have a material.
    pub fn material(self) -> Option<Material> {
        match self {
            Self::Stone | Self::Cobblestone => Some(Material::Stone),
            Self::CoalOre | Self::CoalBlock => Some(Material::Coal),
            Self::IronOre | Self::IronBlock => Some(Material::Iron),
            Self::GoldOre | Self::GoldBlock => Some(Material::Gold),
            Self::LapisOre | Self::LapisBlock => Some(Material::Lapis),
            Self::RedstoneOre | Self::RedstoneBlock => Some(Material::Redstone),
            Self::EmeraldOre | Self::EmeraldBlock => Some(Material::Emerald),
            Self::DiamondOre | Self::DiamondBlock => Some(Material::Diamond),
            Self::QuartzOre => Some(Material::Quartz),
            Self::AncientDebris | Self::NetheriteBlock => Some(Material::AncientDebris),
            Self::Obsidian => Some(Material::Obsidian),
            Self::CryingObsidian => Some(Material::CryingObsidian),
            Self::Amethyst => Some(Material::Amethyst),
            Self::Netherrack | Self::Endstone => None,
        }
    }

    /// Returns the hardness of the block,
    /// which determines how long it takes to mine.
    ///
    /// Hardness is measured in the same units as a pickaxe's
    /// [`mining_power`](crate::pickaxe::Pickaxe::mining_power): a block breaks
    /// once accumulated mining power reaches its hardness. Values roughly track
    /// Minecraft (Stone `1.5`, Obsidian `50.0`), with the compressed *block*
    /// forms being tougher than their ore counterparts.
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

    /// Amount of raw material dropped, before Fortune. Compressed block forms
    /// yield `ITEMS_PER_BLOCK`; everything else yields 1.
    pub fn drop_amount(self) -> u32 {
        match self {
            Self::Cobblestone
            | Self::CoalBlock
            | Self::IronBlock
            | Self::GoldBlock
            | Self::LapisBlock
            | Self::RedstoneBlock
            | Self::EmeraldBlock
            | Self::DiamondBlock
            | Self::NetheriteBlock => ITEMS_PER_BLOCK,
            _ => 1,
        }
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

    /// The `(ore, compressed)` pairs the enum's grouping promises.
    const PAIRS: &[(Block, Block)] = &[
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

    /// An ore and its compressed form are two shapes of one resource, so they
    /// must agree on everything that identifies the resource. Only the two
    /// quantities that express "compressed" may differ.
    #[test]
    fn ore_and_compressed_forms_describe_the_same_resource() {
        for &(ore, compressed) in PAIRS {
            assert_eq!(
                ore.material(),
                compressed.material(),
                "{ore:?} and {compressed:?} drop different materials"
            );
            assert_eq!(
                ore.world(),
                compressed.world(),
                "{ore:?} and {compressed:?} live in different worlds"
            );
            assert_eq!(
                ore.min_pickaxe_tier(),
                compressed.min_pickaxe_tier(),
                "{ore:?} and {compressed:?} need different pickaxe tiers"
            );
        }
    }

    /// The whole point of the compressed form: harder to break, but worth nine
    /// times as much. Either half alone would make it pointless or free.
    #[test]
    fn compressed_forms_are_tougher_and_drop_nine() {
        for &(ore, compressed) in PAIRS {
            assert!(
                compressed.hardness() > ore.hardness(),
                "{compressed:?} ({}) is not tougher than {ore:?} ({})",
                compressed.hardness(),
                ore.hardness()
            );
            assert_eq!(ore.drop_amount(), 1, "{ore:?} should drop a single item");
            assert_eq!(
                compressed.drop_amount(),
                ITEMS_PER_BLOCK,
                "{compressed:?} should drop a full block's worth"
            );
        }
    }

    #[test]
    fn filler_blocks_drop_nothing() {
        // Netherrack and Endstone are the "dirt" of their worlds: they fill the
        // grid and cost time, but yield no material.
        assert_eq!(Block::Netherrack.material(), None);
        assert_eq!(Block::Endstone.material(), None);
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
