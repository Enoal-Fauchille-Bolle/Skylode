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

use skylode_core::{block::Block, mine_kind::MineKind, pickaxe::PickaxeTier, tunables::LEVEL_CAP};

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

/// Which sub-tab of the Upgrades screen is showing (UI.md §5.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpgradeTab {
    /// The pickaxe ladder — a single linear roadmap, no rung skippable.
    Pickaxe,
    /// The six enchant tracks, each at its frontier.
    Enchants,
    /// The twelve mines' size and richness tracks.
    Mines,
}

/// One row of an Upgrades sub-tab's list (UI.md §5.4).
///
/// Two mark channels, because the Pickaxe ladder carries both: `cursor`/`current`
/// are where you are and where the selection sits (`▸`/`●`), while `mark` is
/// **cumulative reachability** — `✓`/`~`/`✗`, "reachable buying every rung from
/// here". All fixture data; the real marks are a phase-6 core read, and the ladder
/// invariant (the `✓` region is a contiguous prefix) is asserted on the fixture.
#[derive(Clone, Debug)]
pub struct UpgradeRow {
    /// The row's own text — the rung label, or a track line already laid out.
    pub text: String,
    /// The reachability mark: `✓`, `~`, `✗`, `—`, or empty.
    pub mark: String,
    /// Whether the selection cursor sits here — drawn `▸`.
    pub cursor: bool,
    /// Whether this is the player's current position — drawn `●` (Pickaxe only).
    pub current: bool,
}

/// One sub-tab of the Upgrades screen: a list on the left, a detail pane on the
/// right (UI.md §5.4).
///
/// The detail pane is a **pre-formatted block of lines**, transcribed from the
/// frame — the dip box, the costs and the affordability are all placeholder prose
/// the core does not yet answer (phases 5–6), so there is nothing to derive and
/// the box art travels as text.
///
/// **`rows` is the whole ladder, not the visible slice.** It used to be the slice,
/// with a `scroll: Option<(total, position)>` beside it saying how much had been cut
/// off — which meant the view decided how many rows fit, and therefore that a taller
/// terminal could not show more of them. How many fit is a property of the `Rect`,
/// so it is now answered where the `Rect` is known: the screen windows this list at
/// render time through [`crate::screen::window`]. Whether a scrollbar is drawn stops
/// being data and becomes `rows.len() > visible`.
#[derive(Clone, Debug)]
pub struct UpgradeSubtab {
    /// Header rows printed above the list (the column titles), if any.
    pub header: Vec<String>,
    /// Every row of the list, in ladder order.
    pub rows: Vec<UpgradeRow>,
    /// The index of the topmost drawn row — scroll position, not selection.
    ///
    /// Carried rather than derived from the cursor because scrolling has to be
    /// *minimal*: a window recomputed from the selection alone would jump under the
    /// player on every keypress. The screen adjusts it only when the cursor would
    /// otherwise fall off an edge, which is what [`crate::screen::window`] does.
    pub offset: usize,
    /// The detail pane, already laid out line by line.
    pub detail: Vec<String>,
    /// The screen-local footer for this sub-tab.
    pub footer: String,
}

impl UpgradeSubtab {
    /// Where the selection sits, as an index into [`Self::rows`].
    ///
    /// Derived from the rows' own `cursor` flag rather than stored beside them: two
    /// copies of "which row is selected" can disagree, and this way the marked row
    /// and the scrolled-to row are the same fact read twice. The scan is 46 elements
    /// at worst, once per redraw.
    ///
    /// Falls back to `0` when no row claims the cursor — a list has to draw
    /// somewhere, and refusing would make an empty ladder unrenderable.
    pub fn cursor(&self) -> usize {
        self.rows.iter().position(|row| row.cursor).unwrap_or(0)
    }
}

/// The Upgrades screen: the three sub-tabs and which one is showing (UI.md §5.4).
///
/// `active` is a front-end cursor, fixed here until sub-tab switching is wired; the
/// data for all three is carried so each renders on its own for the frame tests.
#[derive(Clone, Debug)]
pub struct UpgradesView {
    /// The sub-tab currently drawn.
    pub active: UpgradeTab,
    /// The pickaxe ladder.
    pub pickaxe: UpgradeSubtab,
    /// The six enchant tracks.
    pub enchants: UpgradeSubtab,
    /// The mines' size and richness tracks.
    pub mines: UpgradeSubtab,
}

