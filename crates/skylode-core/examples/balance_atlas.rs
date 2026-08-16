//! Generates `docs/BALANCE.md` — every price in the game, read out of the code.
//!
//! ```sh
//! cargo run -p skylode-core --example balance_atlas > docs/BALANCE.md
//! ```
//!
//! # Why this is a program and not a document
//!
//! There was a hand-written version of this atlas, and its fate is the argument. It
//! opened by promising that *"every number here is computed, never estimated"*, it
//! carried a `file:line` source for each claim, and it was right on the day it was
//! written. Phase 10 then replaced a single `COST_GROWTH` of 1.15 with four
//! per-track slopes and moved `COST_BASE` from 10 to 100 — and the document went on
//! stating the old ones, in a directory nobody published, for as long as it existed.
//! Nothing was careless about that. A table of derived values has no way to notice
//! that what it derives from has moved.
//!
//! So the atlas is not maintained here; it is *re-derived*. Every figure below comes
//! from calling the same [`economy`](skylode_core::economy) and
//! [`prestige`](skylode_core::prestige) functions the game charges the player with,
//! which makes a wrong number in `docs/BALANCE.md` impossible in a way that
//! proofreading cannot achieve: the only way to get one is to have a wrong number in
//! the game.
//!
//! # What it deliberately does not print
//!
//! Only *prices* — the outputs of the cost functions. The tables that describe
//! structure rather than balance (which block a mine draws, what a dense block drops,
//! the size ladder's dimensions) stay in `docs/MECHANICS.md`, because they are settled
//! design rather than dials, and re-deriving them would move prose out of the document
//! that argues it into one that cannot.
//!
//! Two consequences worth stating. The Efficiency and enchant ladders are walked
//! rung by rung rather than closed-form, because the curve's *reset* points — the
//! Netherite enhancement restarting at step 0, the enchant ramp shifting material —
//! live inside the cost functions and not in the constants. And every price is quoted
//! in the denominations the till actually demands, via
//! [`CostLine`](skylode_core::economy::CostLine), so this document cannot drift from
//! the Upgrades screen either.

use skylode_core::economy::{
    Cost, boost_cost, enchant_cost, mine_richness_cost, mine_size_cost, pickaxe_efficiency_cost,
    pickaxe_tier_cost,
};
use skylode_core::enchant::EnchantType;
use skylode_core::mine_kind::MineKind;
use skylode_core::pickaxe::PickaxeTier;
use skylode_core::prestige;
use skylode_core::tunables;
use skylode_core::world::World;

/// The six tiers, in ladder order.
///
/// Written out rather than read from a constant: `PickaxeTier::ALL_TIERS` is private
/// to `pickaxe`, and widening a type's API so an example can enumerate it would be
/// the tail wagging the dog. The cost of the copy is bounded by the fact that a
/// seventh tier is a design change, not a tuning one.
const TIERS: [PickaxeTier; 6] = [
    PickaxeTier::Wooden,
    PickaxeTier::Stone,
    PickaxeTier::Iron,
    PickaxeTier::Gold,
    PickaxeTier::Diamond,
    PickaxeTier::Netherite,
];

/// The three dimensions, in unlock order. Private in `world` for the same reason.
const WORLDS: [World; 3] = [World::Overworld, World::Nether, World::End];

/// The five triggered specials plus Haste — every enchant the world cap gates.
///
/// `Fortune` is excluded and printed on its own: it is capped by the world like these
/// five, but it is the one enchant priced in a single material, so it would be the
/// only row of its table with one column.
const SPECIALS: [EnchantType; 5] = [
    EnchantType::Explosive,
    EnchantType::Jackhammer,
    EnchantType::Nuke,
    EnchantType::Excavator,
    EnchantType::Haste,
];

/// The nine purchasable steps of a mine track: level 0 -> 1, up to level 8 -> 9.
///
/// `MAX_SIZE_LEVEL` and `MAX_RICHNESS_LEVEL` are both `pub(crate)`, and both are ten
/// rungs, so nine purchases separate the ends. If a ladder ever grows, this constant
/// is the one line to move and the totals below follow.
const MINE_TRACK_STEPS: u32 = 9;

