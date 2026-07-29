//! Where a [`GameEvent`] becomes a sentence.
//!
//! One function, one `match`, and it is the **only** place a tick's news is worded.
//! A module of its own rather than an addition to [`crate::format`]: that one puts
//! *numbers* into the shape the frames count (`1 240`, a right-flush column), this
//! one puts *events* into words. They are different questions, and only this one has
//! a second reader coming — UI.md §5.5's History panel is the same buffer rendered
//! without the three-second window, so the sentence a toast shows and the sentence
//! the log keeps cannot be allowed to be written in two places.
//!
//! **The tone is part of the answer, not decoration.** UI.md §4.4 asks a colour to
//! double what the words already say; a toast has no glyph to double, so the pairing
//! is made here, once, where the event that justifies it is still in hand.

use skylode_core::{
    game::GameEvent,
    material::Item,
    reward::{LevelReward, Payout},
};

use crate::{format::grouped, toast::Tone};

/// What the player should be told about `event`, and in what voice.
///
/// **Every variant is answered**, and the `match` being exhaustive is the point: a
/// seventh mechanic added to [`GameEvent`] cannot ship silent, because this file will
/// not compile until someone has decided what it says.
///
/// The wording follows the History panel drawn in UI.md §5.5 (`Excavator!  +1
/// Compressed Iron`, `Mine refilled`) rather than being invented here, so the two
/// renderings of the buffer read alike.
pub fn of(event: &GameEvent) -> (String, Tone) {
    match event {
        GameEvent::LevelUp { level, reward } => (
            format!("Level {level}{}", granted(reward.as_ref())),
            Tone::Success,
        ),
        // **`broken`, never `cells.len()`.** The shape covers ground the swing had
        // already cleared — deliberately, so the flash draws a square and not the
        // four cells that happened to be left standing — and quoting it would promise
        // a haul the inventory never received.
        GameEvent::SpatialProc { kind, broken, .. } => (
            format!("{} — {} blocks", kind.name(), grouped(count(*broken))),
            Tone::Neutral,
        ),
        // The one proc worth an exclamation mark: it is the rarest thing in a swing,
        // and the only one whose payout is a denomination the player otherwise has to
        // mint by hand.
        GameEvent::ExcavatorProc { item } => (format!("Excavator!  +1 {item}"), Tone::Success),
        // The mine, not *a* mine: the event carries a kind because a run owns twelve,
        // but only the one the player is standing in can refill today, and naming it
        // would read as news about somewhere else.
        GameEvent::MineRefilled { .. } => ("Mine refilled".to_owned(), Tone::Neutral),
        GameEvent::BoostExpired => ("Redstone boost ended".to_owned(), Tone::Neutral),
    }
}

/// The tail of a level-up line: what the level handed over, or nothing at all.
///
/// **A level with no reward still gets a sentence**, which is why this returns the
/// empty string rather than the caller branching. [`reward_for_level`] answers
/// [`None`] only at the two ends of the ladder, and *reaching a level* is worth
/// saying even where it pays nothing.
///
/// The boost charge is deliberately unmentioned. It rides beside the payout on its
/// own cadence — every fifth level, world levels included — and UI.md §5.6 settles
/// that a garnish which lands that often announces nothing and so dilutes nothing.
///
/// [`reward_for_level`]: skylode_core::reward::reward_for_level
fn granted(reward: Option<&LevelReward>) -> String {
    match reward.map(|reward| &reward.payout) {
        Some(Payout::World(world)) => format!(" — The {} opens", world.name()),
        Some(Payout::Ore(lines)) => format!(" — {}", ore(lines)),
        None => String::new(),
    }
}