impl UpgradesView {
    /// The sub-tab that `active` names.
    ///
    /// A total accessor, not a decision the view makes: the screen asks for the
    /// showing sub-tab and gets it, rather than re-matching the enum at each of the
    /// three places it draws from.
    pub fn active_subtab(&self) -> &UpgradeSubtab {
        match self.active {
            UpgradeTab::Pickaxe => &self.pickaxe,
            UpgradeTab::Enchants => &self.enchants,
            UpgradeTab::Mines => &self.mines,
        }
    }
}

/// One mine's row in the Mines list (UI.md §5.2).
///
/// The world grouping, the mine name and whether it is two-material are all read
/// from `kind` in the screen; only `detail` — the size and richness the run has
/// bought, or a lock reason for a mine still closed — is fixture data. That lock
/// reason is a **string here on purpose**: `MineLock` keeps its two axes private
/// (the "nameable lock reason" is TODO-TUI phase 4's core gap), so phase 2 prints
/// the frame's text rather than deriving it.
#[derive(Clone, Debug)]
pub struct MineListRow {
    /// Which mine this row is — the source of its name and world.
    pub kind: MineKind,
    /// The right-hand column: `8 x 5   R 6`, or `locked   Netherite` when shut.
    pub detail: String,
}

/// The detail pane of the selected mine (UI.md §5.2).
///
/// Placeholder numbers throughout — sizes, block counts and richness are run state
/// the tick owns (phase 7). `value_percent` drives the richness **dial**, the free
/// reversible cursor drawn only on the three two-material mines; `dial_split` is
/// its readout (`Crying 64%   Obsidian 36%`). The two gate lines carry their own
/// `✓` because the tick against the player's tier is not derivable from `View` yet.
#[derive(Clone, Debug)]
pub struct MineDetail {
    /// The world row: `Nether        Lv 15  ✓`.
    pub world_line: String,
    /// The pickaxe-gate row: `Diamond pickaxe      ✓`.
    pub gate_line: String,
    /// Grid size, as `(width, height)`.
    pub size: (u32, u32),
    /// The purchased size level.
    pub size_level: u32,
    /// Standing blocks over total.
    pub blocks_standing: u32,
    /// Total blocks in the grid.
    pub blocks_total: u32,
    /// The purchased richness level.
    pub richness_level: u32,
    /// The richness ceiling (9 today).
    pub richness_max: u32,
    /// The dial's value-material weight, as a percent; the common weight is its
    /// complement. Drives the bar fill on the two-material mines.
    pub value_percent: u32,
    /// The dial readout: `Crying 64%   Obsidian 36%`.
    pub dial_split: String,
    /// The mine-specific note under the dial (Obsidian's optimum-not-maximum).
    pub note: Vec<String>,
}

/// The Mines screen: the world-grouped list and the selected mine's detail pane
/// (UI.md §5.2).
///
/// `selected` is the mine the detail pane describes and the list marks `▸`; it is a
/// front-end cursor, fixed here until `↑↓` is wired (phase 4).
#[derive(Clone, Debug)]
pub struct MinesView {
    /// The twelve mines, in display order; the screen groups them by world.
    pub rows: Vec<MineListRow>,
    /// The mine under the cursor.
    pub selected: MineKind,
    /// The selected mine's detail pane.
    pub detail: MineDetail,
}

/// One material's row in the Inventory table (UI.md §5.3).
///
/// Held in both denominations, exactly as the player carries them: the raw count
/// and the compressed count are separate numbers, never a single total, because
/// costs are paid in the denomination they are quoted in and the screen must show
/// which one the player is short of. `material` is a display string (fixture here;
/// phase 5 fills the table from `Inventory`).
#[derive(Clone, Debug)]
pub struct InvRow {
    /// The material's display name, e.g. `Ancient Debris`.
    pub material: String,
    /// Compressed units held.
    pub compressed: u32,
    /// Raw units held.
    pub raw: u32,
}

