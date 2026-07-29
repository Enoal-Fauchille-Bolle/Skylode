//! Pickaxe enchantments.
//!
//! Enchantments modify how a [`Pickaxe`](crate::pickaxe::Pickaxe) mines. This
//! module provides:
//! - [`EnchantType`]: the kind of enchantment (Efficiency, Fortune, …) plus the
//!   dispatch to its level cap, which is keyed by the pickaxe tier, by the world,
//!   or by nothing, depending on the enchant — and, for the three spatial ones,
//!   the dispatch to the *shape* a proc breaks
//!   ([`blast_cells`](EnchantType::blast_cells)).
//! - [`Enchants`]: a compact per-pickaxe store mapping each active enchantment
//!   to its current level.
//! - [`Enchant`]: a standalone `(type, level)` pair used when an enchantment
//!   needs to be passed around on its own.

use crate::error::CoreError;
use crate::material::{Item, Material};
use crate::pickaxe::PickaxeTier;
use crate::rng::Rng;
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The radius of the smallest [`Explosive`](EnchantType::Explosive) square: a
/// 3x3 around the impact.
///
/// Non-zero, so the enchant is worth something the moment it is installed. A
/// radius of 0 would be a level the player paid for that breaks the one cell the
/// swing had already broken.
const EXPLOSIVE_RADIUS_MIN: u8 = 1;

/// The radius of the largest Explosive square: a 7x7, or 49 of a full mine's 200
/// cells.
///
/// The ceiling exists to keep Explosive and [`Nuke`](EnchantType::Nuke) distinct.
/// Nuke's whole point is clearing the *grid*; an Explosive allowed to grow with
/// every level would reach that on its own and leave Nuke buying nothing but a
/// different proc rate. Under a quarter of the grid is the gap that keeps them two
/// enchants.
const EXPLOSIVE_RADIUS_MAX: u8 = 3;

/// How many enchant levels buy one extra ring on the Explosive square.
///
/// Three, and that is **not** an arbitrary step: it lines the radius bands up with
/// [`World::enchant_cap`] (3 / 6 / 10), so each dimension owns exactly one square
/// size — 3x3 in the Overworld, 5x5 in the Nether, 7x7 in the End. A player who
/// reaches a new world sees the blast visibly grow, and a 7x7 is proof of the End
/// rather than of patience. Changing this number breaks that alignment silently,
/// which is what `explosive_bands_line_up_with_the_world_caps` is there to catch.
///
/// [`World::enchant_cap`]: crate::world::World::enchant_cap
const EXPLOSIVE_RADIUS_BAND: u8 = 3;

/// The Chebyshev radius of [`Explosive`](EnchantType::Explosive)'s square at
/// `level`: 1, 2 or 3, in bands of [`EXPLOSIVE_RADIUS_BAND`].
///
/// A **formula plus a clamp**, not a table, for the reason `mine::value_weight` is
/// one: this is a single monotone one-dimensional curve, and a table would invite
/// the bands to drift out of step with the world caps they mirror.
///
/// `saturating_sub` rather than `level - 1` because `level` is a `u8` and level 0 is
/// reachable — an enchant the player has never bought reads as 0 through
/// [`Enchants::get_level`]. On unsigned arithmetic `0 - 1` is not a small negative
/// number, it is a **panic in debug and a wrap to 255 in release**, and the release
/// half is the dangerous one: a wrapped level would land in the top band and hand
/// out a 7x7 to a player with no Explosive at all. Callers are expected to skip
/// level 0 entirely (see [`blast_cells`](EnchantType::blast_cells)); this makes the
/// function safe even if one forgets.
fn explosive_radius(level: u8) -> u8 {
    let band = level.saturating_sub(1) / EXPLOSIVE_RADIUS_BAND;
    EXPLOSIVE_RADIUS_MIN + band.min(EXPLOSIVE_RADIUS_MAX - EXPLOSIVE_RADIUS_MIN)
}

/// How many levels the proc ramp spans: from level 1 to level 10, the highest
/// [`World::enchant_cap`] in the game.
///
/// The divisor of the ramp in [`proc_permille`](EnchantType::proc_permille), and
/// the reason that method clamps rather than trusts its argument — a level past the
/// End's cap would otherwise ramp *past* the quoted ceiling.
///
/// [`World::enchant_cap`]: crate::world::World::enchant_cap
const PROC_RAMP_SPAN: u32 = 9;

/// The order enchant procs are rolled in, and therefore the order they consume the
/// generator.
///
/// **This is a reproducibility contract, not a list.** A save stores a position in
/// the PRNG sequence, so which enchant draws first decides what every later draw
/// in the run returns. Reorder these four and every existing save quietly
/// continues on different dice — nothing fails, which is exactly the problem.
/// `the_proc_order_follows_the_declaration_order` is what turns that silence into a
/// failing test.
///
/// The order is the enum's own declaration order. Rust cannot iterate a plain enum,
/// so it has to be written down; anchoring it to the declaration means there is one
/// obvious answer to "where does a new enchant go" rather than a convention to
/// remember.
///
/// [`Excavator`](EnchantType::Excavator) comes **last**, and that placement is what
/// let it land without disturbing a single existing run: a level-0 enchant is skipped
/// before it draws, so a player who never bought it consumes the same sequence as
/// before it existed. Appending rather than inserting is the general rule a future
/// enchant should follow for the same reason.
///
/// **Nothing iterates this, and that is the cost of splitting the resolution.** The
/// spatials loop over [`SPATIAL_PROC_ORDER`] inside [`Mine`]; the Excavator resolves
/// on its own in [`resolve_excavator`](Enchants::resolve_excavator). So this constant
/// is no longer the code that *produces* the order — it is the statement of what the
/// order must be, which the tests hold the two halves to. Deleting it as unused would
/// delete the only place the whole sequence is written down.
///
/// [`Mine`]: crate::mine::Mine
// Dead outside the tests by construction, per the paragraph above.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the contract the two resolutions are tested against"
    )
)]
pub(crate) const PROC_ORDER: [EnchantType; 4] = [
    EnchantType::Explosive,
    EnchantType::Jackhammer,
    EnchantType::Nuke,
    EnchantType::Excavator,
];

/// The enchants [`Mine::resolve_spatial_procs`] rolls: a **prefix** of
/// [`PROC_ORDER`], and the prefix is the whole point.
///
/// The four triggered enchants do not resolve in one place. Three of them reshape
/// the grid and belong to [`Mine`]; [`Excavator`](EnchantType::Excavator) substitutes
/// a *drop* and touches no cell, so it resolves here in `enchant` through
/// [`resolve_excavator`](Enchants::resolve_excavator) — putting it on `Mine` would
/// hand the grid a say in the inventory it has no business having.
///
/// Splitting the resolution splits the draw, though, and the draw order is the
/// contract [`PROC_ORDER`] exists to state. So the ordering that used to be
/// guaranteed by one loop is now a promise between two functions: the spatials draw
/// first, the Excavator draws after. `spatial_proc_order_is_a_prefix_of_proc_order`
/// is what turns that promise into something the compiler runs — being a prefix is
/// exactly the statement "the spatials come first, and nothing was skipped".
///
/// [`Mine`]: crate::mine::Mine
/// [`Mine::resolve_spatial_procs`]: crate::mine::Mine
pub(crate) const SPATIAL_PROC_ORDER: [EnchantType; 3] = [
    EnchantType::Explosive,
    EnchantType::Jackhammer,
    EnchantType::Nuke,
];

/// Every cell of the inclusive rectangle `x0..=x1` by `y0..=y1`, row by row.
///
/// Shared by the shapes rather than written out at each arm, so "which rectangle"
/// and "walk a rectangle" stay separate questions. An inverted range (`x0 > x1`)
/// yields nothing, which is what makes an impact off the grid answer with an empty
/// blast instead of a bad index.
fn rect_cells(x0: u8, x1: u8, y0: u8, y1: u8) -> Vec<(u8, u8)> {
    (y0..=y1)
        .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
        .collect()
}

/// A single enchantment together with its level.
///
/// This is a detached value type; the enchantments actually installed on a
/// pickaxe live in [`Enchants`], which stores them more compactly as a map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enchant {
    /// The kind of enchantment (Efficiency, Fortune, …).
    enchant_type: EnchantType,
    /// The level of the enchantment, which must not exceed
    level: u8,
}

