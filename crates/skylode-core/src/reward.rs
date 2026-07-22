//! What a level-up hands over.
//!
//! One entry point, [`reward_for_level`], and it is a **pure function of the level**
//! with no randomness and no run state. That is a requirement rather than a
//! simplification: the Levels screen draws the whole 1→[`LEVEL_CAP`] ladder in advance,
//! rungs the player has not reached included, so a reward that were *drawn* could not
//! be shown before it fired — and freezing the draw would mean carrying a PRNG state in
//! the save to remember fifty numbers a formula already knows.
//!
//! ## The two axes, and why they are two types
//!
//! A level-up gives a **payout** — a world *xor* a bundle of ore — plus a **garnish**
//! that runs on its own rhythm and ignores which payout fired. [`Payout`] is an enum
//! because its rule is exclusive: levels 15 and 30 open a dimension and pay no ore,
//! that world *being* the reward, and a bundle on top would split one announcement in
//! two and dilute the half that matters. Modelled as two `Option` fields, "both" and
//! "neither" would be writable and the rule would live in a `debug_assert`; as an enum
//! the compiler carries it.
//!
//! The boost charge is *orthogonal* — a world level grants one too — so it is a field
//! beside the enum, readable without a `match`. This is
//! [`MineLock`](crate::mine_kind::MineLock)'s lesson run the other way: model each axis
//! as what it is, rather than forcing one shape onto both.
//!
//! ## Why the bundle mirrors a price
//!
//! Its budget is `LEVEL_REWARD_BASE * level` raw, and it quotes the same materials in
//! the same 50 / 35 / 15 proportions as the [enchant cost](crate::economy::enchant_cost)
//! of the matching rung — the world's enchant material, plus that rung's abundant and
//! scarce [fuel ores](crate::economy::enchant_fuel). The player recognises what they
//! receive because it is the shape of what they spend, and **sharing** that table
//! rather than copying it is what stops the two drifting: re-balancing the fuel
//! re-balances the rewards in the same edit.
//!
//! ## Two roundings, and they differ on purpose
//!
//! A price must reconcile — its lines add back to the quoted total, or the player pays
//! a rounding error. **A reward has no quoted total**: nothing is announced but the
//! lines themselves, so all three shares are floored independently and 0 to 2 raw of a
//! budget simply go unpaid. The End bundle keeps [`split_rare`]'s exact complement,
//! because there the two lines *are* a split of one number.
//!
//! ## What is not here
//!
//! Nothing consumes any of this in phase 6: the ore wants an
//! [`Inventory`](crate::inventory::Inventory) and the charges a counter, both of which
//! arrive with phase 7's `GameState`. The rule is written now, ahead of the runtime
//! that will call it, exactly as `can_mine` and `Boost::tick` were.

use crate::economy::{
    FUEL_ABUNDANT_PERMILLE, FUEL_PRINCIPAL_PERMILLE, FUEL_SCARCE_PERMILLE,
    RECIPE_RAMP_START_PERMILLE, enchant_fuel, split_rare,
};
use crate::material::{Item, Material};
use crate::mine_kind::MineKind;
use crate::tunables::{
    END_UNLOCK_LEVEL, LEVEL_CAP, LEVEL_REWARD_BASE, LEVEL_REWARD_BOOST_EVERY,
    LEVEL_REWARD_EMERALD_EVERY, LEVEL_REWARD_EMERALD_PERMILLE,
};
use crate::world::World;

/// The lowest level that rewards anything.
///
/// Level 1 is where the player *starts*, so it is never **reached**: handing it a
/// bundle would pay out for crossing a threshold nobody crossed. The same reasoning
/// makes [`World::unlocked_by_reaching`] return [`None`] there.
const FIRST_REWARDED_LEVEL: u32 = 2;

/// How many rungs of the [fuel table](enchant_fuel) one world's levels walk through.
///
/// Three each for the Overworld and the Nether, which is what makes the table's six
/// entries cover the levels below the End exactly. It is the same 3 / 3 / 4 split the
/// enchant ladder divides its ten levels by — the reward walks the first two thirds of
/// it and the End's ramp covers the rest.
const BANDS_PER_WORLD: u32 = 3;