fn main() {
    header();
    curve_parameters();
    tier_jumps();
    efficiency();
    netherite_enhancement();
    enchants();
    fortune();
    mine_track("Mine size", mine_size_cost);
    mine_track("Mine richness", mine_richness_cost);
    boost();
    prestige_ladder();
    footer();
}

// --- Formatting -------------------------------------------------------------------

/// One price, in the denominations it is owed in: `4C+42 Diamond`, `50 Redstone`.
///
/// The `C` suffix is the Compressed unit, worth [`RAW_PER_COMPRESSED`] raw. A
/// multi-material price joins its lines with ` + `, in the order the `Cost` quotes
/// them — which is the order the Upgrades screen renders, so a reader can match a row
/// here against a row on screen without translating.
///
/// [`RAW_PER_COMPRESSED`]: skylode_core::tunables::RAW_PER_COMPRESSED
fn quote(cost: &Cost) -> String {
    let parts: Vec<String> = cost
        .lines()
        .iter()
        .map(|line| {
            let name = line.material.name();
            match (line.compressed, line.raw) {
                (0, raw) => format!("{raw} {name}"),
                (compressed, 0) => format!("{compressed}C {name}"),
                (compressed, raw) => format!("{compressed}C+{raw} {name}"),
            }
        })
        .collect();
    parts.join(" + ")
}

/// A price as one raw number, for the totals a reader wants to compare across tracks.
fn raw_total(cost: &Cost) -> u32 {
    cost.lines()
        .iter()
        .map(|line| line.compressed * tunables::RAW_PER_COMPRESSED + line.raw)
        .sum()
}

/// `12 345` — the thousands separator the game itself uses by default.
fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

// --- Sections ---------------------------------------------------------------------

fn header() {
    println!("# Skylode - Balance");
    println!();
    println!("Every price in the game, in the denominations the till demands them in.");
    println!(
        "`1C` = one Compressed unit = {} raw.",
        tunables::RAW_PER_COMPRESSED
    );
    println!();
    println!("> **This file is generated. Do not edit it.**");
    println!(">");
    println!("> ```sh");
    println!("> cargo run -p skylode-core --example balance_atlas > docs/BALANCE.md");
    println!("> ```");
    println!(">");
    println!("> The figures come from calling the same `economy` and `prestige` functions");
    println!("> the game charges the player with, so they cannot disagree with the game.");
    println!("> A hand-kept predecessor of this table went stale the day phase 10 replaced");
    println!("> one cost slope with four; this one cannot. The *rules* these prices");
    println!("> implement live in [MECHANICS.md](MECHANICS.md), and the *reasons* behind");
    println!("> them in [DECISIONS.md](DECISIONS.md).");
    println!();
}

fn curve_parameters() {
    println!("## The parameters every price is generated from");
    println!();
    println!("Each track is a curve `cost(n) = base * growth^n`, so a few numbers");
    println!("generate the several hundred below. Changing one moves a whole column.");
    println!();
    println!("| Parameter | Value | Track it shapes |");
    println!("| --- | --- | --- |");
    println!(
        "| `COST_BASE` | {} | every track but enchants |",
        tunables::COST_BASE
    );
    println!(
        "| `SIZE_COST_GROWTH` | {} | mine size |",
        tunables::SIZE_COST_GROWTH
    );
    println!(
        "| `RICHNESS_COST_GROWTH` | {} | mine richness |",
        tunables::RICHNESS_COST_GROWTH
    );
    println!(
        "| `UPGRADE_COST_GROWTH` | {} | tier jumps and Efficiency |",
        tunables::UPGRADE_COST_GROWTH
    );
    println!(
        "| `NETHERITE_ENHANCEMENT_COST_GROWTH` | {} | Efficiency 6..=15 |",
        tunables::NETHERITE_ENHANCEMENT_COST_GROWTH
    );
    println!(
        "| `ENCHANT_COST_BASE` | {} | enchants and Fortune |",
        grouped(tunables::ENCHANT_COST_BASE)
    );
    println!(
        "| `ENCHANT_COST_GROWTH` | {} | enchants and Fortune |",
        tunables::ENCHANT_COST_GROWTH
    );
    println!(
        "| `BOOST_COST` | {} | one boost charge |",
        tunables::BOOST_COST
    );
    println!(
        "| `AMETHYST_PER_CLIMB` | {} | prestige, measured not chosen |",
        grouped(tunables::AMETHYST_PER_CLIMB)
    );
    println!(
        "| `PRESTIGE_SURCHARGE_BASE` | {} | prestige |",
        grouped(tunables::PRESTIGE_SURCHARGE_BASE)
    );
    println!(
        "| `PRESTIGE_SURCHARGE_PER_RANK_PERMILLE` | {} | prestige |",
        tunables::PRESTIGE_SURCHARGE_PER_RANK_PERMILLE
    );
    println!(
        "| `PRESTIGE_MULT_PER_RANK_PERMILLE` | {} | prestige |",
        tunables::PRESTIGE_MULT_PER_RANK_PERMILLE
    );
    println!();
}

