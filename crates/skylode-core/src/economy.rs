//! What upgrades cost, and how those costs are paid.
//!
//! The four upgrade paths in the core — [`Pickaxe::upgrade`], the enchant ladder,
//! and a mine's two tracks ([`upgrade_size_level`], [`set_richness_setting`]'s
//! ceiling) — are *free* today: they consult no inventory. This module is the
//! price tag and the till. It answers three questions, in three layers:
//!
//! 1. **How much?** [`cost_curve`] is the shared geometric curve every price is
//!    read off; a [`Cost`] is a concrete price, one [`CostLine`] per material.
//! 2. **In what?** The per-track functions (added alongside this) turn a mine, a
//!    tier, or an enchant into a [`Cost`] — the place the *"which material"*
//!    mapping lives.
//! 3. **Can they pay, and take it if so?** The transactional purchase path checks
//!    solvency, debits, and applies the upgrade, refusing as a no-op.
//!
//! ## Why a cost is a *list* of lines
//!
//! The obvious shape for a price is one material and one number. Two rules in
//! `docs/MECHANICS.md` forbid it. An enchant upgrade is "the world's enchant
//! material **plus a mix of raw ores from the earlier mines**", and the richness
//! track on the two-material mines is paid in "a mix that shifts as it climbs" —
//! mostly End Stone low down, increasingly Amethyst high up. Both are several
//! materials in one price, so a [`Cost`] is a `Vec` of [`CostLine`]s. A
//! single-material price ([`Cost::single`]) is just the one-line case, not a
//! different type.
//!
//! ## Why each line carries *two* denominations
//!
//! Costs are quoted, and must be paid, in both raw and Compressed — `6 Compressed
//! Iron + 50 Iron`, never the flat `650 Iron` the same value would make (see
//! [`Item`] and `docs/DECISIONS.md`). So a [`CostLine`] is not "how much of a
//! material" but "how much in each denomination of it", and the split is fixed by
//! [`RAW_PER_COMPRESSED`].
//!
//! [`Pickaxe::upgrade`]: crate::pickaxe::Pickaxe::upgrade
//! [`upgrade_size_level`]: crate::mine::Mine::upgrade_size_level
//! [`set_richness_setting`]: crate::mine::Mine::set_richness_setting

use crate::enchant::EnchantType;
use crate::error::CoreError;
use crate::inventory::Inventory;
use crate::material::{Item, Material};
use crate::mine::Mine;
use crate::mine::{MAX_RICHNESS_LEVEL, MAX_SIZE_LEVEL};
use crate::mine_kind::MineKind;
use crate::pickaxe::{Pickaxe, PickaxeTier};
use crate::rng::Rng;
use crate::tunables::{
    BOOST_COST, COST_BASE, ENCHANT_COST_BASE, ENCHANT_COST_GROWTH, RAW_PER_COMPRESSED,
    RICHNESS_COST_GROWTH, SIZE_COST_GROWTH, UPGRADE_COST_GROWTH,
};
use crate::world::World;

/// The total raw cost of the `n`-th step on the geometric curve
/// `base * growth^n`, rounded to a whole number of raw items.
///
/// `n` is the **0-indexed step being bought**: the first upgrade of a track is
/// `cost_curve(base, growth, 0)` = `base`, and each further step multiplies by
/// `growth`. Which state maps to `n` is each track's own business (a pickaxe's
/// Efficiency level, a mine's size level, …); this function only knows the curve.
///
/// **The curve is passed in, not read from a constant**, because a slope is only
/// meaningful against the production growth of the track it prices. Size takes a mine
/// from 9 cells to 200 over nine steps; Netherite's Efficiency has fifteen steps and
/// multiplies nothing. One slope for both makes the shorter track free or the longer
/// one unaffordable — at the size track's slope, the last Efficiency level costs 643
/// full grids. Callers reach for the named per-track helper below rather than this
/// function, so no call site has to remember which pair of constants it wanted.
///
/// **`f64` and `round`, not integer arithmetic.** `growth` is `1.15`, which has
/// no exact integer form, so the curve is evaluated in floating point and rounded
/// once at the end. Unlike the mine generator, this is *safe* to do in `f64`: a
/// price draws no RNG and is **never stored in the save** — it is recomputed from
/// the player's state every time the Upgrades screen asks — so a value that
/// differed by one item between two machines would change nothing a save carries
/// and no sequence a replay must reproduce. The determinism contract the core
/// keeps is about draws, and there is no draw here.
///
/// **The cast saturates, and that is the overflow story.** Rust's `as` from `f64`
/// to `u32` clamps: a curve that runs past `u32::MAX` (around `n = 143` at these
/// constants, far beyond any real upgrade track) overflows the `f64` to `+inf`,
/// which the cast pins to `u32::MAX` rather than wrapping to a *cheap* price. So
/// the function is total without a bounds check, and the only thing lost past the
/// clamp is strict monotonicity, tested only over the range real tracks live in.
///
/// **`powf(f64::from(n))`, not `powi(n as i32)`.** The exponent must be widened
/// through `f64`, not narrowed through `i32`: a `u32` fits exactly in an `f64`
/// mantissa, but `n as i32` *wraps a large `n` to a negative exponent* — turning
/// `cost_curve(u32::MAX)` into `base * growth⁻¹`, the cheap price the saturation
/// exists to rule out.
pub fn cost_curve(base: u32, growth: f64, n: u32) -> u32 {
    (f64::from(base) * growth.powf(f64::from(n))).round() as u32
}

/// The price of the `n`-th step of a mine's **size** track — the steepest curve in
/// the game, because size is the track that multiplies production.
fn size_curve(n: u32) -> u32 {
    cost_curve(COST_BASE, SIZE_COST_GROWTH, n)
}

/// The price of the `n`-th step of a mine's **richness** track, on a gentler slope
/// than [`size_curve`]: richness multiplies a same-material mine's yield by 4.6 across
/// its rungs where size multiplies cell count by 22.
fn richness_curve(n: u32) -> u32 {
    cost_curve(COST_BASE, RICHNESS_COST_GROWTH, n)
}

/// The price of the `n`-th step of either **pickaxe** track — Efficiency within a
/// tier, or the tier jump. Shared, because they are two halves of one ladder, and
/// held below [`size_curve`]'s slope because Netherite's Efficiency runs fifteen steps.
fn upgrade_curve(n: u32) -> u32 {
    cost_curve(COST_BASE, UPGRADE_COST_GROWTH, n)
}

/// The price of the `n`-th level of an **enchant**: the one track with its own base,
/// ten times the others, paired with the gentlest slope.
///
/// That pairing is what spreads an enchant's cost across the three worlds. An
/// enchant's ten levels are split 3 / 3 / 4 between them, and since step zero costs
/// the base whatever the slope, a high base with a flat slope is the only shape that
/// makes the Overworld's three levels a real expense — 11.5 % of the ladder rather
/// than 2.3 %. See [`ENCHANT_COST_BASE`].
fn enchant_curve(n: u32) -> u32 {
    cost_curve(ENCHANT_COST_BASE, ENCHANT_COST_GROWTH, n)
}

/// Where the **recipe** ramps start: a quarter of the total, in permille.
///
/// Applies to the two prices that buy a *recipe* rather than a mine — Netherite's
/// Efficiency `6..=15` and the End's enchants. Both open on a rare-material minority,
/// because the player meets them holding a mine still tuned toward its common cell.
/// The End's [level-up bundles](crate::reward) open here too, so that what the player
/// receives in that dimension keeps the proportion of what they spend in it.
///
/// The **mine** tracks start at zero instead, and the difference is not a tuning
/// choice: a mine's first rung has to be payable out of what an *un-enriched* mine
/// produces, and an un-enriched mine has barely any of its rare cell. A recipe has no
/// such constraint — the player brings the materials to it.
pub(crate) const RECIPE_RAMP_START_PERMILLE: u32 = 250;

/// Where every ramp ends, in permille: the same 91 % the richness dial reaches at its
/// own ceiling.
///
/// **Matching `value_weight(MAX_RICHNESS_LEVEL)` is the whole point**, not a
/// coincidence to be tidied away. It is what makes the dial's top rung the setting the
/// last step of a track actually wants, so the optimum ratio the player farms toward
/// *moves* up the dial as they climb instead of sitting at one rung. Pinned at a fixed
/// share — as Netherite's Efficiency once was, at a flat 3:1 — the optimum parks at
/// dial 1.7 of 9, and the seven rungs above it can only overshoot the recipe, which
/// leaves most of that mine's own richness track not worth buying.
const RARE_RAMP_END_PERMILLE: u32 = 910;

/// The rare material's share of a composite price, in permille: a linear ramp from
/// `start` to [`RARE_RAMP_END_PERMILLE`] across `span` steps, read at `step`.
///
/// The single ramp behind **all four** of the game's composite prices — a mine's two
/// tracks on the three two-material mines, Netherite's Efficiency `6..=15`, and the
/// End's enchants. They differ in where they start ([`RECIPE_RAMP_START_PERMILLE`] for
/// the two recipes, zero for the two mine tracks) and in how many steps they run over;
/// that they climb *toward the dial's own ceiling* is one rule, stated once. Before
/// this they were three unrelated fractions that happened to share a module.
///
/// **Saturating rather than wrapping at the ends.** `step` is clamped to `span`, so a
/// caller that walks past the last rung — a save carrying a level the current table no
/// longer defines, which phase 9 must survive — reads the top of the ramp rather than
/// running off it. A `span` of zero yields `start`, since a ramp with no steps has
/// nowhere to climb.
///
/// Arithmetic in `u64`: the product can exceed `u32` on an absurd span even though
/// real spans are single digits, and the guard costs nothing.
fn rare_permille(step: u32, span: u32, start: u32) -> u32 {
    if span == 0 {
        return start;
    }
    let step = u64::from(step.min(span));
    let climb = u64::from(RARE_RAMP_END_PERMILLE.saturating_sub(start));
    start + (climb * step / u64::from(span)) as u32
}

/// Splits `total` into a `(common, rare)` pair, the rare part taking
/// [`rare_permille`] of it.
///
/// The common part is `total - rare` rather than its own multiplication, which is what
/// makes the split **exact**: the two parts always add back to the total, so moving a
/// price from one material to two never changes what it costs. A price that quietly
/// gained or lost an item every time its shape changed would be a rounding error the
/// player pays.
///
/// `pub(crate)` for [`reward`](crate::reward), whose End bundles ride the same ramp:
/// sharing the split is what keeps a reward the mirror of a price rather than a second
/// table drifting beside it.
pub(crate) fn split_rare(total: u32, step: u32, span: u32, start: u32) -> (u32, u32) {
    let rare = (u64::from(total) * u64::from(rare_permille(step, span, start)) / 1_000) as u32;
    (total - rare, rare)
}

