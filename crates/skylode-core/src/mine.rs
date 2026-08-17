//! Generated mines.
//!
//! A [`Mine`] is the grid of [`Block`]s the player digs through. Its dimensions
//! scale with a size level, from a tiny 3x3 starter mine up to a 20x10 mine at
//! the top of the `MINE_SIZES` table.

use crate::block::{Block, TICKS_PER_HARDNESS};
use crate::enchant::{EnchantType, Enchants, SPATIAL_PROC_ORDER};
use crate::error::CoreError;
use crate::mine_kind::MineKind;
use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

/// Dimensions `(width, height)` for each of the 10 mine size levels.
///
/// Indexed by [`Mine::size_level`]; larger levels are both wider and taller, so
/// a higher-level mine holds progressively more blocks to clear.
const MINE_SIZES: [(u8, u8); 10] = [
    (3, 3),
    (4, 3),
    (6, 4),
    (8, 5),
    (10, 6),
    (12, 7),
    (14, 8),
    (16, 8),
    (18, 9),
    (20, 10),
];

/// The highest size level the table describes.
///
/// Derived from `MINE_SIZES` rather than written out, so extending the table
/// raises the ceiling and no second place has to be remembered.
///
/// `pub(crate)` for [`economy`](crate::economy), which prices the track and needs the
/// same bound to know how far its cost ramp runs. It read a local copy of the number
/// before, and a copy is exactly what this constant exists to avoid.
pub(crate) const MAX_SIZE_LEVEL: u32 = MINE_SIZES.len() as u32 - 1;

/// The most cells any grid in the game can hold.
///
/// **An audit ceiling for [`GameState::validate`](crate::game::GameState)**, which uses
/// it as the bound on what a single tick can bring down: a swing breaks its impact
/// block and whatever its blasts reach, and every one of those cells is in the grid
/// standing in front of the player, so no tick can break more than one full grid.
///
/// Folded over the whole table rather than read off its last row. The rows happen to
/// be monotone today, and a ceiling that quietly assumed so would be wrong the day a
/// re-balance made one level wider and shorter than the one below it — in the
/// direction that refuses honest saves.
pub(crate) const MAX_CELLS: u64 = {
    let mut max = 0;
    let mut level = 0;
    while level < MINE_SIZES.len() {
        let (width, height) = MINE_SIZES[level];
        let cells = width as u64 * height as u64;
        if cells > max {
            max = cells;
        }
        level += 1;
    }
    max
};

/// The highest richness level a mine can reach: 10 rungs, `0..=9`.
///
/// Richness is a *curve*, not an irregular table like `MINE_SIZES`, so it is a
/// formula ([`value_weight`]) plus this bound rather than a laid-out array. The
/// count is provisional; phase 10 balance sets the final shape.
///
/// `pub(crate)` for [`economy`](crate::economy), for the reason [`MAX_SIZE_LEVEL`] is:
/// the cost ramp has to span exactly the rungs that exist. That module kept its own
/// `RICHNESS_MIX_SPAN` at the same value, with nothing but care keeping the two in
/// step — a re-balance moving one and not the other would have made the rare share
/// overshoot or never arrive, silently.
///
/// **Now `pub`, and the front-end is why.** The Mine panel prints `Richness  level
/// 6 / 9` and the Mines detail pane draws the dial against the same ceiling
/// (`docs/UI.md` §5.1, §5.2), so a crate that could not see this number
/// had to mirror it — and a mirrored ceiling is one that goes on printing `/ 9`
/// after phase-10 balance moves it, with no test anywhere able to notice. There is
/// no matching read for [`get_richness_level`](Mine::get_richness_level)'s
/// *current* value, because that one is per-mine and already a method; this is the
/// bound, which belongs to the rules and not to any one grid.
pub const MAX_RICHNESS_LEVEL: u32 = 9;

/// Weight (out of 100) of the *value* cell at richness `0`: the "mixed mine as
/// first specified". Non-zero, so a mine at richness 0 already sprinkles in its
/// value cell — the only way the dense blocks ever enter the game. Provisional.
const RICHNESS_BASE_WEIGHT: u32 = 10;

/// How much the value cell's weight climbs per richness rung. With
/// [`RICHNESS_BASE_WEIGHT`] and [`MAX_RICHNESS_LEVEL`] this ramps the value
/// weight from 10% at richness 0 to 91% at the top. Provisional.
const RICHNESS_WEIGHT_STEP: u32 = 9;

/// The weight (out of 100) of the value cell at a given richness setting.
///
/// A **formula**, not a table: unlike the two-dimensional, hand-tuned
/// `MINE_SIZES`, richness is a single monotone one-dimensional curve, which a
/// formula states directly — and which the [`tunables`](crate::tunables) doctrine
/// prefers ("a curve is its parameters, not a table"). Kept in `mine` beside
/// `MINE_SIZES` rather than in `tunables` because it is composition local to the
/// mine, read nowhere else.
///
/// Deliberately **not** capped below 100%. An earlier design bounded it there as
/// an anti-brick invariant, but the richness *dial* is free and reversible: a
/// player who over-enriched a mine slides the setting back down to harvest the
/// common cell again, so the dial — not a weight cap — is what keeps a run from
/// stranding. The only real constraint is that the two weights never both be
/// zero, which holds because the base weight is non-zero.
///
/// Clamps `setting` like [`get_size`](Mine::get_size) clamps `size_level`: phase 9
/// reads this field straight back out of a save that may be hand-edited or
/// corrupted, and it must yield a valid composition rather than run off the ramp.
fn value_weight(setting: u32) -> u32 {
    RICHNESS_BASE_WEIGHT + setting.min(MAX_RICHNESS_LEVEL) * RICHNESS_WEIGHT_STEP
}

/// Draws one cell of a `kind` mine: its [value](MineKind::value_block) block with
/// weight `value_w`, its [common](MineKind::common_block) block with the rest.
///
/// The single point where a cell's identity is decided, shared by the two things
/// that lay cells down — [`reset`](Mine::reset), which draws a whole grid, and
/// [`set_richness_setting`](Mine::set_richness_setting), which redraws the
/// standing ones. Two copies of the draw could disagree, and the disagreement
/// would read as "the dial changes the odds", which is precisely what it must not
/// do: the dial changes `value_w` and nothing else.
///
/// A free function rather than a method because the redraw holds `self.grid`
/// mutably while it calls this, and a `&self` method would be a second borrow.
///
/// The `None` arm is unreachable — `value_w < 100` for every setting, so both
/// weights are positive and `weighted` always chooses — but it falls back to the
/// common cell rather than reaching for the `unwrap`/`expect` the workspace lints
/// deny. Erring toward the common cell is also the arm that cannot hand out free
/// value.
fn draw_cell(kind: MineKind, value_w: u32, rng: &mut Rng) -> Block {
    match rng.weighted(&[100 - value_w, value_w]) {
        Some(1) => kind.value_block(),
        _ => kind.common_block(),
    }
}

/// A generated mine: its [`MineKind`] identity plus the laid-out grid the player
/// mines through.
///
/// The kind is what a mine *is* — "the Iron mine" — and it answers every question
/// about the block pool ([`common_block`](MineKind::common_block) /
/// [`value_block`](MineKind::value_block)), the world, and the gating tier, so the
/// grid does not have to carry them. What the grid carries is the run's *state*:
/// which cells are still standing.
///
/// `PartialEq` but **not `Eq`**: [`break_progress`](Mine::break_ratio) is an `f32`,
/// and `f32` is not `Eq`. Nothing is lost — the tests' `assert_eq!` only ever
/// needed `PartialEq`, and no other type embeds a `Mine` to inherit the bound.
/// [`dig`](Mine::dig) refusing a `NaN` mining power is what keeps the reflexivity
/// `Eq` would have promised true in practice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mine {
    /// Which of the twelve canonical mines this is; the source of its block pool.
    kind: MineKind,
    /// Size tier of the mine; indexes into `MINE_SIZES` via
    /// [`get_size`](Mine::get_size).
    size_level: u32,
    /// The richness *ceiling*: how high the dial may be pushed. Bought (phase 5),
    /// permanent, one-way — raised a level at a time by
    /// [`upgrade_richness_level`](Mine::upgrade_richness_level) and read through
    /// [`get_richness_level`](Mine::get_richness_level). A fresh mine starts at 0,
    /// so the dial cannot move until a level is bought.
    richness_level: u32,
    /// The richness *dial*: `<= richness_level`, set freely and for free, and the
    /// only field that actually shapes the grid. It is the weight the composition
    /// gives the [value cell](MineKind::value_block) — see [`value_weight`].
    richness_setting: u32,
    /// The 2D grid the player actually mines, row by row: `Some(block)` is a cell
    /// still standing, `None` a hole where one was broken.
    ///
    /// The `None` **is** the hole mask, fused into the composition rather than
    /// kept beside it as a parallel `Vec<Vec<bool>>`. Two structures could
    /// disagree — a mask sized for a 3x3 over a grown 20x10 grid — and every
    /// reset and enlargement would have to remember to resize both. Fused, that
    /// state is unrepresentable, and the compiler makes every reader of a cell
    /// answer the question "is it still there?".
    grid: Vec<Vec<Option<Block>>>,
    /// Mining power accumulated against [`target`](Mine::target), in the units
    /// [`TICKS_PER_HARDNESS`] converts to hardness.
    ///
    /// **One counter, not one per cell.** The progress belongs to the *aim*, not
    /// to the grid: a player who has chipped halfway through an Obsidian has not
    /// half-chipped the two hundred cells around it, and storing it per cell would
    /// hand out a mine that remembers every abandoned swing forever.
    break_progress: f32,
    /// The cell being dug, `None` when nothing is aimed at yet.
    ///
    /// Held **across ticks**, and that is the whole reason it is a field. Drawing
    /// a fresh random cell every tick would leave `break_progress` accumulating
    /// against a different block each time — a counter measuring nothing. It is
    /// also what the UI highlights and draws its crack glyph on.
    ///
    /// Coordinates, not a `Block`: the grid is the record of what stands there,
    /// and a copy of the block would be a second one to keep in step with
    /// [`take`](Mine::take) and the dial's redraw.
    target: Option<(u8, u8)>,
}

/// What one [`dig`](Mine::dig) brought down: the block, and the cell it stood in.
///
/// The cell is not a convenience for the renderer — it is the `impact` the spatial
/// enchants centre their shapes on, and [`dig`](Mine::dig) is the only place it can
/// be observed. See there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Dug {
    /// The block that fell.
    pub(crate) block: Block,
    /// Where it stood, in grid coordinates.
    pub(crate) cell: (u8, u8),
}

/// One spatial enchant firing on one swing: which enchant, where it landed, and
/// what it brought down.
///
/// **Two lists, and they are not the same list.** `cells` is the *shape* — every
/// grid coordinate the blast covered, holes included — while `broken` is what
/// actually stood there and has to be paid for. They diverge exactly when a shape
/// overlaps ground the swing has already cleared, which on a half-dug grid is most
/// of the time, and each has one reader that cannot use the other: the front-end
/// flashes the shape (a blast the player watches must look like a blast, not like
/// the four cells that happened to be left), and the tick banks the blocks.
///
/// `cells` is already clipped to the grid by
/// [`blast_cells`](crate::enchant::EnchantType), so a coordinate here always names
/// a real cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpatialProc {
    /// Which of the three spatial enchants fired.
    pub(crate) kind: EnchantType,
    /// The shape it covered, in grid coordinates.
    pub(crate) cells: Vec<(u8, u8)>,
    /// The blocks that were standing in that shape, and so fell.
    pub(crate) broken: Vec<Block>,
}

impl Mine {
    /// Creates a fresh, full mine of the given [`MineKind`] at its smallest size.
    ///
    /// Takes `&mut Rng` because the grid is drawn, not filled: even a richness-0
    /// mine is a weighted mix of common and value cells, and every draw in the
    /// game comes from the seeded generator so runs stay reproducible.
    pub fn new(kind: MineKind, rng: &mut Rng) -> Self {
        let mut mine = Mine {
            kind,
            size_level: 0,
            richness_level: 0,
            richness_setting: 0,
            grid: Vec::new(),
            break_progress: 0.0,
            target: None,
        };
        mine.reset(rng);
        mine
    }

    /// Returns which of the twelve canonical mines this is.
    pub fn kind(&self) -> MineKind {
        self.kind
    }

    /// Returns the richness *ceiling* — the highest the dial may be set. Bought,
    /// permanent, one-way; raised a level at a time by
    /// [`upgrade_richness_level`](Mine::upgrade_richness_level).
    pub fn get_richness_level(&self) -> u32 {
        self.richness_level
    }

    /// Whether the bought richness ceiling is already at its top rung, so
    /// [`upgrade_richness_level`](Mine::upgrade_richness_level) would refuse — for a
    /// UI to grey out the buy button before it is pressed.
    pub fn is_richness_maxed(&self) -> bool {
        self.richness_level >= MAX_RICHNESS_LEVEL
    }