/// A payout's lines as one comma-separated phrase: `+115 Quartz, +80 Ancient Debris`.
///
/// Quoted as raw totals with a `+`, matching §5.6's roadmap and §6.4's offline
/// summary: the strict two-denomination rule governs **paying**, never receiving, and
/// a level's bundle is always credited raw ([`Payout::Ore`] says so in the core).
fn ore(lines: &[(Item, u32)]) -> String {
    lines
        .iter()
        .map(|&(item, amount)| format!("+{} {item}", grouped(amount)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A block count as the `u32` [`grouped`] takes.
///
/// The core counts blocks in a [`usize`] because it counts a [`Vec`]'s length; the
/// separator helper takes a [`u32`] because every other number on screen is one. The
/// saturation is unreachable — the largest blast in the game is a maxed Nuke over a
/// 20×10 grid — and is here so the conversion needs no `unwrap` the crate's lints
/// would refuse anyway.
fn count(blocks: usize) -> u32 {
    u32::try_from(blocks).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use skylode_core::{
        enchant::EnchantType, material::Material, mine_kind::MineKind, world::World,
    };

    use super::*;

    #[test]
    fn a_level_up_names_the_level_and_what_it_paid() {
        let (text, tone) = of(&GameEvent::LevelUp {
            level: 23,
            reward: Some(LevelReward {
                payout: Payout::Ore(vec![
                    (Item::Raw(Material::Quartz), 115),
                    (Item::Raw(Material::AncientDebris), 80),
                ]),
                boost_charges: 0,
            }),
        });
        assert_eq!(text, "Level 23 — +115 Quartz, +80 Ancient Debris");
        assert_eq!(tone, Tone::Success);
    }

    #[test]
    fn a_world_level_says_what_opened_instead_of_a_bundle() {
        // Levels 15 and 30 pay a dimension and no loot, and the payout being an enum
        // is what stops this line from having to say "and no ore".
        let (text, _) = of(&GameEvent::LevelUp {
            level: 15,
            reward: Some(LevelReward {
                payout: Payout::World(World::Nether),
                boost_charges: 1,
            }),
        });
        assert_eq!(text, "Level 15 — The Nether opens");
    }

    #[test]
    fn a_level_that_pays_nothing_is_still_announced() {
        let (text, tone) = of(&GameEvent::LevelUp {
            level: 2,
            reward: None,
        });
        assert_eq!(text, "Level 2");
        assert_eq!(tone, Tone::Success);
    }

    #[test]
    fn a_blast_counts_the_blocks_it_broke_and_not_the_cells_it_covered() {
        // The whole reason `broken` exists as a field. Twenty-five cells of shape over
        // a half-dug grid, nine blocks standing in them: the sentence quotes the nine,
        // because nine is what reached the inventory.
        let (text, tone) = of(&GameEvent::SpatialProc {
            kind: EnchantType::Explosive,
            origin: (4, 4),
            cells: vec![(0, 0); 25],
            broken: 9,
        });
        assert_eq!(text, "Explosive — 9 blocks");
        assert_eq!(tone, Tone::Neutral);
    }

    #[test]
    fn a_big_blast_groups_its_thousands_like_every_other_number() {
        let (text, _) = of(&GameEvent::SpatialProc {
            kind: EnchantType::Nuke,
            origin: (0, 0),
            cells: Vec::new(),
            broken: 1_200,
        });
        assert_eq!(text, "Nuke — 1 200 blocks");
    }

    #[test]
    fn an_excavator_names_the_denomination_it_substituted() {
        let (text, tone) = of(&GameEvent::ExcavatorProc {
            item: Item::Compressed(Material::Iron),
        });
        assert_eq!(text, "Excavator!  +1 Compressed Iron");
        assert_eq!(tone, Tone::Success);
    }

    #[test]
    fn a_refill_does_not_name_the_mine_the_player_is_standing_in() {
        let (text, tone) = of(&GameEvent::MineRefilled {
            kind: MineKind::Iron,
        });
        assert_eq!(text, "Mine refilled");
        assert_eq!(tone, Tone::Neutral);
    }

    #[test]
    fn a_lapsed_boost_says_so_without_alarm() {
        // Neutral and not a refusal: nothing was denied, a timer ran out. The tone is
        // what tells a glance which of the two just happened.
        let (text, tone) = of(&GameEvent::BoostExpired);
        assert_eq!(text, "Redstone boost ended");
        assert_eq!(tone, Tone::Neutral);
    }
}