fn tier_jumps() {
    println!("## Pickaxe: tier jumps");
    println!();
    println!("Paid in the tier being **left**, at that tier's step on the curve —");
    println!("leaving Gold costs Gold. The jump is the last thing a tier is for.");
    println!();
    println!("| Leaving | Price | Raw total |");
    println!("| --- | --- | --- |");
    let mut total = 0;
    for tier in TIERS {
        if tier.next().is_none() {
            continue;
        }
        let cost = pickaxe_tier_cost(tier);
        total += raw_total(&cost);
        println!(
            "| {} | {} | {} |",
            tier.name(),
            quote(&cost),
            grouped(raw_total(&cost))
        );
    }
    println!("| **Total** | | **{}** |", grouped(total));
    println!();
}

fn efficiency() {
    println!("## Pickaxe: Efficiency, per tier");
    println!();
    println!("Levels 1..=5 on every tier, priced on the shared `UPGRADE_COST_GROWTH`");
    println!("slope and paid in the current tier's material. Netherite's 6..=15 is a");
    println!("separate track and has its own section.");
    println!();
    print!("| Tier |");
    for level in 1..=5 {
        print!(" {level} |");
    }
    println!(" Total |");
    print!("| --- |");
    for _ in 1..=5 {
        print!(" --- |");
    }
    println!(" --- |");
    for tier in TIERS {
        print!("| {} |", tier.name());
        let mut total = 0;
        for current in 0..5u8 {
            let cost = pickaxe_efficiency_cost(tier, current);
            total += raw_total(&cost);
            print!(" {} |", quote(&cost));
        }
        println!(" **{}** |", grouped(total));
    }
    println!();
}

fn netherite_enhancement() {
    println!("## Pickaxe: the Netherite enhancement, Efficiency 6..=15");
    println!();
    println!("Ten rungs on a slope of its own, paid in Obsidian and Crying Obsidian");
    println!("on a sliding mix. It restarts the curve at step 0 rather than");
    println!("continuing from 5, so the enhancement is priced as its own ladder.");
    println!();
    println!("| Efficiency | Price | Raw total |");
    println!("| --- | --- | --- |");
    let mut total = 0;
    for current in 5..15u8 {
        let cost = pickaxe_efficiency_cost(PickaxeTier::Netherite, current);
        total += raw_total(&cost);
        println!(
            "| {} | {} | {} |",
            current + 1,
            quote(&cost),
            grouped(raw_total(&cost))
        );
    }
    println!("| **Total** | | **{}** |", grouped(total));
    println!();
}

