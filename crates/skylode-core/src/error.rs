//! Errors returned by fallible core operations.
//!
//! The workspace lints warn on `unwrap`, `expect` and `panic`, so an operation
//! the player can legitimately get wrong — spending what they do not have,
//! buying a level that does not exist — returns a [`CoreError`] rather than
//! trapping.
//!
//! Errors carry *numbers*, not just a kind. A refusal the UI can only render as
//! "you can't afford that" is a dead end; one that knows the player is 40 Iron
//! short can say so, and can offer the missing step.
//!
//! **A refusal changes nothing.** Every operation returning a [`CoreError`] is a
//! no-op on the failing path: no partial debit, no half-applied upgrade. The
//! economy leans on this — a purchase checks, debits, then upgrades, and any of
//! the three refusing must leave the player exactly where they were.

use crate::enchant::EnchantType;
use crate::material::Item;
use std::fmt;

/// Something the rules would not allow.
///
/// The gates still to be built (a locked mine, a pickaxe tier too low for a
/// block) belong here too, and land with the systems that enforce them.
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
    /// The enchant already sits at the highest level the game has rules for.
    ///
    /// Carries the `cap` because it is not a property of the enchant alone:
    /// Efficiency stops at 5 on every tier but Netherite, where it climbs to 15
    /// (see [`EnchantType::max_level`]). "Fortune is capped" is a fact; "Fortune
    /// is capped at 10" is one the player can plan around.
    EnchantAtCap {
        /// The enchant that has nowhere left to climb.
        kind: EnchantType,
        /// The cap it is sitting at, for the tier it was asked about.
        cap: u8,
    },
    /// A Netherite pickaxe with a maxed Efficiency: the upgrade path is spent.
    ///
    /// The refusal is load-bearing, not cosmetic. The tier ladder ends at
    /// Netherite, and the upgrade step that trades a maxed Efficiency for the
    /// next tier has no tier left to buy — taking it anyway would wipe Efficiency
    /// for nothing and drop the player from 235 mining power back to 10,
    /// *permanently*. See [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe).
    PickaxeFullyUpgraded,
    /// A tier jump was asked for while Efficiency is still below its cap.
    ///
    /// The two pickaxe upgrades are bought separately, and a tier jump **resets
    /// Efficiency to 0** — so buying it early would throw away levels the player
    /// paid for. The rule is: fill Efficiency to its
    /// [cap](crate::pickaxe::PickaxeTier::efficiency_cap) first, then trade the
    /// maxed enchant for the next tier and re-climb on a stronger base. Carries
    /// both numbers so the UI can say how many Efficiency levels are still owed
    /// before the tier button opens.
    EfficiencyNotMaxed {
        /// The Efficiency level the pickaxe currently holds.
        current: u8,
        /// The cap it must reach on this tier before the jump is allowed.
        cap: u8,
    },
    /// The mine already fills the largest grid the size table holds.
    ///
    /// Size levels past the table buy no blocks: the dimensions stop growing.
    /// Refusing rather than incrementing is what keeps a paid upgrade from
    /// charging for nothing once the economy lands.
    MineSizeMaxed {
        /// The largest size level, which the mine is already at.
        level: u32,
    },
    /// The mine's bought richness ceiling is already at the highest level.
    ///
    /// The sibling of [`MineSizeMaxed`](CoreError::MineSizeMaxed) for the mine's
    /// other paid track: the richness *level* (the ceiling the dial may reach) has
    /// a top rung, and a purchase past it would charge for nothing. Distinct from
    /// [`RichnessAboveCeiling`](CoreError::RichnessAboveCeiling), which refuses the
    /// free *dial* — this refuses the *purchase* that would raise the ceiling.
    RichnessLevelMaxed {
        /// The highest richness level, which the mine is already at.
        level: u32,
    },
    /// The richness dial was pushed past the ceiling the player has bought.
    ///
    /// The dial moves freely and for free, but only *below* the level bought on
    /// the mine's richness track: the ceiling is the purchase, the dial is what
    /// the purchase entitles you to.
    ///
    /// Carries both numbers rather than the ceiling alone because the gap is the
    /// actionable part. "The dial won't go there" leaves the UI a greyed-out
    /// control and nothing to say; `requested - ceiling` lets it name the levels
    /// still to buy. See
    /// [`set_richness_setting`](crate::mine::Mine::set_richness_setting).
    RichnessAboveCeiling {
        /// The setting that was asked for.
        requested: u32,
        /// The highest setting the mine's bought richness level allows.
        ceiling: u32,
    },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientItems { item, needed, held } => {
                write!(f, "need {needed} {item}, have {held}")
            }
            Self::EnchantAtCap { kind, cap } => {
                write!(f, "{} is already at its cap of {cap}", kind.name())
            }
            Self::PickaxeFullyUpgraded => write!(f, "the pickaxe is fully upgraded"),
            Self::EfficiencyNotMaxed { current, cap } => {
                write!(
                    f,
                    "Efficiency must reach its cap of {cap} before the tier advances (at {current})"
                )
            }
            Self::MineSizeMaxed { level } => {
                write!(f, "the mine is already at its largest size, level {level}")
            }
            Self::RichnessLevelMaxed { level } => {
                write!(
                    f,
                    "the mine's richness is already at its highest level, {level}"
                )
            }
            Self::RichnessAboveCeiling { requested, ceiling } => {
                write!(
                    f,
                    "richness {requested} is above the bought ceiling of {ceiling}"
                )
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

    /// A cap the player cannot see is a wall they keep walking into. The message
    /// names the number, so the Upgrades screen can say why the button is dead
    /// without knowing anything about Efficiency's tier-dependent ceiling.
    #[test]
    fn a_capped_enchant_names_the_cap_it_is_sitting_at() {
        let err = CoreError::EnchantAtCap {
            kind: EnchantType::Fortune,
            cap: 10,
        };
        assert_eq!(err.to_string(), "Fortune is already at its cap of 10");
    }

    #[test]
    fn a_spent_pickaxe_and_a_maxed_mine_say_so() {
        assert_eq!(
            CoreError::PickaxeFullyUpgraded.to_string(),
            "the pickaxe is fully upgraded"
        );
        assert_eq!(
            CoreError::MineSizeMaxed { level: 9 }.to_string(),
            "the mine is already at its largest size, level 9"
        );
        assert_eq!(
            CoreError::RichnessLevelMaxed { level: 9 }.to_string(),
            "the mine's richness is already at its highest level, 9"
        );
    }

    /// The tier-jump refusal names both numbers, so the Upgrades screen can say how
    /// many Efficiency levels are still owed before the tier button opens.
    #[test]
    fn a_tier_jump_before_efficiency_is_maxed_names_both_numbers() {
        let err = CoreError::EfficiencyNotMaxed { current: 2, cap: 5 };
        assert_eq!(
            err.to_string(),
            "Efficiency must reach its cap of 5 before the tier advances (at 2)"
        );
    }

    /// The message names both numbers, so the Upgrades screen can turn a refusal
    /// into the next step — "buy 4 more richness levels" — rather than a dead
    /// control.
    #[test]
    fn a_dial_past_its_ceiling_names_both_numbers() {
        let err = CoreError::RichnessAboveCeiling {
            requested: 7,
            ceiling: 3,
        };
        assert_eq!(
            err.to_string(),
            "richness 7 is above the bought ceiling of 3"
        );
    }
}