/// A price in a mine's own material: one line on the nine same-material mines, and a
/// common-to-rare [mix](split_rare) on the three that hold two materials.
///
/// Shared by both mine tracks. Size and richness quote the same shape for the same
/// reason — a mine funds its growth out of what it produces, and on a two-material
/// mine it produces two things. A line is dropped when its part rounds to nothing,
/// which is why richness step 0 is a single common line.
fn mine_cost(kind: MineKind, total: u32, step: u32, span: u32) -> Cost {
    let (common, value) = (kind.common_material(), kind.value_material());
    if common == value {
        return Cost::single(common, total);
    }

    // Zero, not `RECIPE_RAMP_START_PERMILLE`: a mine's first rung must be payable out
    // of what the un-enriched mine already produces.
    let (common_part, rare_part) = split_rare(total, step, span, 0);
    let mut lines = Vec::new();
    if common_part > 0 {
        lines.push(CostLine::from_raw_total(common, common_part));
    }
    if rare_part > 0 {
        lines.push(CostLine::from_raw_total(value, rare_part));
    }
    Cost::new(lines)
}

/// A price in one material, split across the two denominations the player holds
/// it in: `compressed` Compressed units and `raw` loose items.
///
/// The split is not cosmetic — it is *what is owed in each denomination*, and the
/// strict payment rule means a player must hold exactly this shape (or compress to
/// reach it), never merely the equivalent raw value. See the module docs and
/// [`Item`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostLine {
    /// The material owed.
    pub material: Material,
    /// How many Compressed units of it are owed.
    pub compressed: u32,
    /// How many raw items of it are owed, always `< RAW_PER_COMPRESSED`.
    pub raw: u32,
}

impl CostLine {
    /// Splits a raw total into the two denominations: `total / 100` Compressed
    /// units and the `total % 100` remainder in raw items.
    ///
    /// This is the single place the `650 -> 6 Compressed + 50 raw` decomposition
    /// happens, so the invariant `compressed * RAW_PER_COMPRESSED + raw == total`
    /// holds by construction and `raw < RAW_PER_COMPRESSED` always. Quoting the
    /// large end in Compressed units is what keeps the price readable — a small
    /// composite reads better than one long number (`docs/MECHANICS.md`).
    pub fn from_raw_total(material: Material, total: u32) -> Self {
        Self {
            material,
            compressed: total / RAW_PER_COMPRESSED,
            raw: total % RAW_PER_COMPRESSED,
        }
    }

    /// The line's demands as `(Item, amount)` pairs, in the denomination each is
    /// owed in — the shape the till checks against and debits from the
    /// [`Inventory`].
    ///
    /// **Zero-amount denominations are dropped.** A line of `6 Compressed + 0 raw`
    /// owes only the Compressed units; yielding `(Raw, 0)` too would make the
    /// affordability message name a shortfall of nothing, and give the debit a
    /// no-op removal to run. So the caller iterates only what is actually due.
    pub fn requirements(&self) -> Vec<(Item, u32)> {
        let mut reqs = Vec::new();
        if self.compressed > 0 {
            reqs.push((Item::Compressed(self.material), self.compressed));
        }
        if self.raw > 0 {
            reqs.push((Item::Raw(self.material), self.raw));
        }
        reqs
    }
}

/// A complete price: one [`CostLine`] per material.
///
/// A `Vec`, because a price can span several materials (see the module docs). A
/// single-material price is the one-line case built by [`single`](Cost::single);
/// the multi-material ones (enchants, two-material richness) push more lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cost {
    lines: Vec<CostLine>,
}

impl Cost {
    /// A price made of the given lines, in the order they are quoted.
    ///
    /// The order is the caller's — the Upgrades screen renders the lines as
    /// written — so the mixed richness cost lists its common material before its
    /// rare one, and reads the way the design describes it.
    pub fn new(lines: Vec<CostLine>) -> Self {
        Self { lines }
    }

    /// A single-material price: `total` raw items of `material`, split into the
    /// two denominations by [`CostLine::from_raw_total`].
    ///
    /// The common case — every same-material track (pickaxe, size, and richness on
    /// the nine ore mines) prices this way. The multi-material tracks build their
    /// `Cost` from [`new`](Cost::new) instead.
    pub fn single(material: Material, total: u32) -> Self {
        Self::new(vec![CostLine::from_raw_total(material, total)])
    }

    /// Borrows the lines this price is made of, for a UI to render or the till to
    /// walk.
    pub fn lines(&self) -> &[CostLine] {
        &self.lines
    }
}

// --- Per-track costs (step 2) ---
//
// Each function turns a piece of run state into a `Cost`: the price the Upgrades
// screen renders, and the till (step 3) debits. They are **pure and take explicit
// state** rather than borrowing the pickaxe or mine, so a test can price a state
// the purchase path cannot yet produce, and the cost query stays separable from
// the mutation that spends it.
//
// The "which material" tables below are all **provisional** — their *shape* is
// settled, their *numbers* are phase-10 balance, exactly the status
// `EnchantType::proc_permille` carries. They stay local to `economy` rather than
// moving to `tunables` for the reason `mine`'s richness weights do: they are
// composition local to one module, read nowhere else.

/// The material a pickaxe upgrade at `tier` is paid in — provisional.
///
/// One material per tier, taken from the "pickaxe tier upgrades" row of the worlds
/// table in `docs/MECHANICS.md`. The mapping is chosen so the material funding a
/// tier is one the *previous* tier can already mine: Ancient Debris needs a
/// Diamond pickaxe, so it funds the jump to Netherite — matching the doc's
/// "Ancient Debris = Netherite tier upgrades" exactly. Total by construction, so
/// no pickaxe upgrade is ever unpriced.
fn pickaxe_material(tier: PickaxeTier) -> Material {
    match tier {
        PickaxeTier::Wooden => Material::Stone,
        PickaxeTier::Stone => Material::Coal,
        PickaxeTier::Iron => Material::Iron,
        PickaxeTier::Gold => Material::Gold,
        PickaxeTier::Diamond => Material::Diamond,
        PickaxeTier::Netherite => Material::AncientDebris,
    }
}

/// The tier's position on the ladder, `Wooden = 0 … Netherite = 5`: the step index
/// the tier-jump curve is read at, so reaching a later tier costs more.
fn tier_index(tier: PickaxeTier) -> u32 {
    match tier {
        PickaxeTier::Wooden => 0,
        PickaxeTier::Stone => 1,
        PickaxeTier::Iron => 2,
        PickaxeTier::Gold => 3,
        PickaxeTier::Diamond => 4,
        PickaxeTier::Netherite => 5,
    }
}

/// The Efficiency levels Netherite shares with every other tier: `1..=5`, the
/// standard cap. Netherite's climb goes on to 15, and those extra levels
/// (`6..=15`) are the **post-Netherite enhancement**, priced apart from the
/// ordinary tier upgrade below.
const NETHERITE_BASE_EFFICIENCY: u8 = 5;

/// The cost of the next **Efficiency** level on `tier`, given the level already
/// held — the first of the pickaxe's two separately-priced actions.
///
/// [`upgrade_curve`] at the level held, in the tier's material, so the price climbs
/// within a tier and **restarts** at the next: a tier jump resets Efficiency, and the
/// fresh climb is cheap *in count* but paid in a scarcer material (Stone → Coal →
/// Iron → …). The material is the cross-tier escalation, the curve the within-tier
/// one — together they keep every rung worth more than the last without one long
/// number. Provisional.
///
/// **Netherite is the exception, above [`NETHERITE_BASE_EFFICIENCY`].** Its
/// Efficiency `1..=5` is the ordinary tier upgrade in Ancient Debris, but `6..=15`
/// is the *post-Netherite enhancement*, which `docs/MECHANICS.md` pays in Obsidian
/// **and** Crying Obsidian both — the Obsidian mine's two materials. So past the
/// standard cap this returns a **two-material** cost, on the same
/// [ramp](rare_permille) the End's enchants and the two-material mine tracks use: a
/// quarter Crying at the first enhancement level, climbing to the dial's own ceiling
/// at the last.
///
/// **The climbing share is what gives that mine's dial a reason to move.** At the flat
/// 3:1 this once used, the ratio the recipe wanted never changed, so the optimum dial
/// setting sat at 1.7 of 9 for the whole enhancement and the seven rungs above it
/// could only overshoot it — a richness track the player was better off not buying.
/// Ramped, the optimum walks up the dial alongside the Efficiency level, and the
/// mine's last rung is exactly what its last level asks for.
///
/// It is the one pickaxe cost that is not a single material, which is why the split
/// lives here rather than in [`pickaxe_material`].
pub fn pickaxe_efficiency_cost(tier: PickaxeTier, current_level: u8) -> Cost {
    let total = upgrade_curve(u32::from(current_level));

    if tier == PickaxeTier::Netherite && current_level >= NETHERITE_BASE_EFFICIENCY {
        let span = u32::from(tier.efficiency_cap() - NETHERITE_BASE_EFFICIENCY - 1);
        let step = u32::from(current_level - NETHERITE_BASE_EFFICIENCY);
        let (obsidian, crying) = split_rare(total, step, span, RECIPE_RAMP_START_PERMILLE);

        let mut lines = Vec::new();
        for (material, amount) in [
            (Material::Obsidian, obsidian),
            (Material::CryingObsidian, crying),
        ] {
            if amount > 0 {
                lines.push(CostLine::from_raw_total(material, amount));
            }
        }
        return Cost::new(lines);
    }

    Cost::single(pickaxe_material(tier), total)
}

/// The cost of advancing **out of** `from` — the pickaxe's second priced action, kept
/// distinct from buying Efficiency (Enoal's call to price the two apart, mirroring the
/// two phases already inside [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe::upgrade)).
///
/// **The tier being left, not the one being reached**, and both halves of the price
/// come from it: [`upgrade_curve`] at *its* index, in *its* material. Leaving Gold
/// costs Gold. The jump is the last thing a tier is for, so it is priced as that
/// tier's final purchase rather than as a down payment on the next — the player spends
/// what they have been mining all along, not a material the mine they are about to
/// unlock has not given them yet.
///
/// Taking the material from one tier and the curve index from another, as this once
/// did, made the price describe two concepts at once and left no answer to "which tier
/// is this the price of?". Provisional in its numbers; the keying is not.
///
/// A corollary worth naming: [`Netherite`](PickaxeTier::Netherite) is never a source,
/// since there is nothing past it to reach — the mirror of the old shape, where
/// [`Wooden`](PickaxeTier::Wooden) was never a target.
pub fn pickaxe_tier_cost(from: PickaxeTier) -> Cost {
    Cost::single(pickaxe_material(from), upgrade_curve(tier_index(from)))
}