/// The kinds of enchantment a pickaxe can have.
///
/// Derives [`Ord`]/[`Eq`] so it can be used as a [`BTreeMap`] key inside
/// [`Enchants`]. Each variant's effective level cap comes from
/// [`max_level`](EnchantType::max_level).
///
/// The seven split into three groups by **what their cap is keyed on**, and that
/// split is the shape of the whole module:
///
/// - [`Efficiency`](EnchantType::Efficiency) is keyed by the **pickaxe tier**
///   ([`PickaxeTier::efficiency_cap`]).
/// - The other six — [`Fortune`](EnchantType::Fortune) and the five *specials* — are
///   keyed by the **world** ([`World::enchant_cap`]). All six are on sale from the
///   Overworld onwards; a new dimension unlocks none of them and only raises their
///   shared ceiling.
///
/// That is also why the two progression axes stay independent: the tier moves
/// Efficiency, the mining level moves the world and with it the other six, and
/// neither axis alone advances the player.
///
/// **Fortune was once a third group, keyed by nothing**, capped at a flat 10. Its
/// ceiling is still 10 — that is the world's own top cap — but it is now *reached*
/// in three steps rather than one. Keyed by nothing, Fortune was the single upgrade
/// in the game that a level-1 player could max, which made the one lever no
/// progression paced. What the third group actually protected was Efficiency, and
/// Efficiency still has it.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EnchantType {
    /// Increases mining speed, by `level² + 1` added to the pickaxe's base power.
    /// Capped by the pickaxe tier: 5, or 15 at Netherite.
    Efficiency,
    /// Increases the drop rate of ores, multiplying the loot by `1 + level`.
    /// Capped by the world, like the five specials: 3, 6, then 10.
    Fortune,
    /// On a proc, clears a **Chebyshev square** centred on the impact cell. The
    /// only spatial enchant whose level buys area as well as frequency, in three
    /// bands aligned with the world caps: 3x3, 5x5, then 7x7. See
    /// [`blast_cells`](EnchantType::blast_cells).
    /// Capped by the world; see [`World::enchant_cap`].
    Explosive,
    /// On a proc, clears **one full-width row** — the mine's whole width, so it is
    /// *mine size* rather than the enchant's level that scales its reach. Always a
    /// single row: a multi-row band would blur it into
    /// [`Explosive`](EnchantType::Explosive)'s square.
    /// Capped by the world; see [`World::enchant_cap`].
    Jackhammer,
    /// On a proc, clears the **whole grid**, from wherever the impact landed. Its
    /// geometry never changes with level — there is no area past "all of it" — so
    /// the level buys frequency alone.
    ///
    /// **No cooldown.** Emptying the mine is its own limiter: a re-proc finds
    /// nothing standing until the batch reset refills the grid.
    /// Capped by the world; see [`World::enchant_cap`].
    Nuke,
    /// On a proc, substitutes one [`Compressed`] unit of the mined material for the
    /// block's whole raw drop. The one triggered enchant that changes the *loot*
    /// rather than the grid; see
    /// [`resolve_excavator`](Enchants::resolve_excavator).
    ///
    /// The only thing in the game that mints a Compressed unit without paying its
    /// 100 raw, which is what makes it a windfall rather than a prettier drop. It
    /// substitutes the drop *after* it leaves the block, so no block gains a
    /// Compressed unit of its own — see [`Block::drops`].
    ///
    /// **Fortune does not multiply it**, and that is a balance decision rather than
    /// an oversight. `substitutes` is meant at full strength: the proc replaces the
    /// loot, it does not join it. Composing the two would put the game's rarest
    /// burst under its largest multiplier — 11 Compressed, 1100 raw, from one swing
    /// at the caps — and a windfall that swings by a factor of eleven stops being
    /// legible to the player and starts dominating every balance number around it.
    /// A flat 100 is the whole of what a proc is worth, whatever else the pickaxe
    /// carries.
    ///
    /// **Compressed of the mined material, and nothing else.** Earlier drafts
    /// offered "a Compressed unit *or* an Emerald"; the Emerald branch is dropped.
    /// It made sense when Emerald read as a premium currency, and in Skylode it is
    /// one Overworld material among eight — so on almost every mine it would have
    /// been the strictly worse half of a coin flip, and the player would have
    /// suffered the outcome rather than understood it.
    /// Capped by the world; see [`World::enchant_cap`].
    ///
    /// [`Compressed`]: crate::material::Item::Compressed
    /// [`Block::drops`]: crate::block::Block::drops
    Excavator,
    /// Multiplies mining speed permanently, by
    /// `1 + HASTE_PER_LEVEL * level` — linear, where
    /// [`Efficiency`](EnchantType::Efficiency) is quadratic, so the two compound
    /// instead of racing. See [`HASTE_PER_LEVEL`](crate::tunables::HASTE_PER_LEVEL).
    /// Capped by the world; see [`World::enchant_cap`].
    Haste,
}

/// The set of enchantments installed on a pickaxe.
///
/// Stored as a sparse map: an enchantment absent from the map is treated as
/// level 0, so only active enchantments consume memory. The `levels` field is
/// private — callers go through the methods below to keep the "absent == 0"
/// invariant intact.
///
/// A [`BTreeMap`], for [`Inventory`](crate::inventory::Inventory)'s reason: its
/// order is what a save is written in, and an unspecified one would make the same
/// pickaxe serialise differently on every write. The order it sorts by is
/// [`EnchantType`]'s declaration order, which is not
/// [`PROC_ORDER`] — nothing here reads the map to
/// decide who rolls first, and nothing should.
///
/// `transparent` for [`Inventory`](crate::inventory::Inventory)'s reason: a save
/// writes `{"Fortune": 3}`, not a wrapper around a private field name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Enchants {
    levels: BTreeMap<EnchantType, u8>,
}

impl Enchant {
    /// Constructs a standalone `(type, level)` enchantment pair.
    pub fn new(enchant_type: EnchantType, level: u8) -> Self {
        Self {
            enchant_type,
            level,
        }
    }
}

impl EnchantType {
    /// Every enchant, in declaration order.
    ///
    /// **Public for the reason [`MineKind::ALL`](crate::mine_kind::MineKind::ALL) is:
    /// a front-end has to *list* them and an enum cannot enumerate itself.** The
    /// Upgrades screen's Enchants sub-tab draws one row per track including the ones
    /// the player owns nothing of (`docs/UI.md` §5.4.1 prints `Nuke 0 → I`), so
    /// [`Enchants::iter`] cannot serve it — that iterator yields only the levels a
    /// pickaxe actually has, and the rows would appear as they were bought.
    ///
    /// **All seven, [`Efficiency`](EnchantType::Efficiency) included, and the screen
    /// filters rather than this constant.** Efficiency is not sold in the enchant
    /// shop — it is a pickaxe upgrade priced on the tier ladder — but that is already
    /// stated once, by [`enchant_cost`](crate::economy::enchant_cost) answering
    /// [`None`] for it. A six-entry constant here would be a second statement of the
    /// same rule, free to disagree with the first. Filtering on the price leaves the
    /// six in the frame's own order (Fortune, Explosive, Jackhammer, Nuke, Excavator,
    /// Haste), which is a property of the declaration order and worth not disturbing.
    ///
    /// Distinct from [`PROC_ORDER`], which is a *reproducibility contract* over the
    /// four triggered enchants and must never be reordered. This one is a display
    /// list; they agree today only because both follow the declaration.
    ///
    /// An array and not a slice, so the length is in the type.
    /// `all_enchants_covers_every_variant` is what catches a variant added to the
    /// enum and forgotten here, since nothing in the language ties the two together.
    pub const ALL: [Self; 7] = [
        Self::Efficiency,
        Self::Fortune,
        Self::Explosive,
        Self::Jackhammer,
        Self::Nuke,
        Self::Excavator,
        Self::Haste,
    ];