/// The Inventory screen: the table, the cursor, and the compress-first context
/// (UI.md §5.3).
///
/// `selected` is a **front-end cursor**, not game state — it moves to `App` when
/// `↑↓` is wired (phase 5); here it is fixed on the row the frame highlights.
/// `hint` is the compress-first refusal spelled out (`Efficiency V wants 6
/// Compressed + 50`), a **placeholder** until the three-state affordability query
/// lands (phase 5) — the frame is drawn mid-refusal on purpose, so the panel names
/// the missing *denomination* rather than claiming the player cannot afford it.
#[derive(Clone, Debug)]
pub struct InventoryView {
    /// The fifteen materials, in the fixed display order the frame lists.
    pub rows: Vec<InvRow>,
    /// Which row the cursor sits on, indexing `rows`.
    pub selected: usize,
    /// The compress-first context lines, already wrapped to the panel width.
    pub hint: Vec<String>,
}

/// One run-progress row in the Stats "This run" panel (UI.md §5.5).
///
/// **Run progress, not achievements** — every row is a predicate over the run that
/// resets with a prestige, which the tick will evaluate (phase 7); here it is
/// fixture data. `detail` carries the frame's trailing text verbatim (`Lv 30`,
/// `23/30`, `Stone 20x10 R9  ✓`), so a sub-mark inside it is just part of the
/// string, distinct from the row's own leading `done`/`current` mark.
#[derive(Clone, Debug)]
pub struct Milestone {
    /// Whether the run has cleared this goal — drawn `✓`.
    pub done: bool,
    /// The next goal in line, the one the run is working toward — drawn `▸`.
    pub current: bool,
    /// The goal itself, e.g. `Reach the End`.
    pub text: String,
    /// The frame's right-hand detail, or empty: `Lv 30    23/30`.
    pub detail: String,
}

/// The three panels of the Stats screen (UI.md §5.5).
///
/// All **placeholder** data: the prestige figures, the lifetime counters, the run
/// milestones and the history are what the tick and the save own (phase 7). The
/// worlds table and the level cap are *not* here — the screen derives them from
/// `World` and `LEVEL_CAP`, which already answer them. `blocks_broken` is a `u32`
/// for now because `format::grouped` takes one and the fixture fits; the lifetime
/// type is settled when `GameState` fills this in.
#[derive(Clone, Debug)]
pub struct StatsView {
    /// Prestige rank, pre-formatted: `II` (placeholder — no roman helper yet).
    pub prestige_rank: String,
    /// The current global multiplier: `×1.20`.
    pub multiplier: String,
    /// What the next rank would grant: `×1.30`.
    pub next_multiplier: String,
    /// What the next prestige costs, in `prestige_material`.
    pub prestige_cost: u32,
    /// The material a prestige is paid in: `Amethyst`.
    pub prestige_material: String,
    /// How much of it the player holds.
    pub prestige_held: u32,
    /// Lifetime blocks broken — survives prestige.
    pub blocks_broken: u32,
    /// Lifetime playtime, pre-formatted: `14h 22m`.
    pub playtime: String,
    /// Time in the current run, pre-formatted: `3h 07m` — resets with a prestige.
    pub this_run: String,
    /// The run-progress rows of the "This run" panel.
    pub milestones: Vec<Milestone>,
    /// The event history, the toast log verbatim: `20:14  Excavator!  +1 …`.
    ///
    /// The whole log, newest first — the panel shows as much of it as its box has
    /// rows, which on a tall terminal is a good deal more than the ten UI.md §5.7
    /// had room to draw.
    pub history: Vec<String>,
    /// The topmost drawn history line. Zero in the fixture: a log read newest-first
    /// is one nobody scrolls *up* in.
    pub history_offset: usize,
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
    /// The **whole** Levels roadmap, `1..=LEVEL_CAP` (UI.md §5.6).
    ///
    /// It was the visible window until the screens learned to window their own
    /// lists; carrying the slice meant a taller terminal could not show more of the
    /// ladder, because the extra rows were never in the view to begin with.
    pub levels: Vec<LevelRow>,
    /// The topmost drawn row of that roadmap — scroll position, not selection.
    ///
    /// The cursor is `player_level`, so unlike the Upgrades sub-tabs there is
    /// nothing to derive; only where the window sits is state.
    pub levels_offset: usize,
    /// The three panels of the Stats screen (UI.md §5.5).
    pub stats: StatsView,
    /// The Inventory table and its compress panel (UI.md §5.3).
    pub inventory: InventoryView,
    /// The Mines list and the selected mine's detail pane (UI.md §5.2).
    pub mines: MinesView,
    /// The Upgrades screen's three sub-tabs (UI.md §5.4).
    pub upgrades: UpgradesView,
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
        // **The one line that switches grid fixture.** Swap in
        // `sample_grid_small_5x5` or `sample_grid_wireframe_12x7` to see the same
        // screen at another mine size; the `#[expect(dead_code)]` on whichever two
        // are dormant then turns into a build error naming the one you just woke up,
        // which is the reminder to clean the attribute off it.
        let (grid, target) = sample_grid_full_20x10();
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
            grid,
            target: Some(target),
            break_ratio: 0.61,
            target_name: "Iron Block".to_owned(),
            levels: sample_levels(),
            levels_offset: LEVELS_OFFSET,
            stats: sample_stats(),
            inventory: sample_inventory(),
            mines: sample_mines(),
            upgrades: sample_upgrades(),
            colour_mode: ColourMode::default(),
        }
    }
}

