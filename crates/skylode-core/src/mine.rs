//! Generated mines.
//!
//! A [`Mine`] is the grid of [`Block`]s the player digs through. Its dimensions
//! scale with a size level, from a tiny 3x3 starter mine up to a 20x10 mine at
//! the top of the `MINE_SIZES` table.

use crate::block::Block;
use crate::error::CoreError;

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

/// A generated mine: a pool of possible blocks plus the laid-out grid the
/// player mines through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mine {
    /// The main block of the mine, which is always the first in the pool.
    main_block: Block,
    /// The secondary blocks of the mine, which can appear alongside the main
    /// block.
    secondary_blocks: Vec<Block>,
    /// Size tier of the mine; indexes into `MINE_SIZES` via
    /// [`get_size`](Mine::get_size).
    size_level: u32,
    /// The 2D grid of blocks that the player actually mines, row by row.
    grid: Vec<Vec<Block>>,
}

impl Mine {
    /// Creates a new mine with the given blocks and size level.
    /// The grid is filled with the main block and the secondary blocks.
    pub fn new(main_block: Block, secondary_blocks: Vec<Block>) -> Self {
        let mut mine = Mine {
            main_block,
            secondary_blocks,
            size_level: 0,
            grid: Vec::new(),
        };
        Self::reset(&mut mine);
        mine
    }

    /// Resets the mine to its initial state, refilling the grid.
    ///
    /// `pub(crate)`: refilling a mine is something the *rules* do — on batch
    /// reset, when the last block falls — not something a front-end may ask for.
    /// A UI able to call this could hand the player an infinite mine by refilling
    /// it on demand.
    pub(crate) fn reset(&mut self) {
        self.grid.clear();
        let (width, height) = self.get_size();
        // Fill the grid with the main block for now
        self.grid = vec![vec![self.main_block; width as usize]; height as usize];
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
    pub(crate) fn upgrade_size_level(&mut self) -> Result<(), CoreError> {
        if self.size_level >= MAX_SIZE_LEVEL {
            return Err(CoreError::MineSizeMaxed {
                level: MAX_SIZE_LEVEL,
            });
        }
        self.size_level += 1;
        self.reset();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mine at the given size level. The block pool and grid are irrelevant to
    /// sizing, which reads `size_level` alone.
    fn mine_at(size_level: u32) -> Mine {
        Mine {
            main_block: Block::default(),
            secondary_blocks: Vec::new(),
            size_level,
            grid: Vec::new(),
        }
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

    #[test]
    fn new_mine_starts_full_of_the_main_block() {
        let main_block = Block::Stone;
        let secondary_blocks = vec![Block::CoalOre, Block::IronOre];
        let mine = Mine::new(main_block, secondary_blocks);
        let (width, height) = mine.get_size();
        for row in &mine.grid {
            assert_eq!(row.len(), width as usize);
            for &block in row {
                assert_eq!(block, main_block);
            }
        }
        assert_eq!(mine.grid.len(), height as usize);
    }

    #[test]
    fn upgrading_the_size_level_increases_the_grid_dimensions() {
        let mut mine = Mine::new(Block::Stone, vec![Block::CoalOre]);
        let (initial_width, initial_height) = mine.get_size();
        assert!(mine.upgrade_size_level().is_ok());
        let (new_width, new_height) = mine.get_size();
        assert!(new_width > initial_width);
        assert!(new_height >= initial_height);
    }

    #[test]
    fn size_level_is_correctly_updated_when_upgrading() {
        let mut mine = Mine::new(Block::Stone, vec![Block::CoalOre]);
        let initial_size_level = mine.get_size_level();
        assert!(mine.upgrade_size_level().is_ok());
        let new_size_level = mine.get_size_level();
        assert_eq!(new_size_level, initial_size_level + 1);
    }

    /// The whole table must be walkable, and the walk must stop exactly at the
    /// end of it — not one level further, where `get_size` starts clamping.
    #[test]
    fn the_size_ladder_ends_at_the_last_row_of_the_table() {
        let mut mine = Mine::new(Block::Stone, vec![Block::CoalOre]);
        while mine.get_size_level() < MAX_SIZE_LEVEL {
            assert!(mine.upgrade_size_level().is_ok());
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
            mine.upgrade_size_level(),
            Err(CoreError::MineSizeMaxed {
                level: MAX_SIZE_LEVEL,
            })
        );

        assert_eq!(mine.get_size_level(), MAX_SIZE_LEVEL);
        assert_eq!(mine.get_size(), size);
    }

    #[test]
    fn reset_clears_the_grid_and_refills_with_main_block() {
        let mut mine = Mine::new(Block::Stone, vec![Block::CoalOre]);
        mine.grid[0][0] = Block::IronOre; // Modify the grid
        mine.reset();
        let (width, height) = mine.get_size();
        for row in &mine.grid {
            assert_eq!(row.len(), width as usize);
            for &block in row {
                assert_eq!(block, Block::Stone);
            }
        }
        assert_eq!(mine.grid.len(), height as usize);
    }

    #[test]
    fn get_grid_returns_a_copy_of_the_grid() {
        let mine = Mine::new(Block::Stone, vec![Block::CoalOre]);
        let grid_copy = mine.get_grid();
        assert_eq!(grid_copy, mine.grid);
        // Modify the copy and ensure the original grid is unaffected
        let mut modified_copy = grid_copy.clone();
        modified_copy[0][0] = Block::IronOre;
        assert_ne!(modified_copy, mine.grid);
    }
}