    /// Returns the human-readable display name of the enchantment.
    pub fn name(self) -> &'static str {
        match self {
            Self::Efficiency => "Efficiency",
            Self::Fortune => "Fortune",
            Self::Explosive => "Explosive",
            Self::Jackhammer => "Jackhammer",
            Self::Nuke => "Nuke",
            Self::Excavator => "Excavator",
            Self::Haste => "Haste",
        }
    }

    /// Returns the maximum level this enchantment can reach.
    ///
    /// The generic front door to a cap: **pure dispatch, holding no number of its
    /// own**. Each of the two groups above keeps the single source of its own
    /// ceiling — [`PickaxeTier::efficiency_cap`] and [`World::enchant_cap`] — and this
    /// method only picks which one applies. A table here would fork that knowledge in
    /// two and let the copies drift.
    ///
    /// Both arguments are **required, and neither is an `Option`**. Every enchant
    /// ignores at least one of them, so it is tempting to let a caller skip the one
    /// it believes irrelevant; a defaulted cap, though, is not refused but *wrong
    /// and plausible*. The paid path (phase 5) would debit Amethyst and hand back a
    /// level ceilinged at the Overworld's, with nothing to say so — the same silent
    /// failure `Enchants::upgrade` returns [`CoreError::EnchantAtCap`] to avoid. In
    /// play there is always a tier and always a world, so requiring both costs a
    /// caller nothing and makes the omission a compile error.
    ///
    /// `world` is the **highest world the player has unlocked**, not the one whose
    /// mine they happen to be standing in: reaching a dimension raises the ceiling
    /// for good, so a pickaxe never weakens by going back to farm an early mine.
    /// Phase 6 owns that set and will pass it.
    pub fn max_level(self, pickaxe_tier: PickaxeTier, world: World) -> u8 {
        match self {
            Self::Efficiency => pickaxe_tier.efficiency_cap(),
            Self::Fortune
            | Self::Explosive
            | Self::Jackhammer
            | Self::Nuke
            | Self::Excavator
            | Self::Haste => world.enchant_cap(),
        }
    }

    /// How often this enchant procs at `level`, in **permille** — 0 for the
    /// enchants that do not proc, and for level 0.
    ///
    /// A linear ramp from the level-1 rate to the level-10 one:
    /// `first + (last - first) * (level - 1) / 9`. Frequency is the axis every
    /// triggered enchant scales on, and for [`Jackhammer`](EnchantType::Jackhammer)
    /// and [`Nuke`](EnchantType::Nuke) it is the *only* one — their shapes never
    /// change, so this method is the whole of what their levels buy.
    ///
    /// **Permille, and integer, on purpose.** The roll is a `u32` comparison in
    /// [`Rng::chance_permille`], never a float: the proc sequence is state a save
    /// resumes, and a run must not depend on how a division rounded. It is also
    /// why the rates are quoted as whole permille here rather than as
    /// probabilities.
    ///
    /// Nuke starts at **1**, which is *double* the 0.5 permille the balance sketch
    /// asked for — 0.5 has no representation in whole permille, and the choice was
    /// to round it up rather than move the whole game to ten-thousandths for one
    /// enchant's opening rate. Worth knowing at the balance pass: if Nuke proves too
    /// frequent early, the fix is the unit, not the curve.
    ///
    /// [`Excavator`](EnchantType::Excavator) sits low for a different reason than
    /// Nuke does. Nuke is throttled because it clears a 200-cell grid; the Excavator
    /// breaks nothing at all, and is throttled because of what it *pays*. A proc
    /// mints 100 raw where the ore cell it replaced was worth 1 — so frequency is
    /// not one lever among several here, it is the entire power budget, and the
    /// curve is the only thing standing between a windfall and an income. Its
    /// ceiling is deliberately below the two spatials': they hand out cells the
    /// player still has to have mined, this hands out a denomination nothing else in
    /// the game produces without paying for it.
    ///
    /// These live here rather than in [`tunables`](crate::tunables) under that
    /// module's second rule — a value that is *one per variant* is a `match` in the
    /// enum's own module, which is also the only shape that turns a new variant into
    /// a compile error instead of a silent default. All the numbers are
    /// provisional; phase 10 balance sets the final ones, and the *shape* — a linear
    /// ramp bounded at both ends — is what is settled.
    ///
    /// **`pub`, unlike the two methods that *apply* an enchant.**
    /// [`blast_cells`](EnchantType::blast_cells) and
    /// [`Enchants::upgrade`](Enchants::upgrade) are `pub(crate)` because they change a
    /// run; this one only answers a question, and it is a question the Upgrades pane
    /// has to ask about a level the player has *not* bought — `4.0% → 6.0%` is the
    /// whole of what a Jackhammer or Nuke level sells, so a front-end unable to read
    /// the curve could only offer the purchase blind. A pure function of a `u8`
    /// guards nothing by being hidden.
    ///
    /// [`Rng::chance_permille`]: crate::rng::Rng
    pub fn proc_permille(self, level: u8) -> u32 {
        let (first, last) = match self {
            Self::Explosive => (20, 200),
            Self::Jackhammer => (15, 150),
            Self::Nuke => (1, 10),
            Self::Excavator => (5, 50),
            // Efficiency, Fortune and Haste are passive multipliers: they are always
            // on, so there is nothing for them to roll.
            Self::Efficiency | Self::Fortune | Self::Haste => return 0,
        };

        if level == 0 {
            return 0;
        }
        // Clamped, not trusted: a level past the End's cap — from a hand-edited
        // save — would otherwise ramp past the quoted ceiling instead of resting
        // on it.
        let step = u32::from(level - 1).min(PROC_RAMP_SPAN);
        first + (last - first) * step / PROC_RAMP_SPAN
    }

    /// The side of [`Explosive`](EnchantType::Explosive)'s square at `level` — 3, 5 or
    /// 7 — and `0` for every other enchant, which breaks no square at all.
    ///
    /// **The one geometric number a front-end may read, and it is `pub` for the reason
    /// `docs/UI.md` §5.4.1 gives in full**: the square grows only every
    /// [`EXPLOSIVE_RADIUS_BAND`] levels, so a pane that printed `5x5` on the step from
    /// II to III would promise a reward the core does not pay. That paragraph is a
    /// warning about *transcribing* the curve, and a front-end handed no way to ask has
    /// no other option — the two copies of `1 + 2 * (1 + (level - 1) / 3).min(3)` that
    /// grew in `skylode-tui`'s read model are what this method deletes.
    ///
    /// Diameter and not the [`explosive_radius`] behind it, because the radius is an
    /// internal convenience — [`blast_cells`](EnchantType::blast_cells) needs a
    /// Chebyshev reach to clip against the grid edges, a player reads `7x7`. Keeping
    /// the radius private is what stops a caller from re-deriving the side and getting
    /// the `2r + 1` wrong.
    ///
    /// `0` rather than [`Option`] for the six enchants with no square: the caller is a
    /// pane that draws a row per moving stat, and "no square" is a row it does not
    /// draw. A `None` would say the same thing one `match` later.
    ///
    /// Level 0 answers `3`, not `0` — the same band-one square
    /// [`explosive_radius`] clamps to. That is deliberate and it is what the pane
    /// wants: a player at Explosive 0 is being shown what level I would buy, and
    /// `blast_cells` refuses level 0 on its own, before any geometry is asked for.
    pub fn explosive_side(self, level: u8) -> u8 {
        match self {
            Self::Explosive => 2 * explosive_radius(level) + 1,
            Self::Efficiency
            | Self::Fortune
            | Self::Jackhammer
            | Self::Nuke
            | Self::Excavator
            | Self::Haste => 0,
        }
    }

    /// The cells a proc of this enchant breaks, radiating from the `impact` cell in
    /// a mine of `size` — empty for the enchants that break nothing.
    ///
    /// **Dispatch over the shapes**, the same way [`max_level`](EnchantType::max_level)
    /// is dispatch over the caps: each spatial enchant states its own geometry and
    /// this method only picks which one applies. The three are deliberately
    /// different *dimensions* of shape, which is what keeps them worth buying
    /// separately — [`Explosive`](EnchantType::Explosive) a 2-D square that grows
    /// with its level, [`Jackhammer`](EnchantType::Jackhammer) a 1-D stripe that
    /// grows with the *mine*, [`Nuke`](EnchantType::Nuke) the whole grid at any
    /// level.
    ///
    /// Every coordinate returned is **inside the grid**. The clipping happens here
    /// rather than being left to [`Mine::take`], and the reason is arithmetic, not
    /// taste: coordinates are `u8`, so an Explosive centred on `x = 0` computes
    /// `0 - 1`, which on an unsigned integer **panics in debug and wraps to 255 in
    /// release**. `take` absorbs an out-of-grid coordinate gracefully, but it never
    /// gets the chance — the subtraction blows up first. `saturating_sub` on the
    /// near edges and `min(last)` on the far ones is what makes a blast at a corner
    /// a smaller blast instead of a crash.
    ///
    /// Level 0 breaks nothing. An enchant the player never bought reads as level 0
    /// through [`get_level`](Enchants::get_level), so this is the difference between
    /// "not installed" and "installed and useless" — and callers skip level 0
    /// before drawing anyway, so no proc is ever rolled for it.
    ///
    /// Returns an owned [`Vec`] rather than an iterator because the four arms have
    /// four different iterator types, and unifying them would mean a `Box<dyn
    /// Iterator>` — a heap allocation and a virtual call to avoid an allocation. A
    /// blast is at most [`capacity`](crate::mine::Mine::capacity) = 200 pairs and
    /// happens on a rare proc, not every tick.
    ///
    /// [`Mine::take`]: crate::mine::Mine
    pub(crate) fn blast_cells(self, level: u8, impact: (u8, u8), size: (u8, u8)) -> Vec<(u8, u8)> {
        let (width, height) = size;
        if level == 0 || width == 0 || height == 0 {
            return Vec::new();
        }
        let (last_x, last_y) = (width - 1, height - 1);
        let (impact_x, impact_y) = impact;

        match self {
            Self::Explosive => {
                let radius = explosive_radius(level);
                rect_cells(
                    impact_x.saturating_sub(radius),
                    impact_x.saturating_add(radius).min(last_x),
                    impact_y.saturating_sub(radius),
                    impact_y.saturating_add(radius).min(last_y),
                )
            }
            // The row spans the mine's whole width, so *mine size* is what scales
            // its reach — the level only buys a better proc chance.
            Self::Jackhammer if impact_y <= last_y => rect_cells(0, last_x, impact_y, impact_y),
            Self::Jackhammer => Vec::new(),
            // Level-independent by design: Nuke's level buys frequency, never area,
            // and there is no area past "all of it" to buy.
            Self::Nuke => rect_cells(0, last_x, 0, last_y),
            // The non-spatial enchants: two passive multipliers and one that
            // substitutes a drop. None of them touches the grid.
            Self::Efficiency | Self::Fortune | Self::Excavator | Self::Haste => Vec::new(),
        }
    }
}