/// The three Upgrades sub-tabs drawn in UI.md §5.4, transcribed from the frames.
///
/// Pickaxe is active. Every row's marks and every detail pane are fixture data:
/// the reachability marks are a phase-6 core read, and the dip numbers, costs and
/// affordability the panes quote are phases 5–6 too. The Pickaxe marks are laid
/// out to honour the ladder invariant — the `✓` region is a contiguous prefix from
/// the current rung — so the fixture is a legal ladder, not an arbitrary one.
/// Roman numerals `I`..=`XV` — exactly the range an Efficiency level can take, since
/// [`PickaxeTier::efficiency_cap`] tops out at 15 on Netherite.
const ROMAN: [&str; 15] = [
    "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV", "XV",
];

/// The rung the fixture's player stands on — dotted `●` in the list.
const CURRENT_RUNG: &str = "Diamond Eff IV";

/// The rung the fixture's cursor sits on — the tier jump the detail pane warns about.
const SELECTED_RUNG: &str = "Netherite Pickaxe";

/// The topmost drawn rung at 80×24, which is what makes the counted frame the
/// counted frame: `window(46, 30, 27, 19)` is `27..46`, and row 27 is
/// `Diamond Eff III`, exactly as UI-EN.md §5.5 drew it.
const PICKAXE_OFFSET: usize = 27;

