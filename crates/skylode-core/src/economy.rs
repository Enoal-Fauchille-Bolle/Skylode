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
use crate::mine_kind::MineKind;
use crate::pickaxe::{Pickaxe, PickaxeTier};
use crate::rng::Rng;
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
fn pay(inventory: &mut Inventory, cost: &Cost) -> Result<(), CoreError> {
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
    let next = tier.next().ok_or(CoreError::PickaxeFullyUpgraded)?;

    pay(inventory, &pickaxe_tier_cost(next))?;
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
pub fn buy_repeatedly(max_count: u32, mut buy_once: impl FnMut() -> Result<(), CoreError>) -> u32 {
    let mut bought = 0;
    while bought < max_count {
        if buy_once().is_err() {
            break;
        }
        bought += 1;
    }
    bought
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

    #[test]
    fn buying_efficiency_debits_and_raises_the_level() {
        let mut pickaxe = Pickaxe::default(); // Wooden, Efficiency 0
        let mut inventory = stocked(&[(Material::Stone, 100)]);

        assert_eq!(buy_pickaxe_efficiency(&mut inventory, &mut pickaxe), Ok(()));
        assert_eq!(efficiency_of(&pickaxe), 1);
        assert_eq!(
            inventory.count(Item::Raw(Material::Stone)),
            90,
            "cost_curve(0) = 10 Stone"
        );
    }

    /// The load-bearing guarantee, on the pickaxe: a refused buy leaves the level
    /// *and* the inventory exactly as they were.
    #[test]
    fn an_unaffordable_efficiency_buy_changes_nothing() {
        let mut pickaxe = Pickaxe::default();
        let mut inventory = stocked(&[(Material::Stone, 5)]); // needs 10
        let before = inventory.clone();

        assert_eq!(
            buy_pickaxe_efficiency(&mut inventory, &mut pickaxe),
            Err(CoreError::InsufficientItems {
                item: Item::Raw(Material::Stone),
                needed: 10,
                held: 5,
            })
        );
        assert_eq!(efficiency_of(&pickaxe), 0, "the level moved on a refusal");
        assert_eq!(inventory, before, "the inventory moved on a refusal");
    }

    /// At the cap the buy is refused *before* it debits — a capped enchant that
    /// still took the ore would be the silent hole the whole till is built to close.
    #[test]
    fn efficiency_refuses_at_the_cap_without_debiting() {
        let mut pickaxe = maxed_efficiency_at(PickaxeTier::Wooden); // Efficiency 5
        let mut inventory = stocked(&[(Material::Stone, 10_000)]);

        assert_eq!(
            buy_pickaxe_efficiency(&mut inventory, &mut pickaxe),
            Err(CoreError::EnchantAtCap {
                kind: EnchantType::Efficiency,
                cap: 5,
            })
        );
        assert_eq!(inventory.count(Item::Raw(Material::Stone)), 10_000);
    }

    /// A tier jump before Efficiency is maxed is refused and debits nothing — the
    /// rule that stops the player throwing away paid Efficiency.
    #[test]
    fn a_tier_jump_needs_efficiency_maxed_first() {
        let mut pickaxe = Pickaxe::default(); // Wooden, Efficiency 0
        let mut inventory = stocked(&[(Material::Coal, 10_000)]);

        assert_eq!(
            buy_pickaxe_tier(&mut inventory, &mut pickaxe),
            Err(CoreError::EfficiencyNotMaxed { current: 0, cap: 5 })
        );
        assert_eq!(pickaxe.get_tier(), PickaxeTier::Wooden);
        assert_eq!(inventory.count(Item::Raw(Material::Coal)), 10_000);
    }

    #[test]
    fn buying_a_tier_jump_advances_and_resets_efficiency() {
        let mut pickaxe = maxed_efficiency_at(PickaxeTier::Wooden); // Wooden, Efficiency 5
        let mut inventory = stocked(&[(Material::Coal, 100)]);

        assert_eq!(buy_pickaxe_tier(&mut inventory, &mut pickaxe), Ok(()));
        assert_eq!(pickaxe.get_tier(), PickaxeTier::Stone);
        assert_eq!(efficiency_of(&pickaxe), 0, "a tier jump resets Efficiency");
        assert_eq!(
            inventory.count(Item::Raw(Material::Coal)),
            88,
            "reaching Stone costs cost_curve(1) = 12 Coal"
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

    #[test]
    fn buying_a_special_enchant_debits_all_its_lines() {
        let mut pickaxe = Pickaxe::default();
        // Fortune level 1 in the Overworld: 10 Emerald + 3 Coal (fuel).
        let mut inventory = stocked(&[(Material::Emerald, 100), (Material::Coal, 100)]);

        assert_eq!(
            buy_enchant(
                &mut inventory,
                &mut pickaxe,
                EnchantType::Fortune,
                World::Overworld
            ),
            Ok(())
        );
        assert_eq!(pickaxe.enchants().get_level(EnchantType::Fortune), 1);
        assert_eq!(inventory.count(Item::Raw(Material::Emerald)), 90);
        assert_eq!(inventory.count(Item::Raw(Material::Coal)), 97);
    }

    /// The multi-line partial-debit guard, stated as a test: short on the *second*
    /// line, and the *first* is not debited either. If this ever fails, `pay` has
    /// started debiting before it finished checking.
    #[test]
    fn a_special_enchant_short_on_one_line_debits_neither() {
        let mut pickaxe = Pickaxe::default();
        // Enough Emerald, but only 2 Coal where the fuel line needs 3.
        let mut inventory = stocked(&[(Material::Emerald, 100), (Material::Coal, 2)]);
        let before = inventory.clone();

        assert!(
            buy_enchant(
                &mut inventory,
                &mut pickaxe,
                EnchantType::Fortune,
                World::Overworld
            )
            .is_err()
        );
        assert_eq!(pickaxe.enchants().get_level(EnchantType::Fortune), 0);
        assert_eq!(
            inventory, before,
            "the Emerald line was debited before the Coal line failed"
        );
    }

    #[test]
    fn a_special_enchant_refuses_at_its_world_cap() {
        // Explosive caps at 3 in the Overworld.
        let mut pickaxe = Pickaxe::default();
        let mut inventory = stocked(&[(Material::Lapis, 10_000), (Material::Coal, 10_000)]);

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
        let mut inventory = stocked(&[(Material::Stone, 100)]);

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
            inventory.count(Item::Raw(Material::Stone)),
            90,
            "Efficiency is priced in the tier material, not an enchant material"
        );
    }

    #[test]
    fn buying_mine_size_grows_the_grid_and_debits() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        let mut inventory = stocked(&[(Material::Iron, 100)]);
        let before_level = mine.get_size_level();

        assert_eq!(buy_mine_size(&mut inventory, &mut mine, &mut rng()), Ok(()));
        assert_eq!(mine.get_size_level(), before_level + 1);
        assert_eq!(inventory.count(Item::Raw(Material::Iron)), 90);
    }

    /// Buying a richness level raises the ceiling and *only* the ceiling: the dial,
    /// stuck at 0 before, can now be pushed to the new rung.
    #[test]
    fn buying_mine_richness_raises_the_ceiling_and_unlocks_the_dial() {
        let mut mine = Mine::new(MineKind::Amethyst, &mut rng());
        let mut inventory = stocked(&[(Material::Endstone, 100)]);
        assert_eq!(mine.get_richness_level(), 0);
        assert!(
            mine.set_richness_setting(1, &mut rng()).is_err(),
            "the dial cannot exceed a ceiling of 0 yet"
        );

        assert_eq!(buy_mine_richness(&mut inventory, &mut mine), Ok(()));
        assert_eq!(mine.get_richness_level(), 1);
        assert_eq!(inventory.count(Item::Raw(Material::Endstone)), 90);
        assert!(
            mine.set_richness_setting(1, &mut rng()).is_ok(),
            "the dial reaches the freshly bought ceiling"
        );
    }

    /// The reason Efficiency and the tier jump are separate purchases: buy-max on
    /// Efficiency stops at the cap instead of rolling on into a tier jump.
    #[test]
    fn buy_max_efficiency_stops_at_the_tier_cap() {
        let mut pickaxe = Pickaxe::default(); // Wooden, cap 5
        let mut inventory = stocked(&[(Material::Stone, 10_000)]);

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
        // cost_curve(0) + cost_curve(1) = 10 + 12 = 22; a third would be 13 more.
        let mut inventory = stocked(&[(Material::Stone, 25)]);

        let bought = buy_repeatedly(10, || buy_pickaxe_efficiency(&mut inventory, &mut pickaxe));

        assert_eq!(bought, 2, "only two levels are affordable");
        assert_eq!(efficiency_of(&pickaxe), 2);
        assert_eq!(inventory.count(Item::Raw(Material::Stone)), 3);
    }

    #[test]
    fn can_afford_reads_every_line_strictly() {
        let cost = enchant_cost(EnchantType::Fortune, 0, World::Overworld); // 10 Emerald + 3 Coal

        let short = stocked(&[(Material::Emerald, 100), (Material::Coal, 2)]);
        assert_eq!(cost.as_ref().map(|c| can_afford(&short, c)), Some(false));

        let enough = stocked(&[(Material::Emerald, 100), (Material::Coal, 100)]);
        assert_eq!(cost.as_ref().map(|c| can_afford(&enough, c)), Some(true));
    }
}
