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
    blocks: Vec<Block>,
    /// Size tier of the mine; indexes into `MINE_SIZES` via
    /// [`get_size`](Mine::get_size).
    size_level: u32,
    /// The 2D grid of blocks that the player actually mines, row by row.
    grid: Vec<Vec<Block>>,
}

impl Mine {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A mine at the given size level. The block pool and grid are irrelevant to
    /// sizing, which reads `size_level` alone.
    fn mine_at(size_level: u32) -> Mine {
        Mine {
            blocks: Vec::new(),
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
    /// past the end must clamp to the largest mine. Indexing straight into the
    /// table would panic instead — and this is reachable, since nothing caps the
    /// field.
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
}