/// The whole pickaxe roadmap — six tiers, each with its Efficiency levels.
///
/// **Generated, not transcribed, and that is a change of kind.** The old fixture
/// held the nineteen rungs that fit an 80×24 window; this holds all 46, because the
/// window is now the screen's business. The count is not written down anywhere: it
/// falls out of walking [`PickaxeTier::next`] and asking each tier its
/// [`efficiency_cap`](PickaxeTier::efficiency_cap) — 5 × (1 + 5) + (1 + 15). If the
/// core ever raises a cap, this ladder grows with it instead of contradicting it.
///
/// The marks are still fixture data (real reachability is a phase-6 core read), and
/// they are placed **relative to the two named rungs** rather than at hardcoded
/// indices, so inserting a tier cannot silently slide the `●` onto the wrong row.
/// They honour the ladder invariant by construction: `""` while owned, then a
/// contiguous `✓` run, then `~`, then `✗` — never a `✓` after a `✗`.
fn pickaxe_ladder() -> Vec<UpgradeRow> {
    let name = |tier: PickaxeTier| match tier {
        PickaxeTier::Wooden => "Wooden",
        PickaxeTier::Stone => "Stone",
        PickaxeTier::Iron => "Iron",
        PickaxeTier::Gold => "Gold",
        PickaxeTier::Diamond => "Diamond",
        PickaxeTier::Netherite => "Netherite",
    };

    let mut labels = Vec::new();
    let mut tier = Some(PickaxeTier::Wooden);
    while let Some(current) = tier {
        labels.push(format!("{} Pickaxe", name(current)));
        for level in 1..=usize::from(current.efficiency_cap()) {
            // `level - 1` cannot underflow: the range starts at one, and a cap of
            // zero would make the loop body unreachable rather than index at -1.
            let numeral = ROMAN.get(level - 1).copied().unwrap_or("?");
            labels.push(format!("{} Eff {numeral}", name(current)));
        }
        tier = current.next();
    }

    // `position` rather than a constant: the two rungs are named, and where they
    // land is whatever the walk above put them at.
    let current = labels.iter().position(|l| l == CURRENT_RUNG).unwrap_or(0);
    let selected = labels.iter().position(|l| l == SELECTED_RUNG).unwrap_or(0);

    labels
        .into_iter()
        .enumerate()
        .map(|(index, text)| UpgradeRow {
            mark: match index {
                // Owned already, so there is nothing to be able to afford.
                i if i <= current => "",
                // Reachable buying every rung from here — the cumulative sense.
                i if i <= selected => "✓",
                // The third state: the ore is held, the denomination is not.
                i if i == selected + 1 => "~",
                _ => "✗",
            }
            .to_owned(),
            cursor: index == selected,
            current: index == current,
            text,
        })
        .collect()
}

