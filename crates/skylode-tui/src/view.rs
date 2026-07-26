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

/// The Pickaxe panel of the Mine screen (UI.md §5.1).
///
/// **Provisional, and partly pre-formatted.** `summary` and the enchant lines are
/// strings the core does not yet compute — the tick owns the boost timer and the
/// enchant roster (phase 3/7). `power` is carried as a number because the screen
/// multiplies it by the boost to show the product, and a formatted string could
/// not be multiplied. When phase 3 wires `Pickaxe`, the strings become derivations
/// and this struct is where that lands, changing nothing under `screen/`.
#[derive(Clone, Debug)]
pub struct PickaxeView {
    /// Name plus the Efficiency level, as one line: `Diamond Pickaxe  Efficiency IV`.
    pub summary: String,
    /// Base mining power, before the boost — the screen shows `power × boost`.
    pub power: f64,
    /// The Fortune line, pre-formatted: `Fortune III   drops ×4` (placeholder).
    pub fortune: String,
    /// The special-enchant roster, pre-formatted: `Exp II   Jck I   Exc I`
    /// (placeholder — the roster arrives with the tick, phase 7).
    pub enchants: String,
}

/// The temporary Redstone boost, shown as the third status gauge (UI.md §5.1).
///
/// The permanent Haste enchant has no countdown and is deliberately absent here;
/// this is the one with a timer. `ratio` is a placeholder until the tick owns the
/// countdown (phase 7) — there is no clock in the core to derive it from yet.
#[derive(Clone, Debug)]
pub struct BoostView {
    /// Seconds left on the boost.
    pub seconds: u32,
    /// The multiplier it applies to mining power, e.g. `1.5`.
    pub multiplier: f64,
    /// How full the countdown gauge is, in `0.0..=1.0` (placeholder; phase 7).
    pub ratio: f32,
}

/// The Mine panel of the Mine screen — the standing mine's own figures (UI.md §5.1).
///
/// The world, the block counts and the grid size are **derived from the grid** in
/// the screen, so they are not fields here. What is left are the three numbers the
/// core does not yet expose: the size level, the richness level, and the value
/// weight. `value_percent` is `Mine::value_weight_percent()`'s answer, which is a
/// phase-3 core read; `richness_max` is carried rather than hardcoded because the
/// core's `MAX_RICHNESS_LEVEL` is `pub(crate)` and this crate cannot see it.
#[derive(Clone, Debug)]
pub struct MinePanelView {
    /// The mine's purchased size level, e.g. `5`.
    pub size_level: u32,
    /// The richness level the player has bought, `0..=richness_max`.
    pub richness_level: u32,
    /// The richness ceiling — 9 in today's rules, carried so the tui need not
    /// mirror the core's `pub(crate)` constant.
    pub richness_max: u32,
    /// The value cells' weight, as a percentage (placeholder; phase 3 derives it
    /// from `Mine::value_weight_percent()`).
    pub value_percent: u32,
}

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
    /// The Pickaxe panel's figures.
    pub pickaxe: PickaxeView,
    /// The Mine panel's figures.
    pub mine_panel: MinePanelView,
    /// The Redstone boost gauge.
    pub boost: BoostView,
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
    /// The name of the block being dug, for the Break gauge label: `Iron Block`.
    ///
    /// A placeholder string, because `Block` exposes its `material` but not a
    /// display name of its own — "Iron Block" is not derivable from the grid cell
    /// without a table the core does not yet own.
    pub target_name: String,
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
            xp_to_next: 2_300,
            mine_name: "Iron Mine".to_owned(),
            pickaxe: PickaxeView {
                summary: "Diamond Pickaxe  Efficiency IV".to_owned(),
                power: 25.0,
                fortune: "Fortune III   drops ×4".to_owned(),
                enchants: "Exp II   Jck I   Exc I".to_owned(),
            },
            mine_panel: MinePanelView {
                size_level: 5,
                richness_level: 0,
                richness_max: 9,
                value_percent: 10,
            },
            boost: BoostView {
                seconds: 12,
                multiplier: 1.5,
                ratio: 0.68,
            },
            raw_held: 480,
            compressed_held: 2,
            mine_kind: MineKind::Iron,
            grid: sample_grid(),
            target: Some((7, 1)),
            break_ratio: 0.61,
            target_name: "Iron Block".to_owned(),
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
        //
        // It lands on the value block on purpose: the Break gauge prints
        // `target_name` ("Iron Block"), and the sample's target must be the cell
        // that name describes, or the label would contradict the crack it draws.
        assert_eq!(cell, Some(Some(Block::IronBlock)));
    }
}
