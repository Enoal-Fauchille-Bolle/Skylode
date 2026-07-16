//! Generated mines.
//!
//! A [`Mine`] is the grid of [`Block`]s the player digs through. Its dimensions
//! scale with a size level, from a tiny 3x3 starter mine up to a 20x10 mine at
//! the top of the `MINE_SIZES` table.

use crate::block::Block;
use crate::error::CoreError;
use crate::mine_kind::MineKind;
use crate::rng::Rng;
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
const MAX_SIZE_LEVEL: u32 = MINE_SIZES.len() as u32 - 1;

/// How much [`mining_power`](crate::pickaxe::Pickaxe::mining_power) a block costs
/// per point of [`hardness`](Block::hardness): a cell yields at
/// `hardness * 30`, so it takes `ceil(30 * hardness / mining_power)` ticks.
///
/// This is Minecraft's, and it is the **unit conversion** between two scales that
/// are not the same one — dig speed and hardness. `getDestroyProgress` reads
/// `dig_speed / hardness / 30` per tick, breaking at `1.0`; rearranged so the
/// progress counter carries the power rather than a fraction, the 30 lands here.
/// Without it the two scales are read as one, and a *fresh Wooden pickaxe
/// instamines Stone* — there is no progressive breaking left to speak of.
///
/// **Not a tunable, for the reason the batch-reset threshold is not one.** It is
/// what makes `DECISIONS.md`'s "1:1 fidelity to Minecraft is kept for hardness"
/// true in practice: the hardness table is only worth porting one-to-one if the
/// break *times* come out one-to-one too, and this is the factor that decides
/// that. Moving it does not tune the game, it revokes the decision. A balance pass
/// that wants faster mining reaches for [`base_power`](crate::pickaxe::PickaxeTier::base_power),
/// which is already Skylode's own curve.
///
/// Minecraft's other divisor — `100`, for mining without the right tool — has no
/// counterpart here and never will: phase 3's mining gate *refuses* a block below
/// the required tier rather than letting the player chip at it. One regime, one
/// constant.
const TICKS_PER_HARDNESS: f32 = 30.0;

/// The highest richness level a mine can reach: 10 rungs, `0..=9`.
///
/// Richness is a *curve*, not an irregular table like `MINE_SIZES`, so it is a
/// formula ([`value_weight`]) plus this bound rather than a laid-out array. The
/// count is provisional; phase 10 balance sets the final shape.
const MAX_RICHNESS_LEVEL: u32 = 9;

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
#[derive(Debug, Clone, PartialEq)]
pub struct Mine {
    /// Which of the twelve canonical mines this is; the source of its block pool.
    kind: MineKind,
    /// Size tier of the mine; indexes into `MINE_SIZES` via
    /// [`get_size`](Mine::get_size).
    size_level: u32,
    /// The richness *ceiling*: how high the dial may be pushed. Bought (phase 5),
    /// permanent, one-way. It has no writer yet — the paid upgrade path does not
    /// exist — so it stays 0 until phase 5 lands; it is read through
    /// [`get_richness_level`](Mine::get_richness_level).
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
    /// permanent, one-way; 0 until the phase-5 upgrade path exists to raise it.
    pub fn get_richness_level(&self) -> u32 {
        self.richness_level
    }

