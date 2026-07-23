//! What the player is carrying.
//!
//! Keyed by [`Item`], not [`Material`], so a resource can be held in both
//! denominations at once: 50 raw Iron and 6 Compressed Iron are two separate
//! entries. Upgrade costs are quoted in both and must be paid in both, so the
//! inventory never quietly converts one into the other — [`compress`] and
//! [`decompress`] are player actions, and they are the only way across.
//!
//! [`compress`]: Inventory::compress
//! [`decompress`]: Inventory::decompress

use crate::error::CoreError;
use crate::material::{Item, Material};
use crate::tunables::RAW_PER_COMPRESSED;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The player's stock, as a sparse map from [`Item`] to count.
///
/// An item absent from the map is held zero times; nothing distinguishes "never
/// had any" from "spent the last one", and no code should need to. The map is
/// private so that invariant cannot be broken from outside: [`remove`] deletes
/// an entry the moment it hits zero, so a count of `0` is never stored.
///
/// **A [`BTreeMap`] and not a [`HashMap`](std::collections::HashMap)**, for the
/// save's sake. A `HashMap`'s iteration order is deliberately unspecified, so the
/// same inventory would serialise to a different text on every write — and a text
/// that varies is one no golden save can pin and no two players can diff. Sorted
/// order makes "the same state writes the same file" a property of the type rather
/// than a convention to remember at every call site, which is the move
/// [`Mine`](crate::mine::Mine)'s grid already made by fusing its hole mask into the
/// cells. It costs nothing to be right here: an inventory holds a handful of
/// entries, where a B-tree walk beats hashing anyway.
///
/// **[`transparent`]**, so a save writes the map itself — `{"iron": 42}` — and not
/// the wrapper around it. The field is private and named for this module's own
/// convenience; a file format has no business depending on that name, and would
/// otherwise break the day it changed.
///
/// [`remove`]: Inventory::remove
/// [`transparent`]: https://serde.rs/container-attrs.html#transparent
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Inventory {
    items: BTreeMap<Item, u32>,
}

impl Inventory {
    /// Creates a new, empty inventory.
    pub fn new() -> Self {
        Inventory {
            items: BTreeMap::new(),
        }
    }

