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

use crate::material::{Item, Material};
use crate::tunables::{COST_BASE, COST_GROWTH, RAW_PER_COMPRESSED};

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
}
