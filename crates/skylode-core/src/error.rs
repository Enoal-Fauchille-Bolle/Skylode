//! Errors returned by fallible core operations.
//!
//! The workspace lints warn on `unwrap`, `expect` and `panic`, so an operation
//! the player can legitimately get wrong — spending what they do not have —
//! returns a [`CoreError`] rather than trapping.
//!
//! Errors carry *numbers*, not just a kind. A refusal the UI can only render as
//! "you can't afford that" is a dead end; one that knows the player is 40 Iron
//! short can say so, and can offer the missing step.

use crate::material::Item;
use std::fmt;

/// Something the rules would not allow.
///
/// One variant today; the concerns still to be built (locked mines, a pickaxe
/// tier too low, an upgrade already maxed) belong here too as they land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreError {
    /// The inventory holds fewer than `needed` of `item`.
    ///
    /// This is also what a player sitting on 650 raw Iron gets when a cost asks
    /// for 6 Compressed Iron: the *value* is there, the *denomination* is not.
    /// The two look identical from the till, so the fields let the caller tell
    /// them apart — if `held` covers the cost once converted, the answer to give
    /// the player is "compress first", not "come back richer". See [`Item`].
    InsufficientItems {
        /// The item the player came up short on, denomination included.
        item: Item,
        /// How many the operation required.
        needed: u32,
        /// How many the inventory actually held.
        held: u32,
    },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientItems { item, needed, held } => {
                write!(f, "need {needed} {item}, have {held}")
            }
        }
    }
}

impl std::error::Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::Material;

    /// The message names the denomination, because "need 6 Iron, have 650" would
    /// read as nonsense to a player holding 650 raw Iron. It is the *Compressed*
    /// Iron they are short of.
    #[test]
    fn insufficient_items_names_the_denomination() {
        let err = CoreError::InsufficientItems {
            item: Item::Compressed(Material::Iron),
            needed: 6,
            held: 0,
        };
        assert_eq!(err.to_string(), "need 6 Compressed Iron, have 0");
    }
}