    /// Raises the richness *ceiling* by one level, or refuses if it is already at
    /// the top rung.
    ///
    /// The **purchase** side of richness: it moves only the ceiling, never the
    /// dial. Buying a level does not enrich the grid on its own — it lets the
    /// player push the free [dial](Mine::set_richness_setting) one rung higher, and
    /// *that* is what re-rolls the standing cells. Buy the ceiling, then set the
    /// dial; the two are deliberately separate actions.
    ///
    /// Refuses at [`MAX_RICHNESS_LEVEL`] with [`CoreError::RichnessLevelMaxed`]
    /// rather than incrementing past it, so a paid purchase never charges for a
    /// level the composition curve cannot use — the refusal
    /// [`upgrade_size_level`](Mine::upgrade_size_level) makes for the size track.
    ///
    /// `pub(crate)` because it is **free** here: the transaction that debits it
    /// lives in [`economy`](crate::economy). Unlike
    /// [`set_richness_setting`](Mine::set_richness_setting), which is `pub` because
    /// the free dial *is* the design, raising the ceiling for free would hand a
    /// front-end unlimited richness.
    pub(crate) fn upgrade_richness_level(&mut self) -> Result<(), CoreError> {
        if self.richness_level >= MAX_RICHNESS_LEVEL {
            return Err(CoreError::RichnessLevelMaxed {
                level: MAX_RICHNESS_LEVEL,
            });
        }
        self.richness_level += 1;
        Ok(())
    }

    /// Returns the richness *dial*: the value cell's weight in the composition.
    pub fn get_richness_setting(&self) -> u32 {
        self.richness_setting
    }

    /// The share of cells the dial currently draws as the
    /// [value block](MineKind::value_block), in **percent**.
    ///
    /// The dial's *meaning*, where [`get_richness_setting`](Mine::get_richness_setting)
    /// is only its position. A setting of 3 tells the player nothing on its own —
    /// what they want to know before spending on the track is how much of the grid
    /// turns valuable, and that is [`value_weight`], which is private because it is
    /// the generator's business.
    ///
    /// Two readers, both of which would otherwise have to reinvent the curve:
    /// `docs/UI.md` §5.1 prints this beside the mine, and the auto-miner
    /// weights its closed-form payout by it — the one place in the game where the
    /// *expected* composition stands in for a grid nobody walks.
    ///
    /// Percent rather than permille because the weights are already out of 100 in
    /// [`draw_cell`], so this is the number itself and not a conversion.
    pub fn value_weight_percent(&self) -> u32 {
        Self::value_weight_percent_for(self.richness_setting)
    }

    /// The same share, for a dial setting rather than for a mine.
    ///
    /// Public for [`size_for_level`](Mine::size_for_level)'s reason exactly: the
    /// Mines screen draws a dial for the mine under the cursor, and eleven of the
    /// twelve have no [`Mine`] behind them until the player walks in. A fresh one is
    /// created at setting 0, so `value_weight_percent_for(0)` is what that row's bar
    /// honestly shows — not a placeholder, the real curve read at the real setting.
    pub fn value_weight_percent_for(setting: u32) -> u32 {
        value_weight(setting)
    }

    /// Moves the richness dial, redrawing the composition of the **standing**
    /// cells at once — and leaving the holes exactly where they are. Refuses a
    /// setting above the bought [ceiling](Mine::get_richness_level), changing
    /// nothing.
    ///
    /// The holes are the whole point. One rule governs this method and the fact
    /// that mines persist across screens — **no free action may ever put a broken
    /// block back**. The dial is free, so if it refilled what it passed over, a
    /// player would break the four Amethyst out of two hundred cells, nudge the
    /// dial, and find four more: a batch reset that never paid for itself by
    /// emptying the mine. Free to reshape what remains, never free to un-break
    /// what is gone.
    ///
    /// Nothing is cached per richness level, for the same reason. A grid
    /// remembered against each setting would hand out a full, untouched mine the
    /// first time the player visited a rung they had never dialled to — the same
    /// free reset, through the window. One hole mask per mine, shared by every
    /// setting.
    ///
    /// **`pub`, unlike [`take`](Mine::take) and
    /// [`upgrade_size_level`](Mine::upgrade_size_level).** Those are `pub(crate)`
    /// because they are free *by accident* — the transaction that will charge for
    /// them does not exist yet. Here the freedom is the design: the dial is what
    /// guarantees an over-enriched mine can always be walked back to harvest its
    /// common cell, so a purchase can slow a run down but never strand it. What
    /// bounds it is the ceiling, not the visibility.
    ///
    /// Asking for the setting the dial already holds is a no-op that draws
    /// nothing: the dial was not moved, so there is nothing to redraw, and the
    /// generator's position — which is run state — must not advance for an order
    /// that said nothing. (Until the phase-5 purchase path raises a ceiling that
    /// is currently always 0, this is the only call that can succeed.)
    ///
    /// The free re-roll this leaves open — wiggling the dial until the value cells
    /// happen to line up under an Explosive — is knowingly accepted for the MVP:
    /// single-player, offline, no leaderboard. See `docs/decisions/0057`.
    ///
    /// **The progress on the targeted cell is forfeit**, though the aim is not: the
    /// cell still stands, so the player keeps pointing at it, but whatever block
    /// they had been chipping at has been redrawn out from under them and the
    /// progress was owed to *that* block. Left standing it would be a small
    /// laundering — chip an Amethyst to 44 of its 45, dial down, collect the
    /// Endstone that replaced it on the next tick for a swing's worth of work. In
    /// practice a player must release Space to touch the dial at all, which zeroes
    /// it anyway; this is the belt to that pair of braces.
    pub fn set_richness_setting(&mut self, setting: u32, rng: &mut Rng) -> Result<(), CoreError> {
        if setting > self.richness_level {
            return Err(CoreError::RichnessAboveCeiling {
                requested: setting,
                ceiling: self.richness_level,
            });
        }
        if setting == self.richness_setting {
            return Ok(());
        }

        self.richness_setting = setting;
        self.break_progress = 0.0;
        let (kind, value_w) = (self.kind, value_weight(setting));
        for cell in self.grid.iter_mut().flatten() {
            if cell.is_some() {
                *cell = Some(draw_cell(kind, value_w, rng));
            }
        }
        Ok(())
    }

    /// Resets the mine to its initial state, refilling the grid.
    ///
    /// Each cell is drawn independently between the kind's
    /// [common](MineKind::common_block) and [value](MineKind::value_block) blocks,
    /// weighted by the richness dial: the value cell's weight is
    /// [`value_weight(richness_setting)`](value_weight), the common cell takes the
    /// rest. The draws come from the seeded [`Rng`] so a run refills the same way
    /// on any machine and after any reload.
    ///
    /// Every cell comes back `Some`: this is the one and only path that puts a
    /// broken block back, and it is paid for by having emptied the mine.
    ///
    /// `pub(crate)`: refilling a mine is something the *rules* do — on batch
    /// reset, when the last block falls — not something a front-end may ask for.
    /// A UI able to call this could hand the player an infinite mine by refilling
    /// it on demand.
    ///
    /// Clears the aim and its progress: every cell the player could have been
    /// working on is gone, and at an enlargement the coordinates may not even be on
    /// the grid any more.
    pub(crate) fn reset(&mut self, rng: &mut Rng) {
        self.break_progress = 0.0;
        self.target = None;

        let (width, height) = self.get_size();
        let (kind, value_w) = (self.kind, value_weight(self.richness_setting));
        self.grid = (0..height)
            .map(|_| {
                (0..width)
                    .map(|_| Some(draw_cell(kind, value_w, rng)))
                    .collect()
            })
            .collect();
    }

    /// Borrows the mine's grid, row by row: `Some(block)` for a cell still
    /// standing, `None` for a hole.
    ///
    /// Lends rather than copies because the borrow is the more general contract: a
    /// caller that genuinely needs an owned snapshot — a renderer on its own
    /// thread, say — says `.to_vec()` and pays for it, while one that only reads
    /// pays nothing. Handing back an owned grid bills every caller for a copy none
    /// of them can decline. The `&` is also what keeps the grid encapsulated: a
    /// shared borrow of a private field can be read and not written, which is
    /// exactly the access a front-end is owed.
    pub fn get_grid(&self) -> &[Vec<Option<Block>>] {
        &self.grid
    }

    /// Returns how many cells the mine holds when full: `width * height`.
    ///
    /// Read from `MINE_SIZES` through [`get_size`](Mine::get_size), never from the
    /// length of the grid. This is the mine's *nominal* size — what
    /// [`remaining_count`](Mine::remaining_count) is a fraction of when the UI
    /// draws a completion bar — and it must stay true of a grid dug down to
    /// nothing, which is exactly when a length-derived answer would be wrong.
    pub fn capacity(&self) -> usize {
        let (width, height) = self.get_size();
        usize::from(width) * usize::from(height)
    }

    /// Returns how many cells are still standing.
    ///
    /// Counted on every call, never stored. A stored counter would be a second
    /// place the truth lives: one break that forgets to decrement it, and the mine
    /// claims blocks the grid does not have — or batch-resets on a grid that still
    /// holds some. Derived, that bug is unwritable. The walk is over at most
    /// [`capacity`](Mine::capacity) = 200 cells, so even called every tick at
    /// 20 tps it costs nothing worth a second source of truth.
    pub fn remaining_count(&self) -> usize {
        self.grid
            .iter()
            .flatten()
            .filter(|cell| cell.is_some())
            .count()
    }

    /// Returns whether every cell has been broken.
    ///
    /// Its own method rather than a `remaining_count() == 0` at each call site
    /// because it is a *rule*, not an observation: emptying the mine is what earns
    /// the batch reset, and the threshold is deliberately not a tunable — any
    /// non-zero value would be the free partial reset the richness design forbids.
    pub fn is_empty(&self) -> bool {
        self.remaining_count() == 0
    }

    /// Returns the block at `(x, y)`, or `None` if the cell is a hole **or** off
    /// the grid.
    ///
    /// The two cases fuse deliberately. The callers are the spatial enchants
    /// (Explosive, Jackhammer, Nuke), which sweep a shape over the grid and ask
    /// one question of each cell: is there something to mine here? A blast at the
    /// corner overhangs the edge exactly the way it overhangs a hole it already
    /// dug, and neither is a mistake — so both answer "nothing here", and the
    /// shape clips itself at the border with no bounds check written by hand.
    pub fn get(&self, x: u8, y: u8) -> Option<Block> {
        *self.grid.get(usize::from(y))?.get(usize::from(x))?
    }

    /// Breaks the cell at `(x, y)`, leaving a hole, and returns the block that
    /// stood there — or `None` if it was already a hole or off the grid.
    ///
    /// Returns the block because the caller has to know what to drop: the grid is
    /// the only record of what was standing, and taking it out is what destroys
    /// that record.
    ///
    /// A no-op rather than a refusal on a hole, for the same reason
    /// [`get`](Mine::get) fuses the cases: an area enchant almost always covers
    /// ground it has already cleared. `Option::take` also makes the double-break
    /// harmless by construction — the second call finds `None` and hands back
    /// `None`, so no drop can be paid twice for one block.
    ///
    /// `pub(crate)` for the reason
    /// [`upgrade_size_level`](Mine::upgrade_size_level) is: it is **free**. It
    /// costs no progress and consults no mining power, so a front-end able to call
    /// it could empty a mine into the inventory on demand. [`dig`](Mine::dig) is
    /// the paying wrapper, which reaches here only once `break_progress` has
    /// covered `hardness * TICKS_PER_HARDNESS`.
    pub(crate) fn take(&mut self, x: u8, y: u8) -> Option<Block> {
        self.grid
            .get_mut(usize::from(y))?
            .get_mut(usize::from(x))?
            .take()
    }

    /// Breaks every cell in `cells`, returning the blocks that actually stood
    /// there.
    ///
    /// The one operation the spatial enchants are built on: they compute a shape
    /// ([`EnchantType::blast_cells`]) and hand it here. Splitting "which cells" from
    /// "break them" is what lets all three enchants share one break path — and one
    /// place where a cell can be paid out — while each keeps its own geometry.
    ///
    /// The returned [`Vec`] is **shorter than `cells`** whenever the shape covered
    /// holes, and that is the contract rather than a caveat: it is the list of
    /// blocks to drop, so a hole must not appear in it. [`take`](Mine::take) already
    /// makes that free — it hands back `None` for a hole or an off-grid cell, and
    /// `filter_map` drops those — so a shape overlapping ground it has already
    /// cleared cannot pay twice. Nor can a duplicated coordinate: the first `take`
    /// leaves a `None` behind, and the second finds it.
    ///
    /// **Does not reset the mine, even when the blast empties it** — and neither
    /// does [`dig`](Mine::dig), for the same reason: other enchants may still be
    /// about to fire on the same swing, and a refill here would drop a full grid
    /// under the ones that have not rolled yet. The swing puts
    /// [refill](Mine::refill_if_empty) **last** — see
    /// [`GameState::tick`](crate::game::GameState::tick) for the whole order — and
    /// every step before it leaves the mine as empty as it found it.
    ///
    /// `pub(crate)`, and for [`take`](Mine::take)'s reason: it is **free**. It
    /// consults no [`break_progress`](Mine::break_ratio) and no mining power, so a
    /// front-end able to call it could empty a mine into the inventory on demand.
    ///
    /// [`EnchantType::blast_cells`]: crate::enchant::EnchantType
    pub(crate) fn blast(&mut self, cells: &[(u8, u8)]) -> Vec<Block> {
        cells.iter().filter_map(|&(x, y)| self.take(x, y)).collect()
    }