    /// Returns the richness *dial*: the value cell's weight in the composition.
    pub fn get_richness_setting(&self) -> u32 {
        self.richness_setting
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
    /// single-player, offline, no leaderboard. See `DECISIONS.md`.
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
    /// Returns the [`Block`], not a drop, because Fortune, Excavator and XP are
    /// the caller's to apply: the mine's business is which block stood there, and
    /// [`Block::drops`] is the block's own table, not the outcome of a swing.
    ///
    /// **The last block to fall takes the batch reset with it**, in the same tick:
    /// the mine refills whole, the way a SkyMines cube regenerates. Doing it here
    /// rather than leaving the caller a `reset_if_empty` to remember is what makes
    /// "a mine is never left empty" an invariant instead of a convention — two
    /// calls to chain is one call to forget, and the forgotten one strands the run
    /// on a grid with nothing left to draw a target from. The block that earned the
    /// reset is still what comes back: the refill happens after the take, so the
    /// player is paid for the swing that emptied the mine.
    ///
    /// `pub(crate)`, and this one is not about being free — it *does* charge, in
    /// progress. It is about **rate**: nothing in core bounds how often this is
    /// called, so a front-end holding it in a render loop would mine as fast as it
    /// could redraw. The cadence is the phase-7 tick's to own, and until that sole
    /// legitimate caller exists the door stays shut.
    // Dead outside the tests until the phase-7 tick calls it; see `upgrade`'s note
    // in `pickaxe` for why this is an `expect` and not an `allow`.
    #[cfg_attr(not(test), expect(dead_code, reason = "awaiting the phase-7 tick"))]
    pub(crate) fn dig(&mut self, mining_power: f32, rng: &mut Rng) -> Option<Block> {
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
        if self.is_empty() {
            self.reset(rng);
        }
        broken
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

    /// Returns this mine's `(width, height)` in blocks.
    ///
    /// Looks the dimensions up in `MINE_SIZES` by `size_level`, clamping anything
    /// past the table to the largest size rather than panicking.
    ///
    /// [`upgrade_size_level`](Mine::upgrade_size_level) is the only thing that
    /// writes the field, and it stops at [`MAX_SIZE_LEVEL`], so a live mine cannot
    /// reach the clamp. It stays because the field is a plain `u32` that phase 9
    /// will read back out of a save file: a hand-edited or corrupted save must
    /// give the player a 20x10 mine, not a panicking core.
    pub fn get_size(&self) -> (u8, u8) {
        let index = self.size_level as usize;
        if index < MINE_SIZES.len() {
            MINE_SIZES[index]
        } else {
            MINE_SIZES[MINE_SIZES.len() - 1] // Return the largest size if out of bounds
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
    /// `pub(crate)` because it is **free**. A mine enlargement is paid for in the
    /// mine's own ore, and the transaction that debits it does not exist yet
    /// (phase 5). Until it does, this is a door onto a 20x10 mine at no cost, and
    /// it stays shut to anything outside the core. The paid entry point will wrap
    /// this one rather than replace it.
    #[cfg_attr(not(test), expect(dead_code, reason = "awaiting the phase-5 economy"))]
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
    use crate::enchant::Enchants;
    use crate::pickaxe::{Pickaxe, PickaxeTier};

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
        let expected = (block.hardness() * TICKS_PER_HARDNESS / power).ceil() as u32;

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
    /// **The same tick**, not the next one. A mine left standing empty is one
    /// `dig` can draw no target from, so the emptiness would not be a pause before
    /// the reward but a hole the run falls into. Doing it inside `dig` is also what
    /// makes it an invariant rather than a convention: there is no second call for
    /// a caller to forget.
    #[test]
    fn an_emptied_mine_refills_in_the_same_tick_as_its_last_block() {
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
        assert_eq!(
            mine.remaining_count(),
            capacity,
            "the mine that gave up its last block was not refilled"
        );
        assert!(!mine.is_empty(), "a dig left the mine standing empty");
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

        assert_eq!(mine.get_size_level(), level, "the refill resized the mine");
        assert_eq!(mine.get_size(), size);
        assert_eq!(mine.get_richness_setting(), 2, "the refill moved the dial");
        assert_eq!(mine.get_richness_level(), 2, "the refill spent the ceiling");
    }

    /// The block handed back on the emptying tick is the one that just fell, not a
    /// cell of the grid that replaced it. `take` runs before `reset`, and that
    /// ordering is the whole of it: reversed, the player would be paid for a block
    /// drawn out of a mine they had not mined — and the one they *had* mined would
    /// vanish unpaid.
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
            mine.dig(10_000.0, &mut rng),
            Some(last),
            "the emptying tick paid out a block from the fresh grid"
        );
        assert_eq!(mine.remaining_count(), mine.capacity());
    }
}
