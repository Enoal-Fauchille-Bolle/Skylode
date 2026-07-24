//! The read model the screens render from.
//!
//! Screens never reach into game state directly — they render a **flat snapshot**.
//! The core now has `GameState` and `tick()`, but nothing is wired to them yet
//! (TUI phase 3), so [`View::sample`] hand-fills the numbers drawn in UI.md §5.
//! When the wiring lands, `GameState` produces this same struct and **nothing
//! under `screen/` changes**. That indirection is the whole reason UI work could
//! start before the rules existed.
//!
//! Keep it plain data: no methods that decide anything, no `Option`s standing in
//! for rules. A computation that belongs to the game belongs to the core.

use skylode_core::{block::Block, mine_kind::MineKind};

use crate::palette::ColourMode;

/// A frame's worth of game state, already reduced to what the UI prints.
#[derive(Clone, Debug)]
pub struct View {
    /// Mining level — the XP axis of the two-axis progression.
    pub player_level: u32,
    /// XP banked toward the next level, counted from zero (UI.md §6.5).
    pub xp: u32,
    /// XP the current level requires in total.
    pub xp_to_next: u32,
    /// Display name of the mine the player is standing in.
    pub mine_name: String,
    /// Pickaxe name plus its enchant summary, as one printable line.
    pub pickaxe: String,
    /// Raw units of the current mine's material that the player holds.
    pub raw_held: u32,
    /// Compressed units of the same material.
    pub compressed_held: u32,
    /// Which of the twelve mines the grid below belongs to — the only thing that
    /// answers what colour its cells take.
    pub mine_kind: MineKind,
    /// The grid itself, in `Mine::get_grid`'s shape: `None` is a broken cell.
    ///
    /// **Owned, and provisionally so.** Borrowing it would put a lifetime
    /// parameter on `View` and therefore on every screen signature, to save a
    /// clone of at most 200 `Option<Block>` per redraw — which at ~30 fps is
    /// nothing. Phase 3 wires this to a real `Mine` and is the right place to
    /// revisit it, with a measurement rather than a guess.
    pub grid: Vec<Vec<Option<Block>>>,
    /// The cell being dug, `None` before the first swing.
    pub target: Option<(u8, u8)>,
    /// How far that cell is from breaking, in `0.0..=1.0`.
    pub break_ratio: f32,
    /// How many colours to ask the terminal for — a player preference that lives
    /// in the save, and that the Settings screen will edit in phase 7.
    pub colour_mode: ColourMode,
}

impl View {
    /// The placeholder save drawn throughout UI.md §5: level 23, Diamond
    /// pickaxe, standing in the Iron Mine.
    ///
    /// These figures are chosen to match the wireframes so a rendered screen can
    /// be compared against the counted frame, not invented independently.
    pub fn sample() -> Self {
        Self {
            player_level: 23,
            xp: 1_240,
            xp_to_next: 3_200,
            mine_name: "Iron Mine".to_owned(),
            pickaxe: "Diamond Pickaxe  Efficiency IV".to_owned(),
            raw_held: 480,
            compressed_held: 2,
            mine_kind: MineKind::Iron,
            grid: sample_grid(),
            target: Some((3, 2)),
            break_ratio: 0.61,
            colour_mode: ColourMode::default(),
        }
    }
}

/// The 12x7 grid drawn in UI.md §5.1, cell for cell.
///
/// Transcribed rather than generated: `Mine::new` would need a seed, and the
/// figure the frame shows — five value cells, seven holes, a target three cells
/// into row two — is what the counted wireframe asserts. A generated grid would
/// make the screen impossible to compare against the document it implements.
fn sample_grid() -> Vec<Vec<Option<Block>>> {
    // One letter each, so the rows below line up as a picture of the frame:
    // `O` an ore cell, `B` an iron block, `X` a hole.
    const O: Option<Block> = Some(Block::IronOre);
    const B: Option<Block> = Some(Block::IronBlock);
    const X: Option<Block> = None;

    vec![
        vec![O, O, O, B, O, O, X, O, O, O, O, O],
        vec![O, X, O, O, O, O, O, B, O, O, X, O],
        vec![O, O, O, O, O, O, O, O, O, O, O, O],
        vec![O, O, X, O, B, O, O, O, O, X, O, O],
        vec![O, O, O, O, O, O, B, O, O, O, O, O],
        vec![X, O, O, B, O, O, O, O, O, O, O, X],
        vec![O, O, O, O, B, O, X, O, O, O, O, O],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_grid_is_the_size_the_wireframe_counts() {
        let view = View::sample();
        assert_eq!(view.grid.len(), 7);
        for row in &view.grid {
            assert_eq!(row.len(), 12, "the grid is not rectangular");
        }
    }

    #[test]
    fn the_sample_target_names_a_standing_cell() {
        // A target pointing at a hole would draw a crack on the terminal's own
        // background, which is a state the rules cannot produce.
        let view = View::sample();
        let cell = view.target.and_then(|(x, y)| {
            view.grid
                .get(usize::from(y))
                .and_then(|row| row.get(usize::from(x)))
                .copied()
        });

        // The nesting is the assertion: the outer `Some` means there *is* a
        // target and it is inside the grid, the inner one that a block stands
        // there. `None` and `Some(None)` are the two ways this can be wrong.
        assert_eq!(cell, Some(Some(Block::IronOre)));
    }
}