    /// Rolls every spatial enchant against the swing that just broke `impact`, and
    /// breaks the shapes that fired — returning the blocks they took.
    ///
    /// One roll per enchant per **interactive** break, in [`SPATIAL_PROC_ORDER`],
    /// which is the order they consume the generator and therefore part of what a
    /// save replays. The auto-miner never reaches here: it is credited in closed form
    /// (`rate × elapsed`), so it cannot draw, and the enchants pay out for playing
    /// rather than for idling.
    ///
    /// **This is not every enchant that procs.** `SPATIAL_PROC_ORDER` is a prefix of
    /// `PROC_ORDER`: [`Excavator`](crate::enchant::EnchantType) rolls too, but it
    /// substitutes a drop instead of reshaping the grid, so it resolves in `enchant`
    /// through [`Enchants::resolve_excavator`] and draws *after* everything here. A
    /// caller resolving a swing owes the sequence both halves in that order.
    ///
    /// **A level-0 enchant is skipped before it draws**, not rolled at 0 permille.
    /// The two are indistinguishable in outcome and completely different in the
    /// sequence: a roll that always fails still advances the generator, so an
    /// enchant the player has not bought would shift every later draw in the run.
    /// Skipping is what let the Excavator be appended to `PROC_ORDER` without
    /// disturbing a single save written before it existed.
    ///
    /// **No chain reaction.** Only the cell the player actually mined rolls; the
    /// cells a blast takes do not roll again. That is what bounds the work — a
    /// swing resolves in at most three blasts — and what stops a lucky Explosive
    /// from cascading through the grid on the balance sheet of one swing.
    ///
    /// **Returns one [`SpatialProc`] per enchant that fired, not one flat list of
    /// blocks.** A flat list answers "what did the swing pay?" and nothing else,
    /// and the front-end needs more than that: `docs/UI.md` §7 draws a
    /// per-enchant flash over *the cells that blast covered*, so an event carrying
    /// only a count would leave it re-deriving the shape from the enchant level and
    /// the grid — a second copy of [`blast_cells`](crate::enchant::EnchantType)
    /// living in the wrong crate. Keeping the procs apart costs nothing here and is
    /// unrecoverable once merged.
    ///
    /// **Does not refill the mine**, for the reason [`blast`](Mine::blast) does not:
    /// the enchants may empty the grid between them, and the refill is the swing's
    /// last step ([`refill_if_empty`](Mine::refill_if_empty)). Callers here are
    /// handed an empty mine and are expected to deal with it.
    ///
    /// Takes no [`World`](crate::world::World): the cap is applied when a level is
    /// *bought* ([`Enchants::upgrade`]), so an installed level is already legal, and
    /// both [`proc_permille`](crate::enchant::EnchantType) and
    /// [`blast_cells`](crate::enchant::EnchantType) clamp what they are handed. A
    /// world here would be a parameter no line reads.
    ///
    /// [`Enchants::upgrade`]: crate::enchant::Enchants
    /// [`Enchants::resolve_excavator`]: crate::enchant::Enchants
    pub(crate) fn resolve_spatial_procs(
        &mut self,
        impact: (u8, u8),
        enchants: &Enchants,
        rng: &mut Rng,
    ) -> Vec<SpatialProc> {
        let size = self.get_size();
        let mut procs = Vec::new();

        for kind in SPATIAL_PROC_ORDER {
            let level = enchants.get_level(kind);
            if level == 0 {
                continue;
            }
            if rng.chance_permille(kind.proc_permille(level)) {
                let cells = kind.blast_cells(level, impact, size);
                let broken = self.blast(&cells);
                procs.push(SpatialProc {
                    kind,
                    cells,
                    broken,
                });
            }
        }

        procs
    }

    /// Applies one tick of mining at `mining_power`, returning the block that
    /// broke — or `None` if this tick only chipped at it.
    ///
    /// The rule, and Minecraft's: progress accumulates on a single
    /// [target](Mine::get_target) until it covers
    /// `hardness * TICKS_PER_HARDNESS`, at which point the cell yields, the
    /// progress returns to 0, and the next tick draws a new target.
    ///
    /// **Instamine is not a branch here.** A power at or above the threshold
    /// satisfies the check on its first tick, so it falls out of the same
    /// arithmetic — and because the leftover is *discarded* rather than carried
    /// into the next block, single-target speed saturates at exactly one block per
    /// tick however far past the threshold the player climbs. That saturation is
    /// what the endgame's other levers exist to answer.
    ///
    /// Returns a [`Dug`] — the block **and the cell it stood in** — not a drop.
    /// [Fortune](crate::pickaxe::Pickaxe::fortune_multiplier), Excavator and XP are
    /// the caller's to apply: the mine's business is which block stood there, and
    /// [`Block::drops`] is the block's own table, not the outcome of a swing.
    ///
    /// **The cell rides along because there is no other way to learn it.** The
    /// spatial enchants take an `impact` to centre their shapes on, and a caller
    /// cannot read [`get_target`](Mine::get_target) after the break — the aim is
    /// cleared, so the next tick draws a fresh cell — nor before it, since at
    /// [instamine](Mine::dig) speeds every tick both draws its target and breaks it,
    /// leaving no moment where the field holds the answer. Returning it is what
    /// makes the swing composable at all.
    ///
    /// **The last block to fall leaves the mine empty, and this method leaves it
    /// that way.** The batch reset is
    /// [`refill_if_empty`](Mine::refill_if_empty)'s, called at the end of the swing
    /// — see there for why the refill cannot happen on the break that earns it. The
    /// block still comes back either way: the take runs first, so the player is
    /// paid for the swing that emptied the mine whoever refills it.
    ///
    /// `pub(crate)`, and this one is not about being free — it *does* charge, in
    /// progress. It is about **rate**: nothing in core bounds how often this is
    /// called, so a front-end holding it in a render loop would mine as fast as it
    /// could redraw. The cadence is the phase-7 tick's to own, and until that sole
    /// legitimate caller exists the door stays shut.
    pub(crate) fn dig(&mut self, mining_power: f32, rng: &mut Rng) -> Option<Dug> {
        // A power that is not a positive, finite number buys nothing — and must
        // not be added: a `NaN` would poison `break_progress` for the rest of the
        // run, past every reset, since nothing compares to it. `NaN <= 0.0` is
        // false, so `is_finite` is what catches it. Same doctrine as `Rng::chance`.
        if mining_power <= 0.0 || !mining_power.is_finite() {
            return None;
        }

        let (x, y) = self.acquire_target(rng)?;
        let block = self.get(x, y)?;
        self.break_progress += mining_power;
        if self.break_progress < block.hardness() * TICKS_PER_HARDNESS {
            return None;
        }

        let broken = self.take(x, y);
        self.break_progress = 0.0;
        self.target = None;
        broken.map(|block| Dug {
            block,
            cell: (x, y),
        })
    }

    /// Drops the progress owed to the targeted cell, leaving the aim where it is.
    ///
    /// The **release** half of active-continuous mining (`docs/MECHANICS.md`): letting
    /// the mine key up does not merely stop the counter, it forfeits it. A mine cannot
    /// know whether a key is down, so this only does the forgetting and
    /// [`tick`](crate::game::GameState::tick) owns the when — the same split
    /// [`dig`](Mine::dig) makes about *rate*.
    ///
    /// **The target survives, deliberately.** Dropping it too would have the next
    /// swing draw a fresh one, and a mine's cells are not interchangeable: tapping the
    /// key until the value cell came up would be a way to fish for it. Keeping the aim
    /// also means this **draws nothing**, which is what lets a released tick stay
    /// inert in the generator's sequence — the property
    /// [`draw_target`](Mine::draw_target) protects one draw at a time.
    ///
    /// Idempotent, so the caller may run it on *every* released tick rather than
    /// detecting the edge — an edge is state, and state a save would have to carry.
    pub(crate) fn forfeit_progress(&mut self) {
        self.break_progress = 0.0;
    }

    /// Refills the grid if the last cell is gone, reporting whether it did.
    ///
    /// The **batch reset**, hoisted out of [`dig`](Mine::dig) so that a whole swing
    /// resolves before the mine comes back. It has to be here rather than there,
    /// because a swing empties a mine in more ways than a break: an Explosive,
    /// a Jackhammer or a Nuke can take the last cells, and
    /// [`resolve_spatial_procs`](Mine::resolve_spatial_procs) may still have
    /// enchants left to roll after the one that did. A refill fired mid-swing would
    /// drop a *full* grid under those, and they would blast cells the player never
    /// mined down to — paying out a grid and a half for one swing.
    ///
    /// So the swing owner puts **procs before refill**, and this is its last step —
    /// [`GameState::tick`](crate::game::GameState::tick) states the whole order and
    /// is the one place that does. That order costs the arrangement `dig`'s rustdoc used
    /// to argue for — "two calls to chain is one call to forget" — and the answer to
    /// the forgetting is no longer visibility but the **return value**: it is
    /// `#[must_use]`, so a caller who drops the answer is told, and the one caller
    /// that exists needs it anyway: [`MineRefilled`](crate::game::GameEvent) is an
    /// announcement the player is owed, so the swing has to know whether one
    /// happened.
    ///
    /// `pub(crate)` for [`reset`](Mine::reset)'s reason, weakened but not gone: this
    /// one is conditional, so a front-end calling it on a standing grid gets
    /// nothing. On an *empty* one it is still a free full mine, and emptiness is a
    /// state a front-end can wait for.
    #[must_use]
    pub(crate) fn refill_if_empty(&mut self, rng: &mut Rng) -> bool {
        if !self.is_empty() {
            return false;
        }
        self.reset(rng);
        true
    }

    /// Returns the cell [`dig`](Mine::dig) is working on, drawing a new one when
    /// the last target is gone — and forfeiting the progress that was owed to it.
    ///
    /// The re-validation (`self.get(x, y).is_some()`) is what makes target
    /// invalidation **structural** rather than a rule each caller remembers. A
    /// target can vanish under the digger in more ways than `dig` can enumerate:
    /// phase 4's area enchants will [`take`](Mine::take) whole blast shapes, and
    /// the cell being aimed at is as likely to be in one as any other. Checking
    /// that the cell still stands answers all of them at once, and the enchants
    /// need not know this field exists.
    ///
    /// Progress resets with the target because it was *earned against a block that
    /// is gone*. Carrying it to the next cell would let a player bank swings on an
    /// Obsidian and spend them on a Netherrack.
    fn acquire_target(&mut self, rng: &mut Rng) -> Option<(u8, u8)> {
        if let Some((x, y)) = self.target
            && self.get(x, y).is_some()
        {
            return Some((x, y));
        }

        self.break_progress = 0.0;
        self.target = self.draw_target(rng);
        self.target
    }

    /// Draws one standing cell, uniformly; `None` only when the mine is empty.
    ///
    /// **Collects the standing cells rather than rejection-sampling the grid**,
    /// and the reason is the generator, not the speed. Re-drawing until a draw
    /// misses a hole would advance the sequence a *variable* number of times,
    /// decided by how holey the grid happens to be — and the position in that
    /// sequence is run state that a save carries and a replay must reproduce. One
    /// draw per target, always, is a contract a golden vector can hold.
    ///
    /// The walk costs at most [`capacity`](Mine::capacity) = 200 cells, which at
    /// 20 tps is nothing — the same trade [`remaining_count`](Mine::remaining_count)
    /// already makes to avoid a second source of truth.
    fn draw_target(&self, rng: &mut Rng) -> Option<(u8, u8)> {
        let standing: Vec<(u8, u8)> = self
            .grid
            .iter()
            .enumerate()
            .flat_map(|(y, row)| {
                row.iter()
                    .enumerate()
                    .filter(|(_, cell)| cell.is_some())
                    .map(move |(x, _)| (x as u8, y as u8))
            })
            .collect();

        let count = NonZeroU32::new(standing.len() as u32)?;
        standing.get(rng.below(count) as usize).copied()
    }

    /// Returns the cell [`dig`](Mine::dig) is currently working on, for the UI to
    /// highlight; `None` before the first swing and right after a block falls.
    pub fn get_target(&self) -> Option<(u8, u8)> {
        self.target
    }

    /// Returns how far the [target](Mine::get_target) is from breaking, in
    /// `0.0..=1.0` — the progress bar and the `.:#` crack glyph both read this.
    ///
    /// A *ratio*, not the raw counter, because the counter alone is meaningless to
    /// a reader: 20 points of progress is nearly through a Netherrack and barely a
    /// scratch on an Obsidian. Handing out the fraction keeps
    /// [`TICKS_PER_HARDNESS`] an implementation detail of the rules rather than a
    /// number every front-end has to know to draw a bar.
    ///
    /// No division by zero is reachable: the softest block in the game is
    /// Netherrack at `0.4`. The clamp guards the tick where an instamining power
    /// overshoots the threshold, which would otherwise report a bar past full for
    /// the instant before the block falls.
    pub fn break_ratio(&self) -> f32 {
        let Some((x, y)) = self.target else {
            return 0.0;
        };
        let Some(block) = self.get(x, y) else {
            return 0.0;
        };
        (self.break_progress / (block.hardness() * TICKS_PER_HARDNESS)).clamp(0.0, 1.0)
    }

    /// Returns the mine's size level, which indexes into `MINE_SIZES`.
    pub fn get_size_level(&self) -> u32 {
        self.size_level
    }