/// The material an enchant `kind` is bought with in `world`, or `None` for
/// [`Efficiency`](EnchantType::Efficiency) — which is a pickaxe upgrade, priced by
/// [`pickaxe_efficiency_cost`], not an enchant-shop purchase.
///
/// [`Fortune`](EnchantType::Fortune) is Emerald, its own currency, in every world; the
/// five specials are the world's [`enchant_material`](World::enchant_material),
/// climbing Lapis → Quartz → Amethyst as the cap climbs with the world.
fn enchant_material(kind: EnchantType, world: World) -> Option<Material> {
    match kind {
        EnchantType::Efficiency => None,
        EnchantType::Fortune => Some(Material::Emerald),
        EnchantType::Explosive
        | EnchantType::Jackhammer
        | EnchantType::Nuke
        | EnchantType::Excavator
        | EnchantType::Haste => Some(world.enchant_material()),
    }
}

/// The share of an enchant's price owed in its **principal** — the world's enchant
/// material — in permille.
///
/// **Not read by [`enchant_cost`], which takes it as the remainder** so that its three
/// lines add back to the quoted total exactly. What this constant adds is the number
/// *itself*, which the price never needed to name and [`reward`](crate::reward) does:
/// a bundle has no quoted total to reconcile against, so it floors all three shares
/// independently. Stated here rather than in `reward` so the 50 / 35 / 15 split lives
/// in one place, and so the assertion that they make a whole can be written at all.
pub(crate) const FUEL_PRINCIPAL_PERMILLE: u32 = 500;

/// The share of an enchant's price owed in its **abundant** fuel ore, in permille.
pub(crate) const FUEL_ABUNDANT_PERMILLE: u32 = 350;

/// The share owed in its **scarce** fuel ore, in permille — the smaller of the two, so
/// the pair reads as "a lot of this, some of that" rather than two equal demands.
pub(crate) const FUEL_SCARCE_PERMILLE: u32 = 150;

/// The pair of ores that fuels the `level`-th level of a special enchant: an abundant
/// one and a scarce one, both from the mines the player is working **now**.
///
/// **Keyed by the level, not by the world**, and that is the whole design. The level
/// *is* the progression scale: each rung slides the pair one notch up the ladder of
/// what the player can currently mine, so every ore serves once as the abundant half
/// and once as the scarce half before dropping out. Keying by the world instead would
/// name one ore for a whole dimension and leave the rest of it unasked-for.
///
/// It also makes the table **total without a bounds check**. The world cap is what
/// stops a player reaching level 7 outside the End, so a level always lands on the
/// band of the world that could sell it — and a level bought late is still fuelled by
/// the ores of its own rung, never by whatever the player happens to have unlocked
/// since. Level 1 costs Stone and Coal whether it is bought at the very start or in
/// the End.
///
/// `None` past the last entry: the End's four levels are priced by a different shape
/// (see [`enchant_cost`]), since that dimension holds a single mine whose rare cell is
/// already the enchant material.
///
/// **Two callers, one table.** [`reward`](crate::reward) reads it at the *band* a
/// mining level falls in, so a level-up pays out the materials an enchant of that rung
/// charges. Sharing the table rather than mirroring it is what makes a re-balance of
/// the fuel move price and reward in a single edit — see `docs/DECISIONS.md`.
pub(crate) fn enchant_fuel(level: u8) -> Option<(Material, Material)> {
    match level {
        1 => Some((Material::Stone, Material::Coal)),
        2 => Some((Material::Iron, Material::Gold)),
        3 => Some((Material::Gold, Material::Diamond)),
        4 => Some((Material::Netherrack, Material::AncientDebris)),
        5 => Some((Material::AncientDebris, Material::Obsidian)),
        6 => Some((Material::Obsidian, Material::CryingObsidian)),
        _ => None,
    }
}

/// How many levels the End's enchant ramp runs over: levels 7 through 10.
///
/// Derived as the gap between the last world's cap and the one before it, so a
/// re-balance of [`World::enchant_cap`] moves the ramp with it instead of leaving it
/// spanning a band that no longer exists.
const END_ENCHANT_SPAN: u32 = 3;

/// The cost of the next level of enchant `kind` in `world`, given the level held
/// — or `None` for Efficiency, which the pickaxe path prices instead.
///
/// Three shapes, and each is the shape its own materials force:
///
/// - **[`Fortune`](EnchantType::Fortune): one line of Emerald.** No fuel. Fortune is
///   the one enchant whose material is keyed to neither the world nor the tier, so
///   there is no "current tier's ore" for it to consume; a fuel line here was an
///   accident of pricing every enchant through one path.
/// - **The five specials, levels 1–6: three lines.** The world's enchant material,
///   plus the [pair of ores](enchant_fuel) of that level — 50 % / 35 % / 15 % of one
///   total. The fuel ores come from the mines the player is working *now*, which is
///   what makes a dimension's own mines matter while the player is in it.
/// - **The five specials, levels 7–10: two lines.** The End holds one mine, and its
///   rare cell *is* the enchant material, so a third line would be a second line of
///   Amethyst — which [`Cost`] forbids by construction. The total splits between End
///   Stone and Amethyst on the same [ramp](rare_permille) the recipes use, so the End
///   fuels its enchants out of the only mine it has.
///
/// **The lines share the total rather than adding to it.** A fuel line that was added
/// on top would make the quoted curve a lie about what the step costs, and it would
/// leave the four composite prices in the game disagreeing about what "a price" means.
/// Numbers provisional; the shapes are settled.
pub fn enchant_cost(kind: EnchantType, current_level: u8, world: World) -> Option<Cost> {
    let material = enchant_material(kind, world)?;
    let total = enchant_curve(u32::from(current_level));
    let level = current_level + 1;

    if kind == EnchantType::Fortune {
        return Some(Cost::single(material, total));
    }

    let Some((abundant, scarce)) = enchant_fuel(level) else {
        // The End: one mine, and its rare cell is already the enchant material.
        //
        // The band's *first* level is step 0, so the ramp opens at its start rather
        // than one notch up — and its last is step `END_ENCHANT_SPAN`, so it reaches
        // the top exactly once instead of saturating a level early.
        let step = u32::from(level).saturating_sub(u32::from(World::Nether.enchant_cap()) + 1);
        let (common, rare) = split_rare(total, step, END_ENCHANT_SPAN, RECIPE_RAMP_START_PERMILLE);
        // End Stone comes from the mine that holds it rather than from a new `World`
        // method: the pairing of a world's filler with its rare cell is a fact about
        // that mine, and `MineKind` already answers it.
        return Some(Cost::new(vec![
            CostLine::from_raw_total(MineKind::Amethyst.common_material(), common),
            CostLine::from_raw_total(material, rare),
        ]));
    };

    let abundant_part = (u64::from(total) * u64::from(FUEL_ABUNDANT_PERMILLE) / 1_000) as u32;
    let scarce_part = (u64::from(total) * u64::from(FUEL_SCARCE_PERMILLE) / 1_000) as u32;
    // The principal takes the remainder, so the three lines add back to the total
    // exactly — the same reason `split_rare` computes its common part that way. It is
    // `FUEL_PRINCIPAL_PERMILLE` of the total up to that rounding, and the constant
    // asserts as much; a price reconciles, a reward does not have to.
    let principal = total - abundant_part - scarce_part;

    let mut lines = Vec::new();
    for (material, amount) in [
        (material, principal),
        (abundant, abundant_part),
        (scarce, scarce_part),
    ] {
        if amount > 0 {
            lines.push(CostLine::from_raw_total(material, amount));
        }
    }
    Some(Cost::new(lines))
}

/// The cost of the next size level of a `kind` mine, given the level held.
///
/// [`size_curve`] in the mine's own material — every mine funds its own growth out of
/// what it produces. On the nine same-material mines that is a single line of
/// [`common_material`](MineKind::common_material); on the three two-material ones it is
/// the same common-to-rare mix richness quotes, because **a two-material mine produces
/// two things and "its own material" means both of them**.
///
/// Reading "its own ore" as the common cell alone is what left Crying Obsidian funding
/// nothing but the Efficiency climb, and made size the one track on those mines that
/// ignored half their output.
pub fn mine_size_cost(kind: MineKind, current_size_level: u32) -> Cost {
    mine_cost(
        kind,
        size_curve(current_size_level),
        current_size_level,
        MAX_SIZE_LEVEL,
    )
}

/// The cost of the next richness level of a `kind` mine, given the level held.
///
/// On the nine same-material mines this is [`richness_curve`] in the mine's own
/// material, exactly like size. On the three two-material mines it is a **shifting
/// mix** of the same total: split between the common material and the rare one, the
/// rare share climbing with the level — mostly End Stone low down, increasingly
/// Amethyst high up (`docs/MECHANICS.md`). That shift is what puts high richness in
/// tension with prestige, since the rare material is what prestige spends too.
///
/// The rare share is **provisional** (phase 10); the *shape* is settled — common
/// before rare, common-heavy low, rare-heavy high, and the common part **never fully
/// gone**, because the ramp tops out at [`RARE_RAMP_END_PERMILLE`], below the whole.
/// The common line is dropped only at level 0, where the rare share is zero and the
/// mix collapses to a single line.
pub fn mine_richness_cost(kind: MineKind, current_richness_level: u32) -> Cost {
    mine_cost(
        kind,
        richness_curve(current_richness_level),
        current_richness_level,
        MAX_RICHNESS_LEVEL,
    )
}

// --- Transactional spending (step 3) ---
//
// The till. Each `buy_*` follows one order so that a refusal changes nothing:
// **is it possible? → is all of it affordable? → debit all → apply**. The
// possibility check comes first because the apply step (`Pickaxe::upgrade`,
// `Mine::upgrade_*`) can itself refuse at a cap, and a debit before a refused apply
// would be the partial payment `error` forbids. Once possibility and affordability
// are established, the apply cannot fail — it is called on a state its own
// pre-check already cleared.

/// Whether the inventory holds every line of `cost`, in the exact denominations
/// quoted.
///
/// Strict, like [`Inventory::has`]: 650 raw Iron does not satisfy a `6 Compressed
/// Iron` line. This is what the Upgrades screen reads to enable a buy button. A
/// [`Cost`] is one line per material by construction, so each line is checked on
/// its own.
pub fn can_afford(inventory: &Inventory, cost: &Cost) -> bool {
    cost.lines()
        .iter()
        .flat_map(CostLine::requirements)
        .all(|(item, amount)| inventory.has(item, amount))
}

