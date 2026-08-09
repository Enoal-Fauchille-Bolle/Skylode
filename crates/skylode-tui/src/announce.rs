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

use crate::{
    config::NumberFormat,
    format::grouped,
    toast::{Salience, Tone},
};

/// What the player should be told about `event`, in what voice, and how loudly.
///
/// **Every variant is answered**, and the `match` being exhaustive is the point: a
/// seventh mechanic added to [`GameEvent`] cannot ship unworded, because this file will
/// not compile until someone has decided what it says.
///
/// The wording follows the History panel drawn in UI.md §5.5 (`Excavator!  +1
/// Compressed Iron`, `Mine refilled`) rather than being invented here, so the two
/// renderings of the buffer read alike.
///
/// **The [`Salience`] is decided here and nowhere else, and that is a boundary and not
/// a convenience.** Whether news deserves to interrupt is a judgement about an
/// interface — a different front-end could answer it differently — so it cannot live in
/// [`skylode_core`], which does not know a screen exists. This function already turns an
/// event into a sentence; the level rides beside the sentence because it is decided from
/// the same thing, the event itself.
///
/// **Three of the five are [`Silent`](Salience::Silent), and the test for it is "is the
/// screen already saying this".** A blast has [`flash`](crate::flash) painting the cells
/// it cleared, a refill has a grid visibly filling back up, and an Excavator's payout is
/// a count in the inventory. Those three are also, by a wide margin, the most frequent —
/// an Excavator at its ceiling rolls 5 % of a swing and a held `Space` swings twenty
/// times a second — so they were spending the interface's only interruption on the
/// three things needing none. They stay in the buffer and so stay in §5.5's History.
pub fn of(event: &GameEvent, format: NumberFormat) -> (String, Tone, Salience) {
    match event {
        // **The sentence says what is waiting, not what arrived**, because as of TUI
        // phase 7 nothing arrives: crossing a level files its reward and the player
        // collects it on the Levels screen. The numbers stay in the toast — the
        // announcement is owed them the instant the level is reached — but the
        // trailing clause is what turns a receipt into an errand.
        //
        // A level that pays nothing gets no clause and so no errand, which is the
        // right reading: the two ends of the ladder leave nothing on the screen.
        //
        // **The one `Major` in the game**, and the errand is why: every other
        // announcement is a receipt for something already done, this one asks the
        // player to go somewhere. A line that ends in an instruction is the one line
        // that must not be erased by the next block to break.
        GameEvent::LevelUp { level, reward } => (
            match granted(reward.as_ref(), format) {
                grants if grants.is_empty() => format!("Level {level}"),
                grants => format!("Level {level}{grants} — claim on 6"),
            },
            Tone::Success,
            Salience::Major,
        ),
        // **`broken`, never `cells.len()`.** The shape covers ground the swing had
        // already cleared — deliberately, so the flash draws a square and not the
        // four cells that happened to be left standing — and quoting it would promise
        // a haul the inventory never received.
        //
        // **Silent, and `flash` is the reason it can be.** UI.md §7 splits this event
        // in two — the toast said *what*, the flash says *where* — and in practice the
        // *where* carries the *what*: a painted Nuke square is not mistakable for a
        // Jackhammer line. The sentence was the redundant half, and it was arriving
        // often enough to own the slot outright.
        GameEvent::SpatialProc { kind, broken, .. } => (
            format!(
                "{} — {} blocks",
                kind.name(),
                grouped(count(*broken), format)
            ),
            Tone::Neutral,
            Salience::Silent,
        ),
        // The one proc worth an exclamation mark: it is the rarest thing in a swing,
        // and the only one whose payout is a denomination the player otherwise has to
        // mint by hand.
        //
        // **Silent all the same, and it is the closest call of the three.** Its news is
        // the least visible elsewhere — a Compressed unit appearing in a pile the player
        // is not looking at. What settles it is the rate: rarest *per swing* is not rare,
        // at twenty swings a second and a 5 % ceiling, and a line arriving about once a
        // second is exactly the chatter this change exists to clear.
        GameEvent::ExcavatorProc { item } => (
            format!("Excavator!  +1 {item}"),
            Tone::Success,
            Salience::Silent,
        ),
        // The mine, not *a* mine: the event carries a kind because a run owns twelve,
        // but only the one the player is standing in can refill today, and naming it
        // would read as news about somewhere else.
        //
        // Silent for the plainest reason of the three: the grid the sentence describes
        // is the thing the player is looking at while it happens.
        GameEvent::MineRefilled { .. } => {
            ("Mine refilled".to_owned(), Tone::Neutral, Salience::Silent)
        }
        // **The one tick event that keeps the slot.** Nothing else on screen retracts
        // when a boost lapses — the gauge simply stops being drawn — so this is a state
        // change with no picture of its own, and it is rare enough to afford one.
        GameEvent::BoostExpired => (
            "Redstone boost ended".to_owned(),
            Tone::Neutral,
            Salience::Normal,
        ),
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
fn granted(reward: Option<&LevelReward>, format: NumberFormat) -> String {
    match reward {
        Some(reward) => format!(" — {}", payout(&reward.payout, format)),
        None => String::new(),
    }
}

/// What a payout hands over, as one phrase and with no leading punctuation.
///
/// **Shared with the Levels roadmap**, which prints the same phrase as its `Grants`
/// column. One wording, two renderings — the toast's is the tail of a sentence and the
/// row's is a cell — and sharing it is what stops the announcement of a level and the
/// row describing that same level from quoting different materials.
///
/// The **boost charge is not in here**, and that is the seam between the two callers:
/// UI.md §5.6's roadmap appends `, +1 charge` and the toast deliberately does not,
/// since a garnish landing every fifth level announces nothing and dilutes the payout
/// beside it. Each caller adds what its own frame asks for.
pub fn payout(payout: &Payout, format: NumberFormat) -> String {
    match payout {
        Payout::World(world) => format!("The {} opens", world.name()),
        Payout::Ore(lines) => ore(lines, format),
    }
}

/// A payout's lines as one comma-separated phrase: `+115 Quartz, +80 Ancient Debris`.
///
/// Quoted as raw totals with a `+`, matching §5.6's roadmap and §6.4's offline
/// summary: the strict two-denomination rule governs **paying**, never receiving, and
/// a level's bundle is always credited raw ([`Payout::Ore`] says so in the core).
fn ore(lines: &[(Item, u32)], format: NumberFormat) -> String {
    lines
        .iter()
        .map(|&(item, amount)| format!("+{} {item}", grouped(amount, format)))
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

    /// A level-up names the level, what it is worth, and where to go and get it.
    ///
    /// The trailing clause is the whole of TUI phase 7's change to this line: the
    /// bundle is filed rather than credited, so the sentence is an errand and not a
    /// receipt.
    #[test]
    fn a_level_up_names_the_level_and_where_to_claim_it() {
        let (text, tone, _) = of(
            &GameEvent::LevelUp {
                level: 23,
                reward: Some(LevelReward {
                    payout: Payout::Ore(vec![
                        (Item::Raw(Material::Quartz), 115),
                        (Item::Raw(Material::AncientDebris), 80),
                    ]),
                    boost_charges: 0,
                }),
            },
            NumberFormat::default(),
        );
        assert_eq!(
            text,
            "Level 23 — +115 Quartz, +80 Ancient Debris — claim on 6"
        );
        assert_eq!(tone, Tone::Success);
    }

    #[test]
    fn a_world_level_says_what_opened_instead_of_a_bundle() {
        // Levels 15 and 30 pay a dimension and no loot, and the payout being an enum
        // is what stops this line from having to say "and no ore".
        let (text, _, _) = of(
            &GameEvent::LevelUp {
                level: 15,
                reward: Some(LevelReward {
                    payout: Payout::World(World::Nether),
                    boost_charges: 1,
                }),
            },
            NumberFormat::default(),
        );
        assert_eq!(text, "Level 15 — The Nether opens — claim on 6");
    }

    #[test]
    fn a_level_that_pays_nothing_is_still_announced() {
        let (text, tone, _) = of(
            &GameEvent::LevelUp {
                level: 2,
                reward: None,
            },
            NumberFormat::default(),
        );
        assert_eq!(text, "Level 2");
        assert_eq!(tone, Tone::Success);
    }

    #[test]
    fn a_blast_counts_the_blocks_it_broke_and_not_the_cells_it_covered() {
        // The whole reason `broken` exists as a field. Twenty-five cells of shape over
        // a half-dug grid, nine blocks standing in them: the sentence quotes the nine,
        // because nine is what reached the inventory.
        let (text, tone, _) = of(
            &GameEvent::SpatialProc {
                kind: EnchantType::Explosive,
                origin: (4, 4),
                cells: vec![(0, 0); 25],
                broken: 9,
            },
            NumberFormat::default(),
        );
        assert_eq!(text, "Explosive — 9 blocks");
        assert_eq!(tone, Tone::Neutral);
    }

    #[test]
    fn a_big_blast_groups_its_thousands_like_every_other_number() {
        let (text, _, _) = of(
            &GameEvent::SpatialProc {
                kind: EnchantType::Nuke,
                origin: (0, 0),
                cells: Vec::new(),
                broken: 1_200,
            },
            NumberFormat::default(),
        );
        assert_eq!(text, "Nuke — 1 200 blocks");
    }

    #[test]
    fn an_excavator_names_the_denomination_it_substituted() {
        let (text, tone, _) = of(
            &GameEvent::ExcavatorProc {
                item: Item::Compressed(Material::Iron),
            },
            NumberFormat::default(),
        );
        assert_eq!(text, "Excavator!  +1 Compressed Iron");
        assert_eq!(tone, Tone::Success);
    }

    #[test]
    fn a_refill_does_not_name_the_mine_the_player_is_standing_in() {
        let (text, tone, _) = of(
            &GameEvent::MineRefilled {
                kind: MineKind::Iron,
            },
            NumberFormat::default(),
        );
        assert_eq!(text, "Mine refilled");
        assert_eq!(tone, Tone::Neutral);
    }

    /// **The whole of the change, in one table.** Three of the five never reach the
    /// screen and two do, and the split is what stops a level-up being erased by a
    /// refill that lands a frame later.
    ///
    /// Asserted as a table rather than one `assert` per test, because what matters is
    /// the *proportion*: a sixth event added at `Normal` without a thought would leave
    /// every one of the individual assertions passing while quietly refilling the slot
    /// with chatter, and this row list is where that shows up as a diff.
    #[test]
    fn only_the_news_the_screen_is_not_already_showing_reaches_the_slot() {
        let cases = [
            (
                GameEvent::LevelUp {
                    level: 23,
                    reward: None,
                },
                Salience::Major,
            ),
            (
                GameEvent::SpatialProc {
                    kind: EnchantType::Nuke,
                    origin: (0, 0),
                    cells: Vec::new(),
                    broken: 200,
                },
                Salience::Silent,
            ),
            (
                GameEvent::ExcavatorProc {
                    item: Item::Compressed(Material::Iron),
                },
                Salience::Silent,
            ),
            (
                GameEvent::MineRefilled {
                    kind: MineKind::Iron,
                },
                Salience::Silent,
            ),
            (GameEvent::BoostExpired, Salience::Normal),
        ];

        for (event, expected) in cases {
            let (_, _, salience) = of(&event, NumberFormat::default());
            assert_eq!(salience, expected, "{event:?}");
        }
    }

    /// A level-up is the one announcement that may not be covered, and the level is
    /// what carries that — not its tone, which it shares with an Excavator proc firing
    /// about once a second.
    #[test]
    fn a_level_up_is_the_only_major_and_its_tone_does_not_say_so() {
        let (_, tone, salience) = of(
            &GameEvent::LevelUp {
                level: 23,
                reward: None,
            },
            NumberFormat::default(),
        );
        let (_, excavator_tone, excavator_salience) = of(
            &GameEvent::ExcavatorProc {
                item: Item::Compressed(Material::Iron),
            },
            NumberFormat::default(),
        );

        assert_eq!(salience, Salience::Major);
        assert_eq!(excavator_salience, Salience::Silent);
        // Same voice, opposite levels: the two axes are genuinely independent, and a
        // ranking read off the tone would have put these two together.
        assert_eq!(tone, excavator_tone);
    }

    #[test]
    fn a_lapsed_boost_says_so_without_alarm() {
        // Neutral and not a refusal: nothing was denied, a timer ran out. The tone is
        // what tells a glance which of the two just happened.
        let (text, tone, _) = of(&GameEvent::BoostExpired, NumberFormat::default());
        assert_eq!(text, "Redstone boost ended");
        assert_eq!(tone, Tone::Neutral);
    }
}