    /// The `(width, height)` a mine of size level `level` has, whether or not such
    /// a mine exists.
    ///
    /// **An associated function and not a method**, because it reads no field — and
    /// that is the entire reason it is public. A run creates its mines *lazily*, on
    /// first entry ([`GameState`](crate::game::GameState)), so eleven of the twelve
    /// have no [`Mine`] at all until the player walks in; the Mines screen
    /// nevertheless prints a size on every row (`docs/UI.md` §5.2), and
    /// `size_for_level(0)` is what it asks for those. The alternative was to build
    /// all twelve grids up front — twelve draws from the generator for mines a run
    /// may never open — or to copy the table into the front-end.
    ///
    /// Anything past the table clamps to the largest size rather than panicking.
    /// [`upgrade_size_level`](Mine::upgrade_size_level) is the only thing that
    /// writes the field, and it stops at [`MAX_SIZE_LEVEL`], so a live mine cannot
    /// reach the clamp. A loaded one cannot either, since
    /// [`validate`](Mine::validate) refuses a save whose size level is past the
    /// table. The clamp stays anyway, because it is what makes this function
    /// *total*: it is called from inside `validate` itself, and an accessor that
    /// panicked on the very state the validator was built to catch would take the
    /// process down before the refusal could be returned.
    pub fn size_for_level(level: u32) -> (u8, u8) {
        let index = level as usize;
        if index < MINE_SIZES.len() {
            MINE_SIZES[index]
        } else {
            MINE_SIZES[MINE_SIZES.len() - 1] // Return the largest size if out of bounds
        }
    }

    /// Returns this mine's `(width, height)` in blocks.
    pub fn get_size(&self) -> (u8, u8) {
        Self::size_for_level(self.size_level)
    }

    /// Whether the mine already fills the largest grid the size table holds, so
    /// [`upgrade_size_level`](Mine::upgrade_size_level) would refuse — for a UI to
    /// grey out the buy button before it is pressed.
    pub fn is_size_maxed(&self) -> bool {
        self.size_level >= MAX_SIZE_LEVEL
    }

    /// Whether this mine could have been produced by the rules.
    ///
    /// **Why a mine needs one at all.** Every other path into these fields goes
    /// through a method that checks — the dial refuses to pass its ceiling,
    /// `upgrade_size_level` stops at [`MAX_SIZE_LEVEL`], `reset` builds the grid
    /// *from* the size. Deserialisation goes through none of them: serde writes
    /// private fields directly, so a save file can describe a mine no play could
    /// reach. This is where that door is shut.
    ///
    /// It lives on [`Mine`] rather than in `save` because the invariants are the
    /// mine's own; the save module composes, and would otherwise need every field
    /// of every type made visible to it.
    ///
    /// The message is a `&'static str` and not a [`CoreError`]. That enum answers
    /// *the player* — "you are 40 Iron short", "buy 4 more richness levels" — and
    /// a broken file gives them nothing to act on but the recovery screen, which
    /// says the same thing whatever the field was. The string is for the bug
    /// report.
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.size_level > MAX_SIZE_LEVEL {
            return Err("a mine's size level is past the largest size there is");
        }
        if self.richness_level > MAX_RICHNESS_LEVEL {
            return Err("a mine's richness level is past the highest one for sale");
        }
        if self.richness_setting > self.richness_level {
            return Err("a mine's richness dial is above the ceiling bought for it");
        }

        // The grid *is* the composition and the hole mask at once, so a grid of the
        // wrong shape is not a cosmetic mismatch: `dig` draws a target from the
        // cells it finds, and the size the level promises is what every buyer of an
        // enlargement paid for.
        let (width, height) = self.get_size();
        let shaped = self.grid.len() == usize::from(height)
            && self.grid.iter().all(|row| row.len() == usize::from(width));
        if !shaped {
            return Err("a mine's grid is not the size its level says it is");
        }

        // A `NaN` here would survive every reset — `NaN >= threshold` is false
        // forever — and quietly turn the mine into one that can never be dug.
        if !self.break_progress.is_finite() || self.break_progress < 0.0 {
            return Err("a mine's break progress is not a number of ticks");
        }