fn enchants() {
    println!("## Enchants: the five specials");
    println!();
    let caps: Vec<String> = WORLDS
        .iter()
        .map(|world| format!("{} at the {}", world.enchant_cap(), world.name()))
        .collect();
    println!("One shared ceiling per world gates how far any of them may be taken:");
    println!("{}.", caps.join(", "));
    println!();
    println!("All five are priced identically at a given level, so one table serves");
    println!("them; the material shifts with the world whose cap the level sits under.");
    println!();
    println!("| Level | Price | Raw total |");
    println!("| --- | --- | --- |");
    let cap = WORLDS
        .iter()
        .map(|world| world.enchant_cap())
        .max()
        .unwrap_or(0);
    let mut total = 0;
    for current in 0..cap {
        // Every special prices the same, so the first one that answers stands for all
        // five. The loop over `SPECIALS` is here to *prove* that rather than assume
        // it: if one of them ever diverges, this table says so out loud instead of
        // quietly printing one enchant's price under a heading that claims five.
        let mut quoted: Option<(String, u32)> = None;
        for kind in SPECIALS {
            if let Some(cost) = enchant_cost(kind, current) {
                let text = quote(&cost);
                match &quoted {
                    None => quoted = Some((text, raw_total(&cost))),
                    Some((first, raw)) if *first != text => {
                        quoted = Some((
                            format!("{first} — **{} differs: {text}**", kind.name()),
                            *raw,
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
        if let Some((text, raw)) = quoted {
            total += raw;
            println!("| {} | {} | {} |", current + 1, text, grouped(raw));
        }
    }
    println!("| **Total per enchant** | | **{}** |", grouped(total));
    println!();
}

fn fortune() {
    println!("## Fortune");
    println!();
    println!("Capped by the world like the specials, but priced in a single material.");
    println!();
    println!("| Level | Price | Raw total |");
    println!("| --- | --- | --- |");
    // The End's cap is the ladder's full height; `max_level` ignores the tier for
    // every enchant but Efficiency, so which tier is passed here cannot matter.
    let cap = EnchantType::Fortune.max_level(PickaxeTier::Netherite, World::End);
    let mut total = 0;
    for current in 0..cap {
        if let Some(cost) = enchant_cost(EnchantType::Fortune, current) {
            total += raw_total(&cost);
            println!(
                "| {} | {} | {} |",
                current + 1,
                quote(&cost),
                grouped(raw_total(&cost))
            );
        }
    }
    println!("| **Total** | | **{}** |", grouped(total));
    println!();
}

fn mine_track(title: &str, price: fn(MineKind, u32) -> Cost) {
    println!("## {title}, per mine");
    println!();
    println!("Nine purchases take a mine from level 0 to level 9. Paid in the mine's");
    println!("own material — on the three two-material mines, in both, on a mix that");
    println!("slides toward the rare one as the track climbs.");
    println!();
    println!("| Mine | First step | Last step | Track total |");
    println!("| --- | --- | --- | --- |");
    for kind in MineKind::ALL {
        let first = price(kind, 0);
        let last = price(kind, MINE_TRACK_STEPS - 1);
        let total: u32 = (0..MINE_TRACK_STEPS)
            .map(|n| raw_total(&price(kind, n)))
            .sum();
        println!(
            "| {} | {} | {} | {} |",
            kind.name(),
            quote(&first),
            quote(&last),
            grouped(total)
        );
    }
    println!();
}

fn boost() {
    println!("## Boost");
    println!();
    let cost = boost_cost();
    println!(
        "One charge costs **{}**, flat and repeatable with no ceiling. It runs for",
        quote(&cost)
    );
    println!(
        "{} seconds at x{}, and firing onto a running boost stacks the duration.",
        tunables::BOOST_DURATION_TICKS as u64 / tunables::TICKS_PER_SECOND,
        tunables::BOOST_MULTIPLIER
    );
    println!();
    println!("The price is `3 * COST_BASE` rather than a literal, so a rebalance of");
    println!("the curve moves it instead of leaving it behind — which is the exact");
    println!("mistake that produced the original imbalance.");
    println!();
}

fn prestige_ladder() {
    println!("## Prestige");
    println!();
    println!("The price is a **sum**, not a step on a geometric curve: one climb's");
    println!("Amethyst income plus a surcharge growing in a straight line. The");
    println!("multiplier applies to ore yield and XP, and deliberately not to speed.");
    println!();
    println!("| Rank reached | Price | Multiplier |");
    println!("| --- | --- | --- |");
    for rank in 0..12u32 {
        let cost = prestige::cost(rank);
        let permille = prestige::multiplier_permille(rank + 1);
        println!(
            "| {} | {} | x{}.{:03} |",
            rank + 1,
            quote(&cost),
            permille / 1000,
            permille % 1000
        );
    }
    println!();
}

fn footer() {
    println!("---");
    println!();
    println!("Generated by `crates/skylode-core/examples/balance_atlas.rs`.");
}
