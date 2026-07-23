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

use crate::tunables::RAW_PER_COMPRESSED;
use crate::world::World;
use serde::de::{Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// What a [`Compressed`](Item::Compressed) key is prefixed with in a save.
///
/// No [`Material`] key begins with it, which is what makes
/// [`Item::from_save_key`] able to split a key by looking at its front and nothing
/// else. `every_material_key_is_a_word_of_its_own` holds that.
const COMPRESSED_PREFIX: &str = "compressed_";

/// A raw resource obtained by mining blocks.
///
/// Materials are the "currency" of progression: blocks drop them, and the
/// player spends/accumulates them. The set is deliberately smaller than the
/// [`Block`](crate::block::Block) set because a resource's ore and dense-block
/// variants collapse onto the same material.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Material {
    // --- Overworld ---
    /// The Overworld's filler: what most of a starter mine is made of.
    Stone,
    Coal,
    Iron,
    Gold,
    Lapis,
    Redstone,
    Emerald,
    Diamond,
    // --- Nether ---
    /// The Nether's filler, the counterpart of [`Stone`](Material::Stone).
    Netherrack,
    Quartz,
    AncientDebris,
    Obsidian,
    CryingObsidian,
    // --- End ---
    /// The End's filler, and the common material of its mixed mine.
    Endstone,
    Amethyst,
}

impl Material {
    /// Returns the human-readable display name of the material.
    ///
    /// Multi-word materials use spaced names (e.g. `"Ancient Debris"`) so the
    /// result can be shown directly in the UI. The variant is spelled
    /// `Endstone`, to match [`Block::Endstone`](crate::block::Block::Endstone),
    /// but it *displays* as "End Stone", which is how Minecraft and the design
    /// docs write it.
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
            Self::Netherrack => "Netherrack",
            Self::Quartz => "Quartz",
            Self::AncientDebris => "Ancient Debris",
            Self::Obsidian => "Obsidian",
            Self::CryingObsidian => "Crying Obsidian",
            Self::Endstone => "End Stone",
            Self::Amethyst => "Amethyst",
        }
    }

    /// The name this material is written under in a save file.
    ///
    /// **A second table, and not [`name`](Material::name), on purpose.** A display
    /// name belongs to the UI and may be reworded — "End Stone" was spaced for
    /// exactly that reason — while a save key is a promise to every file already
    /// written: change it, and a player's Amethyst becomes an unknown word their
    /// next load refuses. Keeping them apart is what lets the first move freely.
    ///
    /// Lowercase and `snake_case` because these are JSON object keys, and
    /// `docs/SYSTEMS.md` asks the file to stay debuggable by hand.
    pub(crate) fn save_key(self) -> &'static str {
        match self {
            Self::Stone => "stone",
            Self::Coal => "coal",
            Self::Iron => "iron",
            Self::Gold => "gold",
            Self::Lapis => "lapis",
            Self::Redstone => "redstone",
            Self::Emerald => "emerald",
            Self::Diamond => "diamond",
            Self::Netherrack => "netherrack",
            Self::Quartz => "quartz",
            Self::AncientDebris => "ancient_debris",
            Self::Obsidian => "obsidian",
            Self::CryingObsidian => "crying_obsidian",
            Self::Endstone => "endstone",
            Self::Amethyst => "amethyst",
        }
    }

    /// The material a save key names, or [`None`] if no material answers to it.
    ///
    /// [`None`] rather than a default: an unknown key is a file this build does
    /// not understand, and guessing at it would load a run the player never
    /// played. The refusal travels up to `save`, which turns it into a load that
    /// fails cleanly.
    pub(crate) fn from_save_key(key: &str) -> Option<Self> {
        // Written as a reverse walk over the same table rather than a second
        // `match`, so the two directions cannot drift apart. Fifteen entries, read
        // at load time only.
        ALL_MATERIALS.iter().copied().find(|m| m.save_key() == key)
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
            Self::Netherrack
            | Self::Quartz
            | Self::AncientDebris
            | Self::Obsidian
            | Self::CryingObsidian => &[World::Nether],
            Self::Endstone | Self::Amethyst => &[World::End],
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

    /// The item a save key names, or [`None`] if no item answers to it.
    ///
    /// The denomination is read off the front of the key and the rest is a
    /// [`Material`], which works because [`COMPRESSED_PREFIX`] is not the start of
    /// any material's own key.
    pub(crate) fn from_save_key(key: &str) -> Option<Self> {
        match key.strip_prefix(COMPRESSED_PREFIX) {
            Some(rest) => Material::from_save_key(rest).map(Self::Compressed),
            None => Material::from_save_key(key).map(Self::Raw),
        }
    }
}