        match self.target {
            Some((x, y)) if self.get(x, y).is_none() => {
                Err("a mine is aimed at a cell that is not standing")
            }
            _ => Ok(()),
        }
    }

    /// Increases the mine's size level by 1 and resets the grid to the new size,
    /// or refuses if the mine is already the largest the table describes.
    ///
    /// The refusal is the point. Without it the level keeps climbing while
    /// [`get_size`](Mine::get_size) clamps, so levels 10, 11, 12… each move the
    /// mine's state and hand back **no extra blocks** — once the enlargement is
    /// paid for in the mine's own ore, that is a purchase that charges the player
    /// for nothing.
    ///
    /// `pub(crate)` because it is **free**: a mine enlargement is paid for in the
    /// mine's own ore, and the transaction that debits it lives in
    /// [`economy`](crate::economy), which is its only caller. A front-end able to
    /// call this directly could grow a mine to 20x10 at no cost.
    pub(crate) fn upgrade_size_level(&mut self, rng: &mut Rng) -> Result<(), CoreError> {
        if self.size_level >= MAX_SIZE_LEVEL {
            return Err(CoreError::MineSizeMaxed {
                level: MAX_SIZE_LEVEL,
            });
        }
        self.size_level += 1;
        self.reset(rng);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::ALL_BLOCKS;
    use crate::boost::Boost;
    use crate::enchant::{EnchantType, Enchants};
    use crate::pickaxe::{Pickaxe, PickaxeTier};
    use crate::tunables::{BOOST_DURATION_TICKS, BOOST_MULTIPLIER};
    use crate::world::World;

    /// A generator on a fixed seed. The composition tests only need *a*
    /// reproducible sequence, not a particular one.
    fn rng() -> Rng {
        Rng::from_seed(42)
    }

    /// A mine at the given size level, richness 0. The kind and grid are
    /// irrelevant to sizing, which reads `size_level` alone.
    fn mine_at(size_level: u32) -> Mine {
        Mine {
            kind: MineKind::default(),
            size_level,
            richness_level: 0,
            richness_setting: 0,
            grid: Vec::new(),
            break_progress: 0.0,
            target: None,
        }
    }

    /// A fully-drawn mine of a given kind, size and richness setting. Builds the
    /// struct directly (as a deserialiser would) so a test can dial in a richness
    /// the phase-5 purchase path cannot yet produce, then fills the grid.
    fn built(kind: MineKind, size_level: u32, richness_setting: u32, rng: &mut Rng) -> Mine {
        let mut mine = Mine {
            kind,
            size_level,
            richness_level: richness_setting,
            richness_setting,
            grid: Vec::new(),
            break_progress: 0.0,
            target: None,
        };
        mine.reset(rng);
        mine
    }

    /// A few draws off a generator, to read its *position*. Two generators that
    /// have been asked for the same work agree here; one that was asked for a
    /// draw the other was not does not.
    fn draws(rng: &mut Rng) -> Vec<Option<usize>> {
        (0..8).map(|_| rng.weighted(&[1, 1])).collect()
    }

    /// How many standing cells of `block` the grid holds. Holes count as neither.
    fn count(mine: &Mine, block: Block) -> usize {
        mine.grid
            .iter()
            .flatten()
            .filter(|&&cell| cell == Some(block))
            .count()
    }

    #[test]
    fn size_level_indexes_the_dimension_table() {
        assert_eq!(mine_at(0).get_size(), (3, 3));
        assert_eq!(mine_at(4).get_size(), (10, 6));
        assert_eq!(mine_at(9).get_size(), (20, 10));
    }

    /// Both tables are answerable without a mine, which is what the Mines screen
    /// needs for the eleven mines a run has never created.
    #[test]
    fn the_dial_curve_can_be_read_without_a_mine_to_read_it_on() {
        // A fresh mine's dial is 0, so this is the row an unvisited mine draws.
        assert_eq!(Mine::value_weight_percent_for(0), value_weight(0));
        // And the method is the function read at the mine's own setting.
        let mut mine = mine_at(0);
        for setting in 0..=MAX_RICHNESS_LEVEL {
            while mine.get_richness_level() < setting {
                assert!(mine.upgrade_richness_level().is_ok());
            }
            assert!(mine.set_richness_setting(setting, &mut rng()).is_ok());
            assert_eq!(
                mine.value_weight_percent(),
                Mine::value_weight_percent_for(setting)
            );
        }
    }

    /// The table is answerable without a mine, which is what the Mines screen needs
    /// for the eleven mines a run has never created.
    #[test]
    fn a_size_can_be_looked_up_without_a_mine_to_look_it_up_on() {
        assert_eq!(Mine::size_for_level(0), (3, 3));
        assert_eq!(Mine::size_for_level(MAX_SIZE_LEVEL), (20, 10));
        // Past the table it clamps rather than panicking: this is the accessor
        // `validate` itself calls, so it has to answer for a level no play produces.
        assert_eq!(
            Mine::size_for_level(MAX_SIZE_LEVEL + 1),
            Mine::size_for_level(MAX_SIZE_LEVEL)
        );
        // And the method is the function read at the mine's own level, not a second
        // lookup that could disagree with it.
        for level in 0..=MAX_SIZE_LEVEL {
            assert_eq!(mine_at(level).get_size(), Mine::size_for_level(level));
        }
    }

    /// A bigger mine must never be a smaller one: mine size is a reward, so the
    /// table has to grow. Height is allowed to plateau, but the block count the
    /// player must clear has to strictly increase, or a size level would cost an
    /// upgrade and hand back nothing.
    #[test]
    fn mines_only_ever_grow_with_their_size_level() {
        for pair in MINE_SIZES.windows(2) {
            let ((width, height), (next_width, next_height)) = (pair[0], pair[1]);
            assert!(
                next_width >= width && next_height >= height,
                "({next_width}, {next_height}) is smaller than ({width}, {height})"
            );

            let (area, next_area) = (
                u32::from(width) * u32::from(height),
                u32::from(next_width) * u32::from(next_height),
            );
            assert!(
                next_area > area,
                "a mine of {next_width}x{next_height} holds no more blocks than one of {width}x{height}"
            );
        }
    }

    /// `size_level` is a `u32` but the table has only 10 rows, so every level
    /// past the end must clamp to the largest mine rather than panic on the
    /// index. `upgrade_size_level` can no longer produce such a level — but the
    /// field is plain data, and phase 9 will read it straight back out of a save
    /// file, so the clamp guards a save the player edited or a disk corrupted.
    /// This test builds those levels the way a deserialiser would.
    #[test]
    fn size_levels_past_the_table_clamp_to_the_largest_mine() {
        let largest = mine_at(9).get_size();
        for size_level in [10, 11, 100, u32::MAX] {
            assert_eq!(
                mine_at(size_level).get_size(),
                largest,
                "size level {size_level} should clamp to the largest mine"
            );
        }
    }

    /// A fresh mine is a *weighted mix*, not a uniform block: at richness 0 the
    /// value cell already appears (its weight is non-zero), which is the only way
    /// the dense blocks enter the game at all — but the common cell dominates. A
    /// large mine and a fixed seed make the proportions stable to assert.
    #[test]
    fn new_mine_is_a_weighted_mix_dominated_by_the_common_block() {
        let mine = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut rng());
        let common = MineKind::Iron.common_block();
        let value = MineKind::Iron.value_block();
        let (width, height) = mine.get_size();

        assert_eq!(mine.grid.len(), height as usize);
        for row in &mine.grid {
            assert_eq!(row.len(), width as usize);
            for &cell in row {
                assert!(
                    cell == Some(common) || cell == Some(value),
                    "stray cell {cell:?}"
                );
            }
        }

        let (commons, values) = (count(&mine, common), count(&mine, value));
        assert!(
            values >= 1,
            "richness 0 must still sprinkle in the value cell"
        );
        assert!(
            commons > values,
            "the common cell must dominate at richness 0 ({commons} vs {values})"
        );
    }

    #[test]
    fn a_mine_reports_its_kind() {
        assert_eq!(
            Mine::new(MineKind::Quartz, &mut rng()).kind(),
            MineKind::Quartz
        );
    }

    #[test]
    fn upgrading_the_size_level_increases_the_grid_dimensions() {
        let mut mine = Mine::new(MineKind::Stone, &mut rng());
        let (initial_width, initial_height) = mine.get_size();
        assert!(mine.upgrade_size_level(&mut rng()).is_ok());
        let (new_width, new_height) = mine.get_size();
        assert!(new_width > initial_width);
        assert!(new_height >= initial_height);
    }

    #[test]
    fn size_level_is_correctly_updated_when_upgrading() {
        let mut mine = Mine::new(MineKind::Stone, &mut rng());
        let initial_size_level = mine.get_size_level();
        assert!(mine.upgrade_size_level(&mut rng()).is_ok());
        let new_size_level = mine.get_size_level();
        assert_eq!(new_size_level, initial_size_level + 1);
    }

    /// The whole table must be walkable, and the walk must stop exactly at the
    /// end of it — not one level further, where `get_size` starts clamping.
    #[test]
    fn the_size_ladder_ends_at_the_last_row_of_the_table() {
        let mut mine = Mine::new(MineKind::Stone, &mut rng());
        while mine.get_size_level() < MAX_SIZE_LEVEL {
            assert!(mine.upgrade_size_level(&mut rng()).is_ok());
        }
        assert_eq!(mine.get_size(), MINE_SIZES[MINE_SIZES.len() - 1]);
    }

    /// The refusal that keeps a paid enlargement honest. Past the table the
    /// dimensions stop growing while `size_level` used to keep climbing, so the
    /// player would be sold levels 10, 11, 12… and receive not one extra block.
    /// The refused call must also leave the level where it was: a debit followed
    /// by a no-op is the same theft, one step later.
    #[test]
    fn a_mine_at_the_largest_size_refuses_to_grow_and_changes_nothing() {
        let mut mine = mine_at(MAX_SIZE_LEVEL);
        let size = mine.get_size();

        assert_eq!(
            mine.upgrade_size_level(&mut rng()),
            Err(CoreError::MineSizeMaxed {
                level: MAX_SIZE_LEVEL,
            })
        );

        assert_eq!(mine.get_size_level(), MAX_SIZE_LEVEL);
        assert_eq!(mine.get_size(), size);
    }

    #[test]
    fn reset_rebuilds_the_whole_grid_from_the_block_pool() {
        let mut mine = built(MineKind::Stone, 3, 0, &mut rng());
        let common = MineKind::Stone.common_block();
        let value = MineKind::Stone.value_block();
        mine.grid[0][0] = Some(Block::IronOre); // a block from neither pool
        mine.reset(&mut rng());
        let (width, height) = mine.get_size();
        assert_eq!(mine.grid.len(), height as usize);
        for row in &mine.grid {
            assert_eq!(row.len(), width as usize);
            for &cell in row {
                assert!(
                    cell == Some(common) || cell == Some(value),
                    "stray cell {cell:?}"
                );
            }
        }
    }

    /// The grid the UI renders and the cell the rules read are one truth, so the
    /// two ways out of it must never disagree — holes included, which is the half
    /// worth pinning: a renderer that drew a block the rules had already taken
    /// would be showing the player ore they cannot mine.
    #[test]
    fn get_grid_and_get_report_the_same_cells() {
        let mut mine = built(MineKind::Stone, 2, 0, &mut rng());
        assert!(mine.take(1, 1).is_some());

        let (width, height) = mine.get_size();
        let grid = mine.get_grid();
        assert_eq!(grid.len(), usize::from(height));
        for y in 0..height {
            assert_eq!(grid[usize::from(y)].len(), usize::from(width));
            for x in 0..width {
                assert_eq!(
                    grid[usize::from(y)][usize::from(x)],
                    mine.get(x, y),
                    "the two readings disagree at ({x}, {y})"
                );
            }
        }
    }

    /// A fresh mine reports richness 0 on both tracks: the ceiling has no buyer
    /// yet, and the dial sits at the floor.
    #[test]
    fn a_fresh_mine_has_no_richness() {
        let mine = Mine::new(MineKind::Iron, &mut rng());
        assert_eq!(mine.get_richness_level(), 0);
        assert_eq!(mine.get_richness_setting(), 0);
    }

    /// The ceiling ladder is walkable to its top rung and stops exactly there. The
    /// refusal is what keeps a paid purchase honest, the same way the size track's
    /// is: past [`MAX_RICHNESS_LEVEL`] the weight formula clamps, so a player sold
    /// level 10 would receive not one extra value cell. `is_richness_maxed` must
    /// agree with the refusal at every step, since the UI greys the button off the
    /// former while the purchase is refused by the latter.
    #[test]
    fn the_richness_ceiling_ends_at_its_top_rung_and_refuses_past_it() {
        let mut mine = Mine::new(MineKind::Stone, &mut rng());
        while !mine.is_richness_maxed() {
            assert!(mine.upgrade_richness_level().is_ok());
        }
        assert_eq!(mine.get_richness_level(), MAX_RICHNESS_LEVEL);

        assert_eq!(
            mine.upgrade_richness_level(),
            Err(CoreError::RichnessLevelMaxed {
                level: MAX_RICHNESS_LEVEL,
            })
        );
        assert_eq!(
            mine.get_richness_level(),
            MAX_RICHNESS_LEVEL,
            "a refused upgrade must not creep the ceiling past the table"
        );
    }

    /// The formula's shape: a non-zero floor (richness 0 is still mixed), a linear
    /// climb, and a clamp past the top rung — so the value weight never reaches
    /// 100%, which keeps the common weight (`100 - value`) strictly positive and
    /// the two-way distribution non-degenerate at every setting.
    #[test]
    fn value_weight_ramps_then_clamps_and_never_starves_the_common_cell() {
        assert_eq!(value_weight(0), 10);
        assert_eq!(value_weight(1), 19);
        assert_eq!(value_weight(MAX_RICHNESS_LEVEL), 91);
        for setting in [MAX_RICHNESS_LEVEL + 1, 100, u32::MAX] {
            assert_eq!(
                value_weight(setting),
                value_weight(MAX_RICHNESS_LEVEL),
                "settings past the top rung must clamp"
            );
        }
        for setting in 0..=(MAX_RICHNESS_LEVEL + 2) {
            let value = value_weight(setting);
            assert!(
                value < 100,
                "value weight {value} would starve the common cell"
            );
        }
    }

    /// Richness *is* the weight of the value cell: pushing the dial up puts more
    /// value cells in the grid. Same seed, same mine, only the setting differs.
    #[test]
    fn richness_setting_shifts_weight_toward_the_value_cell() {
        let value = MineKind::Iron.value_block();
        let poor = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut rng());
        let rich = built(
            MineKind::Iron,
            MAX_SIZE_LEVEL,
            MAX_RICHNESS_LEVEL,
            &mut rng(),
        );
        assert!(
            count(&rich, value) > count(&poor, value),
            "a richer dial must yield more value cells ({} vs {})",
            count(&rich, value),
            count(&poor, value)
        );
    }

    /// Every setting — including ones past the top rung, as a corrupt save might
    /// hold — must fill the grid completely from the two-block pool. This is the
    /// only structural invariant richness carries: the composition always
    /// describes a valid distribution, so `weighted` never returns `None` and no
    /// cell is left unset.
    #[test]
    fn every_setting_yields_a_full_grid_from_the_pool() {
        let (common, value) = (
            MineKind::Amethyst.common_block(),
            MineKind::Amethyst.value_block(),
        );
        for setting in 0..=(MAX_RICHNESS_LEVEL + 3) {
            let mine = built(MineKind::Amethyst, 5, setting, &mut rng());
            let (width, height) = mine.get_size();
            assert_eq!(mine.grid.len(), height as usize);
            for row in &mine.grid {
                assert_eq!(row.len(), width as usize);
                for &cell in row {
                    assert!(
                        cell == Some(common) || cell == Some(value),
                        "setting {setting}: stray cell {cell:?}"
                    );
                }
            }
        }
    }

    /// The determinism contract, at the grid level: the same seed refills the
    /// same mine identically, and different seeds do not. Without this a reloaded
    /// run would re-roll its mines, and balance tests could assert nothing.
    #[test]
    fn the_same_seed_reproduces_the_same_grid() {
        let a = built(
            MineKind::Amethyst,
            MAX_SIZE_LEVEL,
            5,
            &mut Rng::from_seed(7),
        );
        let b = built(
            MineKind::Amethyst,
            MAX_SIZE_LEVEL,
            5,
            &mut Rng::from_seed(7),
        );
        let c = built(
            MineKind::Amethyst,
            MAX_SIZE_LEVEL,
            5,
            &mut Rng::from_seed(8),
        );
        assert_eq!(a.grid, b.grid, "same seed must refill identically");
        assert_ne!(a.grid, c.grid, "different seeds must differ");
    }

    /// A drawn mine has no holes in it: every one of its `capacity` cells stands.
    /// This is what makes `remaining_count` a measure of progress at all.
    #[test]
    fn a_fresh_mine_is_full_to_its_capacity() {
        let mine = built(MineKind::Iron, 4, 0, &mut rng());
        assert_eq!(mine.capacity(), 10 * 6);
        assert_eq!(mine.remaining_count(), mine.capacity());
        assert!(!mine.is_empty());
    }

    /// `capacity` is the mine's *nominal* size and must not drift with the digging:
    /// it is the denominator the UI shows progress against, so a `capacity` that
    /// shrank as blocks fell would leave the player forever at 100%. Each break
    /// takes exactly one cell off `remaining_count` — no more, no less.
    #[test]
    fn capacity_is_the_nominal_size_not_the_blocks_left() {
        let mut mine = built(MineKind::Stone, 2, 0, &mut rng());
        let capacity = mine.capacity();

        for broken in 1..=3 {
            assert!(mine.take(broken - 1, 0).is_some());
            assert_eq!(mine.capacity(), capacity, "capacity must not move");
            assert_eq!(mine.remaining_count(), capacity - usize::from(broken));
        }
    }

    /// The break, end to end: `take` hands back the block that `get` was reporting
    /// — the caller needs it to know what to drop, and the grid is the only record
    /// of what stood there — and leaves a hole behind it.
    #[test]
    fn taking_a_cell_returns_the_block_and_leaves_a_hole() {
        let mut mine = built(MineKind::Iron, 3, 0, &mut rng());
        let standing = mine.get(2, 1);
        assert!(standing.is_some(), "a fresh mine stands at (2, 1)");

        assert_eq!(mine.take(2, 1), standing);
        assert_eq!(mine.get(2, 1), None, "the cell must now be a hole");
    }

    /// Breaking a hole, or swinging off the edge, is a no-op and not a refusal:
    /// an area enchant almost always covers ground it has already cleared, and the
    /// corner of the grid clips the same way. The load-bearing half is
    /// `remaining_count`, which must not move — a second `take` that paid a second
    /// drop for one block would be free ore.
    #[test]
    fn taking_a_hole_or_a_cell_off_the_grid_yields_nothing() {
        let mut mine = built(MineKind::Stone, 3, 0, &mut rng());
        assert!(mine.take(0, 0).is_some());
        let remaining = mine.remaining_count();

        assert_eq!(mine.take(0, 0), None, "breaking a hole must yield nothing");
        let (width, height) = mine.get_size();
        assert_eq!(mine.take(width, 0), None);
        assert_eq!(mine.take(0, height), None);

        assert_eq!(
            mine.remaining_count(),
            remaining,
            "no-op breaks must not consume a block"
        );
    }

    /// Emptying the grid is what earns the batch reset, so `is_empty` has to turn
    /// over exactly when the last cell falls — not one break early, not never.
    #[test]
    fn a_mine_emptied_cell_by_cell_reports_empty() {
        let mut mine = built(MineKind::Amethyst, 0, 0, &mut rng());
        let (width, height) = mine.get_size();

        for y in 0..height {
            for x in 0..width {
                assert!(!mine.is_empty(), "still standing at ({x}, {y})");
                assert!(mine.take(x, y).is_some());
            }
        }

        assert!(mine.is_empty());
        assert_eq!(mine.remaining_count(), 0);
    }

    /// Off the grid reads as `None` on every side, which is what lets the spatial
    /// enchants sweep a shape across a corner without a single bounds check.
    #[test]
    fn get_off_the_grid_is_none_on_every_side() {
        let mine = built(MineKind::Iron, 0, 0, &mut rng());
        let (width, height) = mine.get_size();

        assert!(mine.get(width - 1, height - 1).is_some(), "the far corner");
        assert_eq!(mine.get(width, 0), None, "past the right edge");
        assert_eq!(mine.get(0, height), None, "past the bottom edge");
        assert_eq!(mine.get(u8::MAX, u8::MAX), None, "far outside");
    }

    /// The heart of the dial: it reshapes what is still standing and does **not**
    /// touch what is gone. A dial that refilled the holes would be a free batch
    /// reset — break the four Amethyst out of two hundred, nudge the dial, find
    /// four more — which is the "no free action may ever put a broken block back"
    /// rule in MECHANICS.md. `remaining_count` is the load-bearing assertion:
    /// were one hole refilled, it would climb.
    #[test]
    fn moving_the_dial_rerolls_the_standing_cells_and_leaves_the_holes() {
        let mut rng = rng();
        let mut mine = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut rng);
        mine.richness_level = MAX_RICHNESS_LEVEL; // the phase-5 purchase, by hand

        let holes = [(0, 0), (3, 2), (5, 4)];
        for (x, y) in holes {
            assert!(mine.take(x, y).is_some());
        }
        let (remaining, capacity) = (mine.remaining_count(), mine.capacity());

        assert_eq!(
            mine.set_richness_setting(MAX_RICHNESS_LEVEL, &mut rng),
            Ok(())
        );

        for (x, y) in holes {
            assert_eq!(
                mine.get(x, y),
                None,
                "the dial refilled the hole at ({x}, {y})"
            );
        }
        assert_eq!(
            mine.remaining_count(),
            remaining,
            "the dial must not change how many cells stand"
        );
        assert_eq!(mine.capacity(), capacity);
        assert_eq!(mine.get_richness_setting(), MAX_RICHNESS_LEVEL);
    }

    /// The dial is *immediate*: the player pushes it up and the cells still
    /// standing are richer at once — they do not wait for the next regeneration.
    /// Sibling of `richness_setting_shifts_weight_toward_the_value_cell`, which
    /// proves the same of a grid *built* at a setting; this one proves it of a
    /// grid *moved* to one, which is the path a player actually takes.
    #[test]
    fn moving_the_dial_up_shifts_the_standing_cells_toward_the_value_block() {
        let mut rng = rng();
        let mut mine = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut rng);
        mine.richness_level = MAX_RICHNESS_LEVEL;
        let value = MineKind::Iron.value_block();
        let poor = count(&mine, value);

        assert_eq!(
            mine.set_richness_setting(MAX_RICHNESS_LEVEL, &mut rng),
            Ok(())
        );

        let rich = count(&mine, value);
        assert!(
            rich > poor,
            "the dial must enrich what stands ({rich} value cells, was {poor})"
        );
    }

    /// The dial is free, but only below what was *bought*: the ceiling is the
    /// purchase. The refusal names both numbers so the UI can say how many levels
    /// are still to buy — and, like every refusal in the core, it must leave the
    /// mine exactly as it found it, grid included.
    #[test]
    fn a_dial_above_the_bought_ceiling_is_refused_and_changes_nothing() {
        let mut rng = rng();
        let mut mine = built(MineKind::Stone, 3, 1, &mut rng);
        mine.richness_level = 2;
        let grid = mine.grid.clone();

        assert_eq!(
            mine.set_richness_setting(3, &mut rng),
            Err(CoreError::RichnessAboveCeiling {
                requested: 3,
                ceiling: 2,
            })
        );

        assert_eq!(
            mine.get_richness_setting(),
            1,
            "the dial must not have moved"
        );
        assert_eq!(mine.grid, grid, "a refusal must not redraw the grid");
    }

    /// Asking for the setting the dial already holds is not a move, so it draws
    /// nothing. The grid is only half of that: the generator's *position* is run
    /// state, and an order that said nothing must not advance it — otherwise the
    /// sequence a save resumes would depend on how many times the player poked a
    /// dial that never moved. Proven against a twin generator that was asked for
    /// the same work minus the no-op.
    #[test]
    fn setting_the_dial_where_it_already_is_draws_nothing() {
        let (mut dialled, mut untouched) = (rng(), rng());
        let mut mine = built(MineKind::Iron, 2, 0, &mut dialled);
        mine.richness_level = MAX_RICHNESS_LEVEL;
        let grid = mine.grid.clone();

        assert_eq!(mine.set_richness_setting(0, &mut dialled), Ok(()));

        assert_eq!(mine.grid, grid, "a dial that did not move must not redraw");
        let _ = built(MineKind::Iron, 2, 0, &mut untouched);
        assert_eq!(
            draws(&mut dialled),
            draws(&mut untouched),
            "the no-op advanced the generator"
        );
    }

    /// The determinism contract, extended to the dial: a run replays its dial
    /// moves, holes and all. Without this a reloaded save would re-roll every
    /// grid the player had reshaped, and "send me your save, I will reproduce
    /// your bug" would stop being true the moment a dial was touched.
    #[test]
    fn the_same_seed_replays_the_same_dial_moves() {
        fn run(seed: u64) -> Mine {
            let mut rng = Rng::from_seed(seed);
            let mut mine = built(MineKind::Amethyst, 4, 0, &mut rng);
            mine.richness_level = MAX_RICHNESS_LEVEL;
            assert!(mine.take(0, 0).is_some());
            for setting in [5, 2, MAX_RICHNESS_LEVEL] {
                assert_eq!(mine.set_richness_setting(setting, &mut rng), Ok(()));
            }
            mine
        }

        assert_eq!(run(7), run(7), "same seed must replay identically");
        assert_ne!(run(7), run(8), "different seeds must differ");
    }

    /// The refill is the *only* thing that puts broken blocks back, and the mine
    /// must have been emptied to earn it — see the "no free action may ever put a
    /// broken block back" rule in MECHANICS.md. This pins the half `reset` owns:
    /// once called, not one hole survives.
    #[test]
    fn reset_fills_the_holes_back_in() {
        let mut mine = built(MineKind::Stone, 1, 0, &mut rng());
        assert!(mine.take(0, 0).is_some());
        assert!(mine.take(1, 0).is_some());
        assert!(mine.remaining_count() < mine.capacity());

        mine.reset(&mut rng());

        assert_eq!(mine.remaining_count(), mine.capacity());
        assert!(mine.get(0, 0).is_some());
    }

    /// The mining power of an unenchanted pickaxe, which is its tier's base speed
    /// and nothing else — the figure the wiki's break times are quoted against.
    fn bare(tier: PickaxeTier) -> f32 {
        Pickaxe::new(tier, Enchants::new()).mining_power()
    }

    /// Digs until something breaks, and says how many ticks it took.
    fn ticks_to_break(mine: &mut Mine, power: f32, rng: &mut Rng) -> u32 {
        for tick in 1..=100_000 {
            if mine.dig(power, rng).is_some() {
                return tick;
            }
        }
        unreachable!("the block never broke")
    }

    /// **The golden test of the break formula**, and the only one that answers to
    /// an authority outside this repository: Minecraft says a Diamond pickaxe
    /// clears Obsidian in 9.4 seconds, which at 20 tps is 188 ticks. If this number
    /// moves, Skylode has stopped being the game it claims to port.
    ///
    /// Three accidents make the assertion exact rather than approximate, and all
    /// three are why *this* pairing was chosen:
    ///
    /// - Obsidian is the one mine whose two cells share a hardness (Obsidian and
    ///   Crying Obsidian are both 50), so the random target cannot change the
    ///   answer and the test needs no control over the draw.
    /// - Diamond's `base_power` is 8.0, which is Minecraft's own speed for that
    ///   tier — the tier curve only departs from Minecraft at Gold.
    /// - Diamond is Obsidian's `min_pickaxe_tier`, so this is a swing the phase-3
    ///   gate will still allow.
    ///
    /// It pins **both** halves of the formula at once. Drop `TICKS_PER_HARDNESS`
    /// and the block falls on the first tick; pay Efficiency's `+ 1` at level 0 and
    /// the pickaxe reads 9.0 instead of 8.0, landing on 167.
    #[test]
    fn a_diamond_pickaxe_clears_obsidian_in_the_ticks_minecraft_charges() {
        let mut rng = rng();
        let mut mine = built(MineKind::Obsidian, 0, 0, &mut rng);

        let ticks = ticks_to_break(&mut mine, bare(PickaxeTier::Diamond), &mut rng);

        assert_eq!(
            ticks, 188,
            "Minecraft charges 9.4s (188 ticks) for Obsidian with a Diamond pickaxe"
        );
    }

    /// The rule the golden test is one instance of: a block yields on the tick
    /// `ceil(30 * hardness / mining_power)` and not before.
    ///
    /// Reads its own target rather than fixing one, so it holds for whichever cell
    /// the draw lands on — which is the only honest way to test a random target
    /// without pretending to know the seed's mind.
    ///
    /// **The expectation comes from [`Block::ticks_to_break`], not from the formula
    /// written out again here**, which is what makes this the test that holds the
    /// closed form and this loop together. It used to compute
    /// `(hardness * TICKS_PER_HARDNESS / power).ceil()` inline — a second
    /// implementation living in a test, and therefore one that could agree with
    /// `dig` while both drifted away from what the Upgrades screen quotes.
    #[test]
    fn a_block_takes_the_ticks_its_hardness_and_the_pickaxe_agree_on() {
        let mut rng = rng();
        let mut mine = built(MineKind::Stone, 1, 0, &mut rng);
        let power = bare(PickaxeTier::Wooden);

        assert_eq!(
            mine.dig(power, &mut rng),
            None,
            "a bare Wooden pickaxe must not one-shot anything in the Stone mine"
        );
        let Some((x, y)) = mine.get_target() else {
            unreachable!("the first dig draws a target")
        };
        let Some(block) = mine.get(x, y) else {
            unreachable!("the target is a standing cell")
        };
        let Some(expected) = block.ticks_to_break(power) else {
            unreachable!("a finite, positive power always breaks a block eventually")
        };

        let ticks = 1 + ticks_to_break(&mut mine, power, &mut rng);

        assert_eq!(
            ticks,
            expected,
            "{block:?} (hardness {}) at power {power}",
            block.hardness()
        );
    }

    /// The floor of the whole game: a starting pickaxe must have something to chip
    /// *at*. Without `TICKS_PER_HARDNESS` a fresh Wooden pickaxe (2.0) instamines
    /// Stone (1.5), and progressive breaking — the mechanic, the progress bar, the
    /// crack glyph — has no observable existence for the player to progress out of.
    ///
    /// Walks `ALL_BLOCKS` rather than sampling: the guarantee is about the game,
    /// not about the two blocks a test author happened to think of.
    #[test]
    fn a_fresh_wooden_pickaxe_instamines_nothing_in_the_game() {
        let power = Pickaxe::default().mining_power();

        for &block in ALL_BLOCKS {
            assert!(
                power < block.hardness() * TICKS_PER_HARDNESS,
                "a starter pickaxe ({power}) one-shots {block:?} at hardness {}",
                block.hardness()
            );
        }
    }

    /// The ceiling of the game, and the counterpart to the floor above: the best
    /// pickaxe the player can build *with permanent upgrades alone* one-shots the
    /// ores and the dense blocks, and still cannot touch Ancient Debris or
    /// Obsidian.
    ///
    /// That gap is the staging `docs/MECHANICS.md` promises, and it is what the
    /// temporary Redstone boost (phase 5) is for. It is worth a test because it is
    /// the only place the two halves of the mining-power formula are checked
    /// against the hardness table they exist to beat: lose Haste's multiplier and
    /// 235 stalls below the dense blocks, leaving the endgame with nothing to
    /// instamine; let it multiply too much and the boost has no work left to do and
    /// no reason to exist.
    ///
    /// Reads Haste's cap rather than fixing one, so a re-balance re-asks the
    /// question instead of silently drifting past it. That cap is now
    /// [`World::enchant_cap`](crate::world::World::enchant_cap), and the End's is
    /// what makes this pickaxe the game's permanent ceiling; the gap it must leave
    /// is also the upper bound on
    /// [`HASTE_PER_LEVEL`](crate::tunables::HASTE_PER_LEVEL), which that constant's
    /// docs point back here for.
    #[test]
    fn a_hasted_netherite_instamines_the_dense_blocks_but_not_the_obsidian() {
        let tier = PickaxeTier::Netherite;
        // The End: the highest ceiling the game offers, which is what makes this
        // the *permanent* endgame pickaxe and not merely a well-equipped one.
        let world = World::End;
        let mut enchants = Enchants::new();
        for _ in 0..tier.efficiency_cap() {
            assert!(enchants.upgrade_efficiency(tier).is_ok());
        }
        for _ in 0..EnchantType::Haste.max_level(tier, world) {
            assert!(enchants.upgrade(EnchantType::Haste, tier, world).is_ok());
        }
        let power = Pickaxe::new(tier, enchants).mining_power();

        for block in [Block::IronOre, Block::IronBlock] {
            assert!(
                power >= block.hardness() * TICKS_PER_HARDNESS,
                "a maxed hasted pickaxe ({power}) cannot one-shot {block:?}, so the \
                 endgame has nothing left to reach for"
            );
        }
        for block in [Block::AncientDebris, Block::Obsidian] {
            assert!(
                power < block.hardness() * TICKS_PER_HARDNESS,
                "a maxed hasted pickaxe ({power}) already one-shots {block:?}, which \
                 is the Redstone boost's job — it now has none"
            );
        }
    }

    /// The other half of the claim above, and the reason
    /// [`Boost`](crate::boost::Boost) exists at all: the temporary Redstone boost
    /// **closes the gap the permanent ceiling leaves**. Its sibling proves Ancient
    /// Debris and Obsidian are out of reach of everything the player can buy
    /// forever; this proves they are not out of reach of what the player can buy
    /// for thirty seconds.
    ///
    /// Together they bracket [`BOOST_MULTIPLIER`](crate::tunables::BOOST_MULTIPLIER)
    /// from below the way the pair above brackets `HASTE_PER_LEVEL` from above —
    /// which is why the *design* floor is asserted here, against the real hardness
    /// threshold, and not as a magic ratio in `tunables`. A re-balance that lowers
    /// the boost, weakens the tier curve, or hardens Obsidian fails this test and
    /// re-asks the question.
    ///
    /// Goes through [`Boost::multiplier`] rather than the raw constant, so it also
    /// pins that a *running* boost really does multiply — a boost that reported
    /// `1.0` while live would pass every test in `boost` that checks expiry, and
    /// only this one would notice.
    #[test]
    fn the_redstone_boost_is_what_finally_instamines_the_obsidian() {
        let tier = PickaxeTier::Netherite;
        let world = World::End;
        let mut enchants = Enchants::new();
        for _ in 0..tier.efficiency_cap() {
            assert!(enchants.upgrade_efficiency(tier).is_ok());
        }
        for _ in 0..EnchantType::Haste.max_level(tier, world) {
            assert!(enchants.upgrade(EnchantType::Haste, tier, world).is_ok());
        }
        let boost = Boost::new(BOOST_MULTIPLIER, BOOST_DURATION_TICKS);
        let power = Pickaxe::new(tier, enchants).mining_power() * boost.multiplier();

        for block in [Block::AncientDebris, Block::Obsidian] {
            assert!(
                power >= block.hardness() * TICKS_PER_HARDNESS,
                "a boosted maxed pickaxe ({power}) still cannot one-shot {block:?}, \
                 so nothing in the game ever does"
            );
        }
    }

    /// Past instamine, single-target speed **saturates at one block per tick**, and
    /// that saturation is a design load-bearing enough to test: it is the reason
    /// the endgame's levers shift to area enchants and richness instead of more
    /// speed. What enforces it is that `dig` *discards* the leftover progress
    /// rather than subtracting the threshold from it — a carry would let a
    /// sufficiently absurd power cascade through a whole mine in one tick.
    #[test]
    fn progress_never_carries_over_to_the_next_block() {
        let mut rng = rng();
        let mut mine = built(MineKind::Stone, 0, 0, &mut rng);
        let capacity = mine.capacity();

        for broken in 1..=4 {
            assert!(
                mine.dig(10_000.0, &mut rng).is_some(),
                "tick {broken} broke nothing"
            );
            assert_eq!(
                mine.remaining_count(),
                capacity - broken,
                "one tick took more than one block"
            );
        }
    }

    /// `break_progress` only means something if it accumulates against *one* block.
    /// A target re-drawn every tick would leave the counter measuring a different
    /// cell each time — and the UI's highlight flickering across the grid.
    #[test]
    fn the_target_holds_still_while_the_block_stands() {
        let mut rng = rng();
        let mut mine = built(MineKind::Obsidian, 1, 0, &mut rng);
        let power = bare(PickaxeTier::Diamond);

        assert_eq!(mine.dig(power, &mut rng), None);
        let aimed = mine.get_target();
        assert!(aimed.is_some(), "the first dig draws a target");

        for tick in 2..=20 {
            assert_eq!(mine.dig(power, &mut rng), None);
            assert_eq!(mine.get_target(), aimed, "the aim wandered on tick {tick}");
        }
    }

    /// A break reports **where** it happened, and the answer is the cell that was
    /// aimed at — not the fresh target, and not a hole. This is the whole reason
    /// `dig` hands back a [`Dug`] rather than a [`Block`]: the spatial enchants
    /// centre on this coordinate, and nothing else in the API still holds it once
    /// the swing is over.
    #[test]
    fn a_break_reports_the_cell_it_happened_in() {
        let mut rng = rng();
        let mut mine = built(MineKind::Obsidian, 1, 0, &mut rng);
        let power = bare(PickaxeTier::Diamond);

        // Chip until it gives, so the aim is established well before the break.
        let mut aimed = None;
        let dug = loop {
            match mine.dig(power, &mut rng) {
                Some(dug) => break dug,
                None => aimed = mine.get_target(),
            }
        };

        assert_eq!(
            Some(dug.cell),
            aimed,
            "the break named a cell it was not on"
        );
        assert_eq!(
            mine.get(dug.cell.0, dug.cell.1),
            None,
            "the reported cell is a hole now, which is what makes it the right one"
        );
    }

    /// An instamining swing draws its target and breaks it in the same tick, so the
    /// aim is never observable from outside — and the cell still has to come back,
    /// or the endgame's spatial enchants would have nothing to centre on precisely
    /// where they matter most.
    #[test]
    fn an_instamined_break_still_reports_its_cell() {
        let mut rng = rng();
        let mut mine = built(MineKind::Stone, 1, 0, &mut rng);

        assert_eq!(mine.get_target(), None, "nothing is aimed at yet");
        let Some(dug) = mine.dig(10_000.0, &mut rng) else {
            unreachable!("a full grid and an absurd power must break something")
        };

        assert_eq!(mine.get(dug.cell.0, dug.cell.1), None);
        let (width, height) = mine.get_size();
        assert!(
            dug.cell.0 < width && dug.cell.1 < height,
            "cell off the grid"
        );
    }

    /// On break the aim is released, so the next tick draws afresh — "on break, the
    /// next random cell is picked". Leaving it on the hole would spend a tick
    /// re-discovering that nothing is there.
    #[test]
    fn a_broken_block_frees_the_target_for_a_new_draw() {
        let mut rng = rng();
        let mut mine = built(MineKind::Stone, 1, 0, &mut rng);

        assert!(mine.dig(10_000.0, &mut rng).is_some());

        assert_eq!(mine.get_target(), None, "a fallen block stayed aimed at");
        assert_eq!(mine.break_ratio(), 0.0);

        assert!(
            mine.dig(10_000.0, &mut rng).is_some(),
            "the aim never re-drew"
        );
    }

    /// A target can vanish under the digger without `dig` ever knowing: phase 4's
    /// area enchants `take` whole blast shapes, and the aimed-at cell is as likely
    /// to sit in one as any other. `acquire_target` re-validates instead of
    /// trusting, so those enchants need not know this field exists — and the
    /// progress owed to the vanished block dies with it rather than transferring
    /// to whatever is drawn next.
    #[test]
    fn a_target_broken_out_from_under_the_digger_forfeits_its_progress() {
        let mut rng = rng();
        let mut mine = built(MineKind::Obsidian, 1, 0, &mut rng);
        let power = bare(PickaxeTier::Diamond);

        for _ in 0..100 {
            assert_eq!(mine.dig(power, &mut rng), None);
        }
        let Some((x, y)) = mine.get_target() else {
            unreachable!("100 ticks of digging leave a target")
        };
        assert!(mine.break_ratio() > 0.0, "100 ticks bought no progress");

        // What a blast does, seen from here: the cell simply stops being there.
        assert!(mine.take(x, y).is_some());
        assert_eq!(
            mine.break_ratio(),
            0.0,
            "the bar kept filling against a block that is gone"
        );

        assert_eq!(mine.dig(power, &mut rng), None);

        assert_ne!(mine.get_target(), Some((x, y)), "the aim stayed on a hole");
        assert_eq!(
            mine.break_ratio(),
            power / (50.0 * TICKS_PER_HARDNESS),
            "the new block inherited the dead one's progress"
        );
    }

    /// The dial redraws the block being chipped at out from under the player, and
    /// the progress was owed to *that* block. Left standing it would launder: chip
    /// a cell to one tick short, nudge the dial, and collect whatever replaced it
    /// for a swing's worth of work.
    ///
    /// The aim itself must **not** move — the cell is still standing, and the
    /// player is still pointing at it. Only what they had bought against its former
    /// occupant is gone.
    #[test]
    fn moving_the_dial_forfeits_the_progress_on_the_targeted_cell() {
        let mut rng = rng();
        let mut mine = built(MineKind::Obsidian, 1, 3, &mut rng);
        let power = bare(PickaxeTier::Diamond);

        for _ in 0..50 {
            assert_eq!(mine.dig(power, &mut rng), None);
        }
        let aimed = mine.get_target();
        assert!(mine.break_ratio() > 0.0, "50 ticks bought no progress");

        assert_eq!(mine.set_richness_setting(1, &mut rng), Ok(()));

        assert_eq!(mine.break_ratio(), 0.0, "the dial laundered the progress");
        assert_eq!(mine.get_target(), aimed, "the dial moved the aim");
    }

    /// A power that is not a positive, finite number is a caller's bug, and the
    /// answer is the module's rule — **a refusal changes nothing**. Not one draw is
    /// taken from the generator either: its position is run state, and a rejected
    /// order must not move it.
    ///
    /// `NaN` is the one that matters. Added, it would sit in `break_progress`
    /// forever — comparing false against every hardness, surviving every reset that
    /// does not overwrite it, and quietly breaking the reflexivity of `Mine`'s
    /// `PartialEq`. It cannot be clamped back, only refused, which is exactly the
    /// reading `Rng::chance` already gives an unspecified probability.
    #[test]
    fn a_mining_power_that_is_not_a_positive_number_breaks_nothing() {
        let mut rng = rng();
        let mut mine = built(MineKind::Stone, 1, 0, &mut rng);
        let untouched = mine.clone();
        let mut unasked = rng.clone();

        for power in [0.0, -5.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                mine.dig(power, &mut rng),
                None,
                "power {power} broke a block"
            );
        }

        assert_eq!(mine, untouched, "a refused dig moved the mine");
        assert_eq!(
            draws(&mut rng),
            draws(&mut unasked),
            "a refused dig drew a target anyway"
        );
    }

    /// What the progress bar and the `.:#` crack glyph read. It must climb, never
    /// overshoot the full bar on the tick an instamining power lands, and fall back
    /// to empty once the block is gone.
    #[test]
    fn break_ratio_walks_from_zero_to_one_and_stops_there() {
        let mut rng = rng();
        let mut mine = built(MineKind::Obsidian, 1, 0, &mut rng);
        let power = bare(PickaxeTier::Diamond);

        assert_eq!(mine.break_ratio(), 0.0, "an untouched mine shows progress");

        let mut previous = 0.0;
        for tick in 1..=187 {
            assert_eq!(mine.dig(power, &mut rng), None);
            let ratio = mine.break_ratio();
            assert!(
                ratio > previous,
                "the bar stalled at {ratio} on tick {tick}"
            );
            assert!(
                ratio <= 1.0,
                "the bar ran past full at {ratio} on tick {tick}"
            );
            previous = ratio;
        }

        assert!(
            mine.dig(power, &mut rng).is_some(),
            "the 188th tick must land"
        );
        assert_eq!(mine.break_ratio(), 0.0, "the bar survived the break");
    }

    /// "The mine depletes to 0, then fully and instantly refills" — the SkyMines
    /// cube regeneration, and the one thing that earns the broken blocks back.
    ///
    /// `dig` empties the mine and **leaves it empty**: the refill is the swing's,
    /// not the break's. Digging the last cell and finding a full grid again would
    /// mean the spatial procs that have not rolled yet get a fresh two hundred
    /// cells to blast, on the balance sheet of one swing.
    #[test]
    fn a_dig_that_empties_the_mine_leaves_it_for_the_swing_to_refill() {
        let mut rng = rng();
        let mut mine = built(MineKind::Stone, 0, 0, &mut rng);
        let capacity = mine.capacity();

        for broken in 1..capacity {
            assert!(mine.dig(10_000.0, &mut rng).is_some());
            assert_eq!(mine.remaining_count(), capacity - broken);
        }

        assert!(
            mine.dig(10_000.0, &mut rng).is_some(),
            "the last block must still drop"
        );
        assert!(mine.is_empty(), "dig refilled a mine it was owed to leave");
    }

    /// The other half: the refill *does* happen, on the call that closes the swing,
    /// and a grid with anything still standing is left alone. Both answers matter —
    /// the first is what keeps `dig` able to draw a target next tick, the second is
    /// what stops a swing on a half-dug mine from resetting it.
    #[test]
    fn a_refill_fires_on_an_empty_grid_and_on_no_other() {
        let mut rng = rng();
        let mut mine = built(MineKind::Stone, 0, 0, &mut rng);

        assert!(
            !mine.refill_if_empty(&mut rng),
            "a full grid must not be redrawn"
        );

        for _ in 0..mine.capacity() {
            assert!(mine.dig(10_000.0, &mut rng).is_some());
        }
        assert!(mine.is_empty());

        assert!(mine.refill_if_empty(&mut rng), "the refill did not fire");
        assert_eq!(mine.remaining_count(), mine.capacity());
    }

    /// The refill resets the *grid*, not the mine. What the player bought — the
    /// size their ore paid for, the ceiling, the dial they set — must survive it,
    /// or emptying a mine would quietly undo the progress that made emptying it
    /// possible in the first place.
    #[test]
    fn a_refilled_mine_keeps_its_size_and_its_dial() {
        let mut rng = rng();
        let mut mine = built(MineKind::Stone, 2, 2, &mut rng);
        let (level, size) = (mine.get_size_level(), mine.get_size());

        for _ in 0..mine.capacity() {
            assert!(mine.dig(10_000.0, &mut rng).is_some());
        }
        assert!(mine.refill_if_empty(&mut rng));

        assert_eq!(mine.get_size_level(), level, "the refill resized the mine");
        assert_eq!(mine.get_size(), size);
        assert_eq!(mine.get_richness_setting(), 2, "the refill moved the dial");
        assert_eq!(mine.get_richness_level(), 2, "the refill spent the ceiling");
    }

    /// The block handed back on the emptying tick is the one that just fell, and it
    /// is still the one after the swing refills. `take` runs before any redraw, and
    /// that ordering is the whole of it: reversed, the player would be paid for a
    /// block drawn out of a mine they had not mined — and the one they *had* mined
    /// would vanish unpaid.
    #[test]
    fn the_last_block_still_drops_before_the_refill() {
        let mut rng = rng();
        let mut mine = built(MineKind::Amethyst, 0, 0, &mut rng);

        for _ in 0..mine.capacity() - 1 {
            assert!(mine.dig(10_000.0, &mut rng).is_some());
        }
        assert_eq!(mine.remaining_count(), 1, "exactly one cell must be left");

        // Read it while it still stands: one tick from now it is gone.
        let Some(last) = mine.get_grid().iter().flatten().flatten().copied().next() else {
            unreachable!("one cell is still standing")
        };

        assert_eq!(
            mine.dig(10_000.0, &mut rng).map(|dug| dug.block),
            Some(last),
            "the emptying tick paid out a block the player had not mined"
        );
        assert!(mine.refill_if_empty(&mut rng));
        assert_eq!(mine.remaining_count(), mine.capacity());
    }

    /// The blast, end to end: every standing cell in the shape falls, and each one
    /// is handed back exactly once so the caller can drop it.
    #[test]
    fn a_blast_breaks_every_cell_of_its_shape_and_returns_them() {
        let mut mine = built(MineKind::Iron, 3, 0, &mut rng());
        let shape = [(0, 0), (1, 0), (2, 1)];

        let broken = mine.blast(&shape);

        assert_eq!(broken.len(), shape.len());
        for (x, y) in shape {
            assert_eq!(mine.get(x, y), None, "({x}, {y}) is still standing");
        }
    }

    /// The half that keeps a blast from paying twice. A shape almost always covers
    /// ground already cleared — an Explosive fired near the last one, or two
    /// enchants procing on the same swing — and those cells must be absent from the
    /// returned drops, not present as phantom blocks. `remaining_count` is the
    /// load-bearing assertion: it must not move for the cells that were already
    /// holes.
    #[test]
    fn a_blast_over_holes_pays_only_for_what_was_standing() {
        let mut mine = built(MineKind::Stone, 3, 0, &mut rng());
        assert!(mine.take(0, 0).is_some());
        assert!(mine.take(1, 0).is_some());
        let remaining = mine.remaining_count();

        // Two holes, one standing cell, and one coordinate named twice.
        let broken = mine.blast(&[(0, 0), (1, 0), (2, 0), (2, 0)]);

        assert_eq!(broken.len(), 1, "only (2, 0) was still standing");
        assert_eq!(
            mine.remaining_count(),
            remaining - 1,
            "a blast must consume exactly the cells it paid for"
        );
    }

    /// Off-grid coordinates are absorbed rather than refused, the same way `take`
    /// absorbs them. `blast_cells` clips its shapes, so this is defence in depth —
    /// but a blast that panicked on a stray coordinate would turn a geometry bug
    /// into a crashed run.
    #[test]
    fn a_blast_off_the_grid_breaks_nothing_and_survives() {
        let mut mine = built(MineKind::Iron, 0, 0, &mut rng());
        let (width, height) = mine.get_size();
        let full = mine.remaining_count();

        assert!(
            mine.blast(&[(width, 0), (0, height), (u8::MAX, u8::MAX)])
                .is_empty()
        );
        assert_eq!(mine.remaining_count(), full);
    }

    /// **A blast leaves the mine empty; it does not refill it.** `dig` refills on
    /// the break that takes the last cell, because there the break and the
    /// emptiness are one event. A blast is not: other enchants may still fire on
    /// the same swing, and a refill here would drop a full grid under the ones that
    /// have not rolled yet. Putting refill after the procs is the tick's business
    /// ([`GameState::tick`](crate::game::GameState::tick)), and this test is what
    /// pins the half `blast` deliberately does not do.
    #[test]
    fn a_blast_that_empties_the_mine_does_not_refill_it() {
        let mut mine = built(MineKind::Stone, 0, 0, &mut rng());
        let (width, height) = mine.get_size();
        let whole_grid: Vec<(u8, u8)> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .collect();

        let broken = mine.blast(&whole_grid);

        assert_eq!(
            broken.len(),
            mine.capacity(),
            "every cell should have fallen"
        );
        assert!(mine.is_empty(), "the blast must leave the mine empty");
        assert_eq!(mine.remaining_count(), 0);
    }

    /// The two halves meeting: a shape from `blast_cells` fed straight to `blast`,
    /// which is exactly what the proc resolution will do. A Nuke clears the grid
    /// whatever cell it fires from — including a corner, where every other shape
    /// would be clipped.
    #[test]
    fn a_nuke_shape_clears_the_whole_grid_from_a_corner() {
        let mut mine = built(MineKind::Amethyst, 2, 0, &mut rng());
        let shape = EnchantType::Nuke.blast_cells(1, (0, 0), mine.get_size());

        let broken = mine.blast(&shape);

        assert_eq!(broken.len(), mine.capacity());
        assert!(mine.is_empty());
    }

    /// Every spatial enchant at the End's cap, for the tests whose subject is the
    /// resolution rather than any one enchant.
    fn maxed_enchants() -> Enchants {
        let mut enchants = Enchants::new();
        for kind in [
            EnchantType::Explosive,
            EnchantType::Jackhammer,
            EnchantType::Nuke,
        ] {
            for _ in 0..10 {
                assert!(
                    enchants
                        .upgrade(kind, PickaxeTier::Wooden, World::End)
                        .is_ok()
                );
            }
        }
        enchants
    }

    /// **Decision I, and the half that protects every existing save.** An enchant
    /// the player never bought must be skipped *before* it draws, not rolled at 0
    /// permille: a roll that always fails still advances the generator, so the two
    /// are identical in outcome and completely different in the sequence. Proven
    /// against a twin generator asked for the same work minus the resolution.
    ///
    /// This is what let Excavator be appended to `PROC_ORDER` without disturbing a
    /// run that never bought it, and what a fifth triggered enchant will rely on.
    #[test]
    fn an_enchant_at_level_zero_costs_no_entropy() {
        let (mut resolved, mut untouched) = (rng(), rng());
        let mut mine = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut resolved);
        let _ = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut untouched);

        let broken = mine.resolve_spatial_procs((5, 5), &Enchants::new(), &mut resolved);

        assert!(broken.is_empty(), "an unenchanted pickaxe broke cells");
        assert_eq!(
            draws(&mut resolved),
            draws(&mut untouched),
            "resolving with no enchants advanced the generator"
        );
    }

    /// The determinism contract, at the proc level: the same seed rolls the same
    /// procs. Without it a reloaded save would re-roll its luck, and "send me your
    /// save, I will reproduce your bug" would stop being true the moment an enchant
    /// fired.
    #[test]
    fn the_same_seed_replays_the_same_procs() {
        fn run(seed: u64) -> (usize, usize) {
            let mut rng = Rng::from_seed(seed);
            let mut mine = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut rng);
            let enchants = maxed_enchants();
            let mut broken = 0;
            for swing in 0..20u8 {
                // Counted in *blocks*, not in procs: a replay that fired the same
                // enchants over different ground would still be a divergence, and
                // the proc count alone would not see it.
                broken += mine
                    .resolve_spatial_procs((swing % 20, swing % 10), &enchants, &mut rng)
                    .iter()
                    .map(|p| p.broken.len())
                    .sum::<usize>();
            }
            (broken, mine.remaining_count())
        }

        assert_eq!(run(7), run(7), "same seed must replay identically");
        assert_ne!(run(7), run(99), "different seeds must diverge");
    }

    /// **Procs take cells and never put one back.** `remaining_count` must be
    /// monotone across a long run of swings: a resolution that refilled — because a
    /// Nuke emptied the grid and something reset it — would show up as the count
    /// climbing. Reaching empty at the end is the other half, and proves the
    /// monotonicity was not vacuous.
    #[test]
    fn procs_only_ever_remove_cells_and_never_refill() {
        let mut rng = Rng::from_seed(5);
        let mut mine = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut rng);
        let enchants = maxed_enchants();

        let mut previous = mine.remaining_count();
        for swing in 0..500u32 {
            let impact = ((swing % 20) as u8, (swing % 10) as u8);
            mine.resolve_spatial_procs(impact, &enchants, &mut rng);

            let remaining = mine.remaining_count();
            assert!(
                remaining <= previous,
                "the mine refilled at swing {swing}: {previous} -> {remaining}"
            );
            previous = remaining;
        }

        assert!(
            mine.is_empty(),
            "500 swings at the cap left {previous} cells standing"
        );
    }

    /// **The golden vector of the proc sequence.** These counts pin the order the
    /// enchants draw in (`SPATIAL_PROC_ORDER`), the way the roll is made
    /// (`chance_permille`, an integer comparison on `below(1000)`), and the curve
    /// behind it. Any of the three changing moves these numbers.
    ///
    /// If it fails, the question is not "what are the new counts?" but "what did we
    /// just do to every existing save?" — the run's luck is a position in this
    /// sequence, and a save resumes it.
    #[test]
    fn the_proc_sequence_is_pinned_to_a_golden_vector() {
        let mut rng = Rng::from_seed(42);
        let mut mine = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut rng);
        let enchants = maxed_enchants();

        let swings: Vec<Vec<SpatialProc>> = (0..12u8)
            .map(|swing| mine.resolve_spatial_procs((swing % 20, swing % 10), &enchants, &mut rng))
            .collect();

        let broken: Vec<usize> = swings
            .iter()
            .map(|procs| procs.iter().map(|p| p.broken.len()).sum())
            .collect();
        // Readable as the enchants themselves: a bare 20 is a Jackhammer clearing
        // one full-width row, 55 is an Explosive and a Jackhammer landing on the
        // same swing, and the smaller counts are shapes falling on ground already
        // dug. The zeros are the swings where nothing rolled. Unchanged since the
        // procs were reported one by one, which is the signal the *sequence* did not
        // move — only its shape on the way out.
        assert_eq!(broken, vec![20, 0, 0, 20, 0, 0, 55, 0, 13, 13, 0, 0]);

        // And the count of *procs*, which the flat list could not express. Swing 10
        // is the pair that argues for the split: an enchant fired and broke nothing,
        // because its shape fell entirely on ground earlier swings had cleared. The
        // player is owed the flash either way — something did happen — and under the
        // old return that swing was indistinguishable from one where nothing rolled.
        let fired: Vec<usize> = swings.iter().map(Vec::len).collect();
        assert_eq!(fired, vec![1, 0, 0, 1, 0, 0, 2, 0, 1, 1, 1, 0]);
        assert_eq!(broken[10], 0, "the shape fell on ground already dug");
    }

    /// An Explosive fired at a corner is clipped by `blast_cells`, so the blast
    /// breaks a quadrant and nothing outside the grid — the `u8` underflow this
    /// pairing exists to rule out would have wrapped it to the far edge instead.
    #[test]
    fn an_explosive_shape_at_a_corner_breaks_only_its_quadrant() {
        let mut mine = built(MineKind::Iron, MAX_SIZE_LEVEL, 0, &mut rng());
        let capacity = mine.capacity();
        let shape = EnchantType::Explosive.blast_cells(10, (0, 0), mine.get_size());

        let broken = mine.blast(&shape);

        assert_eq!(broken.len(), 16, "a radius-3 square at the origin is 4x4");
        assert_eq!(mine.remaining_count(), capacity - 16);
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(mine.get(x, y), None, "({x}, {y}) should have fallen");
            }
        }
        assert!(
            mine.get(4, 0).is_some(),
            "the blast reached past its radius"
        );
    }

    /// A mine the rules built is valid: the validator must not be a second, stricter
    /// game that refuses states the first one produces.
    #[test]
    fn a_mine_the_rules_built_is_valid() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        assert!(mine.validate().is_ok());

        mine.dig(1000.0, &mut rng());
        assert!(mine.validate().is_ok(), "a half-dug mine is a normal mine");
    }

    /// The dial is the one field the player moves freely, and its ceiling is the
    /// only thing bounding it. A save that lifted the dial above the level bought
    /// for it would hand out a richness nobody paid for, permanently.
    #[test]
    fn a_dial_above_its_ceiling_is_refused() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        mine.richness_setting = 3;

        assert!(mine.validate().is_err());
    }

    /// `size_level` and `grid` are two statements of the same fact, and only the
    /// grid is real. A mismatch would leave a mine drawing targets from cells the
    /// renderer does not draw, or refusing to draw ones the player can dig.
    #[test]
    fn a_grid_that_is_not_the_size_it_claims_is_refused() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        mine.size_level = 3;

        assert!(mine.validate().is_err());
    }

    /// Progress is measured in ticks of mining power, and there is no such thing as
    /// a negative one. A `NaN` is the worse case and is unreachable through JSON,
    /// which cannot spell it — the check stays because the field is an `f32` and
    /// this is the only place that reads one back from outside.
    #[test]
    fn a_break_progress_that_is_not_a_count_of_ticks_is_refused() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        mine.break_progress = -1.0;
        assert!(mine.validate().is_err());

        mine.break_progress = f32::NAN;
        assert!(mine.validate().is_err());
    }

    /// A size level past the top of the table is refused before the grid check even
    /// looks at it: above the ceiling `get_size` clamps, so the level and its grid
    /// would agree while promising an enlargement no shop could ever have sold.
    #[test]
    fn a_size_level_past_the_largest_is_refused() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        mine.size_level = MAX_SIZE_LEVEL + 1;
        assert!(mine.validate().is_err());
    }

    /// A richness level above the highest one for sale is a ceiling nobody could
    /// have bought, so a save that claims it was tampered with.
    #[test]
    fn a_richness_level_past_the_highest_for_sale_is_refused() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        mine.richness_level = MAX_RICHNESS_LEVEL + 1;
        assert!(mine.validate().is_err());
    }

    /// The aim is held across ticks, so an aim at a hole is not a stale value that
    /// the next tick clears: `acquire_target` keeps a target it believes in, and
    /// `dig` would pour progress into a cell that is not there.
    #[test]
    fn an_aim_at_a_cell_that_is_not_standing_is_refused() {
        let mut mine = Mine::new(MineKind::Iron, &mut rng());
        mine.target = Some((0, 0));
        assert!(
            mine.validate().is_ok(),
            "the fixture must start with a full grid"
        );

        mine.take(0, 0);
        assert!(mine.validate().is_err());
    }
}
