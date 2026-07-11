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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