fn sample_upgrades() -> UpgradesView {
    /// `(text, mark, cursor, current)` → one row.
    fn r(text: &str, mark: &str, cursor: bool, current: bool) -> UpgradeRow {
        UpgradeRow {
            text: text.to_owned(),
            mark: mark.to_owned(),
            cursor,
            current,
        }
    }
    /// Turns a slice of `&str` into owned lines.
    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_owned()).collect()
    }

    let select_footer =
        " ↑↓  select     Enter  buy one level     M  buy to cap     Tab  next screen";

    let pickaxe = UpgradeSubtab {
        header: Vec::new(),
        rows: pickaxe_ladder(),
        offset: PICKAXE_OFFSET,
        detail: lines(&[
            " Netherite Pickaxe             tier jump",
            "",
            " Chain    Diamond Eff V + the jump      ✓",
            " Cost     2 Compressed Diamond",
            "          + 4 Compressed Ancient Debris",
            "          + 60 Ancient Debris",
            "",
            " ┌──────────────────────────────────┐",
            " │ Power  34.0 → 9.0                │",
            " │ Ancient Debris  27 → 100 ticks   │",
            " │ Repaid at Netherite Eff V (35.0) │",
            " └──────────────────────────────────┘",
            "",
            " Unlocks  the End's Amethyst mine,",
            "          gated behind Netherite",
            "",
            " Ceiling  Efficiency 5 → 15",
            "",
            " Enter  buy the chain   (confirms: dip)",
        ]),
        footer: " ↑↓  select     Enter  buy to here     M  buy max     Tab  next screen".to_owned(),
    };

    let enchants = UpgradeSubtab {
        header: lines(&["   Enchant     Level     Cap", ""]),
        rows: vec![
            r("Fortune     III → IV  10", "✓", false, false),
            r("Explosive   II → III  6", "✓", true, false),
            r("Jackhammer  I → II    6", "~", false, false),
            r("Nuke        0 → I     6", "✗", false, false),
            r("Excavator   I → II    6", "✗", false, false),
            r("Haste       0 → I     6", "✗", false, false),
        ],
        // Six tracks, and no terminal this crate will draw into is shorter than the
        // nineteen rows they fit in — so this sub-tab never scrolls and its offset is
        // structurally zero rather than merely happening to be.
        offset: 0,
        detail: lines(&[
            " Explosive                  level II",
            "",
            " Effect   clears a 3x3 square on a",
            "          proc, centred on the cell",
            "",
            " Next     III — still 3x3. The square",
            "          grows to 5x5 at IV, 7x7 at",
            "          VII.",
            "",
            " Cost     3 Compressed Quartz",
            "          + 40 Redstone            ✓",
            "",
            " Cap      6 — the Nether's, and one",
            "          number for all five",
            "          specials. Overworld 3,",
            "          End 10.",
            "",
            " Every level also procs more often.",
            " Enter  buy one level",
        ]),
        footer: select_footer.to_owned(),
    };

    let mines = UpgradeSubtab {
        header: lines(&["   Mine           Track    Next"]),
        // All twelve mines, both tracks each — the six rows above the counted window
        // (Stone, Coal and Iron, the three already maxed or nearly so) were the ones
        // the old fixture cut off to fit nineteen rows.
        rows: vec![
            r("Stone          Size     maxed", "—", false, false),
            r("Stone          Richness maxed", "—", false, false),
            r("Coal           Size     20x10", "~", false, false),
            r("Coal           Richness 8", "~", false, false),
            r("Iron           Size     14x8", "✓", false, false),
            r("Iron           Richness 1", "✓", false, false),
            r("Gold           Size     12x7", "~", false, false),
            r("Gold           Richness 3", "~", false, false),
            r("Lapis          Size     10x6", "✗", false, false),
            r("Lapis          Richness 2", "✗", false, false),
            r("Redstone       Size     8x5", "✓", false, false),
            r("Redstone       Richness 1", "✓", false, false),
            r("Emerald        Size     8x5", "✗", false, false),
            r("Emerald        Richness 1", "✗", false, false),
            r("Diamond        Size     10x6", "✗", false, false),
            r("Diamond        Richness 2", "✗", false, false),
            r("Quartz         Size     10x6", "✗", false, false),
            r("Quartz         Richness 4", "✗", false, false),
            r("Ancient Debris Size     8x5", "✓", false, false),
            r("Ancient Debris Richness 1", "✓", false, false),
            r("Obsidian       Size     10x6", "✗", false, false),
            r("Obsidian       Richness 7", "✗", true, false),
            r("End            Size     Lv 30", "—", false, false),
            r("End            Richness Lv 30", "—", false, false),
        ],
        // Row 6 (`Gold Size`) at the top, which is where the counted frame starts —
        // `window(24, 21, 6, 18)` is `6..24`, the cursor on `Obsidian Richness`.
        offset: 6,
        detail: lines(&[
            " Obsidian Mine — richness",
            "",
            " Ceiling   level 6 → 7",
            " Dial      free, on the Mines screen",
            "",
            " At 7      Crying Obsidian 73%",
            "           Obsidian 27%",
            "",
            " Cost      2 Compressed Obsidian",
            "           + 40 Crying Obsidian",
            "",
            " You hold  0 Compressed Obsidian, 21",
            "           raw · 2 Crying Obsidian  ✗",
            "",
            " This buys the ceiling only. The",
            " dial slides anywhere at or below",
            " it, free and reversible, on the",
            " Mines screen.",
            " Enter  buy the next level",
        ]),
        footer: select_footer.to_owned(),
    };

    UpgradesView {
        active: UpgradeTab::Pickaxe,
        pickaxe,
        enchants,
        mines,
    }
}

