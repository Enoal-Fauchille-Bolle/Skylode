//! Generated mines.
//!
//! A [`Mine`] is the grid of [`Block`]s the player digs through. Its dimensions
//! scale with a size level, from a tiny 3x3 starter mine up to a 20x10 mine at
//! the top of the `MINE_SIZES` table.

use crate::block::Block;
use crate::error::CoreError;
use crate::mine_kind::MineKind;
use crate::rng::Rng;

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

/// A generated mine: its [`MineKind`] identity plus the laid-out grid the player
/// mines through.
///
/// The kind is what a mine *is* — "the Iron mine" — and it answers every question
/// about the block pool ([`common_block`](MineKind::common_block) /
/// [`value_block`](MineKind::value_block)), the world, and the gating tier, so the
/// grid does not have to carry them. What the grid carries is the run's *state*:
/// which cells are still standing.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The 2D grid of blocks that the player actually mines, row by row.
    grid: Vec<Vec<Block>>,
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

    /// Resets the mine to its initial state, refilling the grid.
    ///
    /// Each cell is drawn independently between the kind's
    /// [common](MineKind::common_block) and [value](MineKind::value_block) blocks,
    /// weighted by the richness dial: the value cell's weight is
    /// [`value_weight(richness_setting)`](value_weight), the common cell takes the
    /// rest. The draws come from the seeded [`Rng`] so a run refills the same way
    /// on any machine and after any reload.
    ///
    /// `pub(crate)`: refilling a mine is something the *rules* do — on batch
    /// reset, when the last block falls — not something a front-end may ask for.
    /// A UI able to call this could hand the player an infinite mine by refilling
    /// it on demand.
    pub(crate) fn reset(&mut self, rng: &mut Rng) {
        let (width, height) = self.get_size();
        let common = self.kind.common_block();
        let value = self.kind.value_block();
        let value_w = value_weight(self.richness_setting);
        // [common, value]; both are > 0 for every setting, so `weighted` never
        // returns `None` — but a `None` (and the impossible index) falls back to
        // the common cell rather than reaching for `unwrap`/`expect`, which the
        // workspace lints deny.
        let weights = [100 - value_w, value_w];
        self.grid = (0..height)
            .map(|_| {
                (0..width)
                    .map(|_| match rng.weighted(&weights) {
                        Some(1) => value,
                        _ => common,
                    })
                    .collect()
            })
            .collect();
    }

    /// Returns a copy of the mine's grid of blocks.
    pub fn get_grid(&self) -> Vec<Vec<Block>> {
        self.grid.clone()
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
        };
        mine.reset(rng);
        mine
    }

    /// How many cells of `block` the grid holds.
    fn count(mine: &Mine, block: Block) -> usize {
        mine.grid.iter().flatten().filter(|&&b| b == block).count()
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
            for &block in row {
                assert!(block == common || block == value, "stray block {block:?}");
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
        mine.grid[0][0] = Block::IronOre; // a block from neither pool
        mine.reset(&mut rng());
        let (width, height) = mine.get_size();
        assert_eq!(mine.grid.len(), height as usize);
        for row in &mine.grid {
            assert_eq!(row.len(), width as usize);
            for &block in row {
                assert!(block == common || block == value, "stray block {block:?}");
            }
        }
    }

    #[test]
    fn get_grid_returns_a_copy_of_the_grid() {
        let mine = Mine::new(MineKind::Stone, &mut rng());
        let grid_copy = mine.get_grid();
        assert_eq!(grid_copy, mine.grid);
        // Modify the copy and ensure the original grid is unaffected
        let mut modified_copy = grid_copy.clone();
        modified_copy[0][0] = Block::IronOre;
        assert_ne!(modified_copy, mine.grid);
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
                for &block in row {
                    assert!(
                        block == common || block == value,
                        "setting {setting}: stray block {block:?}"
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
}