/// The single thing a level-up's payout is: a world, or a bundle of ore.
///
/// Never both, never neither — see the module docs for why that is an enum and not two
/// nullable fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payout {
    /// The dimension this level opens. Levels [15](crate::tunables::NETHER_UNLOCK_LEVEL)
    /// and [30](END_UNLOCK_LEVEL), and no others.
    World(World),
    /// A bundle of ore, as `(item, amount)` pairs in the order a UI lists them:
    /// principal, abundant, scarce, then Emerald when the level earns it.
    ///
    /// **Always [`Raw`](Item::Raw), never [`Compressed`](Item::Compressed).** Compressed
    /// units are minted by the player, by hand, and `docs/DECISIONS.md` keeps that a
    /// deliberate step rather than a cosmetic button; handing over ready-made ones
    /// would quietly do it for them. Since
    /// [`compress`](crate::inventory::Inventory::compress) is free and lossless both
    /// ways, crediting raw takes away no ability — only the shortcut. It also makes the
    /// toast (`+115 Quartz`) describe exactly what lands in the inventory.
    ///
    /// [`Item`] rather than [`Material`] because that is what
    /// [`Inventory::add`](crate::inventory::Inventory::add) takes, so phase 7 credits
    /// the bundle by walking it — and because a payout that named only materials could
    /// not express the choice above, it would merely be silent about it.
    Ore(Vec<(Item, u32)>),
}

/// Everything reaching one level hands over.
///
/// The [`payout`](Self::payout) is exclusive; [`boost_charges`](Self::boost_charges)
/// rides beside it on its own cadence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelReward {
    /// The one thing this level pays: a world, or ore.
    pub payout: Payout,
    /// Boost charges granted — `0` or `1` today, a count because the reserve
    /// accumulates and phase 7 will add to it.
    ///
    /// A count rather than a running boost: every boost in the game is identical and a
    /// charge is held until fired, which is what makes crossing several levels at once
    /// — a lump of offline experience — safe. Windows would burn down unwatched.
    /// Redstone itself is never granted; the charges are what it would have bought.
    pub boost_charges: u32,
}

/// What reaching `level` grants, or [`None`] if that level grants nothing.
///
/// [`None`] below [`FIRST_REWARDED_LEVEL`] and past [`LEVEL_CAP`] — the two ends of the
/// ladder, one because starting is not a level-up and the other because there is no
/// such level to reach. Everything between always yields a reward, which is the "never
/// nothing" half of the payout rule made total by the signature.
///
/// **The order inside the function is load-bearing.** [`World::unlocked_by_reaching`]
/// is tested first and returns immediately, so levels 15 and 30 never reach the ore
/// arm. Both are multiples of five *and* of three, and that one ordering resolves both
/// collisions with no special case: the charge is added after, unconditionally, and the
/// Emerald line — which lives inside the ore arm — is simply never reached.
pub fn reward_for_level(level: u32) -> Option<LevelReward> {
    if !(FIRST_REWARDED_LEVEL..=LEVEL_CAP).contains(&level) {
        return None;
    }

    let boost_charges = u32::from(level.is_multiple_of(LEVEL_REWARD_BOOST_EVERY));

    let payout = match World::unlocked_by_reaching(level) {
        Some(world) => Payout::World(world),
        None => Payout::Ore(ore_bundle(level)),
    };

    Some(LevelReward {
        payout,
        boost_charges,
    })
}

/// The world whose band `level` falls in: the last one already open at that level.
///
/// Derived from [`World::is_unlocked_at`] rather than from a table of its own, so a
/// re-balance of the two unlock levels moves the reward bands with the worlds instead
/// of leaving them describing a map that no longer exists. Tested from the top down,
/// since "unlocked at" is cumulative and the End's answer is the specific one.
fn band_world(level: u32) -> World {
    if World::End.is_unlocked_at(level) {
        World::End
    } else if World::Nether.is_unlocked_at(level) {
        World::Nether
    } else {
        World::Overworld
    }
}