impl Enchants {
    /// Creates a new instance of [`Enchants`].
    pub fn new() -> Self {
        Self {
            levels: BTreeMap::new(),
        }
    }

    /// Gets the level of the specified enchantment type.
    /// Returns 0 if the enchantment is not present.
    pub fn get_level(&self, kind: EnchantType) -> u8 {
        self.levels.get(&kind).copied().unwrap_or(0)
    }

    /// Increases the level of the specified enchantment by 1, up to its cap.
    ///
    /// Absent enchantments start from 0, so the first call installs them at
    /// level 1. Calls beyond [`max_level`](EnchantType::max_level) are
    /// **refused**, not quietly dropped: the cap is enforced here rather than
    /// left to each caller, so no code path can hand the player a level the game
    /// has no rules for — and the paid path can tell "bought a level" from "paid
    /// for nothing".
    ///
    /// Both `pickaxe_tier` and `world` are needed because the cap is keyed by one
    /// or the other depending on `kind`; see
    /// [`max_level`](EnchantType::max_level), which this defers to.
    ///
    /// `pub(crate)` for the same reason as
    /// [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe::upgrade): it is free. An
    /// enchant level is bought with the world's enchant material plus a mix of
    /// earlier mines' ore, and that is checked by the paid path in
    /// [`economy`](crate::economy), which reaches this through
    /// [`Pickaxe::upgrade_enchant`](crate::pickaxe::Pickaxe::upgrade_enchant) — the
    /// door that has a world to hand over, unlike
    /// [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe::upgrade)'s Efficiency route.
    pub(crate) fn upgrade(
        &mut self,
        kind: EnchantType,
        pickaxe_tier: PickaxeTier,
        world: World,
    ) -> Result<(), CoreError> {
        self.upgrade_to_cap(kind, kind.max_level(pickaxe_tier, world))
    }

    /// Increases [`Efficiency`](EnchantType::Efficiency) by 1, up to the cap its
    /// `tier` allows.
    ///
    /// The world-free door into [`upgrade`](Enchants::upgrade), and the only one
    /// [`Pickaxe::upgrade`](crate::pickaxe::Pickaxe::upgrade) needs: Efficiency is
    /// keyed by the tier alone, and a `Pickaxe` knows no world. Routing it through
    /// the generic path would mean threading a [`World`] the length of the tier
    /// ladder for a value provably never read — and the only way to supply one
    /// would be to invent it at the call site, which is how a cap stops meaning
    /// anything.
    ///
    /// Enforces the ceiling exactly as `upgrade` does; both delegate to the same
    /// [`upgrade_to_cap`](Enchants::upgrade_to_cap), so neither can drift into
    /// being the lenient one.
    pub(crate) fn upgrade_efficiency(&mut self, tier: PickaxeTier) -> Result<(), CoreError> {
        self.upgrade_to_cap(EnchantType::Efficiency, tier.efficiency_cap())
    }

    /// Rolls [`Excavator`](EnchantType::Excavator) against a break of `material`,
    /// returning the [`Compressed`] unit it mints — or `None` on the far commoner
    /// outcome, which is that the block's own drop stands.
    ///
    /// **The whole drop, replaced.** A `Some` is not a bonus laid beside the loot;
    /// it *is* the loot for that break. The caller applies one or the other, never
    /// both, and never
    /// [Fortune](crate::pickaxe::Pickaxe::fortune_multiplier) on top — see the variant's docs
    /// for why the two rarest levers in the game are kept from composing.
    ///
    /// **Rolled once per swing, on the impact block**, which is the caller's part of
    /// the contract and not something this signature can enforce. A maxed Nuke drops
    /// two hundred cells in a tick; rolling each of them would make the number of
    /// draws per swing depend on a blast's geometry, and a PRNG sequence whose shape
    /// varies with the grid is one no golden vector can pin and no bug report can
    /// reproduce.
    ///
    /// **Draws only if the enchant is owned.** Level 0 returns before touching
    /// `rng`, the same discipline
    /// [`Mine::resolve_spatial_procs`](crate::mine::Mine) follows — and here it is
    /// what made this enchant shippable at all. Appending to [`PROC_ORDER`] can only
    /// disturb a run that reaches the new draw, and a player who never bought the
    /// Excavator never does, so every save written before it existed replays on
    /// exactly the dice it was written with.
    ///
    /// Takes a [`Material`] rather than the [`Block`] that fell: the substitution
    /// depends on the matter and on nothing else — not hardness, not tier, not the
    /// world — so the narrower argument is both the honest one and the one that
    /// keeps this module from having to know what a block is. The tick passes
    /// `block.material()`.
    ///
    /// [`Compressed`]: crate::material::Item::Compressed
    /// [`Block`]: crate::block::Block
    pub(crate) fn resolve_excavator(&self, material: Material, rng: &mut Rng) -> Option<Item> {
        let level = self.get_level(EnchantType::Excavator);
        if level == 0 {
            return None;
        }

        rng.chance_permille(EnchantType::Excavator.proc_permille(level))
            .then_some(Item::Compressed(material))
    }

    /// Bumps `kind` by one level, refusing at `cap`.
    ///
    /// The single place the ceiling is actually applied. Both public doors resolve
    /// a cap and hand it here, so "which cap" and "enforce it" stay separate
    /// questions with one answer each.
    fn upgrade_to_cap(&mut self, kind: EnchantType, cap: u8) -> Result<(), CoreError> {
        let level = self.get_level(kind);
        if level >= cap {
            return Err(CoreError::EnchantAtCap { kind, cap });
        }
        self.levels.insert(kind, level + 1);
        Ok(())
    }

    /// Resets the level of the specified enchantment to 0.
    ///
    /// Removes the entry outright rather than storing a 0, which keeps the
    /// "absent == level 0" invariant true: [`iter`](Enchants::iter) must only
    /// ever yield enchantments the pickaxe actually has.
    ///
    /// `pub(crate)`: this exists to serve the tier jump, which cashes a maxed
    /// Efficiency in for the next tier. Wiping an enchant outside that trade is
    /// a pure loss to the player, and nothing outside the core should be able to
    /// inflict it.
    pub(crate) fn reset_level(&mut self, kind: EnchantType) {
        self.levels.remove(&kind);
    }

    /// Resets all enchantments to level 0.
    /// This will clear the internal map of enchantments.
    pub fn reset(&mut self) {
        self.levels.clear();
    }

