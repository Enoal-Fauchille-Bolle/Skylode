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

    /// Returns the level ceiling this world grants to the five special enchants
    /// — Explosive, Jackhammer, Nuke, Excavator and Haste.
    ///
    /// **One scalar per world, shared by all five**, rather than a cap per
    /// `(enchant, world)` pair. Every special enchant is buyable from the
    /// Overworld onwards; reaching a new dimension unlocks nothing new, it only
    /// raises this ceiling. That makes the number below the whole progression
    /// curve of the special enchants, which is why it is a rule of the world and
    /// not a detail of any one enchant.
    ///
    /// The ceiling and an enchant's effect are **two separate dials**: this one
    /// says how much the player may invest, the enchant's own scaling says what
    /// the investment buys. An effect that grows too fast by level 10 is a bug in
    /// its curve — the blob radius, the row band, the Nuke cooldown — and must be
    /// fixed there. Capping that enchant lower instead would trade a curve bug for
    /// an asymmetry the player can see, and cost this method its single-number
    /// shape.
    ///
    /// A balance dial, but a dial keyed by a variant, so it lives here and not in
    /// [`tunables`](crate::tunables) — step 2 of that module's question, which names
    /// this method in return. The `match` is also what makes a fourth dimension a
    /// compile error instead of a world that silently caps enchants at zero.
    ///
    /// The three values are provisional — phase 10 balances them — but their **order
    /// is not**: it is what makes Lapis, Quartz and Amethyst a ladder rather than
    /// three interchangeable materials, and `enchant_caps_grow_strictly_with_the_world`
    /// is what refuses to let a re-balance flatten it.
    pub fn enchant_cap(self) -> u8 {
        match self {
            Self::Overworld => 3,
            Self::Nether => 6,
            Self::End => 10,
        }
    }

    /// The material that pays for the five special enchants in this world: Lapis
    /// in the Overworld, Quartz in the Nether, Amethyst in the End.
    ///
    /// The counterpart to [`enchant_cap`](World::enchant_cap): the cap says *how
    /// far* the specials can be pushed in a world, this says *what* pushing them
    /// costs there. True to Minecraft, where Lapis is the enchanting currency, and
    /// the reason each world's enchant material is distinct — an enchant bought in
    /// the End is an Amethyst sink, which is what puts it in tension with prestige.
    ///
    /// A **design fact**, not a balance dial: the three materials are named and
    /// fixed in `docs/MECHANICS.md`, so this does not belong in
    /// [`tunables`](crate::tunables). It is keyed by the world variant, so — like
    /// `enchant_cap` — it lives here, where a fourth dimension would be a compile
    /// error rather than a world with no enchant currency. Fortune and Efficiency
    /// are keyed by neither world (Emerald, the pickaxe path) and are priced
    /// elsewhere; this covers only the five specials, whose cap *is* the world's.
    pub fn enchant_material(self) -> Material {
        match self {
            Self::Overworld => Material::Lapis,
            Self::Nether => Material::Quartz,
            Self::End => Material::Amethyst,
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

/// Every [`World`] variant, for tests that must cover the whole enum.
///
/// Test-only; see [`ALL_BLOCKS`](crate::block::ALL_BLOCKS) for the rationale.
/// Ordered by progression, weakest first, because
/// [`enchant`](crate::enchant)'s cap tests walk it in order to assert that the
/// ceiling only ever climbs.
#[cfg(test)]
pub(crate) const ALL_WORLDS: [World; 3] = [World::Overworld, World::Nether, World::End];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::ALL_BLOCKS;
    use crate::material::ALL_MATERIALS;

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

    /// The enchant currency ladder `docs/MECHANICS.md` fixes: Lapis, Quartz,
    /// Amethyst. And it must be a material the world actually produces — a world
    /// whose enchant material fell from no block in it would price its specials in
    /// a currency the player could never earn there.
    #[test]
    fn each_world_prices_its_specials_in_a_material_it_produces() {
        assert_eq!(World::Overworld.enchant_material(), Material::Lapis);
        assert_eq!(World::Nether.enchant_material(), Material::Quartz);
        assert_eq!(World::End.enchant_material(), Material::Amethyst);

        for world in ALL_WORLDS {
            assert!(
                world.materials().contains(&world.enchant_material()),
                "{} prices enchants in {:?}, which it does not produce",
                world.name(),
                world.enchant_material()
            );
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