/// Which rung of the [fuel table](enchant_fuel) a level in `world` reads at.
///
/// A **linear walk of the world's own band**, not a fifty-armed table: the levels a
/// world owns run from its unlock level (exclusive — that level pays the world itself)
/// to the next world's, and they are spread over [`BANDS_PER_WORLD`] rungs by integer
/// division. That yields 2–6 → 1, 7–10 → 2, 11–14 → 3 for the Overworld and 16–20 → 4,
/// 21–25 → 5, 26–29 → 6 for the Nether, which is the table `docs/MECHANICS.md` prints,
/// derived rather than transcribed.
///
/// Integer division can only ever reach `BANDS_PER_WORLD - 1`, since the last level of
/// a band of `n` is `n - 1`, so the rung never runs off the world's share. The two
/// early returns are not dead weight but the same answer to two different absences: a
/// world with no successor, and — should a re-balance ever put two unlock levels
/// adjacent — a band with no levels in it, which is a division by zero.
///
/// **Total, and truthful about the End.** That world owns no rung of the table: its
/// bundles ride the rare ramp instead, and [`ore_bundle`] returns before ever asking.
/// Its "first rung" is therefore the first one *past* the six the table defines, where
/// [`enchant_fuel`] answers [`None`] — which is precisely the statement "the End has no
/// fuel pair", rather than an arbitrary index picked to make the function compile.
fn fuel_band(world: World, level: u32) -> u8 {
    let first_band = (1 + BANDS_PER_WORLD * world_index(world)) as u8;

    let Some(next) = next_world_unlock(world) else {
        return first_band;
    };
    let span = next.saturating_sub(world.unlock_level() + 1);
    if span == 0 {
        return first_band;
    }

    first_band + ((level - world.unlock_level() - 1) * BANDS_PER_WORLD / span) as u8
}

/// The level that opens the world *after* `world`, or [`None`] for the last one — which
/// is what bounds a world's band of levels from above.
///
/// Local to this module rather than a `World::next`: a successor on the dimension
/// ladder is a claim about the whole progression, and nothing else in the crate has
/// needed one yet. If a second caller appears, that is when it earns a place in
/// [`world`](crate::world).
fn next_world_unlock(world: World) -> Option<u32> {
    match world {
        World::Overworld => Some(World::Nether.unlock_level()),
        World::Nether => Some(World::End.unlock_level()),
        World::End => None,
    }
}

/// The world's position on the ladder, `Overworld = 0`: what the [fuel band](fuel_band)
/// offsets by so the Nether's levels read rungs 4–6 rather than 1–3 again.
fn world_index(world: World) -> u32 {
    match world {
        World::Overworld => 0,
        World::Nether => 1,
        World::End => 2,
    }
}

/// `total` × `permille` / 1000, floored — the one share computation the bundle makes.
///
/// In `u64` for the reason [`split_rare`] is: the product overflows `u32` long before
/// either operand looks unreasonable, and the widening costs nothing.
fn share(total: u32, permille: u32) -> u32 {
    (u64::from(total) * u64::from(permille) / 1_000) as u32
}

/// The ore a non-world level pays, as the lines a UI renders in order.
///
/// Two shapes, and each is the one its materials force — the same two
/// [`enchant_cost`](crate::economy::enchant_cost) has, for the same reasons:
///
/// - **Three lines below the End**: the world's enchant material at 50 %, and the
///   level's [fuel pair](enchant_fuel) at 35 % and 15 %.
/// - **Two lines in the End**: that dimension holds one mine whose rare cell already
///   *is* its enchant material, so a third line would be a second line of Amethyst.
///   The budget splits between End Stone and Amethyst on the same
///   [ramp](split_rare) the End's prices climb — a quarter Amethyst at level 31, rising
///   to the richness dial's own ceiling at 50 — so the bundle keeps paying in the
///   proportion the player is spending.
///
/// A line whose share floors to zero is dropped rather than credited as `0`, which is
/// what [`mine_cost`](crate::economy) does with a price line for the same reason: a
/// grant of nothing is not a grant.
fn ore_bundle(level: u32) -> Vec<(Item, u32)> {
    let budget = LEVEL_REWARD_BASE * level;
    let world = band_world(level);
    let mut lines = Vec::new();

    if world == World::End {
        // The band's first level is step 0, so the ramp opens at its start rather than
        // one notch up, and its last is the last step, so it reaches the top exactly
        // once. End Stone comes from the mine that holds it rather than from a new
        // `World` method: the pairing of a world's filler with its rare cell is a fact
        // about that mine, and `MineKind` already answers it.
        let start = END_UNLOCK_LEVEL + 1;
        let (common, rare) = split_rare(
            budget,
            level - start,
            LEVEL_CAP - start,
            RECIPE_RAMP_START_PERMILLE,
        );
        push_line(&mut lines, MineKind::Amethyst.common_material(), common);
        push_line(&mut lines, world.enchant_material(), rare);
    } else {
        push_line(
            &mut lines,
            world.enchant_material(),
            share(budget, FUEL_PRINCIPAL_PERMILLE),
        );
        // A shortened fuel table leaves the principal standing alone rather than
        // panicking: this crate refuses, it does not crash, and a one-line bundle is a
        // survivable answer where an `unreachable!` in a save-loading path is not.
        if let Some((abundant, scarce)) = enchant_fuel(fuel_band(world, level)) {
            push_line(&mut lines, abundant, share(budget, FUEL_ABUNDANT_PERMILLE));
            push_line(&mut lines, scarce, share(budget, FUEL_SCARCE_PERMILLE));
        }
    }

    // Last, and on top of the budget rather than carved out of it, so a third level
    // reads as *better* instead of differently split. Unreachable at 15 and 30, which
    // are multiples of three but returned from the world arm long before here.
    if level.is_multiple_of(LEVEL_REWARD_EMERALD_EVERY) {
        push_line(
            &mut lines,
            Material::Emerald,
            share(budget, LEVEL_REWARD_EMERALD_PERMILLE),
        );
    }

    lines
}

