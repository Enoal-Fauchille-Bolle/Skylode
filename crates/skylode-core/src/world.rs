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
                Material::Netherrack,
                Material::Quartz,
                Material::AncientDebris,
                Material::Obsidian,
                Material::CryingObsidian,
            ],
            Self::End => &[Material::Endstone, Material::Amethyst],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::ALL_BLOCKS;
    use crate::material::ALL_MATERIALS;

    const ALL_WORLDS: [World; 3] = [World::Overworld, World::Nether, World::End];

    #[test]
    fn worlds_have_display_names() {
        assert_eq!(World::Overworld.name(), "Overworld");
        assert_eq!(World::Nether.name(), "Nether");
        assert_eq!(World::End.name(), "End");
    }

    /// `Block::world()` and `World::blocks()` encode the same relation from
    /// opposite ends. A block listed by no world can never be generated; a block
    /// listed by two would leak across dimensions.
    #[test]
    fn each_block_is_listed_by_exactly_the_world_it_claims() {
        for &block in ALL_BLOCKS {
            let listed_in: Vec<World> = ALL_WORLDS
                .iter()
                .copied()
                .filter(|world| world.blocks().contains(&block))
                .collect();
            assert_eq!(
                listed_in,
                vec![block.world()],
                "{block:?} says it lives in {:?}, but World::blocks() lists it in {listed_in:?}",
                block.world()
            );
        }
    }

    /// Mine generation draws from `World::blocks()`, and every block drops a
    /// material. If the world's own material list omits one of them, anything
    /// reading `World::materials()` (shop, inventory, unlock gates) is working
    /// from an incomplete picture of what that world can actually yield.
    #[test]
    fn a_world_lists_every_material_its_own_blocks_drop() {
        for world in ALL_WORLDS {
            for &block in world.blocks() {
                let material = block.material();
                assert!(
                    world.materials().contains(&material),
                    "{}::materials() omits {material:?}, which {block:?} drops in that world",
                    world.name()
                );
            }
        }
    }

    /// `Material::worlds()` and `World::materials()` are two views of one
    /// relation, so it must read the same in both directions.
    #[test]
    fn the_world_material_relation_agrees_in_both_directions() {
        for &material in ALL_MATERIALS {
            for &world in material.worlds() {
                assert!(
                    world.materials().contains(&material),
                    "{material:?}::worlds() claims {}, but {}::materials() does not list {material:?}",
                    world.name(),
                    world.name()
                );
            }
        }

        for world in ALL_WORLDS {
            for &material in world.materials() {
                assert!(
                    material.worlds().contains(&world),
                    "{}::materials() lists {material:?}, but {material:?}::worlds() does not claim {}",
                    world.name(),
                    world.name()
                );
            }
        }
    }

    /// A world with no blocks cannot generate a mine.
    #[test]
    fn every_world_has_blocks_to_mine() {
        for world in ALL_WORLDS {
            assert!(
                !world.blocks().is_empty(),
                "{} has no blocks, so no mine can be generated in it",
                world.name()
            );
        }
    }
}
