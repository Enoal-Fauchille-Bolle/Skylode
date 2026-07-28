//! Game dimensions.
//!
//! A [`World`] is one of the three Minecraft-style dimensions the player can
//! mine in. Each world acts as a registry: it knows which [`Block`]s and
//! [`Material`]s can be found within it, which drives mine generation and
//! progression gating.

use crate::{
    block::Block,
    material::Material,
    tunables::{END_UNLOCK_LEVEL, NETHER_UNLOCK_LEVEL},
};

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

    /// The mining level that opens this world.
    ///
    /// The level axis of the two-axis gate: mining level opens *worlds*, pickaxe
    /// tier opens *mines* inside them (see
    /// [`MineKind::gating_tier`](crate::mine_kind::MineKind::gating_tier)), and
    /// `docs/MECHANICS.md` is explicit that neither axis alone carries
    /// progression.
    ///
    /// The two later thresholds are read from
    /// [`NETHER_UNLOCK_LEVEL`] and [`END_UNLOCK_LEVEL`] rather than written here:
    /// they are dials phase 10 may turn, and the ordering invariant that keeps
    /// them coherent — `Nether < End <= LEVEL_CAP` — is asserted at *compile
    /// time* beside them. What lives here is the keyed lookup, which is the shape
    /// that makes a fourth dimension a compile error instead of a world silently
    /// unlocking at level 0.
    ///
    /// **The Overworld returns `1`, not `0`.** A new player starts at level 1 and
    /// [`Player::xp_for_level`](crate::player::Player::xp_for_level) refuses to
    /// quote a price for level 0, which names no rung of the ladder — so the
    /// starting world's threshold is the starting level, and `is_unlocked_at`
    /// needs no special case for it.
    pub fn unlock_level(self) -> u32 {
        match self {
            Self::Overworld => 1,
            Self::Nether => NETHER_UNLOCK_LEVEL,
            Self::End => END_UNLOCK_LEVEL,
        }
    }

    /// Whether a player at `level` may mine in this world.
    ///
    /// Takes the level rather than a `&Player` so the rule can be tested — and
    /// asked about a hypothetical level — without building a player. The
    /// player-facing form is
    /// [`Player::has_unlocked`](crate::player::Player::has_unlocked).
    ///
    /// **Derived, never stored.** The unlocked set is a monotone function of the
    /// mining level, which the save already holds, so keeping a second copy would
    /// be an invariant to maintain by hand. It also survives prestige for free:
    /// the deep reset takes the mining level back to the start, and the worlds
    /// re-lock on their own rather than needing to be cleared.
    pub fn is_unlocked_at(self, level: u32) -> bool {
        level >= self.unlock_level()
    }

    /// The world that reaching exactly `level` opens, if any.
    ///
    /// `docs/MECHANICS.md` fixes that **every level-up gives exactly one thing,
    /// loot or a world, never both and never nothing**: levels
    /// [15](NETHER_UNLOCK_LEVEL) and [30](END_UNLOCK_LEVEL) grant their dimension
    /// *instead* of the usual bundle. This is the query that rule is written in
    /// terms of, so the level-up reward path can branch on it rather than
    /// re-deriving the two thresholds.
    ///
    /// An **associated function**, not a method, for the reason
    /// [`Player::xp_for_level`](crate::player::Player::xp_for_level) is one: its
    /// whole job is to answer about a level nobody is standing on — the Levels
    /// screen draws the whole 1→50 ladder in advance, world unlocks marked.
    ///
    /// [`None`] for level 1: the Overworld is where the player starts, and
    /// starting is not a level-up. Granting it here would hand the first level a
    /// "world unlocked" it never crossed a threshold for.
    ///
    /// The two constants appear **as patterns**, so should a rebalance ever make
    /// them equal, the compiler reports an unreachable arm instead of silently
    /// dropping one of the two unlocks.
    pub fn unlocked_by_reaching(level: u32) -> Option<Self> {
        match level {
            NETHER_UNLOCK_LEVEL => Some(Self::Nether),
            END_UNLOCK_LEVEL => Some(Self::End),
            _ => None,
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
    use crate::tunables::LEVEL_CAP;

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
        for material in Material::ALL {
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

    /// The gate itself, checked on both sides of each threshold. Off by one here
    /// either strands the player one level short of a dimension or hands it to
    /// them a level early.
    #[test]
    fn a_world_opens_on_its_threshold_and_not_a_level_before() {
        assert!(World::Overworld.is_unlocked_at(1));

        assert!(!World::Nether.is_unlocked_at(NETHER_UNLOCK_LEVEL - 1));
        assert!(World::Nether.is_unlocked_at(NETHER_UNLOCK_LEVEL));

        assert!(!World::End.is_unlocked_at(END_UNLOCK_LEVEL - 1));
        assert!(World::End.is_unlocked_at(END_UNLOCK_LEVEL));
    }

    /// The invariant `tunables` const-asserts over its two constants, restated
    /// over the *enum* — so a fourth dimension slotted in with a threshold out of
    /// order fails here, where naming two constants could not see it. And every
    /// threshold must sit at or below [`LEVEL_CAP`], or a world would open after
    /// the cap has frozen the player short of it.
    #[test]
    fn unlock_levels_climb_with_the_world_and_stay_within_the_cap() {
        for pair in ALL_WORLDS.windows(2) {
            let [earlier, later] = pair else { continue };
            assert!(
                earlier.unlock_level() < later.unlock_level(),
                "{} opens at {} and {} at {}, so the ladder is not a ladder",
                earlier.name(),
                earlier.unlock_level(),
                later.name(),
                later.unlock_level()
            );
        }

        for world in ALL_WORLDS {
            assert!(
                world.unlock_level() <= LEVEL_CAP,
                "{} opens at {}, past the level cap of {LEVEL_CAP}",
                world.name(),
                world.unlock_level()
            );
        }
    }

    /// [`World::unlock_level`] and [`World::unlocked_by_reaching`] are two views
    /// of one table — the standing gate and the moment it is crossed — and a
    /// level-up that announced a world the gate had not opened, or opened one
    /// silently, would be one of them drifting from the other.
    ///
    /// The Overworld is the deliberate exception: it is where the player starts,
    /// so it is never *reached*.
    #[test]
    fn the_gate_and_the_crossing_agree_at_every_level() {
        for level in 0..=LEVEL_CAP {
            let granted = World::unlocked_by_reaching(level);
            let expected = ALL_WORLDS
                .into_iter()
                .find(|world| *world != World::Overworld && world.unlock_level() == level);
            assert_eq!(
                granted, expected,
                "reaching level {level} grants {granted:?} but the thresholds say {expected:?}"
            );
        }
    }

    /// Starting is not a level-up. If level 1 granted the Overworld, the reward
    /// path would owe the player a "world unlocked" for a threshold they never
    /// crossed — and `docs/MECHANICS.md` allows exactly one thing per level-up.
    #[test]
    fn starting_in_the_overworld_is_not_a_world_unlock() {
        assert_eq!(World::unlocked_by_reaching(1), None);
        assert_eq!(World::unlocked_by_reaching(0), None);
        assert_eq!(
            World::unlocked_by_reaching(NETHER_UNLOCK_LEVEL),
            Some(World::Nether)
        );
        assert_eq!(
            World::unlocked_by_reaching(END_UNLOCK_LEVEL),
            Some(World::End)
        );
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
