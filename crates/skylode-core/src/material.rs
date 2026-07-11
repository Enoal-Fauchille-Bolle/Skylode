//! Raw materials.
//!
//! A [`Material`] is the resource a [`Block`](crate::block::Block) yields when
//! mined. Several blocks can map to the same material (e.g. both `IronOre` and
//! `IronBlock` yield [`Material::Iron`]), and a material can be sourced from
//! more than one [`World`].

use crate::world::World;

/// A raw resource obtained by mining blocks.
///
/// Materials are the "currency" of progression: blocks drop them, and the
/// player spends/accumulates them. The set is deliberately smaller than the
/// [`Block`](crate::block::Block) set because ore and compressed-block variants
/// collapse onto the same material.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Material {
    Stone,
    Coal,
    Iron,
    Gold,
    Lapis,
    Redstone,
    Emerald,
    Diamond,
    Quartz,
    AncientDebris,
    Obsidian,
    CryingObsidian,
    Amethyst,
}

impl Material {
    /// Returns the human-readable display name of the material.
    ///
    /// Multi-word materials use spaced names (e.g. `"Ancient Debris"`) so the
    /// result can be shown directly in the UI.
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
            Self::CryingObsidian => "Crying Obsidian",
            Self::Amethyst => "Amethyst",
        }
    }

    /// Returns every [`World`] in which this material can be obtained.
    ///
    /// The slice return type allows a material to be sourced from more than one
    /// world; none currently is, but the drop tables are expected to overlap as
    /// worlds gain blocks. Must stay in step with
    /// [`World::materials`](crate::world::World::materials), which encodes the
    /// same relation from the other side.
    pub fn worlds(self) -> &'static [World] {
        match self {
            Self::Stone
            | Self::Coal
            | Self::Iron
            | Self::Gold
            | Self::Lapis
            | Self::Redstone
            | Self::Emerald
            | Self::Diamond => &[World::Overworld],
            Self::Quartz | Self::AncientDebris | Self::Obsidian | Self::CryingObsidian => {
                &[World::Nether]
            }
            Self::Amethyst => &[World::End],
        }
    }
}

/// Every [`Material`] variant, for tests that must cover the whole enum.
///
/// Test-only; see [`ALL_BLOCKS`](crate::block::ALL_BLOCKS) for the rationale.
#[cfg(test)]
pub(crate) const ALL_MATERIALS: &[Material] = &[
    Material::Stone,
    Material::Coal,
    Material::Iron,
    Material::Gold,
    Material::Lapis,
    Material::Redstone,
    Material::Emerald,
    Material::Diamond,
    Material::Quartz,
    Material::AncientDebris,
    Material::Obsidian,
    Material::CryingObsidian,
    Material::Amethyst,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_materials_covers_every_variant() {
        assert_eq!(
            ALL_MATERIALS.len(),
            13,
            "a Material variant was added or removed: update ALL_MATERIALS"
        );
    }

    /// A material no world produces is dead weight the player can never obtain.
    #[test]
    fn every_material_is_obtainable_in_at_least_one_world() {
        for &material in ALL_MATERIALS {
            assert!(
                !material.worlds().is_empty(),
                "{material:?} belongs to no world, so nothing can ever drop it"
            );
        }
    }

    #[test]
    fn multi_word_materials_are_displayed_with_spaces() {
        assert_eq!(Material::AncientDebris.name(), "Ancient Debris");
        assert_eq!(Material::CryingObsidian.name(), "Crying Obsidian");
    }

    /// Names go straight to the inventory UI, so two materials sharing one
    /// would be indistinguishable to the player.
    #[test]
    fn display_names_are_unique() {
        for (i, &a) in ALL_MATERIALS.iter().enumerate() {
            for &b in &ALL_MATERIALS[i + 1..] {
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