/// Debits `cost` from the inventory, or refuses without touching it.
///
/// **Two passes, and that is the whole point.** The first checks every line; the
/// second debits. A single pass that debited as it checked would take the first
/// material of a multi-line cost and then fail on the second — leaving the player
/// poorer *and* empty-handed, the partial debit the whole [`error`](crate::error)
/// module is built to forbid. Nothing is removed until everything is known
/// affordable, so the second pass cannot fail.
///
/// `pub(crate)` rather than private, for exactly one caller outside this module:
/// [`GameState::prestige`] buys something that is not an upgrade, so it has no
/// `buy_*` of its own here — the price it pays is
/// [`prestige::cost`](crate::prestige::cost) and everything it *applies* is a reset of
/// fields this module cannot see. Routing it through the same till is what keeps "can
/// they afford it" a single implementation, and hands the refusal the `needed`/`held`
/// pair a preview screen wants.
///
/// [`GameState::prestige`]: crate::game::GameState::prestige
pub(crate) fn pay(inventory: &mut Inventory, cost: &Cost) -> Result<(), CoreError> {
    for (item, amount) in cost.lines().iter().flat_map(CostLine::requirements) {
        if !inventory.has(item, amount) {
            return Err(CoreError::InsufficientItems {
                item,
                needed: amount,
                held: inventory.count(item),
            });
        }
    }
    for (item, amount) in cost.lines().iter().flat_map(CostLine::requirements) {
        inventory.remove(item, amount)?;
    }
    Ok(())
}

/// Buys one Efficiency level for the pickaxe: check the cap, debit, apply.
///
/// Refuses at the tier's Efficiency cap with [`CoreError::EnchantAtCap`] — the
/// signal **buy-max reads to stop at the tier boundary** rather than roll on into a
/// tier jump. Priced by [`pickaxe_efficiency_cost`].
pub fn buy_pickaxe_efficiency(
    inventory: &mut Inventory,
    pickaxe: &mut Pickaxe,
) -> Result<(), CoreError> {
    let tier = pickaxe.get_tier();
    let level = pickaxe.enchants().get_level(EnchantType::Efficiency);
    let cap = tier.efficiency_cap();
    if level >= cap {
        return Err(CoreError::EnchantAtCap {
            kind: EnchantType::Efficiency,
            cap,
        });
    }

    pay(inventory, &pickaxe_efficiency_cost(tier, level))?;
    // Efficiency is below the cap, so `upgrade` raises it (never a tier jump) and
    // cannot fail; the pre-check is what makes the debit safe.
    pickaxe.upgrade()
}

/// Buys the jump to the next pickaxe tier: check Efficiency is maxed, debit, apply
/// (which resets Efficiency on the stronger tier).
///
/// Refuses with [`CoreError::EfficiencyNotMaxed`] while Efficiency is still below
/// its cap — a jump resets it, so buying early would throw away paid levels — and
/// with [`CoreError::PickaxeFullyUpgraded`] at Netherite, where there is no next
/// tier. Priced by [`pickaxe_tier_cost`].
pub fn buy_pickaxe_tier(inventory: &mut Inventory, pickaxe: &mut Pickaxe) -> Result<(), CoreError> {
    let tier = pickaxe.get_tier();
    let level = pickaxe.enchants().get_level(EnchantType::Efficiency);
    let cap = tier.efficiency_cap();
    if level < cap {
        return Err(CoreError::EfficiencyNotMaxed {
            current: level,
            cap,
        });
    }
    // `next` is checked but not priced: the jump is paid in the tier being *left*
    // (see `pickaxe_tier_cost`). What it establishes is that there is somewhere to go.
    tier.next().ok_or(CoreError::PickaxeFullyUpgraded)?;

    pay(inventory, &pickaxe_tier_cost(tier))?;
    // Efficiency is at the cap and a next tier exists, so `upgrade` performs the
    // jump (not an Efficiency bump) and cannot fail.
    pickaxe.upgrade()
}

/// Buys one level of a special enchant — Fortune or a world-capped enchant:
/// check the cap, debit, apply.
///
/// Priced by [`enchant_cost`] and applied through
/// [`Pickaxe::upgrade_enchant`](crate::pickaxe::Pickaxe::upgrade_enchant). `world`
/// is the highest world the player has unlocked, which the cap needs (phase 6 owns
/// that set). The cap is checked *before* paying, since a purchase at the cap would
/// otherwise debit and then be refused by the apply.
///
/// [`Efficiency`](EnchantType::Efficiency) is not a shop enchant — passed here it
/// is **routed to [`buy_pickaxe_efficiency`]** rather than refused, so a caller
/// that does not special-case it still prices and applies it correctly.
pub fn buy_enchant(
    inventory: &mut Inventory,
    pickaxe: &mut Pickaxe,
    kind: EnchantType,
    world: World,
) -> Result<(), CoreError> {
    let tier = pickaxe.get_tier();
    let level = pickaxe.enchants().get_level(kind);
    let Some(cost) = enchant_cost(kind, level, world) else {
        // `enchant_cost` is `None` only for Efficiency, which the pickaxe path
        // prices (in the tier material) and applies.
        return buy_pickaxe_efficiency(inventory, pickaxe);
    };

    let cap = kind.max_level(tier, world);
    if level >= cap {
        return Err(CoreError::EnchantAtCap { kind, cap });
    }

    pay(inventory, &cost)?;
    // Level is below the cap, so the apply cannot fail.
    pickaxe.upgrade_enchant(kind, world)
}

/// Buys the next size level of a mine: check it is not maxed, debit, apply
/// (growing and refilling the grid).
///
/// Refuses at the top of the size table with [`CoreError::MineSizeMaxed`]. Takes
/// `&mut Rng` because the enlargement redraws the grid at its new size, and every
/// draw comes from the seeded generator. Priced by [`mine_size_cost`].
pub fn buy_mine_size(
    inventory: &mut Inventory,
    mine: &mut Mine,
    rng: &mut Rng,
) -> Result<(), CoreError> {
    if mine.is_size_maxed() {
        return Err(CoreError::MineSizeMaxed {
            level: mine.get_size_level(),
        });
    }

    pay(
        inventory,
        &mine_size_cost(mine.kind(), mine.get_size_level()),
    )?;
    mine.upgrade_size_level(rng)
}

/// Buys the next richness *level* — the ceiling — of a mine: check it is not maxed,
/// debit, apply.
///
/// Raises only the ceiling; the player then moves the free
/// [dial](crate::mine::Mine::set_richness_setting) to use it. Refuses at the top
/// rung with [`CoreError::RichnessLevelMaxed`]. Priced by [`mine_richness_cost`] —
/// a single common-material line on the same-material mines, a shifting common →
/// rare mix on the two-material ones.
pub fn buy_mine_richness(inventory: &mut Inventory, mine: &mut Mine) -> Result<(), CoreError> {
    if mine.is_richness_maxed() {
        return Err(CoreError::RichnessLevelMaxed {
            level: mine.get_richness_level(),
        });
    }

    pay(
        inventory,
        &mine_richness_cost(mine.kind(), mine.get_richness_level()),
    )?;
    mine.upgrade_richness_level()
}

/// What one temporary Redstone [`Boost`](crate::boost::Boost) costs — a single flat line of Redstone.
///
/// Takes no state, unlike every other `*_cost` in this module, and that is the
/// whole difference between a consumable and a ladder: [`cost_curve`] is indexed by
/// *how far up a track the player already is*, and a boost has no level held to
/// read. Pricing it off the number already bought would make it climb for no design
/// reason, and would need a counter in the save to do it. Provisional, like the
/// other tables here; see [`BOOST_COST`].
///
/// Still a [`Cost`], not a bare number, so the Upgrades screen renders it through
/// the same two-denomination path as everything else — `3 Compressed Redstone`.
pub fn boost_cost() -> Cost {
    Cost::single(Material::Redstone, BOOST_COST)
}

/// Buys one Redstone boost **charge**: debit, and nothing else.
///
/// **A charge, not a running boost, and the distinction is the whole signature.** A
/// bought boost does not start — the player holds it and fires it when the mine in
/// front of them is worth it. Returning a [`Boost`](crate::boost::Boost) here would
/// hand the caller an
/// object already counting down its ticks, so a charge sitting in
/// reserve would be indistinguishable from one burning, and a player who bought three
/// before a session would find all three expired.
///
/// The reserve is therefore a plain count on phase 7's game state, not a collection of
/// [`Boost`](crate::boost::Boost)s: every boost in the game is identical, so a stored
/// one carries no information beyond *how many*. [`Boost`](crate::boost::Boost) stays
/// the type of a boost that is **running**, minted at activation from
/// [`BOOST_MULTIPLIER`](crate::tunables::BOOST_MULTIPLIER) and
/// [`BOOST_DURATION_TICKS`](crate::tunables::BOOST_DURATION_TICKS); this function only
/// sells the right to mint one.
///
/// Refuses with [`CoreError::InsufficientItems`] and changes nothing, like every
/// other buy — there is no cap to check first, since a consumable has no ceiling to
/// hit. Repeatable at a flat price ([`boost_cost`]).
pub fn buy_boost(inventory: &mut Inventory) -> Result<(), CoreError> {
    pay(inventory, &boost_cost())
}

