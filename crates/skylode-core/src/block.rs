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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    // --- Overworld ---
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
