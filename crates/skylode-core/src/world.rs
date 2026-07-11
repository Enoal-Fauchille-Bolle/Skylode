//! Game dimensions.
//!
//! A [`World`] is one of the three Minecraft-style dimensions the player can
//! mine in. Each world acts as a registry: it knows which [`Block`]s and
//! [`Material`]s can be found within it, which drives mine generation and
//! progression gating.

use crate::{block::Block, material::Material};

/// One of the game's three dimensions.
///
/// Worlds are ordered roughly by progression: the player starts in the
/// [`Overworld`](World::Overworld) and unlocks the harder
/// [`Nether`](World::Nether) and [`End`](World::End) later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum World {
    /// The starting dimension; common ores from Stone up to Diamond.
    Overworld,
    /// The second dimension; Netherrack, Quartz and Netherite-tier resources.
    Nether,
    /// The final dimension; End-specific blocks such as Endstone and Amethyst.
    End,
}

impl World {
    /// Returns the name of the world as a string.
    pub fn name(self) -> &'static str {
        match self {
            Self::Overworld => "Overworld",
            Self::Nether => "Nether",
            Self::End => "End",
        }
    }

    /// Returns the blocks that can be found in the world.
    pub fn blocks(self) -> &'static [Block] {
        match self {
            Self::Overworld => &[
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
            ],
            Self::Nether => &[
                Block::Netherrack,
                Block::QuartzOre,
                Block::AncientDebris,
                Block::NetheriteBlock,
                Block::Obsidian,
                Block::CryingObsidian,
            ],
            Self::End => &[Block::Endstone, Block::Amethyst],
        }
    }

    /// Returns the materials that can be found in the world.
    pub fn materials(self) -> &'static [Material] {
        match self {
            Self::Overworld => &[
                Material::Stone,
                Material::Coal,
                Material::Iron,
                Material::Gold,
                Material::Lapis,
                Material::Redstone,
                Material::Diamond,
                Material::Emerald,
            ],
            Self::Nether => &[
                Material::AncientDebris,
                Material::Obsidian,
                Material::CryingObsidian,
            ],
            Self::End => &[Material::Amethyst],
        }
    }
}