/// Written as a bare string — `"iron"`, `"compressed_iron"` — and not as the
/// derived shape of the enum.
///
/// **Hand-written because an [`Item`] is a map *key*.** The inventory is a map from
/// item to count, and JSON demands that an object's keys be strings; the derive
/// would emit `{"Raw": "Iron"}`, which `serde_json` rejects outright in key
/// position (at run time, never at compile time — hence
/// `an_inventory_survives_the_round_trip`). A string is also what makes the file
/// readable, which is the reason `docs/SYSTEMS.md` chose JSON over anything denser.
///
/// [`collect_str`](Serializer::collect_str) with a
/// [`format_args!`] rather than a `String`: the prefixed name is written straight
/// into the output, so the common case — an autosave every ten seconds — allocates
/// nothing for it.
impl Serialize for Item {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Raw(material) => serializer.serialize_str(material.save_key()),
            Self::Compressed(material) => {
                serializer.collect_str(&format_args!("{COMPRESSED_PREFIX}{}", material.save_key()))
            }
        }
    }
}

/// The one shape an [`Item`] can arrive in.
///
/// A visitor is serde's answer to "the data format decides what it hands you": it
/// offers one method per kind of value, and the ones left unimplemented become type
/// errors with a decent message for free.
///
/// It sits at module scope rather than nested inside [`Item::deserialize`] — serde's
/// usual home for a one-off visitor — for a coverage reason and no other: tarpaulin's
/// line parser counts an `impl` declared inside a function body as an executable line
/// no region ever covers, and drops the crate below 100%. Hoisted here it is an
/// ordinary private helper, invisible outside this module.
struct ItemKey;

impl Visitor<'_> for ItemKey {
    type Value = Item;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an item key such as \"iron\" or \"compressed_iron\"")
    }

    fn visit_str<E: serde::de::Error>(self, key: &str) -> Result<Item, E> {
        Item::from_save_key(key).ok_or_else(|| E::invalid_value(Unexpected::Str(key), &self))
    }
}