/// Appends one raw line, unless it would grant nothing.
fn push_line(lines: &mut Vec<(Item, u32)>, material: Material, amount: u32) {
    if amount > 0 {
        lines.push((Item::Raw(material), amount));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunables::NETHER_UNLOCK_LEVEL;
    use std::collections::HashMap;

    /// The bundle a level pays, as `(material, amount)` — the denomination is asserted
    /// once, by `every_bundle_is_credited_in_raw_only`, so the other tests read as the
    /// design does.
    fn bundle(level: u32) -> Vec<(Material, u32)> {
        match reward_for_level(level) {
            Some(LevelReward {
                payout: Payout::Ore(lines),
                ..
            }) => lines
                .into_iter()
                .map(|(item, amount)| (item.material(), amount))
                .collect(),
            _ => unreachable!("level {level} pays a bundle of ore"),
        }
    }

    #[test]
    fn the_two_ends_of_the_ladder_reward_nothing() {
        assert_eq!(reward_for_level(0), None);
        assert_eq!(reward_for_level(1), None);
        assert_eq!(reward_for_level(LEVEL_CAP + 1), None);
        assert!(reward_for_level(FIRST_REWARDED_LEVEL).is_some());
        assert!(reward_for_level(LEVEL_CAP).is_some());
    }

    /// The "never nothing" half of the payout rule, made total: every level between the
    /// two ends yields a reward, so a caller never has to ask *why* a level was empty.
    #[test]
    fn every_level_between_the_ends_rewards_something() {
        for level in FIRST_REWARDED_LEVEL..=LEVEL_CAP {
            assert!(
                reward_for_level(level).is_some(),
                "level {level} rewards nothing"
            );
        }
    }

    #[test]
    fn the_two_world_levels_pay_a_world_and_no_ore() {
        for (level, world) in [
            (NETHER_UNLOCK_LEVEL, World::Nether),
            (END_UNLOCK_LEVEL, World::End),
        ] {
            assert_eq!(
                reward_for_level(level).map(|reward| reward.payout),
                Some(Payout::World(world))
            );
        }
    }

    /// The ordering inside `reward_for_level` doing its two jobs at once: 15 and 30 are
    /// multiples of five *and* of three, and the charge must survive the early return
    /// while the Emerald must not. If this fails, the world arm has moved.
    #[test]
    fn a_world_level_keeps_its_charge_and_loses_its_emerald() {
        for level in [NETHER_UNLOCK_LEVEL, END_UNLOCK_LEVEL] {
            let Some(reward) = reward_for_level(level) else {
                unreachable!("level {level} opens a world")
            };
            assert_eq!(reward.boost_charges, 1, "level {level} lost its charge");
            assert!(level.is_multiple_of(LEVEL_REWARD_EMERALD_EVERY));
            assert!(matches!(reward.payout, Payout::World(_)));
        }
    }

    #[test]
    fn boost_charges_land_ten_times_in_a_run() {
        let total: u32 = (FIRST_REWARDED_LEVEL..=LEVEL_CAP)
            .filter_map(reward_for_level)
            .map(|reward| reward.boost_charges)
            .sum();
        assert_eq!(total, 10);
    }

    /// The three levels the UI spec draws in `organization/UI-EN.md` §5.7.5, one per
    /// Overworld band edge and one mid-Nether. They also pin the rounding decision: at
    /// level 23 the three lines come to 229 of a 230 budget, because each share is
    /// floored on its own rather than the principal taking a remainder.
    #[test]
    fn a_bundle_quotes_its_levels_fuel_band() {
        assert_eq!(
            bundle(2),
            [
                (Material::Lapis, 10),
                (Material::Stone, 7),
                (Material::Coal, 3)
            ]
        );
        assert_eq!(
            bundle(13),
            [
                (Material::Lapis, 65),
                (Material::Gold, 45),
                (Material::Diamond, 19)
            ]
        );
        assert_eq!(
            bundle(23),
            [
                (Material::Quartz, 115),
                (Material::AncientDebris, 80),
                (Material::Obsidian, 34)
            ]
        );
    }

    /// The band edges, walked level by level rather than sampled: this is the test that
    /// fails if `fuel_band`'s integer division is off by one anywhere, and it is stated
    /// over the materials rather than over the rung number so it reads as the table in
    /// `docs/MECHANICS.md` does.
    #[test]
    fn the_band_boundaries_fall_where_the_table_says() {
        let expected = |level: u32| match level {
            2..=6 => (Material::Lapis, Material::Stone, Material::Coal),
            7..=10 => (Material::Lapis, Material::Iron, Material::Gold),
            11..=14 => (Material::Lapis, Material::Gold, Material::Diamond),
            16..=20 => (
                Material::Quartz,
                Material::Netherrack,
                Material::AncientDebris,
            ),
            21..=25 => (
                Material::Quartz,
                Material::AncientDebris,
                Material::Obsidian,
            ),
            26..=29 => (
                Material::Quartz,
                Material::Obsidian,
                Material::CryingObsidian,
            ),
            _ => unreachable!("level {level} is a world level"),
        };

        for level in FIRST_REWARDED_LEVEL..END_UNLOCK_LEVEL {
            if level == NETHER_UNLOCK_LEVEL {
                continue;
            }
            let lines = bundle(level);
            let (principal, abundant, scarce) = expected(level);
            assert_eq!(lines[0].0, principal, "level {level} principal");
            assert_eq!(lines[1].0, abundant, "level {level} abundant");
            assert_eq!(lines[2].0, scarce, "level {level} scarce");
        }
    }

    /// The band walk answers for **every** world, the End included, and its answer
    /// there is the first rung past the table — where [`enchant_fuel`] says [`None`],
    /// which is the honest way to say "this world has no fuel pair". Pinned so the arm
    /// is not deleted as dead code by a reader who has not seen why it is right.
    #[test]
    fn the_fuel_band_answers_for_every_world() {
        assert_eq!(fuel_band(World::Overworld, FIRST_REWARDED_LEVEL), 1);
        assert_eq!(fuel_band(World::Nether, END_UNLOCK_LEVEL - 1), 6);

        let end_band = fuel_band(World::End, LEVEL_CAP);
        assert_eq!(end_band, 7);
        assert_eq!(enchant_fuel(end_band), None);
    }

    /// The End is two lines on the rare ramp: a quarter Amethyst at its first level,
    /// the richness dial's own ceiling of 91 % at its last. Unlike the three-line
    /// bundles these **do** add back to the budget exactly, because `split_rare` takes
    /// the common part as a remainder.
    #[test]
    fn the_end_pays_two_lines_on_the_rare_ramp() {
        assert_eq!(
            bundle(31),
            [(Material::Endstone, 233), (Material::Amethyst, 77)]
        );
        assert_eq!(
            bundle(50),
            [(Material::Endstone, 45), (Material::Amethyst, 455)]
        );

        for level in (END_UNLOCK_LEVEL + 1)..=LEVEL_CAP {
            let paid: u32 = bundle(level)
                .iter()
                .filter(|(material, _)| *material != Material::Emerald)
                .map(|(_, amount)| amount)
                .sum();
            assert_eq!(paid, LEVEL_REWARD_BASE * level, "level {level}");
        }
    }

    /// Emerald is the last line, worth a quarter of the budget *on top of* it — so a
    /// third level pays more than its neighbours rather than the same total rearranged.
    #[test]
    fn emerald_rides_every_third_level_and_adds_to_the_budget() {
        assert_eq!(
            bundle(18),
            [
                (Material::Quartz, 90),
                (Material::Netherrack, 63),
                (Material::AncientDebris, 27),
                (Material::Emerald, 45),
            ]
        );

        for level in FIRST_REWARDED_LEVEL..=LEVEL_CAP {
            if World::unlocked_by_reaching(level).is_some() {
                continue;
            }
            let has_emerald = bundle(level)
                .iter()
                .any(|(material, _)| *material == Material::Emerald);
            assert_eq!(
                has_emerald,
                level.is_multiple_of(LEVEL_REWARD_EMERALD_EVERY),
                "level {level}"
            );
        }
    }

    /// The denomination decision, asserted where it can be broken. Compressed units are
    /// the player's to mint (`docs/DECISIONS.md`); a reward that handed them over
    /// ready-made would do that step for them.
    #[test]
    fn every_bundle_is_credited_in_raw_only() {
        for level in FIRST_REWARDED_LEVEL..=LEVEL_CAP {
            let Some(LevelReward {
                payout: Payout::Ore(lines),
                ..
            }) = reward_for_level(level)
            else {
                continue;
            };
            for (item, _) in lines {
                assert!(
                    matches!(item, Item::Raw(_)),
                    "level {level} grants {item:?}"
                );
            }
        }
    }

    /// The mirror of the invariant [`Cost`](crate::economy::Cost) holds by construction
    /// — one line per material. A bundle that named a material twice would render as
    /// two grants of the same ore, and would mean the principal had collided with a
    /// fuel ore.
    #[test]
    fn no_bundle_ever_names_a_material_twice() {
        for level in FIRST_REWARDED_LEVEL..=LEVEL_CAP {
            if World::unlocked_by_reaching(level).is_some() {
                continue;
            }
            let lines = bundle(level);
            let mut seen: Vec<Material> = lines.iter().map(|(material, _)| *material).collect();
            seen.sort();
            let count = seen.len();
            seen.dedup();
            assert_eq!(seen.len(), count, "level {level} repeats a material");
        }
    }

    /// **The ledger.** Every bundle of a full run, accumulated per material and checked
    /// against `organization/PRICES-FR.md` D-13 — the totals the balance pass was
    /// argued from, and the reason the rewards are believed to be ~3 % of what a run
    /// must buy rather than an income.
    ///
    /// If this fails, the question is not "what are the new numbers?" but "what did we
    /// just change about the reward model?" — the same standing this crate gives its
    /// golden RNG vector.
    #[test]
    fn a_full_run_pays_what_the_ledger_says() {
        let mut totals: HashMap<Material, u32> = HashMap::new();
        for level in FIRST_REWARDED_LEVEL..=LEVEL_CAP {
            if World::unlocked_by_reaching(level).is_some() {
                continue;
            }
            for (material, amount) in bundle(level) {
                *totals.entry(material).or_default() += amount;
            }
        }

        for (material, expected) in [
            (Material::Amethyst, 4_916),
            (Material::Endstone, 3_184),
            (Material::Quartz, 1_575),
            (Material::Emerald, 904),
            (Material::Obsidian, 555),
            (Material::AncientDebris, 535),
            (Material::Lapis, 520),
            (Material::Netherrack, 314),
            (Material::Gold, 224),
            (Material::CryingObsidian, 164),
            (Material::Iron, 118),
            (Material::Diamond, 74),
            (Material::Stone, 69),
            (Material::Coal, 29),
        ] {
            assert_eq!(
                totals.get(&material).copied().unwrap_or_default(),
                expected,
                "{material:?}"
            );
        }

        // Redstone is never granted: the boost charges are what it would have bought.
        assert_eq!(totals.get(&Material::Redstone), None);
        assert_eq!(totals.values().sum::<u32>(), 13_181);
    }
}
