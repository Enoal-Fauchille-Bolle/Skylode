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

/// One row of the Levels roadmap (UI.md §5.6).
///
/// `grants` is a **placeholder string** — the core exposes `xp_for_level` but not
/// yet a `loot_for_level`, so what each level pays is transcribed from the frame
/// rather than derived; the tick wires the real bundles (phase 7). `xp` is the
/// per-level requirement counted from zero (`level × 100` on today's curve), which
/// is what the status bar's `1 240 / 2 300` also counts against.
#[derive(Clone, Debug)]
pub struct LevelRow {
    /// The level this row is for.
    pub level: u32,
    /// What reaching it grants, pre-formatted: `+115 Quartz, +80 A. Debris, …`, or
    /// a world line like `The Nether opens, +1 charge`.
    pub grants: String,
    /// The XP that level costs, counted from zero.
    pub xp: u32,
}

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
    /// The visible window of the Levels roadmap (UI.md §5.6).
    ///
    /// The **window**, not the whole 1..50 ladder: phase 2 has no scroll state to
    /// pick a window with, so the fixture *is* the window the frame draws. The
    /// scrollbar's total comes from the core's `LEVEL_CAP`, not from this length.
    pub levels: Vec<LevelRow>,
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
            levels: sample_levels(),
            colour_mode: ColourMode::default(),
        }
    }
}

/// The Levels roadmap window drawn in UI.md §5.6, levels 13..=31.
///
/// Transcribed from the frame rather than generated: `xp` follows `level × 100`
/// and could be computed, but the grants cannot — `loot_for_level` does not exist
/// in the core yet — so the whole window is fixture data, kept together so the
/// screen can be compared row for row against the document it implements. Levels
/// 15 and 30 grant a world and no loot, which is why their lines look different.
fn sample_levels() -> Vec<LevelRow> {
    // `(level, grants, xp)` triples, verbatim from the wireframe.
    [
        (13, "+65 Lapis, +45 Gold, +19 Diamond", 1_300),
        (14, "+70 Lapis, +49 Gold, +21 Diamond", 1_400),
        (15, "The Nether opens, +1 charge", 1_500),
        (16, "+80 Quartz, +56 Netherrack, +24 A. Debris", 1_600),
        (17, "+85 Quartz, +59 Netherrack, +25 A. Debris", 1_700),
        (
            18,
            "+90 Quartz, +63 Netherrack, +27 A. Debris, +45 Emerald",
            1_800,
        ),
        (19, "+95 Quartz, +66 Netherrack, +28 A. Debris", 1_900),
        (
            20,
            "+100 Quartz, +70 Netherrack, +30 A. Debris, +1 charge",
            2_000,
        ),
        (
            21,
            "+105 Quartz, +73 A. Debris, +31 Obsidian, +52 Emerald",
            2_100,
        ),
        (22, "+110 Quartz, +77 A. Debris, +33 Obsidian", 2_200),
        (23, "+115 Quartz, +80 A. Debris, +34 Obsidian", 2_300),
        (
            24,
            "+120 Quartz, +84 A. Debris, +36 Obsidian, +60 Emerald",
            2_400,
        ),
        (
            25,
            "+125 Quartz, +87 A. Debris, +37 Obsidian, +1 charge",
            2_500,
        ),
        (26, "+130 Quartz, +91 Obsidian, +39 Crying Obs.", 2_600),
        (
            27,
            "+135 Quartz, +94 Obsidian, +40 Crying Obs., +67 Emerald",
            2_700,
        ),
        (28, "+140 Quartz, +98 Obsidian, +42 Crying Obs.", 2_800),
        (29, "+145 Quartz, +101 Obsidian, +43 Crying Obs.", 2_900),
        (30, "The End opens, +1 charge", 3_000),
        (31, "+233 End Stone, +77 Amethyst", 3_100),
    ]
    .into_iter()
    .map(|(level, grants, xp)| LevelRow {
        level,
        grants: grants.to_owned(),
        xp,
    })
    .collect()
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