    /// Returns an iterator over the enchantments and their levels.
    /// Each item in the iterator is a tuple of (EnchantType, level).
    pub fn iter(&self) -> impl Iterator<Item = (EnchantType, u8)> + '_ {
        self.levels.iter().map(|(&k, &v)| (k, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::world::ALL_WORLDS;

    /// The five *special* enchants: the ones whose cap is keyed by the world.
    ///
    /// Together with [`Efficiency`](EnchantType::Efficiency) (keyed by the tier)
    /// and [`Fortune`](EnchantType::Fortune) (keyed by nothing) this partitions
    /// [`EnchantType::ALL`], which `the_three_cap_groups_partition_every_enchant`
    /// checks — an enchant that fell out of all three would have a cap no test
    /// here ever looks at.
    const SPECIAL_ENCHANTS: [EnchantType; 5] = [
        EnchantType::Explosive,
        EnchantType::Jackhammer,
        EnchantType::Nuke,
        EnchantType::Excavator,
        EnchantType::Haste,
    ];

    /// A tier for tests whose subject is not the tier.
    ///
    /// Named rather than picked inline so the claim is visible: these tests assert
    /// something the tier does not change, and
    /// `the_five_special_enchants_and_fortune_ignore_the_pickaxe_tier` is what
    /// holds that claim up.
    const ANY_TIER: PickaxeTier = PickaxeTier::Wooden;

    /// A world for tests whose subject is not the world; the counterpart to
    /// [`ANY_TIER`], backed by `efficiency_and_fortune_ignore_the_world`.
    ///
    /// The End specifically, because it is the loosest: a test that needs a few
    /// levels of a special enchant must not trip over a ceiling it never meant to
    /// exercise.
    const ANY_WORLD: World = World::End;

    /// The one thing the compiler cannot check about [`EnchantType::ALL`]: that it
    /// still lists *every* variant. The `match`es in this module are exhaustive, so a
    /// new enchant already breaks the build — but nothing ties the enum to an array,
    /// and a variant missing from the list would simply never be drawn.
    #[test]
    fn all_enchants_covers_every_variant() {
        assert_eq!(
            EnchantType::ALL.len(),
            7,
            "an EnchantType variant was added or removed: update EnchantType::ALL"
        );
    }

    /// The Enchants sub-tab's six rows, in the order `docs/UI.md` §5.4.1 draws them,
    /// obtained the way a front-end obtains them: walk
    /// [`EnchantType::ALL`] and drop whatever the enchant shop does not price.
    ///
    /// Pinned here because [`EnchantType::ALL`]'s rustdoc makes the claim, and a
    /// rustdoc claim with nothing holding it up is the kind that goes on being read
    /// long after it stopped being true. It is also what would notice a *reordered*
    /// declaration — legal for this constant, unlike for `PROC_ORDER`, but not free:
    /// it would silently reshuffle a screen.
    #[test]
    fn dropping_what_the_shop_does_not_price_leaves_the_six_rows_in_frame_order() {
        let shop: Vec<EnchantType> = EnchantType::ALL
            .into_iter()
            .filter(|&kind| crate::economy::enchant_cost(kind, 0, ANY_WORLD).is_some())
            .collect();

        assert_eq!(
            shop,
            vec![
                EnchantType::Fortune,
                EnchantType::Explosive,
                EnchantType::Jackhammer,
                EnchantType::Nuke,
                EnchantType::Excavator,
                EnchantType::Haste,
            ],
            "the Enchants sub-tab would draw its rows in another order"
        );
    }

    /// Names are what the pickaxe screen shows, so a blank or duplicated one
    /// would leave the player unable to tell two enchantments apart.
    #[test]
    fn enchant_names_are_present_and_unique() {
        for (i, &a) in EnchantType::ALL.iter().enumerate() {
            assert!(!a.name().is_empty(), "{a:?} has no display name");
            for &b in &EnchantType::ALL[i + 1..] {
                assert_ne!(
                    a.name(),
                    b.name(),
                    "{a:?} and {b:?} share the display name {:?}",
                    a.name()
                );
            }
        }
    }

    /// `Enchant` and `Enchants` are two shapes of the same `(type, level)`
    /// fact — a detached pair on one side, a sparse map on the other. Anything
    /// that hands an `Enchant` around must be able to round-trip it through the
    /// set actually installed on a pickaxe.
    #[test]
    fn a_detached_enchant_round_trips_through_the_installed_set() {
        let detached = Enchant::new(EnchantType::Fortune, 3);
        assert_eq!(detached.enchant_type, EnchantType::Fortune);
        assert_eq!(detached.level, 3);

        let mut installed = Enchants::new();
        for _ in 0..detached.level {
            assert!(
                installed
                    .upgrade(detached.enchant_type, ANY_TIER, ANY_WORLD)
                    .is_ok()
            );
        }

        let read_back = Enchant::new(
            detached.enchant_type,
            installed.get_level(detached.enchant_type),
        );
        assert_eq!(read_back, detached);
    }

    #[test]
    fn an_absent_enchant_reads_as_level_zero() {
        // The core of the sparse-map design: nothing is stored until it is
        // earned, and callers still see a level for every enchant.
        assert_eq!(Enchants::new().get_level(EnchantType::Fortune), 0);
        assert_eq!(Enchants::new().iter().count(), 0);
    }

    #[test]
    fn upgrading_an_absent_enchant_installs_it_at_level_one() {
        let mut enchants = Enchants::new();
        assert!(
            enchants
                .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                .is_ok()
        );
        assert_eq!(enchants.get_level(EnchantType::Fortune), 1);
    }

    #[test]
    fn reset_level_leaves_the_other_enchants_alone() {
        let mut enchants = Enchants::new();
        assert!(
            enchants
                .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                .is_ok()
        );
        assert!(
            enchants
                .upgrade(EnchantType::Haste, ANY_TIER, ANY_WORLD)
                .is_ok()
        );

        enchants.reset_level(EnchantType::Fortune);

        assert_eq!(enchants.get_level(EnchantType::Fortune), 0);
        assert_eq!(enchants.get_level(EnchantType::Haste), 1);
    }

    #[test]
    fn reset_clears_every_enchant() {
        let mut enchants = Enchants::new();
        assert!(
            enchants
                .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                .is_ok()
        );
        assert!(
            enchants
                .upgrade(EnchantType::Haste, ANY_TIER, ANY_WORLD)
                .is_ok()
        );

        enchants.reset();

        assert_eq!(enchants.get_level(EnchantType::Fortune), 0);
        assert_eq!(enchants.get_level(EnchantType::Haste), 0);
        assert_eq!(enchants.iter().count(), 0);
    }

    /// `iter()` is what a UI walks to list "what is on this pickaxe", so it must
    /// yield only enchants the player actually has. The struct promises
    /// "absent == level 0" and that only active enchants consume memory —
    /// a level-0 entry left in the map breaks both.
    #[test]
    fn iter_yields_only_active_enchants() {
        let mut enchants = Enchants::new();
        assert!(
            enchants
                .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                .is_ok()
        );
        enchants.reset_level(EnchantType::Fortune);

        assert_eq!(
            enchants.iter().count(),
            0,
            "a level-0 entry survived reset_level, so the UI would list an enchant the player does not have"
        );
    }

    /// Only Efficiency reads the tier, and only Netherite changes the answer.
    /// That raised cap is the whole reward for reaching the final tier.
    #[test]
    fn efficiency_caps_at_five_everywhere_but_netherite() {
        for tier in [
            PickaxeTier::Wooden,
            PickaxeTier::Stone,
            PickaxeTier::Iron,
            PickaxeTier::Gold,
            PickaxeTier::Diamond,
        ] {
            assert_eq!(EnchantType::Efficiency.max_level(tier, ANY_WORLD), 5);
        }
        assert_eq!(
            EnchantType::Efficiency.max_level(PickaxeTier::Netherite, ANY_WORLD),
            15
        );
    }

    #[test]
    fn the_five_special_enchants_and_fortune_ignore_the_pickaxe_tier() {
        for kind in SPECIAL_ENCHANTS.iter().chain(&[EnchantType::Fortune]) {
            assert_eq!(
                kind.max_level(PickaxeTier::Wooden, ANY_WORLD),
                kind.max_level(PickaxeTier::Netherite, ANY_WORLD),
                "{} must not depend on the pickaxe tier",
                kind.name()
            );
        }
    }

    /// The mirror of the test above, and together they are what keeps the two
    /// progression axes from collapsing into one. The tier moves **Efficiency and
    /// nothing else**; the world moves the other six. If a world also raised
    /// Efficiency's ceiling, one investment would advance both axes at once — and
    /// Netherite's cap of 15 would be unreachable outside the End, deleting the final
    /// tier's whole reward.
    ///
    /// Fortune was once tested here alongside Efficiency, capped by neither axis. It
    /// now sits with the specials: its ceiling of 10 is unchanged, but it is reached
    /// world by world rather than available in full from level 1.
    #[test]
    fn only_efficiency_ignores_the_world() {
        for world in ALL_WORLDS {
            assert_eq!(
                EnchantType::Efficiency.max_level(ANY_TIER, world),
                EnchantType::Efficiency.max_level(ANY_TIER, World::Overworld),
                "Efficiency must not depend on the world, but {} changes it",
                world.name()
            );
        }
    }

    /// Fortune climbs with the world like the five specials — the amendment that
    /// removed the third cap group. What it must *not* lose is its ceiling: the End's
    /// cap is still 10, the level past which `docs/DECISIONS.md` says more Fortune
    /// buys nothing. A re-balance of `World::enchant_cap` that moved the top would
    /// silently delete that point, so it is pinned here rather than left implied.
    #[test]
    fn fortune_climbs_with_the_world_and_still_tops_out_at_ten() {
        assert_eq!(
            EnchantType::Fortune.max_level(ANY_TIER, World::Overworld),
            3
        );
        assert_eq!(EnchantType::Fortune.max_level(ANY_TIER, World::Nether), 6);
        assert_eq!(EnchantType::Fortune.max_level(ANY_TIER, World::End), 10);
    }

    /// `docs/MECHANICS.md`: "Overworld enchants use Lapis (lowest level cap),
    /// Nether uses Quartz (higher), End uses Amethyst (maximum)". Strictly
    /// increasing, not merely non-decreasing: a world that raised no ceiling would
    /// leave its enchant material buying nothing, and the ladder Lapis → Quartz →
    /// Amethyst is the entire reason the three materials are distinct.
    #[test]
    fn enchant_caps_grow_strictly_with_the_world() {
        for pair in ALL_WORLDS.windows(2) {
            let (lower, higher) = (pair[0], pair[1]);
            assert!(
                lower.enchant_cap() < higher.enchant_cap(),
                "{} caps enchants at {} and {} at {}: reaching {} must raise the ceiling",
                lower.name(),
                lower.enchant_cap(),
                higher.name(),
                higher.enchant_cap(),
                higher.name()
            );
        }
    }

    /// One ceiling per world, shared by all five — not a cap per
    /// `(enchant, world)` pair. `max_level` is dispatch, so this is really a test
    /// that no special enchant has quietly grown a table of its own.
    #[test]
    fn the_five_special_enchants_share_their_world_cap() {
        for world in ALL_WORLDS {
            for kind in SPECIAL_ENCHANTS {
                assert_eq!(
                    kind.max_level(ANY_TIER, world),
                    world.enchant_cap(),
                    "{} in {} must read the world's shared cap",
                    kind.name(),
                    world.name()
                );
            }
        }
    }

    /// `docs/MECHANICS.md`: "All five enchants are available as soon as you can
    /// enchant (Overworld); progressing to a new world only raises the level cap."
    /// A special enchant capped at 0 in the Overworld would be *unlocked* by the
    /// Nether instead — a different design, and not this one.
    #[test]
    fn every_special_enchant_is_reachable_in_the_overworld() {
        for kind in SPECIAL_ENCHANTS {
            assert!(
                kind.max_level(ANY_TIER, World::Overworld) > 0,
                "{} caps at 0 in the Overworld, so reaching the Nether would unlock it \
                 rather than merely raise its ceiling",
                kind.name()
            );
        }
    }

    /// Every enchant must fall in exactly one cap group. One that fell out of all
    /// three would still answer `max_level` — via whichever arm of the `match`
    /// caught it — but no test here would be watching that answer.
    #[test]
    fn the_three_cap_groups_partition_every_enchant() {
        for kind in EnchantType::ALL {
            let groups = [
                kind == EnchantType::Efficiency,
                kind == EnchantType::Fortune,
                SPECIAL_ENCHANTS.contains(&kind),
            ];
            assert_eq!(
                groups.iter().filter(|&&in_group| in_group).count(),
                1,
                "{} must be keyed by the tier, by the world, or by nothing — exactly one",
                kind.name()
            );
        }
    }

    #[test]
    fn every_enchant_has_a_reachable_cap() {
        for kind in EnchantType::ALL {
            assert!(
                kind.max_level(ANY_TIER, ANY_WORLD) > 0,
                "{} caps at 0, so it can never be earned",
                kind.name()
            );
        }
    }

    /// A level above the cap is a level the game has no rules for: `max_level`
    /// would no longer bound what the player can hold.
    #[test]
    fn upgrade_stops_at_the_enchant_cap() {
        let cap = EnchantType::Fortune.max_level(ANY_TIER, ANY_WORLD);
        let mut enchants = Enchants::new();
        for _ in 0..cap {
            assert!(
                enchants
                    .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                    .is_ok()
            );
        }

        for _ in 0..5 {
            let _ = enchants.upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD);
        }

        assert_eq!(
            enchants.get_level(EnchantType::Fortune),
            cap,
            "Enchants::upgrade let Fortune climb past its cap of {cap}"
        );
    }

    /// The cap must *refuse*, not shrug. A silent no-op is indistinguishable from
    /// a successful upgrade at the call site, so the paid path (phase 5) would
    /// happily debit the player's ore for a level they never received — the same
    /// hole `Inventory::remove` used to have when it clamped an over-large
    /// withdrawal to zero.
    #[test]
    fn upgrading_a_capped_enchant_is_refused_and_changes_nothing() {
        let cap = EnchantType::Fortune.max_level(ANY_TIER, ANY_WORLD);
        let mut enchants = Enchants::new();
        for _ in 0..cap {
            assert!(
                enchants
                    .upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD)
                    .is_ok()
            );
        }

        assert_eq!(
            enchants.upgrade(EnchantType::Fortune, ANY_TIER, ANY_WORLD),
            Err(CoreError::EnchantAtCap {
                kind: EnchantType::Fortune,
                cap,
            })
        );
        assert_eq!(enchants.get_level(EnchantType::Fortune), cap);
    }