/// The Mines list and detail pane drawn in UI.md §5.2, from the frame.
///
/// Obsidian is selected — a two-material mine, so the detail pane shows the
/// richness dial. The list's sizes and richness levels are fixture data; the End
/// mine is drawn locked, its lock reason (`Netherite`) a string, since the core
/// does not yet expose a nameable one.
fn sample_mines() -> MinesView {
    // `(kind, detail)` in display order — Overworld, then Nether, then the locked
    // End mine. `MineKind::Amethyst` is the End mine, named "End".
    let rows = [
        (MineKind::Stone, "20 x 10   R 9"),
        (MineKind::Coal, "18 x 9   R 7"),
        (MineKind::Iron, "12 x 7   R 0"),
        (MineKind::Gold, "10 x 6   R 2"),
        (MineKind::Lapis, "8 x 5   R 1"),
        (MineKind::Redstone, "6 x 4   R 0"),
        (MineKind::Emerald, "6 x 4   R 0"),
        (MineKind::Diamond, "8 x 5   R 1"),
        (MineKind::Quartz, "8 x 5   R 3"),
        (MineKind::AncientDebris, "6 x 4   R 0"),
        (MineKind::Obsidian, "8 x 5   R 6"),
        (MineKind::Amethyst, "locked   Netherite"),
    ]
    .into_iter()
    .map(|(kind, detail)| MineListRow {
        kind,
        detail: detail.to_owned(),
    })
    .collect();

    let note = [
        "The enhancement past Netherite eats",
        "both of them, so this dial has an",
        "optimum, not a maximum.",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    MinesView {
        rows,
        selected: MineKind::Obsidian,
        detail: MineDetail {
            world_line: "Nether        Lv 15  ✓".to_owned(),
            gate_line: "Diamond pickaxe      ✓".to_owned(),
            size: (8, 5),
            size_level: 3,
            blocks_standing: 31,
            blocks_total: 40,
            richness_level: 6,
            richness_max: 9,
            value_percent: 64,
            dial_split: "Crying 64%   Obsidian 36%".to_owned(),
            note,
        },
    }
}

/// The Inventory table and compress panel drawn in UI.md §5.3, from the frame.
///
/// The counts are fixture data; the compress panel's derived numbers (value,
/// compressible-now) are computed in the screen, not stored here. The frame is
/// drawn **mid-refusal** — Iron is selected, worth 680 but short the compressed
/// denomination an upgrade wants — which is why `hint` names the denomination.
fn sample_inventory() -> InventoryView {
    // `(material, compressed, raw)`, in the fixed display order the frame lists.
    let rows = [
        ("Stone", 12, 4_508),
        ("Coal", 3, 871),
        ("Iron", 2, 480),
        ("Gold", 0, 312),
        ("Lapis", 1, 44),
        ("Redstone", 0, 128),
        ("Emerald", 0, 17),
        ("Diamond", 0, 9),
        ("Netherrack", 2, 340),
        ("Quartz", 0, 73),
        ("Ancient Debris", 4, 60),
        ("Obsidian", 0, 21),
        ("Crying Obsidian", 0, 2),
        ("End Stone", 0, 0),
        ("Amethyst", 0, 38),
    ]
    .into_iter()
    .map(|(material, compressed, raw)| InvRow {
        material: material.to_owned(),
        compressed,
        raw,
    })
    .collect();

    let hint = [
        "Efficiency V wants",
        "6 Compressed + 50.",
        "You hold the value, not",
        "the denomination.",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    InventoryView {
        rows,
        // Iron: the row the frame highlights and the compress panel details.
        selected: 2,
        hint,
    }
}

/// The Stats panels drawn in UI.md §5.5, transcribed from the frame.
///
/// Placeholder throughout: the prestige numbers, the lifetime counters, the run
/// milestones and the history are the tick's and the save's to fill (phase 7).
/// Kept together so the three panels can be read against the one wireframe.
fn sample_stats() -> StatsView {
    // `(done, current, text, detail)` for each "This run" row.
    let milestones = [
        (true, false, "Break your first block", ""),
        (true, false, "Reach the Nether", "Lv 15"),
        (true, false, "Diamond pickaxe", ""),
        (false, true, "Reach the End", "Lv 30    23/30"),
        (false, false, "Netherite pickaxe", ""),
        (false, false, "Instamine Obsidian", ""),
        (false, false, "Max out a mine", "Stone 20x10 R9  ✓"),
        (false, false, "Reach mining level 50", "23/50"),
    ]
    .into_iter()
    .map(|(done, current, text, detail)| Milestone {
        done,
        current,
        text: text.to_owned(),
        detail: detail.to_owned(),
    })
    .collect();

    let history = [
        "20:14  Excavator!  +1 Compressed Iron",
        "20:13  Explosive — 9 blocks cleared",
        "20:13  Mine refilled",
        "20:11  Level 23 — +115 Quartz, +80 A. Debris",
        "20:09  Compress first: need 6 Compressed Iron",
        "20:04  Jackhammer — 8 blocks",
        "20:02  Welcome back — 6h away, +12 480 Iron",
        "19:58  Bought Diamond Pickaxe Efficiency IV",
        "19:51  Richness dial: Obsidian 46% → 64%",
        "19:44  Mine refilled",
        // Past the ten rows the counted frame had room for. They change nothing at
        // 80×24 — the window still starts at zero and still ends after ten — and are
        // what a taller Stats panel now has to show.
        "19:39  Nuke — 21 blocks cleared",
        "19:36  Mine refilled",
        "19:30  Level 22 — +110 Quartz, +77 A. Debris",
        "19:28  Bought Obsidian richness level 6",
        "19:22  Excavator!  +1 Compressed Obsidian",
        "19:15  Explosive — 9 blocks cleared",
        "19:11  Mine refilled",
        "19:04  Entered the Obsidian Mine",
        "18:57  Bought Ancient Debris size 8x5",
        "18:49  Level 21 — +105 Quartz, +73 A. Debris",
        "18:42  Jackhammer — 8 blocks",
        "18:35  Mine refilled",
        "18:20  Prestige II — ×1.20 on everything",
        "18:19  Reached 6 540 Amethyst",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    StatsView {
        prestige_rank: "II".to_owned(),
        multiplier: "×1.20".to_owned(),
        next_multiplier: "×1.30".to_owned(),
        prestige_cost: 6_540,
        prestige_material: "Amethyst".to_owned(),
        prestige_held: 0,
        blocks_broken: 418_297,
        playtime: "14h 22m".to_owned(),
        this_run: "3h 07m".to_owned(),
        milestones,
        history,
        history_offset: 0,
    }
}

/// The topmost drawn level at 80×24 — index 12, which is level 13.
///
/// That is the row UI.md §5.6 starts its window on, and `window(50, 22, 12, 19)` is
/// `12..31`: levels 13..=31, the counted frame exactly.
const LEVELS_OFFSET: usize = 12;

/// The **whole** Levels roadmap, `1..=LEVEL_CAP`.
///
/// It used to be the window UI.md §5.6 drew — levels 13..=31 — because the view
/// decided what fit. It no longer does, so this is the full ladder and the screen
/// windows it; [`LEVELS_OFFSET`] is what keeps 13 at the top at 80×24.
///
/// **Two sources, deliberately not merged.** The nineteen levels the wireframe
/// counted stay *verbatim* below, so the frame can still be compared row for row
/// against the document; every other level gets a generated filler line, because
/// `loot_for_level` does not exist in the core and inventing thirty-one more
/// hand-written reward strings would be inventing balance, not fixture data. `xp`
/// is `level × 100` throughout, which is the curve the counted rows already follow.
fn sample_levels() -> Vec<LevelRow> {
    let counted = counted_levels();
    (1..=LEVEL_CAP)
        .map(|level| {
            let row = counted.iter().find(|(counted, _, _)| *counted == level);
            LevelRow {
                level,
                grants: row.map_or_else(
                    || filler_grants(level),
                    |(_, grants, _)| (*grants).to_owned(),
                ),
                // The counted rows keep their transcribed XP rather than a
                // recomputed one, so a wireframe row stays verbatim to the digit
                // even if the curve is ever retuned under it.
                xp: row.map_or(level * 100, |(_, _, xp)| *xp),
            }
        })
        .collect()
}

/// A stand-in reward line for a level the wireframe never drew.
///
/// Keyed off the world the level opens into, so the materials at least name things
/// the player could plausibly be holding at that point. It is **placeholder prose**
/// and says nothing about balance — the real bundles arrive with the tick (phase 7).
fn filler_grants(level: u32) -> String {
    let (common, value) = match level {
        1..=14 => ("Stone", "Iron"),
        15..=29 => ("Netherrack", "Quartz"),
        _ => ("End Stone", "Amethyst"),
    };
    format!("+{} {common}, +{} {value}", level * 10, level * 3)
}

/// The nineteen rows UI.md §5.6 counted, as `(level, grants, xp)`.
///
/// Levels 15 and 30 grant a world and no loot, which is why their lines look
/// different from the rest.
fn counted_levels() -> Vec<(u32, &'static str, u32)> {
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
    .to_vec()
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
