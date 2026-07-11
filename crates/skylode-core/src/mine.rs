//! Generated mines.
//!
//! A [`Mine`] is the grid of [`Block`]s the player digs through. Its dimensions
//! scale with a size level, from a tiny 3x3 starter mine up to a 20x10 mine at
//! the top of the `MINE_SIZES` table.

use crate::block::Block;

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

/// A generated mine: a pool of possible blocks plus the laid-out grid the
/// player mines through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mine {
    /// The blocks that make up the mine. The first entry is the main block;
    /// the rest are secondary blocks that can also appear.
    pub blocks: Vec<Block>,

    /// Size tier of the mine; indexes into `MINE_SIZES` via
    /// [`get_size`](Mine::get_size).
    pub size_level: u32,
    /// The 2D grid of blocks that the player actually mines, row by row.
    pub grid: Vec<Vec<Block>>,
}

impl Mine {
    // Per-block mining tick, disabled while the model moves from a single
    // `material`/`break_progress` block to the grid-based `Mine` above. When
    // re-enabled it will accumulate `mining_power` against the block's hardness
    // and return the drop amount once the block breaks (Fortune applied by the
    // caller):
    //
    // pub fn tick(&mut self, mining_power: f32) -> f32 {
    //     self.break_progress += mining_power;
    //     let hardness = self.material.hardness();
    //     if self.break_progress >= hardness {
    //         self.break_progress = 0.0;
    //         1.0 // base drop; Fortune multiplies at the caller
    //     } else {
    //         0.0
    //     }
    // }

    /// Returns this mine's `(width, height)` in blocks.
    ///
    /// Looks the dimensions up in `MINE_SIZES` by
    /// [`size_level`](Mine::size_level). Because `size_level` is a `u32` while
    /// the table has only 10 entries, any out-of-range level is clamped to the
    /// largest size rather than panicking.
    pub fn get_size(&self) -> (u8, u8) {
        let index = self.size_level as usize;
        if index < MINE_SIZES.len() {
            MINE_SIZES[index]
        } else {
            MINE_SIZES[MINE_SIZES.len() - 1] // Return the largest size if out of bounds
        }
    }
}