    /// Efficiency is the one enchant whose ceiling moves with the tier, so the
    /// cap `upgrade_efficiency` enforces has to follow the tier it is handed —
    /// including which of the two calls is the one that gets refused.
    #[test]
    fn the_cap_upgrade_enforces_follows_the_tier() {
        let mut wooden = Enchants::new();
        let mut netherite = Enchants::new();
        for _ in 0..15 {
            let _ = wooden.upgrade_efficiency(PickaxeTier::Wooden);
            assert!(netherite.upgrade_efficiency(PickaxeTier::Netherite).is_ok());
        }

        assert_eq!(wooden.get_level(EnchantType::Efficiency), 5);
        assert_eq!(netherite.get_level(EnchantType::Efficiency), 15);
    }

    /// The two doors must enforce the *same* ceiling. They resolve it by different
    /// routes — one through `max_level`'s dispatch, one straight from the tier —
    /// and if those ever disagreed, `Pickaxe::upgrade` would be selling Efficiency
    /// levels under a rule of its own.
    #[test]
    fn both_upgrade_doors_stop_efficiency_at_the_same_level() {
        for tier in [PickaxeTier::Wooden, PickaxeTier::Netherite] {
            for world in ALL_WORLDS {
                let mut generic = Enchants::new();
                let mut direct = Enchants::new();
                for _ in 0..20 {
                    let _ = generic.upgrade(EnchantType::Efficiency, tier, world);
                    let _ = direct.upgrade_efficiency(tier);
                }
                assert_eq!(
                    generic.get_level(EnchantType::Efficiency),
                    direct.get_level(EnchantType::Efficiency),
                    "the generic and world-free doors disagree on {tier:?} in {}",
                    world.name()
                );
            }
        }
    }

    /// The three enchants that break cells in a shape. The other four break
    /// nothing, which `only_the_spatial_enchants_break_cells` holds up.
    const SPATIAL_ENCHANTS: [EnchantType; 3] = [
        EnchantType::Explosive,
        EnchantType::Jackhammer,
        EnchantType::Nuke,
    ];

    /// The largest mine in the game, which is the interesting one for shapes: a
    /// 3x3 would clip every blast against its own edges and hide a radius bug.
    const FULL_MINE: (u8, u8) = (20, 10);

    /// How many cells `kind` breaks — the readable half of most shape assertions.
    fn cells(kind: EnchantType, level: u8, impact: (u8, u8), size: (u8, u8)) -> usize {
        kind.blast_cells(level, impact, size).len()
    }

    /// The radius curve, band by band. Level 10 is the End's cap and must land on
    /// the top band rather than past it — the `min` in `explosive_radius` is what
    /// stops the ramp, and without it level 10 would ask for a radius of 4.
    #[test]
    fn the_explosive_square_grows_in_three_bands() {
        let radii: Vec<u8> = (1..=10).map(explosive_radius).collect();
        assert_eq!(radii, vec![1, 1, 1, 2, 2, 2, 3, 3, 3, 3]);
    }

    /// The side a front-end prints must be the side a proc actually breaks.
    ///
    /// **Measured off [`blast_cells`](EnchantType::blast_cells) rather than off
    /// [`explosive_radius`]**, which would only restate `2r + 1` in a second place.
    /// The blast is taken in the middle of the largest mine so no edge clips it, and
    /// its cell count is squared back into a side — the number the pane writes.
    #[test]
    fn the_explosive_side_is_the_square_a_proc_really_clears() {
        let centre = (FULL_MINE.0 / 2, FULL_MINE.1 / 2);
        for level in 1..=10 {
            let broken = cells(EnchantType::Explosive, level, centre, FULL_MINE);
            let side = usize::from(EnchantType::Explosive.explosive_side(level));
            assert_eq!(
                side * side,
                broken,
                "Explosive {level} is drawn {side}x{side} and breaks {broken} cells"
            );
        }
    }

    /// Six of the seven break no square, and must say `0` rather than the band-one
    /// `3x3` a bare call to [`explosive_radius`] would hand back.
    ///
    /// The `match` in [`explosive_side`](EnchantType::explosive_side) is what makes a
    /// new variant a compile error here instead of a silent square; this is what holds
    /// the six existing arms to what they claim.
    #[test]
    fn only_explosive_has_a_square_to_name() {
        for kind in EnchantType::ALL {
            let side = kind.explosive_side(5);
            if kind == EnchantType::Explosive {
                assert!(side > 0, "Explosive must name a square");
            } else {
                assert_eq!(side, 0, "{} breaks no square", kind.name());
            }
        }
    }

    /// The alignment [`EXPLOSIVE_RADIUS_BAND`] exists to produce: each dimension
    /// owns exactly one square size, so a 7x7 is proof the player reached the End.
    /// If the band width or a world cap moves, this is what says so — the two
    /// numbers live in different modules and nothing else ties them together.
    #[test]
    fn explosive_bands_line_up_with_the_world_caps() {
        let sizes: Vec<usize> = ALL_WORLDS
            .iter()
            .map(|world| {
                let level = EnchantType::Explosive.max_level(ANY_TIER, *world);
                cells(EnchantType::Explosive, level, (10, 5), FULL_MINE)
            })
            .collect();

        assert_eq!(
            sizes,
            vec![9, 25, 49],
            "each world must own exactly one square size (3x3, 5x5, 7x7)"
        );
    }

    /// The arithmetic trap this module is most exposed to: a `u8` impact at the
    /// origin computes `0 - radius`, which panics in debug and — worse — wraps to
    /// 255 in release, putting the blast on the far side of the grid. A corner
    /// blast must simply be a smaller blast.
    #[test]
    fn a_blast_at_the_origin_is_clipped_rather_than_wrapped() {
        let blast = EnchantType::Explosive.blast_cells(10, (0, 0), FULL_MINE);

        assert_eq!(blast.len(), 16, "a radius-3 square at the origin is 4x4");
        for &(x, y) in &blast {
            assert!(x <= 3 && y <= 3, "({x}, {y}) is not in the corner quadrant");
        }
    }

