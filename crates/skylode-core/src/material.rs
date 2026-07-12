//! Materials, and the two denominations they are held in.
//!
//! A [`Material`] is the resource a [`Block`](crate::block::Block) yields when
//! mined. Several blocks can map to the same material (e.g. both `IronOre` and
//! `IronBlock` yield [`Material::Iron`]), and a material can be sourced from
//! more than one [`World`].
//!
//! An [`Item`] is a material *as the player holds it*: either raw, or bundled
//! into a Compressed unit worth [`RAW_PER_COMPRESSED`]. The distinction only
//! exists inside the inventory — the world produces materials, and only ever
//! drops them raw.

use crate::world::World;
use std::fmt;

/// How many raw items one Compressed unit is worth.
///
/// 100 is round enough that the player can do the arithmetic in their head: an
/// upgrade priced at 650 Iron is quoted as `6 Compressed Iron + 50 Iron`, which
/// reads better than the single large number. See `docs/DECISIONS.md`.
///
/// Not to be confused with `RAW_PER_DENSE_BLOCK` (9), which is what *mining* a
/// dense block yields. The two are unrelated ratios for unrelated things; see
/// [`Item`].
// TODO(phase-1): move to the `tunables` module with the other open constants.
pub const RAW_PER_COMPRESSED: u32 = 100;

/// A raw resource obtained by mining blocks.
///
/// Materials are the "currency" of progression: blocks drop them, and the
/// player spends/accumulates them. The set is deliberately smaller than the
/// [`Block`](crate::block::Block) set because a resource's ore and dense-block
/// variants collapse onto the same material.
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

/// A material as the player holds it: raw, or bundled into a Compressed unit.
///
/// No block in the ground contains anything but [`Item::Raw`]. A
/// [`Compressed`](Item::Compressed) unit is *minted*: the player trades
/// [`RAW_PER_COMPRESSED`] raw items for one — and back, since the trade is free
/// and lossless in both directions. The lone shortcut is the
/// [`Excavator`](crate::enchant::EnchantType::Excavator) enchant, which
/// substitutes a Compressed unit for the drop as it leaves the rock; that is the
/// pickaxe minting one on the player's behalf, and it is why the enchant is worth
/// having at all.
///
/// Two consequences worth spelling out, because both are load-bearing:
///
/// - **Compression is not a [`Material`] variant.** A material is what a world
///   produces and a block drops; no block drops a Compressed unit. Folding the
///   denomination into `Material` would put a member in that set that belongs to
///   no [`World`] and falls from no `Block`, and would make
///   [`worlds`](Material::worlds) a partial function pretending to be total.
/// - **The two denominations are not fungible at the till.** Upgrade costs are
///   quoted in both (`6 Compressed Iron + 50 Iron`) and must be paid in exactly
///   that shape: a player holding 650 raw Iron is *refused* until they compress.
///   That refusal is what makes compressing a step in the upgrade path rather
///   than a cosmetic button, and it costs nothing to clear, since the conversion
///   is free and reversible. See `docs/DECISIONS.md`.
///
/// Distinct again from a *dense block* (`IronBlock`, `Cobblestone`): that is a
/// grid cell you mine, which is tougher and drops nine **raw** items. Nine, not
/// a hundred, and mined, not minted — see [`block`](crate::block).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Item {
    /// A single unit of the material, as mined.
    Raw(Material),
    /// A bundle worth [`RAW_PER_COMPRESSED`] raw units, minted by the player.
    Compressed(Material),
}

impl Item {
    /// The underlying material, whichever denomination it is held in.
    pub fn material(self) -> Material {
        match self {
            Self::Raw(material) | Self::Compressed(material) => material,
        }
    }

    /// What one of these is worth in raw items: `1`, or [`RAW_PER_COMPRESSED`].
    ///
    /// This is the *exchange rate*, not a licence to pay a Compressed cost with
    /// raw items — see the type docs. It is what `compress`/`decompress` trade
    /// on, and what a UI uses to tell the player how far off a cost they are.
    pub fn raw_value(self) -> u32 {
        match self {
            Self::Raw(_) => 1,
            Self::Compressed(_) => RAW_PER_COMPRESSED,
        }
    }
}

/// Renders as `Iron` or `Compressed Iron`.
///
/// The denomination is part of the name because it is part of the identity: an
/// error reading "need 6 Iron, have 650" would be nonsense to the player it is
/// addressed to. It is Compressed Iron they are short of.
impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raw(material) => f.write_str(material.name()),
            Self::Compressed(material) => write!(f, "Compressed {}", material.name()),
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

    /// Every item the inventory can hold: both denominations of every material.
    fn all_items() -> Vec<Item> {
        ALL_MATERIALS
            .iter()
            .flat_map(|&m| [Item::Raw(m), Item::Compressed(m)])
            .collect()
    }

    /// The same uniqueness guarantee as `display_names_are_unique`, extended to
    /// the denomination: the inventory lists raw and Compressed side by side, so
    /// two of its rows sharing a label would be two piles the player cannot tell
    /// apart — and they are worth a hundredfold different amounts.
    #[test]
    fn item_display_names_are_unique_across_denominations() {
        let items = all_items();
        for (i, &a) in items.iter().enumerate() {
            for &b in &items[i + 1..] {
                assert_ne!(
                    a.to_string(),
                    b.to_string(),
                    "{a:?} and {b:?} share the display name {a}"
                );
            }
        }
    }

    #[test]
    fn compressed_items_are_worth_a_hundred_raw() {
        assert_eq!(Item::Raw(Material::Iron).raw_value(), 1);
        assert_eq!(
            Item::Compressed(Material::Iron).raw_value(),
            RAW_PER_COMPRESSED
        );
    }

    /// Both denominations answer to the same material, which is what lets a cost
    /// quoted in both add up to a single number of raw items.
    #[test]
    fn both_denominations_share_one_material() {
        for &material in ALL_MATERIALS {
            assert_eq!(Item::Raw(material).material(), material);
            assert_eq!(Item::Compressed(material).material(), material);
        }
    }
}