    /// Adds `amount` of `item`.
    ///
    /// Saturates at `u32::MAX` rather than overflowing. An idle game runs for
    /// weeks unattended, so the ceiling is reachable in a way it would not be in
    /// an active game, and a wrapped counter — a player's fortune silently
    /// becoming zero — is a far worse failure than a stuck one.
    pub fn add(&mut self, item: Item, amount: u32) {
        let entry = self.items.entry(item).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    /// Removes `amount` of `item`, or fails without touching the inventory.
    ///
    /// Partial removal is never the right answer to an unaffordable cost: it
    /// would take the player's items *and* refuse them the thing they were
    /// buying. So the check comes first, and the whole call is a no-op on
    /// failure.
    pub fn remove(&mut self, item: Item, amount: u32) -> Result<(), CoreError> {
        let held = self.count(item);
        if held < amount {
            return Err(CoreError::InsufficientItems {
                item,
                needed: amount,
                held,
            });
        }
        if held == amount {
            self.items.remove(&item);
        } else {
            self.items.insert(item, held - amount);
        }
        Ok(())
    }

    /// How many of `item` the player holds, in that denomination alone.
    pub fn count(&self, item: Item) -> u32 {
        self.items.get(&item).copied().unwrap_or(0)
    }

    /// Whether the player holds at least `amount` of `item`, in that
    /// denomination alone.
    ///
    /// Deliberately *not* value-aware: holding 650 raw Iron does not satisfy
    /// `has(Compressed(Iron), 6)`. Costs are paid in the denomination they are
    /// quoted in; use [`raw_value`](Inventory::raw_value) to ask the other
    /// question — "could they afford this if they compressed first?".
    pub fn has(&self, item: Item, amount: u32) -> bool {
        self.count(item) >= amount
    }

    /// Everything the player holds of `material`, counted in raw items.
    ///
    /// This is the *wealth* query, blind to denomination: 6 Compressed Iron and
    /// 50 raw Iron is 650. It is what tells a UI that a refused purchase is one
    /// [`compress`](Inventory::compress) away from succeeding rather than out of
    /// reach — the difference between "compress first" and "go mine more".
    ///
    /// Saturates, for the reason [`add`](Inventory::add) does.
    pub fn raw_value(&self, material: Material) -> u32 {
        let compressed = self
            .count(Item::Compressed(material))
            .saturating_mul(RAW_PER_COMPRESSED);
        self.count(Item::Raw(material)).saturating_add(compressed)
    }

    /// Mints `units` Compressed units of `material` from the raw items held.
    ///
    /// Fails, and changes nothing, if the player does not hold
    /// `units * RAW_PER_COMPRESSED` raw items.
    pub fn compress(&mut self, material: Material, units: u32) -> Result<(), CoreError> {
        let raw = Item::Raw(material);
        let held = self.count(raw);
        // Divide the stock down rather than multiplying the request up: for an
        // absurd `units` the product overflows u32, while the quotient cannot.
        if held / RAW_PER_COMPRESSED < units {
            return Err(CoreError::InsufficientItems {
                item: raw,
                needed: units.saturating_mul(RAW_PER_COMPRESSED),
                held,
            });
        }
        // The guard proves `units <= held / RAW_PER_COMPRESSED`, so this product
        // fits in a u32.
        self.remove(raw, units * RAW_PER_COMPRESSED)?;
        self.add(Item::Compressed(material), units);
        Ok(())
    }

    /// Breaks `units` Compressed units of `material` back into raw items.
    ///
    /// The exact inverse of [`compress`](Inventory::compress): free, and lossless
    /// in both directions. That is what keeps a run un-brickable — a player who
    /// compressed everything and then meets a cost with a raw part can always
    /// walk it back, so the strict payment rule can never trap them.
    pub fn decompress(&mut self, material: Material, units: u32) -> Result<(), CoreError> {
        self.remove(Item::Compressed(material), units)?;
        self.add(
            Item::Raw(material),
            units.saturating_mul(RAW_PER_COMPRESSED),
        );
        Ok(())
    }

    /// Whether this stock could have been produced by the rules.
    ///
    /// One invariant, the one the type docs open with: **a count of `0` is never
    /// stored.** [`remove`](Inventory::remove) deletes the entry instead, so
    /// "absent" and "held zero times" are the same state and no code has to tell
    /// them apart. A save could carry the difference back in, and then two
    /// inventories holding exactly the same items would stop comparing equal —
    /// which is quietly the end of every test in this crate that compares one.
    ///
    /// See [`Mine::validate`](crate::mine::Mine) for why the message is a plain
    /// string.
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.items.values().any(|&count| count == 0) {
            return Err("an inventory holds a count of zero, which is not a state it has");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_json;

    #[test]
    fn add_and_remove_track_the_count() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Stone), 10);
        assert_eq!(inventory.count(Item::Raw(Material::Stone)), 10);
        assert!(inventory.remove(Item::Raw(Material::Stone), 5).is_ok());
        assert_eq!(inventory.count(Item::Raw(Material::Stone)), 5);
    }

    /// The old inventory silently clamped an over-large removal to zero, which
    /// would have handed the player anything they could not afford, for free.
    #[test]
    fn removing_more_than_held_fails_and_changes_nothing() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Stone), 10);

        let err = inventory.remove(Item::Raw(Material::Stone), 15);
        assert_eq!(
            err,
            Err(CoreError::InsufficientItems {
                item: Item::Raw(Material::Stone),
                needed: 15,
                held: 10,
            })
        );
        assert_eq!(
            inventory.count(Item::Raw(Material::Stone)),
            10,
            "a failed removal must leave the stock untouched"
        );
    }

    #[test]
    fn has_answers_per_denomination() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Stone), 10);
        assert!(inventory.has(Item::Raw(Material::Stone), 5));
        assert!(!inventory.has(Item::Raw(Material::Stone), 15));
    }

    /// The strict payment rule, stated as a test: raw items do not satisfy a
    /// Compressed cost, however many of them there are. If this ever passes,
    /// compressing has become a button with no consequence.
    #[test]
    fn raw_items_do_not_satisfy_a_compressed_cost() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 650);

        assert!(!inventory.has(Item::Compressed(Material::Iron), 6));
        assert_eq!(
            inventory.raw_value(Material::Iron),
            650,
            "the value is there even though the denomination is not"
        );
    }

    #[test]
    fn compress_trades_a_hundred_raw_for_one_unit() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 650);

        assert!(inventory.compress(Material::Iron, 6).is_ok());
        assert_eq!(inventory.count(Item::Compressed(Material::Iron)), 6);
        assert_eq!(inventory.count(Item::Raw(Material::Iron)), 50);
    }

    /// Compressing is a conversion, not a creation: the player's wealth is the
    /// same on both sides of it. This is the invariant that lets the rest of the
    /// economy reason in raw items and ignore how the stock is shaped.
    #[test]
    fn compressing_and_decompressing_conserve_raw_value() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 650);

        assert!(inventory.compress(Material::Iron, 6).is_ok());
        assert_eq!(inventory.raw_value(Material::Iron), 650);

        assert!(inventory.decompress(Material::Iron, 6).is_ok());
        assert_eq!(inventory.raw_value(Material::Iron), 650);
        assert_eq!(inventory.count(Item::Raw(Material::Iron)), 650);
        assert_eq!(inventory.count(Item::Compressed(Material::Iron)), 0);
    }

    #[test]
    fn compressing_more_than_the_raw_stock_allows_fails() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 99);

        let err = inventory.compress(Material::Iron, 1);
        assert_eq!(
            err,
            Err(CoreError::InsufficientItems {
                item: Item::Raw(Material::Iron),
                needed: RAW_PER_COMPRESSED,
                held: 99,
            })
        );
        assert_eq!(inventory.count(Item::Raw(Material::Iron)), 99);
        assert_eq!(inventory.count(Item::Compressed(Material::Iron)), 0);
    }

    /// `units * RAW_PER_COMPRESSED` overflows u32 long before `units` does, so
    /// the affordability check must not compute it. It fails as a shortfall, the
    /// same as any other unaffordable request.
    #[test]
    fn compressing_an_absurd_number_of_units_fails_rather_than_overflowing() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 1_000);

        assert!(inventory.compress(Material::Iron, u32::MAX).is_err());
        assert_eq!(inventory.count(Item::Raw(Material::Iron)), 1_000);
    }

    #[test]
    fn decompressing_more_units_than_held_fails() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Compressed(Material::Iron), 2);

        assert!(inventory.decompress(Material::Iron, 3).is_err());
        assert_eq!(inventory.count(Item::Compressed(Material::Iron)), 2);
        assert_eq!(inventory.count(Item::Raw(Material::Iron)), 0);
    }

    /// The two denominations are independent stocks, and compressing one
    /// material must not disturb another.
    #[test]
    fn materials_do_not_bleed_into_each_other() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 100);
        inventory.add(Item::Raw(Material::Gold), 100);

        assert!(inventory.compress(Material::Iron, 1).is_ok());
        assert_eq!(inventory.count(Item::Raw(Material::Gold)), 100);
        assert_eq!(inventory.count(Item::Compressed(Material::Gold)), 0);
    }

    /// The map is keyed by [`Item`], which JSON cannot express as an object key
    /// on its own — the derived shape would be `{"Raw": "Iron"}` in key position,
    /// and `serde_json` refuses that at **run time**, never at compile time. So
    /// this test is the only thing standing between a hand-written key format and
    /// a save that fails the first time a player picks up an ore.
    #[test]
    fn an_inventory_survives_the_round_trip() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 42);
        inventory.add(Item::Compressed(Material::Iron), 3);
        inventory.add(Item::Raw(Material::Amethyst), 7);

        let written = test_json::write(&inventory);
        let read: Inventory = test_json::read(&written);

        assert_eq!(read, inventory);
    }

    /// `docs/SYSTEMS.md` picks JSON *because* the file can be read by a human. An
    /// inventory that wrote its wrapper struct, or its variants' Rust names, would
    /// still round-trip and still lose that. Sorted, because the map is ordered:
    /// the same stock writes the same text, every time.
    #[test]
    fn an_inventory_writes_itself_readably() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Compressed(Material::Iron), 3);
        inventory.add(Item::Raw(Material::Iron), 42);

        assert_eq!(
            test_json::write(&inventory),
            r#"{"iron":42,"compressed_iron":3}"#
        );
    }

    /// The "absent means zero" invariant, checked from the one direction that can
    /// break it. Two inventories holding the same items would otherwise stop
    /// comparing equal depending on which one had been *spent* down to nothing.
    #[test]
    fn a_stored_zero_is_not_a_state_an_inventory_has() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 5);
        assert!(inventory.validate().is_ok());

        // Only a save can put this back; `remove` deletes the entry instead.
        inventory.items.insert(Item::Raw(Material::Gold), 0);
        assert!(inventory.validate().is_err());
    }

    /// Spending the last of something leaves a valid inventory, not an entry at
    /// zero — the path the check above exists to protect.
    #[test]
    fn spending_the_last_of_an_item_leaves_no_entry_behind() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 5);
        assert!(inventory.remove(Item::Raw(Material::Iron), 5).is_ok());

        assert!(inventory.validate().is_ok());
        assert_eq!(inventory, Inventory::new());
    }
}