    /// Every shape, at every level, from every cell of a full mine, must return
    /// coordinates that are actually on the grid. This is the property the whole
    /// clipping design exists for, and asserting it over the entire grid is what
    /// makes it a guarantee rather than three lucky examples.
    #[test]
    fn no_shape_ever_leaves_the_grid() {
        let (width, height) = FULL_MINE;
        for kind in SPATIAL_ENCHANTS {
            for level in 0..=10 {
                for y in 0..height {
                    for x in 0..width {
                        for &(cell_x, cell_y) in &kind.blast_cells(level, (x, y), FULL_MINE) {
                            assert!(
                                cell_x < width && cell_y < height,
                                "{} at level {level} from ({x}, {y}) reached ({cell_x}, {cell_y})",
                                kind.name()
                            );
                        }
                    }
                }
            }
        }
    }

    /// A shape must not name the same cell twice. `blast` is immune to it anyway —
    /// the second `take` finds the hole the first left — but a duplicate would mean
    /// the geometry is wrong, and relying on `take` to hide it is how a shape that
    /// double-counts survives to meet a caller that does not.
    #[test]
    fn no_shape_names_a_cell_twice() {
        for kind in SPATIAL_ENCHANTS {
            let blast = kind.blast_cells(10, (7, 4), FULL_MINE);
            let mut unique = blast.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), blast.len(), "{} repeats a cell", kind.name());
        }
    }

    /// Jackhammer's reach is the *mine's* width, never its own level: that is what
    /// makes mine size the upgrade that scales it, and what keeps it from blurring
    /// into Explosive's square. One row tall at level 1 and at level 10 alike.
    #[test]
    fn jackhammer_spans_the_full_width_at_every_level() {
        for level in 1..=10 {
            let blast = EnchantType::Jackhammer.blast_cells(level, (7, 4), FULL_MINE);
            assert_eq!(blast.len(), usize::from(FULL_MINE.0), "level {level}");
            for &(_, y) in &blast {
                assert_eq!(y, 4, "level {level} left the impact row");
            }
        }
    }

    /// A wider mine is a wider Jackhammer — the whole of its scaling, stated
    /// directly rather than inferred from the two tests above.
    #[test]
    fn a_bigger_mine_makes_a_longer_jackhammer_row() {
        let small = cells(EnchantType::Jackhammer, 1, (1, 1), (3, 3));
        let large = cells(EnchantType::Jackhammer, 1, (1, 1), FULL_MINE);
        assert!(large > small, "{large} is no longer than {small}");
    }

    /// Nuke's level buys frequency, never area: there is no area past "all of it",
    /// and a Nuke that grew with its level would be an Explosive with a better
    /// ceiling.
    #[test]
    fn nuke_covers_the_whole_grid_at_every_level_and_from_anywhere() {
        let expected = usize::from(FULL_MINE.0) * usize::from(FULL_MINE.1);
        for level in 1..=10 {
            for impact in [(0, 0), (7, 4), (19, 9)] {
                assert_eq!(
                    cells(EnchantType::Nuke, level, impact, FULL_MINE),
                    expected,
                    "level {level} from {impact:?}"
                );
            }
        }
    }

    /// Nuke must strictly dominate the other two, and they must not collapse into
    /// each other. This is what the radius ceiling buys: an Explosive allowed to
    /// keep growing would reach the whole grid and leave Nuke selling nothing but a
    /// different proc rate.
    ///
    /// Deliberately **not** a total order on the counts — at the cap a 7x7 (49)
    /// clears more than a 20-wide row (20), and that is fine. What matters is that
    /// the three cover different *shapes*, and that neither of the small two
    /// approaches the grid.
    #[test]
    fn nuke_dominates_the_other_two_shapes() {
        let at_cap = |kind: EnchantType| cells(kind, 10, (7, 4), FULL_MINE);
        let (explosive, jackhammer, nuke) = (
            at_cap(EnchantType::Explosive),
            at_cap(EnchantType::Jackhammer),
            at_cap(EnchantType::Nuke),
        );

        assert_ne!(
            explosive, jackhammer,
            "a square and a row that clear the same count are one enchant sold twice"
        );
        assert!(
            explosive < nuke,
            "Explosive ({explosive}) reaches Nuke ({nuke})"
        );
        assert!(
            jackhammer < nuke,
            "Jackhammer ({jackhammer}) reaches Nuke ({nuke})"
        );
        assert!(
            explosive * 2 < nuke,
            "Explosive ({explosive}) clears half the grid ({nuke}) and crowds Nuke out"
        );
    }

    /// Level 0 is "never bought", not "bought and useless". Callers skip it before
    /// rolling a proc, so this is the belt to that brace — and the half that would
    /// otherwise hand a free blast to a player with no enchant at all.
    #[test]
    fn a_level_zero_enchant_breaks_nothing() {
        for kind in EnchantType::ALL {
            assert_eq!(
                cells(kind, 0, (7, 4), FULL_MINE),
                0,
                "{} breaks cells at level 0",
                kind.name()
            );
        }
    }

    /// The four non-spatial enchants touch the grid not at all: two are passive
    /// multipliers and Excavator substitutes a drop *after* it leaves the block.
    /// A shape appearing on one of them would be an enchant quietly gaining a
    /// second effect.
    #[test]
    fn only_the_spatial_enchants_break_cells() {
        for kind in EnchantType::ALL {
            let breaks = cells(kind, 10, (7, 4), FULL_MINE) > 0;
            assert_eq!(
                breaks,
                SPATIAL_ENCHANTS.contains(&kind),
                "{} disagrees with its spatial classification",
                kind.name()
            );
        }
    }

    /// An impact outside the grid must not produce cells outside the grid. A live
    /// impact always comes from [`Mine::get_target`](crate::mine::Mine::get_target)
    /// and is therefore on the grid, so this guards the same thing
    /// `size_levels_past_the_table_clamp_to_the_largest_mine` does: coordinates are
    /// plain data that phase 9 reads back out of a save file.
    ///
    /// Nuke is the deliberate exception — it ignores the impact entirely, so an
    /// out-of-grid one still clears the grid. That is the shape being
    /// level- *and* position-independent, not a leak.
    /// Note the invariant is **in-bounds, not empty**: an impact one cell past the
    /// right edge still clips to a square that overlaps the grid, and that is
    /// correct. Only a row whose `y` is off the grid has nothing left to break.
    #[test]
    fn an_impact_off_the_grid_never_yields_cells_off_the_grid() {
        let (width, height) = FULL_MINE;
        for impact in [(width, 5), (7, height), (u8::MAX, u8::MAX)] {
            for kind in SPATIAL_ENCHANTS {
                for &(x, y) in &kind.blast_cells(10, impact, FULL_MINE) {
                    assert!(
                        x < width && y < height,
                        "{} from {impact:?} reached ({x}, {y})",
                        kind.name()
                    );
                }
            }
        }

        // The row guard specifically: with its `y` off the grid there is no row
        // left to break, where a clipped square may still overlap.
        assert_eq!(
            cells(EnchantType::Jackhammer, 10, (7, height), FULL_MINE),
            0,
            "a row below the grid must break nothing"
        );
    }

    /// A degenerate mine has no cells to break, and must produce an empty blast
    /// rather than an underflow on `width - 1`. Unreachable from `MINE_SIZES`,
    /// whose smallest entry is 3x3 — but `blast_cells` takes the size as plain
    /// data, and phase 9 reads mine dimensions back out of a save file.
    #[test]
    fn a_mine_with_no_cells_yields_an_empty_blast() {
        for kind in SPATIAL_ENCHANTS {
            for size in [(0, 0), (0, 5), (5, 0)] {
                assert_eq!(
                    cells(kind, 10, (0, 0), size),
                    0,
                    "{} on a {size:?} mine",
                    kind.name()
                );
            }
        }
    }

    /// The ramp lands exactly on its quoted ends. Level 1 and level 10 are the two
    /// numbers the balance pass reasons about, so an off-by-one in the `(level - 1)`
    /// or in the divisor would silently sell a different enchant than the one
    /// designed.
    #[test]
    fn the_proc_ramp_lands_on_both_of_its_quoted_ends() {
        assert_eq!(EnchantType::Explosive.proc_permille(1), 20);
        assert_eq!(EnchantType::Explosive.proc_permille(10), 200);
        assert_eq!(EnchantType::Jackhammer.proc_permille(1), 15);
        assert_eq!(EnchantType::Jackhammer.proc_permille(10), 150);
        assert_eq!(EnchantType::Nuke.proc_permille(1), 1);
        assert_eq!(EnchantType::Nuke.proc_permille(10), 10);
        assert_eq!(EnchantType::Excavator.proc_permille(1), 5);
        assert_eq!(EnchantType::Excavator.proc_permille(10), 50);
    }

    /// **The Excavator's ceiling stays under both spatials', and that is the design
    /// rather than a coincidence of tuning.** A proc mints 100 raw where the cell it
    /// replaced held 1, so frequency is its entire power budget — whereas Explosive
    /// and Jackhammer hand out cells the player would have mined anyway, only sooner.
    /// A balance pass that raises this above them has changed what the enchant is,
    /// and should have to say so here.
    ///
    /// Nuke is excluded: it is throttled far below everything for its own reason —
    /// it clears the grid — so comparing against it would prove nothing.
    #[test]
    fn the_excavator_procs_less_often_than_the_two_common_spatials() {
        let excavator = EnchantType::Excavator.proc_permille(10);
        for kind in [EnchantType::Explosive, EnchantType::Jackhammer] {
            assert!(
                excavator < kind.proc_permille(10),
                "the Excavator now procs at least as often as {}",
                kind.name()
            );
        }
    }

    /// A level the player paid for must never make an enchant proc *less* often.
    /// Integer division makes that worth checking rather than assuming: the ramp
    /// truncates, and a badly ordered expression could flatten or dip.
    #[test]
    fn proc_chances_never_fall_as_the_level_climbs() {
        for kind in PROC_ORDER {
            for level in 1..10u8 {
                assert!(
                    kind.proc_permille(level + 1) >= kind.proc_permille(level),
                    "{} dips from level {level} to {}",
                    kind.name(),
                    level + 1
                );
            }
            assert!(
                kind.proc_permille(10) > kind.proc_permille(1),
                "{} is no more frequent at the cap than at level 1",
                kind.name()
            );
        }
    }

    /// A level past the End's cap rests on the ceiling instead of ramping past it.
    /// Unreachable through [`Enchants::upgrade`], which enforces the cap — but the
    /// level is plain data that phase 9 reads back out of a save file.
    #[test]
    fn a_level_past_the_cap_rests_on_the_quoted_ceiling() {
        for kind in PROC_ORDER {
            let ceiling = kind.proc_permille(10);
            for level in [11, 50, u8::MAX] {
                assert_eq!(
                    kind.proc_permille(level),
                    ceiling,
                    "{} ramps past its ceiling at level {level}",
                    kind.name()
                );
            }
        }
    }

    /// Level 0 is "never bought": it must not proc, and — more importantly — the
    /// resolution skips it before drawing, so it costs no entropy either.
    #[test]
    fn a_level_zero_enchant_never_procs() {
        for kind in EnchantType::ALL {
            assert_eq!(kind.proc_permille(0), 0, "{} procs at level 0", kind.name());
        }
    }

    /// Only the enchants in [`PROC_ORDER`] roll — Efficiency, Fortune and Haste are
    /// passive multipliers, always on, with nothing to roll for.
    #[test]
    fn only_the_enchants_in_the_proc_order_ever_proc() {
        for kind in EnchantType::ALL {
            let procs = kind.proc_permille(10) > 0;
            assert_eq!(
                procs,
                PROC_ORDER.contains(&kind),
                "{} disagrees with its presence in PROC_ORDER",
                kind.name()
            );
        }
    }

    /// **The reproducibility contract.** `PROC_ORDER` decides which enchant draws
    /// first, and a save stores a position in that sequence — so reordering it
    /// would leave every existing run continuing on different dice, silently. This
    /// anchors it to the enum's declaration order, which is the one rule that makes
    /// "where does a new enchant go" have an obvious answer.
    #[test]
    fn the_proc_order_follows_the_declaration_order() {
        let declared: Vec<EnchantType> = EnchantType::ALL
            .iter()
            .copied()
            .filter(|kind| PROC_ORDER.contains(kind))
            .collect();

        assert_eq!(
            declared,
            PROC_ORDER.to_vec(),
            "PROC_ORDER has drifted from the order the enum declares"
        );
    }

    /// Every enchant [`Mine`](crate::mine::Mine) rolls must have a shape to break,
    /// and vice versa. One without the other is an enchant that draws and does
    /// nothing, or one that has a shape no roll ever reaches.
    #[test]
    fn the_spatial_proc_list_and_the_spatial_list_describe_the_same_enchants() {
        let mut ordered = SPATIAL_PROC_ORDER.to_vec();
        let mut spatial = SPATIAL_ENCHANTS.to_vec();
        ordered.sort_unstable();
        spatial.sort_unstable();
        assert_eq!(ordered, spatial);
    }

    /// A generator's position, read as the next eight draws. Two generators that
    /// agree here have consumed the same amount of entropy; two that do not have
    /// diverged, whatever their outcomes looked like.
    fn draws(rng: &mut Rng) -> Vec<Option<usize>> {
        (0..8).map(|_| rng.weighted(&[1, 1])).collect()
    }

    /// An `Enchants` holding `level` of the Excavator and nothing else.
    fn with_excavator(level: u8) -> Enchants {
        let mut enchants = Enchants::new();
        enchants.levels.insert(EnchantType::Excavator, level);
        enchants
    }

    /// **The test that protects every save written before this enchant existed.**
    /// Appending to `PROC_ORDER` is only free because an unbought enchant returns
    /// before it touches the generator — roll it at 0 permille instead and the draw
    /// still happens, shifting every later draw in the run onto different dice.
    /// Nothing would fail; the run would just quietly stop being the run that was
    /// saved. Proven against a twin generator asked for the same work minus the
    /// resolution.
    #[test]
    fn an_unbought_excavator_never_procs_and_never_draws() {
        let (mut resolved, mut untouched) = (Rng::from_seed(11), Rng::from_seed(11));
        let enchants = Enchants::new();

        for _ in 0..50 {
            assert_eq!(
                enchants.resolve_excavator(Material::Iron, &mut resolved),
                None,
                "an unbought Excavator minted a Compressed unit"
            );
        }

        assert_eq!(
            draws(&mut resolved),
            draws(&mut untouched),
            "resolving an unbought Excavator advanced the generator"
        );
    }

    /// What a proc pays: one Compressed unit of the material that was mined, never a
    /// raw one and never another material's. The Compressed denomination is the whole
    /// point — it is the only thing in the game that mints one without paying its
    /// 100 raw — and the material is the only thing about the block that matters.
    #[test]
    fn a_proc_mints_one_compressed_unit_of_the_mined_material() {
        let enchants = with_excavator(10);
        let mut rng = Rng::from_seed(3);
        let mut procs = 0;

        for material in [Material::Iron, Material::Diamond, Material::Amethyst] {
            for _ in 0..500 {
                if let Some(item) = enchants.resolve_excavator(material, &mut rng) {
                    procs += 1;
                    assert_eq!(
                        item,
                        Item::Compressed(material),
                        "the Excavator substituted something other than Compressed {material:?}"
                    );
                }
            }
        }

        assert!(procs > 0, "1500 rolls at the cap never procced");
    }

    /// **The determinism contract at this enchant's level.** A save stores a position
    /// in the sequence, so the same seed must mint the same windfalls — otherwise a
    /// reloaded run re-rolls its luck and "send me your save" stops being true the
    /// moment the Excavator fires.
    #[test]
    fn the_same_seed_mints_the_same_windfalls() {
        fn run(seed: u64) -> usize {
            let enchants = with_excavator(10);
            let mut rng = Rng::from_seed(seed);
            (0..2000)
                .filter(|_| {
                    enchants
                        .resolve_excavator(Material::Iron, &mut rng)
                        .is_some()
                })
                .count()
        }

        assert_eq!(run(7), run(7), "same seed must replay identically");
        assert_ne!(run(7), run(99), "different seeds must diverge");
    }

    /// A bought level must actually buy something. Level 1 is the one worth pinning:
    /// it is the level a player reaches first, and the level a botched `saturating_sub`
    /// or an off-by-one in the ramp would silently flatten to nothing.
    #[test]
    fn a_level_one_excavator_still_procs() {
        let enchants = with_excavator(1);
        let mut rng = Rng::from_seed(19);

        assert!(
            (0..5000).any(|_| enchants
                .resolve_excavator(Material::Coal, &mut rng)
                .is_some()),
            "5000 rolls at level 1 never procced"
        );
    }

    /// **The other half of the reproducibility contract, now that the resolution is
    /// split in two.** The spatials draw inside `Mine`, the Excavator draws here, and
    /// nothing but this test says the first group goes first.
    ///
    /// A prefix is the exact statement wanted: it pins that the spatials come first,
    /// *in their order*, and that none of them was dropped from the grid's loop —
    /// three claims one `starts_with` covers. It also decides where a fifth triggered
    /// enchant goes: appended, or this fails.
    #[test]
    fn spatial_proc_order_is_a_prefix_of_proc_order() {
        assert!(
            PROC_ORDER.starts_with(&SPATIAL_PROC_ORDER),
            "the spatial enchants no longer draw first, or no longer draw in order"
        );
        assert_eq!(
            PROC_ORDER[SPATIAL_PROC_ORDER.len()..],
            [EnchantType::Excavator],
            "the Excavator is no longer the last thing a swing draws"
        );
    }
}
