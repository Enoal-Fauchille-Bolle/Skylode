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
use crate::material::{Item, Material};
use crate::mine_kind::MineKind;
use crate::pickaxe::PickaxeTier;
use crate::tunables::{COST_BASE, COST_GROWTH, RAW_PER_COMPRESSED};
use crate::world::World;

/// The total raw cost of the `n`-th step on the geometric curve
/// `base * growth^n`, rounded to a whole number of raw items.
///
/// `n` is the **0-indexed step being bought**: the first upgrade of a track is
/// `cost_curve(0)` = [`COST_BASE`], and each further step multiplies by
/// [`COST_GROWTH`]. Which state maps to `n` is each track's own business (a
/// pickaxe's Efficiency level, a mine's size level, …); this function only knows
/// the curve.
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
/// `cost_curve(u32::MAX)` into `10 * 1.15⁻¹ ≈ 9`, the cheap price the saturation
/// exists to rule out.
pub fn cost_curve(n: u32) -> u32 {
    (f64::from(COST_BASE) * COST_GROWTH.powf(f64::from(n))).round() as u32
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
    /// [`Inventory`](crate::inventory::Inventory).
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

/// The post-Netherite enhancement is paid mostly in the common Obsidian, with one
/// part in this many owed in the rare Crying Obsidian — a provisional 3:1.
///
/// `docs/MECHANICS.md` settles that the enhancement *consumes both*, and that the
/// player tunes their Obsidian mine's richness dial toward the recipe's optimum
/// ratio; this is the provisional stand-in for that ratio. Phase 10 sets the real
/// one; the settled part is that Crying is the minority share of a two-material
/// cost.
const CRYING_SHARE_DIVISOR: u32 = 4;

/// The cost of the next **Efficiency** level on `tier`, given the level already
/// held — the first of the pickaxe's two separately-priced actions.
///
/// `cost_curve(current_level)` in the tier's material, so the price climbs within
/// a tier and **restarts** at the next: a tier jump resets Efficiency, and the
/// fresh climb is cheap *in count* but paid in a scarcer material (Stone → Coal →
/// Iron → …). The material is the cross-tier escalation, the curve the within-tier
/// one — together they keep every rung worth more than the last without one long
/// number. Provisional.
///
/// **Netherite is the exception, above [`NETHERITE_BASE_EFFICIENCY`].** Its
/// Efficiency `1..=5` is the ordinary tier upgrade in Ancient Debris, but `6..=15`
/// is the *post-Netherite enhancement*, which `docs/MECHANICS.md` pays in Obsidian
/// **and** Crying Obsidian both — the Obsidian mine's two materials. So past the
/// standard cap this returns a **two-material** cost: mostly the common Obsidian,
/// one part in [`CRYING_SHARE_DIVISOR`] the rare Crying. It is the one pickaxe cost
/// that is not a single material, which is why the split lives here rather than in
/// [`pickaxe_material`].
pub fn pickaxe_efficiency_cost(tier: PickaxeTier, current_level: u8) -> Cost {
    let total = cost_curve(u32::from(current_level));

    if tier == PickaxeTier::Netherite && current_level >= NETHERITE_BASE_EFFICIENCY {
        let crying = total / CRYING_SHARE_DIVISOR;
        let obsidian = total - crying;
        let mut lines = Vec::new();
        if obsidian > 0 {
            lines.push(CostLine::from_raw_total(Material::Obsidian, obsidian));
        }
        if crying > 0 {
            lines.push(CostLine::from_raw_total(Material::CryingObsidian, crying));
        }
        return Cost::new(lines);
    }

    Cost::single(pickaxe_material(tier), total)
}

/// The cost of advancing **to** `target_tier` — the pickaxe's second priced
/// action, kept distinct from buying Efficiency (Enoal's call to price the two
/// apart, mirroring the two phases already inside
/// [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe::upgrade)).
///
/// `cost_curve(tier_index(target_tier))` in the target tier's material, so
/// reaching Netherite is paid in Ancient Debris and reaching Diamond in Diamond,
/// as the worlds table reads. Provisional.
pub fn pickaxe_tier_cost(target_tier: PickaxeTier) -> Cost {
    Cost::single(
        pickaxe_material(target_tier),
        cost_curve(tier_index(target_tier)),
    )
}

/// How much of an enchant's total is also owed in earlier-mine ore: one part in
/// this many — provisional.
///
/// The provisional stand-in for `docs/MECHANICS.md`'s "mix of raw ores from the
/// earlier mines". Phase 10 sets the real ratio; what is settled is that there
/// *is* a second line, so old mines stay useful as enchant fuel.
const ENCHANT_FUEL_DIVISOR: u32 = 3;

/// The material an enchant `kind` is bought with in `world`, or `None` for
/// [`Efficiency`](EnchantType::Efficiency) — which is a pickaxe upgrade, priced by
/// [`pickaxe_efficiency_cost`], not an enchant-shop purchase.
///
/// [`Fortune`](EnchantType::Fortune) is Emerald, its own Overworld currency keyed
/// by neither tier nor world; the five specials are the world's
/// [`enchant_material`](World::enchant_material), climbing Lapis → Quartz →
/// Amethyst as the cap climbs with the world.
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

/// The earlier-mine ore that also fuels enchants bought in `world` — provisional.
///
/// One earlier material per world: Coal in the Overworld, an Overworld ore in the
/// Nether, a Nether ore in the End — always something the player mined on the way
/// here, never the enchant's own material. Provisional; the *shape* it pins is a
/// second [`CostLine`] in an earlier material.
fn enchant_fuel_material(world: World) -> Material {
    match world {
        World::Overworld => Material::Coal,
        World::Nether => Material::Iron,
        World::End => Material::Quartz,
    }
}

/// The cost of the next level of enchant `kind` in `world`, given the level held
/// — or `None` for Efficiency, which the pickaxe path prices instead.
///
/// Two lines: the enchant's own material for the full `cost_curve(level)`, plus a
/// **provisional** fuel line of an earlier ore ([`enchant_fuel_material`]) for one
/// part in [`ENCHANT_FUEL_DIVISOR`] of it — the "mix of earlier mines' ores" that
/// keeps old mines useful long after their tier is passed. The fuel line is
/// dropped only if it would round to nothing (a defensive guard against a tiny
/// base cost; at the current base it never does). Multi-material shape settled,
/// numbers provisional.
pub fn enchant_cost(kind: EnchantType, current_level: u8, world: World) -> Option<Cost> {
    let material = enchant_material(kind, world)?;
    let total = cost_curve(u32::from(current_level));

    let mut lines = vec![CostLine::from_raw_total(material, total)];
    let fuel = total / ENCHANT_FUEL_DIVISOR;
    if fuel > 0 {
        lines.push(CostLine::from_raw_total(enchant_fuel_material(world), fuel));
    }
    Some(Cost::new(lines))
}

/// The cost of the next size level of a `kind` mine, given the level held.
///
/// `cost_curve(current_size_level)` in the mine's own common material — every mine
/// funds its own growth out of what it mostly produces
/// ([`MineKind::common_material`]).
pub fn mine_size_cost(kind: MineKind, current_size_level: u32) -> Cost {
    Cost::single(kind.common_material(), cost_curve(current_size_level))
}

/// Over how many richness levels the two-material cost mix slides from all-common
/// to (nearly) all-rare — provisional, and matching `mine`'s richness rung count.
///
/// Kept local and provisional for the reason [`ENCHANT_FUEL_DIVISOR`] is; phase 10
/// reconciles it with `mine`'s own `MAX_RICHNESS_LEVEL`.
const RICHNESS_MIX_SPAN: u32 = 9;

/// The cost of the next richness level of a `kind` mine, given the level held.
///
/// On the nine same-material mines this is `cost_curve(level)` in the mine's own
/// material, exactly like size. On the three two-material mines it is a **shifting
/// mix** of the same total: split between the common material and the rare one,
/// the rare share climbing with the level — mostly End Stone low down, increasingly
/// Amethyst high up (`docs/MECHANICS.md`). That shift is what puts high richness in
/// tension with prestige, since the rare material is what prestige spends too.
///
/// The rare share is **provisional** (phase 10); the *shape* is settled — common
/// before rare, common-heavy low, rare-heavy high, and the common part **never
/// fully gone** across the real levels, because the rare share tops out at
/// `(MAX_RICHNESS_LEVEL - 1) / RICHNESS_MIX_SPAN < 1`. The common line is dropped
/// only at level 0, where the rare share is zero and the mix is a single line.
pub fn mine_richness_cost(kind: MineKind, current_richness_level: u32) -> Cost {
    let total = cost_curve(current_richness_level);
    let (common, value) = (kind.common_material(), kind.value_material());

    if common == value {
        return Cost::single(common, total);
    }

    // u64 intermediate so the share cannot overflow even on an absurd total; the
    // real totals are tiny, but the guard costs nothing and states the intent.
    let rare_share = current_richness_level.min(RICHNESS_MIX_SPAN);
    let rare_part =
        (u64::from(total) * u64::from(rare_share) / u64::from(RICHNESS_MIX_SPAN)) as u32;
    let common_part = total - rare_part;

    let mut lines = Vec::new();
    if common_part > 0 {
        lines.push(CostLine::from_raw_total(common, common_part));
    }
    if rare_part > 0 {
        lines.push(CostLine::from_raw_total(value, rare_part));
    }
    Cost::new(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The curve opens at exactly [`COST_BASE`]: the first step of any track costs
    /// the base term, since `growth^0 = 1`. This is the anchor every later price
    /// is a multiple of.
    #[test]
    fn the_curve_opens_at_the_base_cost() {
        assert_eq!(cost_curve(0), COST_BASE);
    }

    /// Prices must strictly rise, step after step — that is the whole point of a
    /// geometric sink. Asserted over a range wider than any real upgrade track (60
    /// steps) but well short of the `u32::MAX` clamp, past which the saturating
    /// cast deliberately stops the climb.
    #[test]
    fn the_curve_strictly_increases_over_the_range_tracks_use() {
        for n in 0..60 {
            assert!(
                cost_curve(n + 1) > cost_curve(n),
                "step {} ({}) is no dearer than step {n} ({})",
                n + 1,
                cost_curve(n + 1),
                cost_curve(n)
            );
        }
    }

    /// A non-finite or oversized curve value must not wrap to a *cheap* price. The
    /// saturating `f64 as u32` cast is what guarantees it: an absurd step clamps to
    /// `u32::MAX`, the most expensive answer, never a small one.
    #[test]
    fn the_curve_saturates_rather_than_wrapping() {
        assert_eq!(cost_curve(u32::MAX), u32::MAX);
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
            cost_curve(5),
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
}