/// Repeats a single purchase up to `max_count` times, stopping at the first
/// refusal, and returns how many succeeded.
///
/// The engine behind **buy-×N** (`max_count = n`) and **buy-max**
/// (`max_count = u32::MAX`): each call re-reads the state, so the price climbs step
/// by step and the loop halts the moment the next one is unaffordable or the track
/// is capped. A refusal changes nothing, so the failed final attempt costs the
/// player nothing.
///
/// Takes the purchase as a closure because each track's buy has its own arguments
/// — some need a [`World`], [`buy_mine_size`] needs an [`Rng`] — so the caller wraps
/// the one it wants: `buy_repeatedly(5, || buy_pickaxe_efficiency(inv, pickaxe))`.
/// Applied to [`buy_pickaxe_efficiency`], buy-max stops at the Efficiency cap
/// rather than advancing the tier, which is the whole reason Efficiency and the
/// tier jump are separate purchases.
///
/// **The count guard is the left operand on purpose.** `&&` short-circuits, so
/// writing the test the other way round would fire one purchase past `max_count`
/// and then discard it — a debit the caller never asked for, since a successful
/// buy is not undone by ignoring its `Ok`.
pub fn buy_repeatedly(max_count: u32, mut buy_once: impl FnMut() -> Result<(), CoreError>) -> u32 {
    let mut bought = 0;
    while bought < max_count && buy_once().is_ok() {
        bought += 1;
    }
    bought
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every per-track curve, as `(name, step -> price)` — so the invariants below
    /// are asserted on all four rather than on whichever one was written first.
    ///
    /// The tracks are independent dials now, and a re-balance that flattened only the
    /// richness curve would slip past a test naming only the size one.
    /// A track's cost curve, named: `(track, step -> price)`.
    type NamedCurve = (&'static str, fn(u32) -> u32);

    const CURVES: &[NamedCurve] = &[
        ("size", size_curve),
        ("richness", richness_curve),
        ("upgrade", upgrade_curve),
        ("enchant", enchant_curve),
    ];

    /// The three fuel shares make exactly one whole. This is what lets
    /// [`enchant_cost`] take its principal as a *remainder* while
    /// [`reward`](crate::reward) computes the same share as a *number*, without the two
    /// ever meaning different things — and it is a `const` block, so a re-balance that
    /// left the three summing to 990 stops the crate compiling rather than quietly
    /// paying out 99 % of every bundle.
    #[test]
    fn the_fuel_shares_make_one_whole() {
        const {
            assert!(
                FUEL_PRINCIPAL_PERMILLE + FUEL_ABUNDANT_PERMILLE + FUEL_SCARCE_PERMILLE == 1_000
            );
            assert!(FUEL_PRINCIPAL_PERMILLE > FUEL_ABUNDANT_PERMILLE);
            assert!(FUEL_ABUNDANT_PERMILLE > FUEL_SCARCE_PERMILLE);
        }
    }

    /// Each curve opens at exactly its own base: the first step of a track costs the
    /// base term, since `growth^0 = 1`. This is the anchor every later price on that
    /// track is a multiple of — and the reason the *base* is what a balance pass turns
    /// to change the early game, whatever it does to the slope.
    #[test]
    fn each_curve_opens_at_its_own_base_cost() {
        assert_eq!(size_curve(0), COST_BASE);
        assert_eq!(richness_curve(0), COST_BASE);
        assert_eq!(upgrade_curve(0), COST_BASE);
        assert_eq!(enchant_curve(0), ENCHANT_COST_BASE);
    }

    /// Prices must strictly rise, step after step, on every track — that is the whole
    /// point of a geometric sink. Asserted over 20 steps: wider than any real track
    /// (size runs 9, the enchant ladder 10, Netherite's Efficiency 15) but short of
    /// the `u32::MAX` clamp, past which the saturating cast deliberately stops the
    /// climb. **The steeper the slope the sooner that clamp arrives** — the size curve
    /// reaches it around step 38, where the old single 1.15 slope needed 143 — which
    /// is why this range is stated against the tracks rather than picked large.
    #[test]
    fn every_curve_strictly_increases_over_the_range_tracks_use() {
        for &(name, curve) in CURVES {
            for n in 0..20 {
                assert!(
                    curve(n + 1) > curve(n),
                    "{name}: step {} ({}) is no dearer than step {n} ({})",
                    n + 1,
                    curve(n + 1),
                    curve(n)
                );
            }
        }
    }

    /// A non-finite or oversized curve value must not wrap to a *cheap* price. The
    /// saturating `f64 as u32` cast is what guarantees it: an absurd step clamps to
    /// `u32::MAX`, the most expensive answer, never a small one.
    #[test]
    fn every_curve_saturates_rather_than_wrapping() {
        for &(name, curve) in CURVES {
            assert_eq!(curve(u32::MAX), u32::MAX, "{name} wrapped to a cheap price");
        }
    }

    /// The ramp reaches both of its ends exactly, and never leaves them. The end
    /// matters most: it is the dial's own ceiling, and a ramp stopping short of it
    /// would leave the last rung of a mine's richness track buying nothing the
    /// recipe wants.
    #[test]
    fn the_rare_ramp_spans_exactly_its_two_ends() {
        for span in 1..12u32 {
            for start in [0, RECIPE_RAMP_START_PERMILLE] {
                assert_eq!(rare_permille(0, span, start), start);
                assert_eq!(rare_permille(span, span, start), RARE_RAMP_END_PERMILLE);
                // Past the last rung the ramp saturates rather than running off:
                // phase 9 may hand back a level this table no longer defines.
                assert_eq!(rare_permille(span + 5, span, start), RARE_RAMP_END_PERMILLE);
            }
        }
    }

    /// The rare share climbs, never dips — the whole reason the ramp replaced a fixed
    /// fraction. A share that went backwards would move the optimum dial setting
    /// *down* as the player climbed, which is the opposite of the intent.
    #[test]
    fn the_rare_ramp_never_goes_backwards() {
        for span in 1..12u32 {
            for step in 0..span {
                assert!(
                    rare_permille(step + 1, span, 0) >= rare_permille(step, span, 0),
                    "span {span}: the rare share fell between step {step} and {}",
                    step + 1
                );
            }
        }
    }

    /// Splitting a price across two materials must not change what it costs. The
    /// common part is computed as the remainder for exactly this reason: a split that
    /// rounded both halves independently would gain or lose an item, and the player
    /// would pay the rounding.
    #[test]
    fn splitting_a_price_never_changes_its_total() {
        for total in [0, 1, 7, 100, 999, 12_345, u32::MAX] {
            for step in 0..=9 {
                let (common, rare) = split_rare(total, step, 9, 0);
                assert_eq!(common + rare, total, "total {total} at step {step}");
            }
        }
    }

    /// A ramp with no steps has nowhere to climb, so it answers with its start rather
    /// than dividing by zero. Not reachable from any real track — every span is a
    /// table length — but the function is total and says so.
    #[test]
    fn a_ramp_with_no_span_stays_at_its_start() {
        assert_eq!(
            rare_permille(0, 0, RECIPE_RAMP_START_PERMILLE),
            RECIPE_RAMP_START_PERMILLE
        );
        assert_eq!(rare_permille(7, 0, 0), 0);
    }

    /// The split is exact and reversible: the two denominations always add back up
    /// to the raw total, and the raw remainder is always a genuine remainder
    /// (`< RAW_PER_COMPRESSED`). This is the invariant that lets the rest of the
    /// economy reason in raw totals and trust the denomination breakdown.
    #[test]
    fn a_line_splits_a_total_without_losing_or_inventing_value() {
        for total in [0, 1, 50, 99, 100, 101, 650, 1_000, 12_345, u32::MAX] {
            let line = CostLine::from_raw_total(Material::Iron, total);
            assert!(
                line.raw < RAW_PER_COMPRESSED,
                "raw remainder {} is a whole Compressed unit",
                line.raw
            );
            assert_eq!(
                line.compressed * RAW_PER_COMPRESSED + line.raw,
                total,
                "the split of {total} does not add back up"
            );
        }
    }

    /// The worked example from `docs/MECHANICS.md`: 650 raw Iron is quoted as
    /// `6 Compressed Iron + 50 Iron`.
    #[test]
    fn the_documented_example_splits_as_written() {
        let line = CostLine::from_raw_total(Material::Iron, 650);
        assert_eq!(line.compressed, 6);
        assert_eq!(line.raw, 50);
    }

    /// A line owes only the denominations it actually has, named with the right
    /// material — the shape the till walks.
    #[test]
    fn requirements_name_only_the_non_zero_denominations() {
        let line = CostLine::from_raw_total(Material::Iron, 650);
        assert_eq!(
            line.requirements(),
            vec![
                (Item::Compressed(Material::Iron), 6),
                (Item::Raw(Material::Iron), 50),
            ]
        );

        // A whole number of Compressed units owes nothing raw, and vice versa.
        assert_eq!(
            CostLine::from_raw_total(Material::Gold, 300).requirements(),
            vec![(Item::Compressed(Material::Gold), 3)]
        );
        assert_eq!(
            CostLine::from_raw_total(Material::Gold, 40).requirements(),
            vec![(Item::Raw(Material::Gold), 40)]
        );
    }

    /// A price owing nothing in a denomination — or nothing at all — lists nothing
    /// for it, so the till never checks a phantom demand.
    #[test]
    fn a_zero_line_demands_nothing() {
        assert!(
            CostLine::from_raw_total(Material::Iron, 0)
                .requirements()
                .is_empty()
        );
    }

    /// `single` is the one-line case of `new`: same material, same split, wrapped
    /// in a one-element list.
    #[test]
    fn a_single_material_cost_is_one_line() {
        let cost = Cost::single(Material::Iron, 650);
        assert_eq!(
            cost.lines(),
            &[CostLine::from_raw_total(Material::Iron, 650)]
        );
    }

    /// A multi-material price keeps its lines in the order they were quoted, so the
    /// UI renders them as the design describes (common before rare).
    #[test]
    fn a_multi_material_cost_preserves_its_line_order() {
        let cost = Cost::new(vec![
            CostLine::from_raw_total(Material::Endstone, 400),
            CostLine::from_raw_total(Material::Amethyst, 120),
        ]);
        assert_eq!(cost.lines().len(), 2);
        assert_eq!(cost.lines()[0].material, Material::Endstone);
        assert_eq!(cost.lines()[1].material, Material::Amethyst);
    }

    // --- Per-track costs (step 2) ---

    /// The whole price in raw items, summed across every line and denomination —
    /// the readable half of the monotonicity assertions below.
    fn raw_total(cost: &Cost) -> u32 {
        cost.lines()
            .iter()
            .map(|line| line.compressed * RAW_PER_COMPRESSED + line.raw)
            .sum()
    }

    /// The raw items owed in one material across a cost's lines.
    fn part(cost: &Cost, material: Material) -> u32 {
        cost.lines()
            .iter()
            .filter(|line| line.material == material)
            .map(|line| line.compressed * RAW_PER_COMPRESSED + line.raw)
            .sum()
    }

    /// The pickaxe upgrade material follows the worlds table; the tier the doc
    /// names explicitly is the anchor — Netherite is bought in Ancient Debris,
    /// whether the step is an Efficiency level or the jump into the tier.
    #[test]
    fn pickaxe_upgrades_are_priced_in_the_tier_material() {
        assert_eq!(
            pickaxe_efficiency_cost(PickaxeTier::Wooden, 0).lines()[0].material,
            Material::Stone
        );
        assert_eq!(
            pickaxe_efficiency_cost(PickaxeTier::Netherite, 0).lines()[0].material,
            Material::AncientDebris
        );
        assert_eq!(
            pickaxe_tier_cost(PickaxeTier::Netherite).lines()[0].material,
            Material::AncientDebris
        );
    }

    /// Netherite's Efficiency `1..=5` is the ordinary tier upgrade in Ancient
    /// Debris, but `6..=15` is the post-Netherite enhancement, paid in Obsidian
    /// **and** Crying Obsidian both (the worlds table's "consumes both"). The switch
    /// sits at the standard Efficiency cap of 5, mostly the common Obsidian.
    #[test]
    fn netherite_efficiency_is_paid_in_obsidian_and_crying_past_the_standard_cap() {
        // Up to the standard cap: still one line of Ancient Debris.
        let below = pickaxe_efficiency_cost(PickaxeTier::Netherite, 4); // buys Eff 5
        assert_eq!(below.lines().len(), 1);
        assert_eq!(below.lines()[0].material, Material::AncientDebris);

        // Past it: Obsidian (common, first and dominant) then Crying (rare).
        let above = pickaxe_efficiency_cost(PickaxeTier::Netherite, 5); // buys Eff 6
        let materials: Vec<Material> = above.lines().iter().map(|l| l.material).collect();
        assert_eq!(
            materials,
            vec![Material::Obsidian, Material::CryingObsidian]
        );
        assert!(
            part(&above, Material::Obsidian) > part(&above, Material::CryingObsidian),
            "Obsidian, the common cell, must dominate the recipe"
        );

        // The switch changes *what* is owed, never *how much*: the raw total is
        // still the curve's, merely split across two materials.
        assert_eq!(
            raw_total(&above),
            upgrade_curve(5),
            "splitting the cost across two materials must not change the total"
        );
    }

    /// Efficiency climbs in price within a tier — the within-tier escalation half
    /// of the pricing (the material is the cross-tier half).
    #[test]
    fn efficiency_gets_dearer_with_each_level() {
        let tier = PickaxeTier::Netherite;
        for level in 0..14u8 {
            assert!(
                raw_total(&pickaxe_efficiency_cost(tier, level + 1))
                    > raw_total(&pickaxe_efficiency_cost(tier, level)),
                "Efficiency {} is no dearer than {level}",
                level + 1
            );
        }
    }

    /// Reaching a later tier costs strictly more than reaching an earlier one, so
    /// the tier jump is a real escalating price and not a flat toll.
    #[test]
    fn reaching_a_later_tier_costs_more() {
        let targets = [
            PickaxeTier::Stone,
            PickaxeTier::Iron,
            PickaxeTier::Gold,
            PickaxeTier::Diamond,
            PickaxeTier::Netherite,
        ];
        for pair in targets.windows(2) {
            assert!(
                raw_total(&pickaxe_tier_cost(pair[1])) > raw_total(&pickaxe_tier_cost(pair[0])),
                "reaching {:?} costs no more than {:?}",
                pair[1],
                pair[0]
            );
        }
    }

    /// Efficiency is a pickaxe upgrade, not an enchant-shop line — so the enchant
    /// cost function refuses to price it, and the purchase path routes it through
    /// [`pickaxe_efficiency_cost`] instead.
    #[test]
    fn efficiency_is_not_sold_at_the_enchant_shop() {
        assert_eq!(enchant_cost(EnchantType::Efficiency, 3, World::End), None);
    }

    /// Fortune is bought in Emerald, its own currency, whatever world the player
    /// has reached.
    #[test]
    fn fortune_is_priced_in_emerald() {
        for world in [World::Overworld, World::Nether, World::End] {
            let cost = enchant_cost(EnchantType::Fortune, 2, world);
            assert_eq!(
                cost.as_ref().map(|c| c.lines()[0].material),
                Some(Material::Emerald)
            );
        }
    }

    /// A special enchant is bought in its world's material, plus a provisional
    /// fuel line in an *earlier* ore — never more of the enchant's own material.
    /// This is the "old mines stay useful" multi-line shape.
    #[test]
    fn a_special_enchant_is_priced_in_its_world_material_plus_earlier_fuel() {
        let cost = enchant_cost(EnchantType::Explosive, 5, World::End);
        assert_eq!(
            cost.as_ref().map(|c| c.lines()[0].material),
            Some(World::End.enchant_material())
        );

        let lines = cost.map(|c| c.lines().to_vec()).unwrap_or_default();
        assert!(lines.len() >= 2, "the earlier-ore fuel line is missing");
        let primary = lines[0].material;
        for line in &lines[1..] {
            assert_ne!(
                line.material, primary,
                "fuel must be an earlier ore, not more of the enchant's own material"
            );
        }
    }

    #[test]
    fn mine_size_is_priced_in_the_mines_own_material() {
        assert_eq!(
            mine_size_cost(MineKind::Iron, 0).lines()[0].material,
            MineKind::Iron.common_material()
        );
    }

    /// On a same-material mine the richness dial has one sensible position, so its
    /// cost is a single common-material line, exactly like size.
    #[test]
    fn richness_on_a_same_material_mine_is_one_common_line() {
        let cost = mine_richness_cost(MineKind::Iron, 4);
        assert_eq!(cost.lines().len(), 1);
        assert_eq!(cost.lines()[0].material, MineKind::Iron.common_material());
    }

    /// On a two-material mine the richness cost is a mix that slides from common
    /// toward rare as it climbs — common-heavy low, rare-heavy high, common never
    /// fully gone, common line always first. The direction of the shift is the
    /// settled part; the exact shares are phase 10.
    #[test]
    fn richness_on_a_two_material_mine_shifts_from_common_to_rare() {
        let kind = MineKind::Amethyst; // End: common End Stone, value Amethyst
        let (common, rare) = (kind.common_material(), kind.value_material());

        // Level 0: the rare share is zero, so it is a single common line.
        let bottom = mine_richness_cost(kind, 0);
        assert_eq!(bottom.lines().len(), 1);
        assert_eq!(bottom.lines()[0].material, common);

        // Low: common dominates. High: rare dominates. That inversion is the shift.
        let low = mine_richness_cost(kind, 1);
        assert!(part(&low, common) > part(&low, rare));

        let high = mine_richness_cost(kind, 8);
        assert!(part(&high, rare) > part(&high, common));
        assert!(
            part(&high, common) > 0,
            "the common part must never fully vanish, or the mine could not fund its own growth"
        );
        assert_eq!(high.lines()[0].material, common, "common line comes first");
        assert_eq!(high.lines()[1].material, rare);
    }

    /// D-3, stated as the test that would have caught the old shape: leaving a tier
    /// is paid in **that tier's** material. Diamond funds the jump to Netherite, not
    /// Ancient Debris — which the player has no way to mine until they arrive.
    #[test]
    fn a_tier_jump_is_paid_in_the_tier_being_left() {
        let expected = [
            (PickaxeTier::Wooden, Material::Stone),
            (PickaxeTier::Stone, Material::Coal),
            (PickaxeTier::Iron, Material::Iron),
            (PickaxeTier::Gold, Material::Gold),
            (PickaxeTier::Diamond, Material::Diamond),
        ];
        for (from, material) in expected {
            let cost = pickaxe_tier_cost(from);
            assert_eq!(cost.lines().len(), 1, "{from:?}: a jump is one material");
            assert_eq!(
                cost.lines()[0].material,
                material,
                "leaving {from:?} must be paid in {material:?}"
            );
        }
    }

    /// Fortune is Emerald and **nothing else**. The fuel line it used to carry came
    /// from pricing every enchant through one path, not from a decision: Fortune's
    /// material is keyed to neither the world nor the tier, so it has no "current
    /// tier's ore" to consume.
    #[test]
    fn fortune_is_one_line_of_emerald_in_every_world() {
        for world in [World::Overworld, World::Nether, World::End] {
            for level in 0..10 {
                let Some(cost) = enchant_cost(EnchantType::Fortune, level, world) else {
                    unreachable!("Fortune is a shop enchant")
                };
                assert_eq!(
                    cost.lines().len(),
                    1,
                    "Fortune picked up a second line at level {level} in {}",
                    world.name()
                );
                assert_eq!(cost.lines()[0].material, Material::Emerald);
            }
        }
    }

    /// D-7: the fuel pair is a function of the **level**, not of where the player
    /// stands. Level 1 costs Stone and Coal bought from the End exactly as it does
    /// from the Overworld — the level is the progression scale, so a rung is fuelled
    /// by the ores of its own rung forever.
    #[test]
    fn enchant_fuel_follows_the_level_not_the_world() {
        let Some(from_overworld) = enchant_cost(EnchantType::Nuke, 0, World::Overworld) else {
            unreachable!("Nuke is a shop enchant")
        };
        let Some(from_end) = enchant_cost(EnchantType::Nuke, 0, World::End) else {
            unreachable!("Nuke is a shop enchant")
        };

        let fuel = |cost: &Cost| -> Vec<Material> {
            cost.lines()[1..].iter().map(|l| l.material).collect()
        };
        assert_eq!(fuel(&from_overworld), vec![Material::Stone, Material::Coal]);
        assert_eq!(
            fuel(&from_end),
            vec![Material::Stone, Material::Coal],
            "level 1 changed its fuel because the player had reached the End"
        );

        // Only the principal moves with the world — that is what the world keys.
        assert_eq!(from_overworld.lines()[0].material, Material::Lapis);
        assert_eq!(from_end.lines()[0].material, Material::Amethyst);
    }

    /// Every fuel ore belongs to the world whose band the level sits in — the point of
    /// the change. An enchant bought in the Nether must never ask for Overworld iron.
    #[test]
    fn enchant_fuel_never_reaches_back_to_an_earlier_world() {
        for (level, world) in [(3u8, World::Nether), (4, World::Nether), (5, World::Nether)] {
            let Some(cost) = enchant_cost(EnchantType::Haste, level, world) else {
                unreachable!("Haste is a shop enchant")
            };
            for line in &cost.lines()[1..] {
                assert_eq!(
                    line.material.worlds(),
                    &[World::Nether],
                    "level {} is fuelled by {:?}, which is not a Nether ore",
                    level + 1,
                    line.material
                );
            }
        }
    }

    /// The three lines of an enchant price **share** the step's total; they do not add
    /// to it. A fuel line stacked on top would make the quoted curve a lie about what
    /// the step costs.
    #[test]
    fn an_enchant_price_shares_one_total_across_its_lines() {
        for world in [World::Overworld, World::Nether, World::End] {
            for level in 0..10 {
                let Some(cost) = enchant_cost(EnchantType::Excavator, level, world) else {
                    unreachable!("Excavator is a shop enchant")
                };
                assert_eq!(
                    raw_total(&cost),
                    enchant_curve(u32::from(level)),
                    "level {} in {} does not add up to its curve step",
                    level + 1,
                    world.name()
                );
            }
        }
    }

    /// The End quotes **two** lines, not three, and the second is the enchant material
    /// itself. A third line would be a second line of Amethyst, which `Cost` forbids
    /// by construction — the End holds one mine, and its rare cell is what enchants
    /// there are bought with.
    #[test]
    fn the_end_fuels_its_enchants_from_its_only_mine() {
        for level in 6..10u8 {
            let Some(cost) = enchant_cost(EnchantType::Jackhammer, level, World::End) else {
                unreachable!("Jackhammer is a shop enchant")
            };
            let materials: Vec<Material> = cost.lines().iter().map(|l| l.material).collect();
            assert_eq!(
                materials,
                vec![Material::Endstone, Material::Amethyst],
                "level {} in the End",
                level + 1
            );
        }

        // And the Amethyst share climbs across the band, as the ramp promises.
        let first = enchant_cost(EnchantType::Jackhammer, 6, World::End);
        let last = enchant_cost(EnchantType::Jackhammer, 9, World::End);
        let share = |c: &Option<Cost>| -> f64 {
            let Some(c) = c else { return 0.0 };
            f64::from(part(c, Material::Amethyst)) / f64::from(raw_total(c))
        };
        assert!(
            share(&last) > share(&first),
            "the Amethyst share does not climb across the End's band"
        );

        // The band's ends land on the ramp's ends exactly. Off by one, the first
        // level opens a notch up the ramp and the last two both saturate at the top —
        // which costs the band a rung at each end without failing anything above.
        assert!(
            (share(&first) - 0.25).abs() < 0.01,
            "the End's first level opens at {:.3}, not at the ramp's start",
            share(&first)
        );
        assert!(
            (share(&last) - 0.91).abs() < 0.01,
            "the End's last level ends at {:.3}, not at the ramp's ceiling",
            share(&last)
        );
    }

    /// D-6: on a two-material mine, **size** is paid in both materials, like richness.
    /// Reading "the mine's own ore" as the common cell alone left Crying Obsidian
    /// funding nothing but the Efficiency climb.
    #[test]
    fn two_material_mine_size_is_paid_in_both_materials() {
        let cost = mine_size_cost(MineKind::Obsidian, 4);
        assert_eq!(cost.lines().len(), 2);
        assert_eq!(cost.lines()[0].material, Material::Obsidian, "common first");
        assert_eq!(cost.lines()[1].material, Material::CryingObsidian);

        // The nine same-material mines are untouched: one line, as before.
        assert_eq!(mine_size_cost(MineKind::Iron, 4).lines().len(), 1);
    }

    /// The rare share of Netherite's enhancement **climbs**, and that is what gives
    /// the Obsidian mine's dial a reason to move. Pinned at a fixed fraction, the
    /// optimum dial setting never changed and seven of the mine's ten rungs could only
    /// overshoot the recipe.
    #[test]
    fn the_netherite_enhancement_asks_for_more_crying_as_it_climbs() {
        let share = |level: u8| -> f64 {
            let cost = pickaxe_efficiency_cost(PickaxeTier::Netherite, level);
            f64::from(part(&cost, Material::CryingObsidian)) / f64::from(raw_total(&cost))
        };

        for level in 5..14u8 {
            assert!(
                share(level + 1) > share(level),
                "the Crying share did not climb from Efficiency {} to {}",
                level + 1,
                level + 2
            );
        }
        assert!(share(5) < 0.3, "the enhancement must open Obsidian-heavy");
        assert!(
            share(14) > 0.8,
            "and end Crying-heavy, at the dial's ceiling"
        );
    }

    // --- Transactional spending (step 3) ---

    fn rng() -> Rng {
        Rng::from_seed(42)
    }

    /// An inventory holding the listed raw materials.
    fn stocked(pairs: &[(Material, u32)]) -> Inventory {
        let mut inventory = Inventory::new();
        for &(material, amount) in pairs {
            inventory.add(Item::Raw(material), amount);
        }
        inventory
    }

    fn efficiency_of(pickaxe: &Pickaxe) -> u8 {
        pickaxe.enchants().get_level(EnchantType::Efficiency)
    }

    /// A pickaxe driven to `tier` with Efficiency filled to that tier's cap — the
    /// state a tier jump is bought from. Goes through the free `upgrade` so the
    /// setup never sells a level the game would not.
    fn maxed_efficiency_at(tier: PickaxeTier) -> Pickaxe {
        let mut pickaxe = Pickaxe::default();
        while pickaxe.get_tier() != tier {
            assert!(pickaxe.upgrade().is_ok());
        }
        while efficiency_of(&pickaxe) < tier.efficiency_cap() {
            assert!(pickaxe.upgrade().is_ok());
        }
        pickaxe
    }

    /// Stocked from the price itself rather than from a number: payment is strict, so
    /// the test has to hold the exact denominations quoted, and deriving them is what
    /// keeps this test true across a re-balance (see [`stocked_for`]).
    #[test]
    fn buying_efficiency_debits_and_raises_the_level() {
        let mut pickaxe = Pickaxe::default(); // Wooden, Efficiency 0
        let cost = pickaxe_efficiency_cost(PickaxeTier::Wooden, 0);
        let mut inventory = stocked_for(&cost, 1);

        assert_eq!(buy_pickaxe_efficiency(&mut inventory, &mut pickaxe), Ok(()));
        assert_eq!(efficiency_of(&pickaxe), 1);
        assert_eq!(
            inventory,
            Inventory::new(),
            "the buy took exactly the price and no more"
        );
    }

    /// The load-bearing guarantee, on the pickaxe: a refused buy leaves the level
    /// *and* the inventory exactly as they were.
    #[test]
    fn an_unaffordable_efficiency_buy_changes_nothing() {
        let mut pickaxe = Pickaxe::default();
        let mut inventory = Inventory::new(); // holds nothing at all
        let before = inventory.clone();

        assert!(buy_pickaxe_efficiency(&mut inventory, &mut pickaxe).is_err());
        assert_eq!(efficiency_of(&pickaxe), 0, "the level moved on a refusal");
        assert_eq!(inventory, before, "the inventory moved on a refusal");
    }

    /// At the cap the buy is refused *before* it debits — a capped enchant that
    /// still took the ore would be the silent hole the whole till is built to close.
    #[test]
    fn efficiency_refuses_at_the_cap_without_debiting() {
        let mut pickaxe = maxed_efficiency_at(PickaxeTier::Wooden); // Efficiency 5
        let mut inventory = stocked_for(&pickaxe_efficiency_cost(PickaxeTier::Wooden, 4), 20);
        let before = inventory.clone();

        assert_eq!(
            buy_pickaxe_efficiency(&mut inventory, &mut pickaxe),
            Err(CoreError::EnchantAtCap {
                kind: EnchantType::Efficiency,
                cap: 5,
            })
        );
        assert_eq!(inventory, before, "a capped buy still took the ore");
    }

    /// A tier jump before Efficiency is maxed is refused and debits nothing — the
    /// rule that stops the player throwing away paid Efficiency.
    #[test]
    fn a_tier_jump_needs_efficiency_maxed_first() {
        let mut pickaxe = Pickaxe::default(); // Wooden, Efficiency 0
        let mut inventory = stocked_for(&pickaxe_tier_cost(PickaxeTier::Wooden), 5);
        let before = inventory.clone();

        assert_eq!(
            buy_pickaxe_tier(&mut inventory, &mut pickaxe),
            Err(CoreError::EfficiencyNotMaxed { current: 0, cap: 5 })
        );
        assert_eq!(pickaxe.get_tier(), PickaxeTier::Wooden);
        assert_eq!(inventory, before);
    }

    #[test]
    fn buying_a_tier_jump_advances_and_resets_efficiency() {
        let mut pickaxe = maxed_efficiency_at(PickaxeTier::Wooden); // Wooden, Efficiency 5
        // Leaving Wooden is paid in Stone, the tier being left — not in Coal, the
        // material of the tier being reached.
        let mut inventory = stocked_for(&pickaxe_tier_cost(PickaxeTier::Wooden), 1);

        assert_eq!(buy_pickaxe_tier(&mut inventory, &mut pickaxe), Ok(()));
        assert_eq!(pickaxe.get_tier(), PickaxeTier::Stone);
        assert_eq!(efficiency_of(&pickaxe), 0, "a tier jump resets Efficiency");
        assert_eq!(
            inventory,
            Inventory::new(),
            "the jump took exactly the price of leaving Wooden"
        );
    }

    #[test]
    fn a_fully_maxed_pickaxe_cannot_jump() {
        let mut pickaxe = maxed_efficiency_at(PickaxeTier::Netherite); // Efficiency 15
        let mut inventory = stocked(&[(Material::AncientDebris, 10_000)]);

        assert_eq!(
            buy_pickaxe_tier(&mut inventory, &mut pickaxe),
            Err(CoreError::PickaxeFullyUpgraded)
        );
    }

    /// A special enchant debits every line of its price at once — here the three of
    /// an Overworld purchase: the world's material plus the level's two fuel ores.
    #[test]
    fn buying_a_special_enchant_debits_all_its_lines() {
        let mut pickaxe = Pickaxe::default();
        let Some(cost) = enchant_cost(EnchantType::Explosive, 0, World::Overworld) else {
            unreachable!("Explosive is a shop enchant, so it has a price")
        };
        assert_eq!(
            cost.lines().len(),
            3,
            "world material plus its two fuel ores"
        );
        let mut inventory = stocked_for(&cost, 1);

        assert_eq!(
            buy_enchant(
                &mut inventory,
                &mut pickaxe,
                EnchantType::Explosive,
                World::Overworld
            ),
            Ok(())
        );
        assert_eq!(pickaxe.enchants().get_level(EnchantType::Explosive), 1);
        assert_eq!(inventory, Inventory::new(), "a line went undebited");
    }

    /// The multi-line partial-debit guard, stated as a test: short on the *second*
    /// line, and the *first* is not debited either. If this ever fails, `pay` has
    /// started debiting before it finished checking.
    #[test]
    fn a_special_enchant_short_on_one_line_debits_neither() {
        let mut pickaxe = Pickaxe::default();
        let Some(cost) = enchant_cost(EnchantType::Explosive, 0, World::Overworld) else {
            unreachable!("Explosive is a shop enchant, so it has a price")
        };
        // Everything the price asks for except its very last line.
        let mut inventory = Inventory::new();
        let mut requirements = cost
            .lines()
            .iter()
            .flat_map(CostLine::requirements)
            .peekable();
        while let Some((item, amount)) = requirements.next() {
            if requirements.peek().is_some() {
                inventory.add(item, amount);
            }
        }
        let before = inventory.clone();

        assert!(
            buy_enchant(
                &mut inventory,
                &mut pickaxe,
                EnchantType::Explosive,
                World::Overworld
            )
            .is_err()
        );
        assert_eq!(pickaxe.enchants().get_level(EnchantType::Explosive), 0);
        assert_eq!(
            inventory, before,
            "the earlier lines were debited before the last one failed"
        );
    }

    #[test]
    fn a_special_enchant_refuses_at_its_world_cap() {
        // Explosive caps at 3 in the Overworld.
        let mut pickaxe = Pickaxe::default();
        let mut inventory = Inventory::new();
        for level in 0..3 {
            let Some(cost) = enchant_cost(EnchantType::Explosive, level, World::Overworld) else {
                unreachable!("Explosive is a shop enchant")
            };
            for (item, amount) in cost.lines().iter().flat_map(CostLine::requirements) {
                inventory.add(item, amount);
            }
        }

        for _ in 0..3 {
            assert!(
                buy_enchant(
                    &mut inventory,
                    &mut pickaxe,
                    EnchantType::Explosive,
                    World::Overworld
                )
                .is_ok()
            );
        }
        assert_eq!(
            buy_enchant(
                &mut inventory,
                &mut pickaxe,
                EnchantType::Explosive,
                World::Overworld
            ),
            Err(CoreError::EnchantAtCap {
                kind: EnchantType::Explosive,
                cap: 3,
            })
        );
        assert_eq!(pickaxe.enchants().get_level(EnchantType::Explosive), 3);
    }

    /// Efficiency handed to the enchant door is priced in the *tier* material and
    /// applied to Efficiency, not treated as a shop enchant — the routing that keeps
    /// a caller from having to special-case it.
    #[test]
    fn efficiency_through_the_enchant_door_uses_the_pickaxe_path() {
        let mut pickaxe = Pickaxe::default();
        let mut inventory = stocked_for(&pickaxe_efficiency_cost(PickaxeTier::Wooden, 0), 1);

        assert_eq!(
            buy_enchant(
                &mut inventory,
                &mut pickaxe,
                EnchantType::Efficiency,
                World::End
            ),
            Ok(())
        );
        assert_eq!(efficiency_of(&pickaxe), 1);
        assert_eq!(
            inventory,
            Inventory::new(),
            "Efficiency is priced in the tier material, not an enchant material"
        );
    }

    #[test]
    fn buying_mine_size_grows_the_grid_and_debits() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        let mut inventory = stocked_for(&mine_size_cost(MineKind::Iron, 0), 1);
        let before_level = mine.get_size_level();

        assert_eq!(buy_mine_size(&mut inventory, &mut mine, &mut rng()), Ok(()));
        assert_eq!(mine.get_size_level(), before_level + 1);
        assert_eq!(inventory, Inventory::new());
    }

    /// Buying a richness level raises the ceiling and *only* the ceiling: the dial,
    /// stuck at 0 before, can now be pushed to the new rung.
    #[test]
    fn buying_mine_richness_raises_the_ceiling_and_unlocks_the_dial() {
        let mut mine = Mine::new(MineKind::Amethyst, &mut rng());
        let mut inventory = stocked_for(&mine_richness_cost(MineKind::Amethyst, 0), 1);
        assert_eq!(mine.get_richness_level(), 0);
        assert!(
            mine.set_richness_setting(1, &mut rng()).is_err(),
            "the dial cannot exceed a ceiling of 0 yet"
        );

        assert_eq!(buy_mine_richness(&mut inventory, &mut mine), Ok(()));
        assert_eq!(mine.get_richness_level(), 1);
        assert_eq!(inventory, Inventory::new());
        assert!(
            mine.set_richness_setting(1, &mut rng()).is_ok(),
            "the dial reaches the freshly bought ceiling"
        );
    }

    /// A maxed track refuses **before** it debits, and the stock proves it: the
    /// inventory here holds exactly what the next step would cost, so a purchase
    /// that checked the purse first would succeed and hand the player a level the
    /// size table cannot render. The untouched inventory is the real assertion —
    /// the error kind alone would not catch a debit followed by a refusal.
    #[test]
    fn buying_size_past_the_top_of_the_table_refuses_and_debits_nothing() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        while !mine.is_size_maxed() {
            assert!(mine.upgrade_size_level(&mut rng()).is_ok());
        }
        let stock = stocked_for(&mine_size_cost(MineKind::Iron, MAX_SIZE_LEVEL), 1);
        let mut inventory = stock.clone();

        assert_eq!(
            buy_mine_size(&mut inventory, &mut mine, &mut rng()),
            Err(CoreError::MineSizeMaxed {
                level: MAX_SIZE_LEVEL,
            })
        );
        assert_eq!(inventory, stock, "a refusal must not debit");
        assert_eq!(mine.get_size_level(), MAX_SIZE_LEVEL);
    }

    /// The richness ceiling's half of the same rule. Past the top rung the weight
    /// formula clamps, so the level sold would buy not one extra value cell.
    #[test]
    fn buying_richness_past_the_top_rung_refuses_and_debits_nothing() {
        let mut mine = Mine::new(MineKind::Amethyst, &mut rng());
        while !mine.is_richness_maxed() {
            assert!(mine.upgrade_richness_level().is_ok());
        }
        let stock = stocked_for(
            &mine_richness_cost(MineKind::Amethyst, MAX_RICHNESS_LEVEL),
            1,
        );
        let mut inventory = stock.clone();

        assert_eq!(
            buy_mine_richness(&mut inventory, &mut mine),
            Err(CoreError::RichnessLevelMaxed {
                level: MAX_RICHNESS_LEVEL,
            })
        );
        assert_eq!(inventory, stock, "a refusal must not debit");
        assert_eq!(mine.get_richness_level(), MAX_RICHNESS_LEVEL);
    }

    /// The reason Efficiency and the tier jump are separate purchases: buy-max on
    /// Efficiency stops at the cap instead of rolling on into a tier jump.
    #[test]
    fn buy_max_efficiency_stops_at_the_tier_cap() {
        let mut pickaxe = Pickaxe::default(); // Wooden, cap 5
        // Enough for the whole climb and more: the cap, not the purse, must stop it.
        let mut inventory = stocked_for(&pickaxe_efficiency_cost(PickaxeTier::Wooden, 4), 10);

        let bought = buy_repeatedly(u32::MAX, || {
            buy_pickaxe_efficiency(&mut inventory, &mut pickaxe)
        });

        assert_eq!(bought, 5, "buy-max must stop at the Efficiency cap");
        assert_eq!(efficiency_of(&pickaxe), 5);
        assert_eq!(
            pickaxe.get_tier(),
            PickaxeTier::Wooden,
            "buy-max Efficiency must not advance the tier"
        );
    }

    /// Buy-×N buys exactly what the stock covers and stops, the rising price
    /// deciding where.
    #[test]
    fn buy_n_stops_when_the_stock_runs_out() {
        let mut pickaxe = Pickaxe::default();
        // Exactly the first two steps of the ladder, and not a scrap more.
        let mut inventory = stocked_for(&pickaxe_efficiency_cost(PickaxeTier::Wooden, 0), 1);
        for (item, amount) in pickaxe_efficiency_cost(PickaxeTier::Wooden, 1)
            .lines()
            .iter()
            .flat_map(CostLine::requirements)
        {
            inventory.add(item, amount);
        }

        let bought = buy_repeatedly(10, || buy_pickaxe_efficiency(&mut inventory, &mut pickaxe));

        assert_eq!(bought, 2, "only two levels are affordable");
        assert_eq!(efficiency_of(&pickaxe), 2);
        assert_eq!(inventory, Inventory::new(), "the third step was part-paid");
    }

    /// An inventory holding exactly `count` times what `cost` demands, in the
    /// denominations it demands them in.
    ///
    /// Reads the cost rather than stocking a raw total, because payment is
    /// **strict**: a price that quotes Compressed units is not satisfied by the
    /// equivalent pile of raw items. Deriving the stock this way also survives a
    /// phase-10 re-balance that moves a price across the 100-item boundary.
    fn stocked_for(cost: &Cost, count: u32) -> Inventory {
        let mut inventory = Inventory::new();
        for (item, amount) in cost.lines().iter().flat_map(CostLine::requirements) {
            inventory.add(item, amount * count);
        }
        inventory
    }

    /// The boost is paid for in Redstone and grants a **charge**, not a running
    /// boost: nothing starts counting down at the till, because the player fires the
    /// charge themselves. Phase 7 owns the reserve that counts them.
    #[test]
    fn buying_a_boost_debits_the_quoted_denomination_only() {
        let mut inventory = stocked_for(&boost_cost(), 1);
        inventory.add(Item::Raw(Material::Redstone), 7); // pocket change it must not touch

        assert_eq!(buy_boost(&mut inventory), Ok(()));

        assert_eq!(
            inventory.count(Item::Compressed(Material::Redstone)),
            0,
            "the quoted denomination was not the one debited"
        );
        assert_eq!(inventory.count(Item::Raw(Material::Redstone)), 7);
    }

    /// A boost is a consumable with no ceiling, so the *only* thing that can refuse
    /// it is the price — and a refusal leaves the Redstone untouched.
    ///
    /// The stock here is deliberately **raw Redstone worth more than the price**:
    /// costs are paid in the denomination they are quoted in, so a player sitting on
    /// loose ore must compress it first. That rule holds at the boost door like
    /// everywhere else.
    #[test]
    fn an_unaffordable_boost_changes_nothing() {
        let mut inventory = stocked(&[(Material::Redstone, BOOST_COST * 2)]);
        let before = inventory.clone();

        assert!(buy_boost(&mut inventory).is_err());
        assert_eq!(inventory, before, "the inventory moved on a refusal");
    }

    /// Unlike every ladder in this module, the boost's price does not climb: two
    /// boosts cost exactly twice one. This is what makes it a consumable rather
    /// than a track, and it is asserted because the *shape* is the decision — the
    /// number itself is phase-10 balance.
    #[test]
    fn a_second_boost_costs_the_same_as_the_first() {
        let mut inventory = stocked_for(&boost_cost(), 2);

        assert!(buy_boost(&mut inventory).is_ok());
        assert!(
            buy_boost(&mut inventory).is_ok(),
            "the second boost was dearer than the first"
        );
        assert!(
            buy_boost(&mut inventory).is_err(),
            "a stock of exactly two boosts bought a third"
        );
    }

    /// Affordability is judged line by line and **in the quoted denomination**: a
    /// purse holding the raw equivalent of a Compressed line does not satisfy it.
    #[test]
    fn can_afford_reads_every_line_strictly() {
        let Some(cost) = enchant_cost(EnchantType::Explosive, 0, World::Overworld) else {
            unreachable!("Explosive is a shop enchant")
        };

        let enough = stocked_for(&cost, 1);
        assert!(can_afford(&enough, &cost));

        // One item short on a single line is short overall.
        let mut short = stocked_for(&cost, 1);
        let (item, _) = cost.lines()[0].requirements()[0];
        assert!(short.remove(item, 1).is_ok());
        assert!(!can_afford(&short, &cost));

        // The whole price in raw items does not buy a price quoted in Compressed.
        let mut loose = Inventory::new();
        for line in cost.lines() {
            loose.add(
                Item::Raw(line.material),
                line.compressed * RAW_PER_COMPRESSED + line.raw,
            );
        }
        assert!(
            !can_afford(&loose, &cost),
            "raw items satisfied a price quoted in Compressed units"
        );
    }
}