/// Reads back what [`Serialize`] wrote, and refuses anything else.
///
/// An unknown key is an error rather than a skipped entry: silently dropping it
/// would hand the player a run missing whatever the word meant, which is a worse
/// outcome than a load that stops and says so.
impl<'de> Deserialize<'de> for Item {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(ItemKey)
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

/// Every [`Material`] variant, in declaration order.
///
/// It exists because an enum cannot enumerate itself (see `block`'s `ALL_BLOCKS`,
/// which states the rationale), and unlike its siblings it is **not**
/// test-only: [`Material::from_save_key`] reads it to walk
/// [`save_key`](Material::save_key) backwards. That is the whole reason the reverse
/// lookup is a walk and not a second `match` — one table, so the two directions of
/// the save's naming cannot drift apart.
pub(crate) const ALL_MATERIALS: &[Material] = &[
    Material::Stone,
    Material::Coal,
    Material::Iron,
    Material::Gold,
    Material::Lapis,
    Material::Redstone,
    Material::Emerald,
    Material::Diamond,
    Material::Netherrack,
    Material::Quartz,
    Material::AncientDebris,
    Material::Obsidian,
    Material::CryingObsidian,
    Material::Endstone,
    Material::Amethyst,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_json;

    #[test]
    fn all_materials_covers_every_variant() {
        assert_eq!(
            ALL_MATERIALS.len(),
            15,
            "a Material variant was added or removed: update ALL_MATERIALS"
        );
    }

    /// Every world's filler yields its own material — Stone, Netherrack, End
    /// Stone. A world whose filler dropped nothing would spend the player's time
    /// for no return, which is the one thing an idle game cannot afford to do
    /// with the block the player hits most often.
    #[test]
    fn every_world_has_a_filler_material() {
        assert_eq!(Material::Stone.worlds(), &[World::Overworld]);
        assert_eq!(Material::Netherrack.worlds(), &[World::Nether]);
        assert_eq!(Material::Endstone.worlds(), &[World::End]);
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

    /// The variant spelling and the display name are allowed to diverge, and for
    /// `Endstone` they deliberately do: the code matches `Block::Endstone`, the
    /// player reads "End Stone".
    #[test]
    fn multi_word_materials_are_displayed_with_spaces() {
        assert_eq!(Material::AncientDebris.name(), "Ancient Debris");
        assert_eq!(Material::CryingObsidian.name(), "Crying Obsidian");
        assert_eq!(Material::Endstone.name(), "End Stone");
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

    /// Two materials sharing a save key would make one of them unreadable: the
    /// reverse walk answers with whichever comes first, so a run's Emeralds could
    /// load back as Diamonds.
    #[test]
    fn save_keys_are_unique() {
        for (index, &a) in ALL_MATERIALS.iter().enumerate() {
            for &b in &ALL_MATERIALS[index + 1..] {
                assert_ne!(
                    a.save_key(),
                    b.save_key(),
                    "{a:?} and {b:?} share the save key {}",
                    a.save_key()
                );
            }
        }
    }

    /// What lets [`Item::from_save_key`] split a key on its prefix alone. A
    /// material whose own key started with `compressed_` would be read back as the
    /// Compressed denomination of something else — or of nothing.
    #[test]
    fn every_material_key_is_a_word_of_its_own() {
        for &material in ALL_MATERIALS {
            assert!(
                !material.save_key().starts_with(COMPRESSED_PREFIX),
                "{material:?}'s key {} collides with the Compressed prefix",
                material.save_key()
            );
        }
    }

    /// The save's two directions have to agree for every variant, not just the
    /// ones a test happened to name.
    #[test]
    fn every_item_survives_its_save_key() {
        for &material in ALL_MATERIALS {
            for item in [Item::Raw(material), Item::Compressed(material)] {
                let key = test_json::write(&item);
                let read: Item = test_json::read(&key);
                assert_eq!(read, item, "{item:?} did not survive the key {key}");
            }
        }
    }

    /// The denomination is part of the key because it is part of the identity —
    /// the same reason it is part of the display name.
    #[test]
    fn a_key_names_its_denomination() {
        assert_eq!(test_json::write(&Item::Raw(Material::Iron)), "\"iron\"");
        assert_eq!(
            test_json::write(&Item::Compressed(Material::Iron)),
            "\"compressed_iron\""
        );
    }

    /// A key this build does not know is a file it cannot honour. Reading it as
    /// anything — or skipping it — would hand the player a run missing whatever
    /// the word meant.
    #[test]
    fn an_unknown_key_is_refused() {
        assert!(serde_json::from_str::<Item>("\"unobtainium\"").is_err());
        assert!(serde_json::from_str::<Item>("\"compressed_unobtainium\"").is_err());
        // The display name is not the save key, and must not be accepted as one.
        assert!(serde_json::from_str::<Item>("\"Ancient Debris\"").is_err());
    }

    /// The rejection is renderable. A load that stops on an unknown word owes the
    /// bug report a message naming what it wanted, which is the visitor's
    /// `expecting`: serde only reaches for it once a `visit_str` has refused.
    #[test]
    fn an_unknown_key_names_what_it_expected() {
        let message = match serde_json::from_str::<Item>("\"unobtainium\"") {
            Ok(item) => unreachable!("an unknown key must not parse: {item:?}"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("item key"), "unhelpful message: {message}");
    }
}
