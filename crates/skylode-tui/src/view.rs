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

use skylode_core::{
    block::Block,
    enchant::EnchantType,
    game::GameState,
    inventory::Inventory,
    material::{Item, Material},
    mine::{MAX_RICHNESS_LEVEL, Mine},
    mine_kind::{MineKind, MineLock},
    pickaxe::PickaxeTier,
    tunables::{BOOST_DURATION_TICKS, LEVEL_CAP, RAW_PER_COMPRESSED, TICKS_PER_SECOND},
};

use crate::{cursor::Cursors, palette::ColourMode};

/// What a readout with nothing to report prints.
///
/// The same em dash the Mine screen's empty gauges use, and for the same reason: a
/// `Fortune 0` or an `Efficiency 0` states a level the player owns, where the truth
/// is that they own no such enchant at all.
const NOTHING: &str = "—";

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
/// from `kind` in the screen. The rest is the run's, and it is **typed rather than
/// pre-formatted**: this row used to carry `detail: String`, the frame's own
/// `8 x 5   R 6` or `locked   Netherite`, because [`MineLock`] was assumed not to
/// exist yet. It does, and it answers both axes separately — so the row hands the
/// screen the facts and the screen decides the wording.
#[derive(Clone, Debug)]
pub struct MineListRow {
    /// Which mine this row is — the source of its name and world.
    pub kind: MineKind,
    /// What this mine is still waiting on, for this player.
    ///
    /// The row prints only the **tier** half. The level half belongs to the world,
    /// and the world already has a header row of its own carrying it — printing it
    /// on all three of that world's mines would say one thing four times.
    pub lock: MineLock,
    /// The grid's `(width, height)`, real or — for a mine never entered — the size
    /// it will be created at.
    pub size: (u8, u8),
    /// The richness **ceiling** bought for this mine: the `R 6` of the right column.
    pub richness_level: u32,
    /// Whether this is the mine the player is standing in — drawn `●`.
    ///
    /// Distinct from the cursor, which the screen reads off
    /// [`MinesView::selected`]: the two start together and part company on the first
    /// `↑`, and a screen that could not tell them apart would stop saying where the
    /// player is the moment they looked at anything else.
    pub current: bool,
}

/// The detail pane of the selected mine (UI.md §5.2).
///
/// **Two pre-formatted lines went away here**, and for the reason the whole `View`
/// exists: `world_line` and `gate_line` carried the frame's `Nether  Lv 15  ✓` and
/// `Diamond pickaxe  ✓` as text, because the `✓` was not derivable. It is now — the
/// requirement halves come from [`MineKind::world`] and
/// [`MineKind::gating_tier`], which the screen already asks for the mine's
/// materials, and the ticks come from [`MineLock`]. Likewise `dial_split`: the
/// screen composes it from `value_percent` and the two material names, so the
/// percentage under the bar cannot disagree with the bar.
#[derive(Clone, Debug)]
pub struct MineDetail {
    /// What this mine is still waiting on — the two `✓`/`✗` of the pane's gate rows.
    pub lock: MineLock,
    /// Grid size, as `(width, height)`.
    pub size: (u8, u8),
    /// The purchased size level.
    pub size_level: u32,
    /// Blocks still standing, or [`None`] for a mine this run has never entered.
    ///
    /// **The [`Option`] is the "never entered" case made structural**, the same
    /// device [`TargetView`] uses for a mine nobody has swung at yet. A run creates
    /// its mines lazily, so eleven of the twelve have no grid to count; a `0` would
    /// claim the player had emptied one, and the grid's own total — `width × height`
    /// — is what the screen divides by, so there is nothing else to carry.
    pub blocks_standing: Option<u32>,
    /// The purchased richness level: the **ceiling**, permanent and paid.
    pub richness_level: u32,
    /// Where the free dial currently sits, `0..=richness_level`.
    ///
    /// Carried beside the ceiling because they are two different numbers that a
    /// single `R 6` conflates, and the pane prints both: `3/6` after the slider's
    /// right arrow. The bar cannot say it on its own — it is filled by
    /// [`value_percent`](MineDetail::value_percent), a curve over the setting rather
    /// than the setting itself — and the gap between the two is exactly what a player
    /// consults before buying a seventh level they might not need.
    pub richness_setting: u32,
    /// The richness ceiling (9 today).
    pub richness_max: u32,
    /// The dial's value-cell weight, as a percent; the common weight is its
    /// complement. Drives the bar fill and the readout below it.
    pub value_percent: u32,
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
/// this is the one with a timer.
///
/// **Only ever held as an `Option`**, because
/// [`GameState::active_boost`](skylode_core::game::GameState::active_boost) is one:
/// a boost either runs or does not exist, and a `BoostView { seconds: 0 }` would be
/// a second way to spell the second case. The screen branches on the `Option` and
/// draws a dash, so "no boost" is a shape the compiler checks rather than a
/// convention about a zero.
#[derive(Clone, Debug)]
pub struct BoostView {
    /// Seconds left on the boost.
    pub seconds: u32,
    /// The multiplier it applies to mining power, e.g. `1.5`.
    pub multiplier: f64,
    /// How full the countdown gauge is, in `0.0..=1.0`.
    pub ratio: f32,
}

/// One material's holdings, as the Haul strip quotes them (UI.md §5.1).
///
/// **Both denominations, never a total**, for the reason `Inventory` keeps them
/// apart: costs are paid in the denomination they are quoted in, so a player short
/// of Compressed Iron while holding six hundred raw is short — and a single summed
/// number would hide exactly the fact the strip exists to show.
#[derive(Clone, Copy, Debug)]
pub struct HaulEntry {
    /// The material's display name.
    pub material: &'static str,
    /// Raw units held.
    pub raw: u32,
    /// Compressed units held.
    pub compressed: u32,
}

impl HaulEntry {
    /// What the two denominations come to in raw units.
    ///
    /// The same arithmetic
    /// [`Inventory::raw_value`](skylode_core::inventory::Inventory::raw_value)
    /// performs, done here because it is a **display sum** and not a rule: nothing
    /// in the game may be *paid for* with this number, which is precisely why the
    /// strip is free to show it.
    pub fn value(self) -> u32 {
        self.raw + self.compressed * RAW_PER_COMPRESSED
    }
}

/// The Haul strip: what the standing mine produces, in both denominations.
///
/// **`value` is an [`Option`], and that is the two-material test made structural.**
/// Nine of the twelve mines drop one material — their
/// [`common_material`](MineKind::common_material) and
/// [`value_material`](MineKind::value_material) are the same — and printing it
/// twice would tell the player their Iron mine produces Iron and also Iron. The
/// three that genuinely produce two (Quartz, Obsidian, End) are the three where
/// [`None`] would be wrong, and they are the same three whose richness dial is a
/// real choice. One `Option`, both facts.
#[derive(Clone, Copy, Debug)]
pub struct HaulView {
    /// The material the mine is mostly made of — its growth currency.
    pub common: HaulEntry,
    /// The material it exists to produce, when that is a *different* one.
    pub value: Option<HaulEntry>,
}

/// The cell being dug, and how far it is from breaking (UI.md §5.1).
///
/// **The pair travels together, and that is the whole reason this type exists.**
/// They used to be two fields — a `target: Option<(u8, u8)>` beside a bare
/// `break_ratio: f32` — which let a ratio of 0.61 sit next to no target at all, a
/// state the rules cannot produce and the screen had to decide what to do about.
/// [`MineGrid`](crate::widget::MineGrid) already models it this way: `.target()` is
/// simply not called when nothing is being dug, so the ratio has nowhere to be.
///
/// There is deliberately **no name field**. The block being dug is the grid cell
/// this points at, and [`Block::name`](skylode_core::block::Block::name) turns it
/// into "Iron Block" at the moment of drawing — so the Break gauge's label cannot
/// disagree with the crack the player is watching, the way a stored name could.
#[derive(Clone, Copy, Debug)]
pub struct TargetView {
    /// The grid cell under the pickaxe, as `(x, y)`.
    pub cell: (u8, u8),
    /// How far that cell is from breaking, in `0.0..=1.0`.
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
    /// The richness ceiling, from the core's
    /// [`MAX_RICHNESS_LEVEL`].
    ///
    /// A field rather than a constant read at the point of drawing, because the
    /// Mines detail pane carries the same ceiling for a mine the player is *not*
    /// standing in — so the two panes ask the same question of two different mines
    /// and both need somewhere to put the answer.
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
    /// XP the current level requires in total, or [`None`] at the level cap.
    ///
    /// The `Option` is
    /// [`Player::experience_to_next_level`](skylode_core::player::Player::experience_to_next_level)'s,
    /// carried through rather than flattened: a `0` here would divide the XP gauge
    /// by zero, which is precisely the sentinel the core refused to return.
    /// [`format::xp_progress`](crate::format::xp_progress) is where all three
    /// screens turn it into words.
    pub xp_to_next: Option<u32>,
    /// Display name of the mine the player is standing in.
    pub mine_name: String,
    /// The Pickaxe panel's figures.
    pub pickaxe: PickaxeView,
    /// The Mine panel's figures.
    pub mine_panel: MinePanelView,
    /// The Redstone boost gauge, or [`None`] when no boost is running.
    pub boost: Option<BoostView>,
    /// The Haul strip: what the standing mine produces, and how much is held.
    pub haul: HaulView,
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
    /// The cell being dug and its progress, [`None`] before the first swing.
    pub target: Option<TargetView>,
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
    /// Everything TUI phase 3 can derive from a real run; the rest is still
    /// [`sample`](View::sample)'s fixture.
    ///
    /// **This is the wire the whole `View` indirection existed for.** The Mine
    /// screen's every figure now comes from `GameState`, and nothing under
    /// `screen/` changed to make that true — which is what the module header
    /// promised while the rules were still being written.
    ///
    /// The `..Self::sample()` at the bottom is not laziness, it is the **progress
    /// marker**. Rust's *functional update syntax* — `Self { a, b, ..other }`, "these
    /// fields explicitly, the remainder taken from `other`" — makes that one line the
    /// literal list of what phases 4 to 7 still owe: the Mines list, the Inventory
    /// table, the Upgrades ladders, the Stats panels and the Levels roadmap. Each
    /// phase lifts its own fields above the `..`, and when the last one goes, the line
    /// goes with it and the compiler resumes checking that every field is accounted
    /// for. A comment would have said the same and never failed.
    ///
    /// It does mean a redraw builds the whole fixture and throws most of it away.
    /// That is why [`App`](crate::app::App) caches the result in a field rather than
    /// projecting inside its `render`: the cost is paid when the state changes, not
    /// thirty times a second.
    pub fn from_state(state: &GameState, cursors: Cursors) -> Self {
        let player = state.player();
        let pickaxe = player.get_pickaxe();
        let enchants = pickaxe.enchants();
        let mine = state.current_mine();
        let kind = mine.kind();

        Self {
            player_level: player.get_level(),
            xp: player.get_experience(),
            xp_to_next: player.experience_to_next_level(),
            mine_name: format!("{} Mine", kind.name()),
            pickaxe: PickaxeView {
                summary: pickaxe_summary(
                    pickaxe.get_tier(),
                    enchants.get_level(EnchantType::Efficiency),
                ),
                // `f64` because the panel multiplies it by the boost and prints the
                // product; the core computes power in `f32`, and the widening is
                // exact — every `f32` is a `f64`.
                power: f64::from(pickaxe.mining_power()),
                fortune: fortune_line(
                    enchants.get_level(EnchantType::Fortune),
                    pickaxe.fortune_multiplier(),
                ),
                enchants: enchant_roster(&enchants.iter().collect::<Vec<_>>()),
            },
            mine_panel: MinePanelView {
                size_level: mine.get_size_level(),
                richness_level: mine.get_richness_level(),
                richness_max: MAX_RICHNESS_LEVEL,
                value_percent: mine.value_weight_percent(),
            },
            boost: state
                .active_boost()
                .map(|boost| boost_view(boost.remaining_ticks(), boost.multiplier())),
            haul: haul_view(kind, player.get_inventory()),
            mine_kind: kind,
            // Cloned, not borrowed. A borrow would put a lifetime parameter on
            // `View` and therefore on all six screen signatures, to save copying at
            // most 200 `Option<Block>` — and the copy happens when the state
            // changes, not per frame, because `App` caches this whole struct.
            grid: mine.get_grid().to_vec(),
            target: mine.get_target().map(|cell| TargetView {
                cell,
                ratio: mine.break_ratio(),
            }),
            mines: mines_view(state, cursors),

            // Everything below is still the fixture. Phases 5-7 own these, one
            // screen at a time; see this function's own note on the `..`.
            ..Self::sample()
        }
    }

    /// The placeholder save drawn throughout UI.md §5: level 23, Diamond
    /// pickaxe, standing in the Iron Mine.
    ///
    /// Every figure is transcribed from a wireframe rather than invented, **with one
    /// deliberate exception: the grid**. `docs/UI.md` §5.1 counts a 12×7 mine, which
    /// is honest about a level-5 mine and silent about the thing worth eyeballing —
    /// whether a *maxed* one still fits the panel reserved for it. The live fixture is
    /// therefore the full 20×10, and [`sample_grid_wireframe_12x7`] is one line away
    /// when comparing against the counted frame is what is wanted.
    ///
    /// The exception has to be carried through, and that is what the three mine
    /// figures below are about: `mine_panel.size_level`, the Mines list row in
    /// [`sample_mines`] and the Size track in [`sample_upgrades`] all describe the
    /// *same* Iron Mine, on three screens the player can reach in two keystrokes.
    /// `the_three_fixtures_agree_on_the_standing_mine` is what stops them drifting
    /// apart the next time the grid is swapped.
    pub fn sample() -> Self {
        // **The one line that switches grid fixture.** Swap in
        // `sample_grid_small_5x5` or `sample_grid_wireframe_12x7` to see the same
        // screen at another mine size; the `#[expect(dead_code)]` on whichever two
        // are dormant then turns into a build error naming the one you just woke up,
        // which is the reminder to clean the attribute off it.
        let (grid, cell) = sample_grid_full_20x10();
        Self {
            player_level: 23,
            xp: 1_240,
            xp_to_next: Some(2_300),
            mine_name: "Iron Mine".to_owned(),
            pickaxe: PickaxeView {
                summary: "Diamond Pickaxe  Efficiency IV".to_owned(),
                power: 25.0,
                fortune: "Fortune III   drops ×4".to_owned(),
                enchants: "Exp II   Jck I   Exc I".to_owned(),
            },
            mine_panel: MinePanelView {
                // The Mine panel derives `Size` and `Blocks n / total` from the grid
                // itself, so those two follow the fixture. `size_level` cannot — it
                // is the *purchased* level, which the core does not yet expose — so
                // it is set to the ceiling here to stay consistent with a 20×10 mine.
                size_level: 9,
                richness_level: 0,
                richness_max: MAX_RICHNESS_LEVEL,
                value_percent: 10,
            },
            boost: Some(BoostView {
                seconds: 12,
                multiplier: 1.5,
                ratio: 0.68,
            }),
            // The Iron mine drops Iron from both its cells, so the strip has one
            // segment — the wireframe's own case. `sample_two_material_haul` below
            // is the other one, for the tests that need it.
            haul: HaulView {
                common: HaulEntry {
                    material: "Iron",
                    raw: 480,
                    compressed: 2,
                },
                value: None,
            },
            mine_kind: MineKind::Iron,
            grid,
            target: Some(TargetView { cell, ratio: 0.61 }),
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

/// Roman numerals `I`..=`XV` — exactly the range an Efficiency level can take, since
/// [`PickaxeTier::efficiency_cap`] tops out at 15 on Netherite.
const ROMAN: [&str; 15] = [
    "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV", "XV",
];

/// `level` as a Roman numeral, or `"?"` past the table.
///
/// The fallback is unreachable in play — [`ROMAN`] spans every level any cap allows —
/// but it exists so the lookup is total: this crate's lints forbid the `unwrap` that
/// would be the alternative, and a panic while drawing a frame is the worst way for a
/// front-end to report that a cap moved.
fn roman(level: u8) -> &'static str {
    // `level - 1` cannot underflow: zero is filtered here rather than by the callers,
    // each of which would otherwise repeat the guard.
    if level == 0 {
        return "?";
    }
    ROMAN.get(usize::from(level) - 1).copied().unwrap_or("?")
}

/// The Pickaxe panel's first line: `Diamond Pickaxe  Efficiency IV`.
///
/// **Takes the tier and the level, not the [`Pickaxe`](skylode_core::pickaxe::Pickaxe)**,
/// and that is a testability constraint rather than a style: `Enchants::upgrade` is
/// `pub(crate)` — deliberately, since a front-end that could call it would enchant for
/// free — so this crate cannot *build* an enchanted pickaxe, and a helper taking one
/// could only ever be exercised at Efficiency 0. The reading belongs to
/// [`View::from_state`]; the wording belongs here.
///
/// An unenchanted pickaxe drops the clause entirely rather than printing
/// `Efficiency 0`: a level of zero is the absence of the enchant, and the panel says
/// so by not mentioning it.
fn pickaxe_summary(tier: PickaxeTier, efficiency: u8) -> String {
    let name = format!("{} Pickaxe", tier.name());
    if efficiency == 0 {
        name
    } else {
        format!("{name}  Efficiency {}", roman(efficiency))
    }
}

/// The Fortune line: `Fortune III   drops ×4`, or [`NOTHING`] at level 0.
///
/// Both numbers, because they answer different questions: the level is what the
/// player bought and what the Upgrades screen prices, the multiplier is what it does
/// to a drop. The core keeps them one apart (`1 + level`), and printing only one
/// would make the panel a place to do arithmetic.
fn fortune_line(level: u8, multiplier: u32) -> String {
    if level == 0 {
        return format!("Fortune {NOTHING}");
    }
    format!("Fortune {}   drops ×{multiplier}", roman(level))
}

/// The special-enchant roster: `Exp II   Jck I   Exc I`, or [`NOTHING`] when bare.
///
/// **Only the five specials, and only the non-zero ones** (UI.md §5.1). Efficiency
/// and Fortune have lines of their own above, so repeating them here would spend a
/// 36-column panel saying the same thing twice; a special at level 0 is one the
/// player has not bought, and listing it would fill the line with absences.
///
/// The abbreviations live in the front-end and not beside
/// [`EnchantType::name`](skylode_core::enchant::EnchantType::name), because they
/// exist for *this panel's width* and nothing else — the Upgrades screen has room
/// for `Jackhammer` and prints it in full. A core that shipped `Jck` would be
/// shipping one screen's layout to every caller.
///
/// The order is [`Enchants::iter`](skylode_core::enchant::Enchants::iter)'s, which
/// is the enum's own declaration order — it iterates a `BTreeMap` — so the roster
/// does not reshuffle itself as levels are bought.
fn enchant_roster(levels: &[(EnchantType, u8)]) -> String {
    let short = |kind: EnchantType| match kind {
        EnchantType::Explosive => Some("Exp"),
        EnchantType::Jackhammer => Some("Jck"),
        EnchantType::Nuke => Some("Nuke"),
        EnchantType::Excavator => Some("Exc"),
        EnchantType::Haste => Some("Hst"),
        // The two with their own lines: not abbreviated because not listed.
        EnchantType::Efficiency | EnchantType::Fortune => None,
    };
    let roster: Vec<String> = levels
        .iter()
        .filter(|(_, level)| *level > 0)
        .filter_map(|(kind, level)| short(*kind).map(|tag| format!("{tag} {}", roman(*level))))
        .collect();
    if roster.is_empty() {
        NOTHING.to_owned()
    } else {
        roster.join("   ")
    }
}

/// The Boost gauge's figures, from a running boost's two numbers.
///
/// **Takes the numbers and not the [`Boost`](skylode_core::boost::Boost)**, for
/// [`pickaxe_summary`]'s reason
/// exactly: `Boost::new` is `pub(crate)` — minting the game's strongest multiplier
/// is not a front-end's business — and a helper taking one could not be tested from
/// this crate at all, since no legal sequence of public calls reaches a boost from a
/// level-1 run. `from_state` unwraps the boost; this formats it.
///
/// `div_ceil` on the seconds, because this is a **countdown**: one tick left is a
/// boost the player still has, and flooring would show `0s` for a twentieth of a
/// second before the gauge vanished. The ratio is against
/// [`BOOST_DURATION_TICKS`] and can exceed 1 — firing a second charge *extends* the
/// timer rather than refreshing it — which the gauge clamps, so an over-long boost
/// reads as a full bar instead of panicking `LineGauge`.
fn boost_view(remaining: u32, multiplier: f32) -> BoostView {
    // Widened, divided, then narrowed. `TICKS_PER_SECOND` is a `u64` because the
    // offline accrual multiplies by it across days, while a tick counter is a `u32`;
    // dividing in the wider type and converting back keeps the whole thing total,
    // where a cast either way would be the compiler taking the programmer's word for
    // it. The `unwrap_or` is unreachable — a `u32` of ticks over twenty is always a
    // `u32` of seconds — and is here because this crate's lints leave no `unwrap`.
    let seconds = u64::from(remaining).div_ceil(TICKS_PER_SECOND);
    BoostView {
        seconds: u32::try_from(seconds).unwrap_or(u32::MAX),
        multiplier: f64::from(multiplier),
        ratio: remaining as f32 / BOOST_DURATION_TICKS as f32,
    }
}

/// The Haul strip's holdings for the mine the player is standing in.
///
/// The two-material test is `common != value`, asked of the core rather than kept as
/// a list here — see [`HaulView`] for why the answer is an [`Option`] and not a
/// second entry.
fn haul_view(kind: MineKind, inventory: &Inventory) -> HaulView {
    let entry = |material: Material| HaulEntry {
        material: material.name(),
        raw: inventory.count(Item::Raw(material)),
        compressed: inventory.count(Item::Compressed(material)),
    };
    let common = kind.common_material();
    let value = kind.value_material();
    HaulView {
        common: entry(common),
        value: (value != common).then(|| entry(value)),
    }
}

/// The Mines screen's whole read model, projected from the run and the cursor.
///
/// **Walks [`MineKind::ALL`] rather than the run's mines**, and that is the shape of
/// the screen's job: the twelve always exist as *kinds*, while a run only holds a
/// [`Mine`] for the ones it has opened. `state.mine(kind)` is therefore an
/// [`Option`] on every row, and the [`None`] arm is not an error case — it is the
/// mine the player has never walked into, drawn from what a fresh one would be:
/// [`Mine::size_for_level(0)`](Mine::size_for_level) and a ceiling of 0.
fn mines_view(state: &GameState, cursors: Cursors) -> MinesView {
    let player = state.player();
    let standing = state.current_mine().kind();

    let rows = MineKind::ALL
        .into_iter()
        .map(|kind| MineListRow {
            kind,
            lock: player.mine_lock(kind),
            size: state
                .mine(kind)
                .map_or_else(|| Mine::size_for_level(0), Mine::get_size),
            richness_level: state.mine(kind).map_or(0, Mine::get_richness_level),
            current: kind == standing,
        })
        .collect();

    let selected = cursors.mine;
    let mine = state.mine(selected);
    let detail = MineDetail {
        lock: player.mine_lock(selected),
        size: mine.map_or_else(|| Mine::size_for_level(0), Mine::get_size),
        size_level: mine.map_or(0, Mine::get_size_level),
        // `u32` from a `usize` count: a grid is 200 cells at its very largest, so the
        // conversion is exact — but it is still fallible in the type system, and this
        // crate's lints leave no `unwrap`, so the saturating form is what says
        // "narrower is fine here" without a panic to explain later.
        blocks_standing: mine.map(|mine| u32::try_from(mine.remaining_count()).unwrap_or(u32::MAX)),
        richness_level: mine.map_or(0, Mine::get_richness_level),
        richness_setting: mine.map_or(0, Mine::get_richness_setting),
        richness_max: MAX_RICHNESS_LEVEL,
        // A mine that does not exist yet would be created at dial 0, and
        // `value_weight_percent` is a pure function of the dial — so the fallback is
        // the weight of a fresh grid, not a placeholder.
        value_percent: mine.map_or_else(
            || Mine::value_weight_percent_for(0),
            Mine::value_weight_percent,
        ),
        note: mine_note(selected),
    };

    MinesView {
        rows,
        selected,
        detail,
    }
}

/// The prose under a mine's dial: what a player should make of *this* dial.
///
/// **Front-end text, not a rule**, which is why it lives here and not beside
/// [`MineKind`]. The pane draws the same slider on all twelve mines, so what differs
/// between them is not the control but the stakes, and that is exactly what a
/// sentence is for. Three cases:
///
/// - **Obsidian** is the one dial in the game a player can set *too high*: the
///   enhancement past Netherite consumes Obsidian and Crying Obsidian both, so its
///   dial has an **optimum** rather than a maximum.
/// - **The nine same-material mines** are the opposite — the value cell is the dense
///   block, worth nine of the ore beside it, so there is no trade at all and the
///   only reason not to max the dial is not having bought the ceiling yet.
/// - **Quartz and the End** get nothing, because "more of the rare one, less of the
///   common one" is what the split under the bar already says in numbers.
fn mine_note(kind: MineKind) -> Vec<String> {
    let lines: &[&str] = match kind {
        MineKind::Obsidian => &[
            "The enhancement past Netherite eats",
            "both of them, so this dial has an",
            "optimum, not a maximum.",
        ],
        // Asked of the materials, not listed by hand: `common != value` is the
        // core's own two-material test, so a thirteenth mine is classified by the
        // rules rather than by whoever remembers to extend a list here.
        kind if kind.common_material() == kind.value_material() => &[
            "Pure gain here — the value cell is",
            "nine of the same ore, so this dial",
            "only ever wants to go up.",
        ],
        _ => &[],
    };
    lines.iter().map(|line| (*line).to_owned()).collect()
}

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
    // The tier names come from the core now — a private table here was a second copy
    // of `PickaxeTier::name`, and the rung labels below are the reason that table
    // returns the bare material: this list writes "Pickaxe" once per tier and never
    // on the thirty Efficiency rungs between.
    let mut labels = Vec::new();
    let mut tier = Some(PickaxeTier::Wooden);
    while let Some(current) = tier {
        labels.push(format!("{} Pickaxe", current.name()));
        for level in 1..=current.efficiency_cap() {
            labels.push(format!("{} Eff {}", current.name(), roman(level)));
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

/// The three Upgrades sub-tabs drawn in UI.md §5.4, transcribed from the frames.
///
/// Pickaxe is active. Every row's marks and every detail pane are fixture data:
/// the reachability marks are a phase-6 core read, and the dip numbers, costs and
/// affordability the panes quote are phases 5–6 too. The Pickaxe marks are laid
/// out to honour the ladder invariant — the `✓` region is a contiguous prefix from
/// the current rung — so the fixture is a legal ladder, not an arbitrary one.
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
            r("Iron           Size     maxed", "—", false, false),
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
/// richness dial — and the player is standing in the Iron mine, which is what puts
/// the `▸` and the `●` on two different rows. The whole fixture describes the save
/// §5 is drawn against: **Lv 23, Diamond pickaxe**, which is the level and tier
/// every [`MineLock`] below is built from, so the ticks in the frame are the ones
/// the rules would give.
///
/// The sizes and richness levels stay fixture data: the run they describe has
/// bought upgrades no fresh run has, and the frame tests are meant to be
/// independent of what the economy currently charges.
fn sample_mines() -> MinesView {
    /// The save §5 is drawn against, and the only two numbers a lock depends on.
    const LEVEL: u32 = 23;
    const TIER: PickaxeTier = PickaxeTier::Diamond;

    // `(kind, size, richness ceiling)` in display order. Every lock is *derived*
    // from the pair above rather than written down, so the fixture cannot claim a
    // mine is open that the rules would shut — which is exactly what the End mine
    // is here to prove: at Lv 23 with a Diamond pickaxe it is closed on both axes.
    let rows = [
        (MineKind::Stone, (20, 10), 9),
        (MineKind::Coal, (18, 9), 7),
        (MineKind::Iron, (20, 10), 0),
        (MineKind::Gold, (10, 6), 2),
        (MineKind::Lapis, (8, 5), 1),
        (MineKind::Redstone, (6, 4), 0),
        (MineKind::Emerald, (6, 4), 0),
        (MineKind::Diamond, (8, 5), 1),
        (MineKind::Quartz, (8, 5), 3),
        (MineKind::AncientDebris, (6, 4), 0),
        (MineKind::Obsidian, (8, 5), 6),
        (MineKind::Amethyst, (6, 4), 0),
    ]
    .into_iter()
    .map(|(kind, size, richness_level)| MineListRow {
        kind,
        lock: kind.lock(LEVEL, TIER),
        size,
        richness_level,
        // The Iron mine, matching `View::sample`'s `mine_kind` and its grid.
        current: kind == MineKind::Iron,
    })
    .collect();

    MinesView {
        rows,
        selected: MineKind::Obsidian,
        detail: MineDetail {
            lock: MineKind::Obsidian.lock(LEVEL, TIER),
            size: (8, 5),
            size_level: 3,
            blocks_standing: Some(31),
            richness_level: 6,
            richness_setting: 6,
            richness_max: MAX_RICHNESS_LEVEL,
            value_percent: Mine::value_weight_percent_for(6),
            note: mine_note(MineKind::Obsidian),
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

/// One cell of a grid fixture. `O` an ore cell, `B` an iron block, `X` a hole.
///
/// Spelled as one letter each so the fixtures below read as *pictures* of the
/// screen rather than as lists of `Some(Block::IronOre)`.
const O: Option<Block> = Some(Block::IronOre);
/// The value block — the stippled cell, and the only legal target (see below).
const B: Option<Block> = Some(Block::IronBlock);
/// A broken cell: the absence of a block, drawn as the terminal's own background.
const X: Option<Block> = None;

/// A grid fixture and the cell being dug in it.
///
/// **Returned together, and that is the point.** They used to be two fields filled
/// in side by side, which let a target name a cell outside the grid it belonged to —
/// a state `the_sample_target_names_a_standing_cell` had to check for by hand. Now
/// swapping fixtures moves both at once, so the pair cannot come apart. The target
/// must land on a `B`: the Break gauge prints `target_name` ("Iron Block"), and a
/// crack drawn on an ore cell would make the label contradict the picture.
type GridFixture = (Vec<Vec<Option<Block>>>, (u8, u8));

/// A **full-size** 20×10 mine — the reserve at capacity.
///
/// This is the live fixture, and it is not the one the wireframes drew. UI.md §5.1
/// counted a 12×7 mine, which is honest about what a level-5 mine looks like and
/// dishonest about what the *panel* has to hold: the grid area is sized for the
/// largest mine in the game, 20 cells by 10 (UI.md §1's arithmetic), and a fixture
/// that never fills it leaves the one thing worth eyeballing — does a maxed mine
/// still fit — untested by eye. [`sample_grid_wireframe_12x7`] is one line away when
/// the comparison against the document is what is wanted.
fn sample_grid_full_20x10() -> GridFixture {
    let grid = vec![
        vec![O, O, O, B, O, O, X, O, O, O, O, O, O, B, O, O, O, O, X, O],
        vec![O, X, O, O, O, O, O, B, O, O, X, O, O, O, O, B, O, O, O, O],
        vec![O, O, O, O, O, O, O, O, O, O, O, O, B, O, O, O, O, O, O, O],
        vec![O, O, X, O, B, O, O, O, O, X, O, O, O, O, O, O, B, O, O, O],
        vec![O, O, O, O, O, O, B, O, O, O, O, O, O, X, O, O, O, O, O, O],
        vec![X, O, O, B, O, O, O, O, O, O, O, O, O, O, B, O, O, O, O, X],
        vec![O, O, O, O, B, O, X, O, O, O, O, O, O, O, O, O, O, B, O, O],
        vec![O, O, B, O, O, O, O, O, X, O, O, B, O, O, O, O, O, O, O, O],
        vec![O, X, O, O, O, O, B, O, O, O, O, O, O, O, X, O, O, B, O, O],
        vec![O, O, O, O, O, B, O, O, O, X, O, O, B, O, O, O, O, O, O, O],
    ];
    (grid, (7, 1))
}

/// A **small** 5×5 mine — the worst case for centring in the reserve.
///
/// Twenty-five cells in an area sized for two hundred, so the margin around it is
/// larger than the mine: this is what "a mine smaller than 20x10 does not grow its
/// panel; it leaves the reserved area partly empty" (UI.md §1) looks like taken to
/// its limit. Swap it in to check that the grid still lands centred and that the
/// panels beside it do not shift.
// `cfg_attr(not(test), …)` rather than a bare `expect`: the tests *do* call this, so
// under `cfg(test)` it is not dead at all and an unconditional expectation would go
// unfulfilled — which `-D warnings` turns into a build error. Dead for the binary,
// alive for the tests, is precisely the state a dormant fixture should be in.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the alternate grid fixture, swapped into View::sample by hand"
    )
)]
fn sample_grid_small_5x5() -> GridFixture {
    let grid = vec![
        vec![O, O, B, O, O],
        vec![O, X, B, O, O],
        vec![O, O, O, O, X],
        vec![B, O, O, O, O],
        vec![O, O, X, O, B],
    ];
    (grid, (2, 1))
}

/// The 12×7 grid drawn in UI.md §5.1, cell for cell.
///
/// Transcribed rather than generated: `Mine::new` would need a seed, and the figure
/// the frame shows — five value cells, seven holes, a target three cells into row
/// two — is what the counted wireframe asserts. A generated grid would make the
/// screen impossible to compare against the document it implements, which is exactly
/// why this one is kept rather than replaced.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the wireframe grid fixture, swapped into View::sample by hand"
    )
)]
fn sample_grid_wireframe_12x7() -> GridFixture {
    let grid = vec![
        vec![O, O, O, B, O, O, X, O, O, O, O, O],
        vec![O, X, O, O, O, O, O, B, O, O, X, O],
        vec![O, O, O, O, O, O, O, O, O, O, O, O],
        vec![O, O, X, O, B, O, O, O, O, X, O, O],
        vec![O, O, O, O, O, O, B, O, O, O, O, O],
        vec![X, O, O, B, O, O, O, O, O, O, O, X],
        vec![O, O, O, O, B, O, X, O, O, O, O, O],
    ];
    (grid, (7, 1))
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use skylode_core::tunables::BOOST_MULTIPLIER;

    use super::*;

    /// A fresh run to project, on a fixed seed.
    ///
    /// The seed is arbitrary and *fixed*: `GameState::new` draws the opening mine's
    /// whole grid from it, so a clock-derived one would give every run of the suite
    /// a different picture. `UNIX_EPOCH` is `now` because it is the reference the
    /// offline accrual counts from, and a test that read the clock would be
    /// measuring how long ago it was written.
    fn fresh_run() -> GameState {
        GameState::new(0x5B1_0DE, UNIX_EPOCH)
    }

    /// A run projected with the cursors a session opens on.
    ///
    /// Most assertions here are about the *run's* half of the projection, where the
    /// cursor is immaterial; spelling it out at each call site would put a parameter
    /// nobody reads in front of the thing being tested. The tests that are about the
    /// cursor build one on purpose.
    fn projected(state: &GameState) -> View {
        View::from_state(state, Cursors::new(state.current_mine().kind()))
    }

    #[test]
    fn a_fresh_run_projects_to_a_bare_level_one_session() {
        // Everything a run has before anything has happened to it, asserted together
        // because *this* is the frame `cargo run` opens on until the phase-7 tick
        // exists — the five states the level-23 fixture never had.
        let view = projected(&fresh_run());

        assert_eq!(view.player_level, 1);
        assert_eq!(view.xp, 0);
        assert_eq!(view.xp_to_next, Some(100));
        assert_eq!(view.mine_name, "Stone Mine");
        assert_eq!(view.mine_kind, MineKind::Stone);
        assert_eq!(view.pickaxe.summary, "Wooden Pickaxe");
        // A bare Wooden pickaxe is its tier and nothing else — the floor the whole
        // game's pacing is measured from (`Pickaxe::mining_power`).
        assert!((view.pickaxe.power - 2.0).abs() < f64::EPSILON);
        assert_eq!(view.pickaxe.fortune, "Fortune —");
        assert_eq!(view.pickaxe.enchants, "—");
        assert!(
            view.boost.is_none(),
            "a fresh run cannot have fired a boost"
        );
        assert!(
            view.target.is_none(),
            "nothing is dug before the first swing"
        );
        assert_eq!(view.haul.common.raw, 0);
        assert!(
            view.haul.value.is_none(),
            "the Stone mine drops one material"
        );
    }

    #[test]
    fn the_projected_grid_is_the_standing_mines_own() {
        // The grid is *copied*, not invented, and it is the one the run is standing
        // in. Compared cell for cell rather than by size, because a projection that
        // built a fresh grid of the right dimensions would pass any shape check and
        // still be showing the player a mine that is not theirs.
        let state = fresh_run();
        let view = projected(&state);
        assert_eq!(view.grid, state.current_mine().get_grid());

        // And it is genuinely mixed — `draw_cell` weights each cell by the richness
        // dial — so the palette has both of the mine's blocks to colour. `Block` is
        // `PartialEq` but not `Ord`, so the distinct count is a linear scan rather
        // than a set; twenty-four possible values makes that free.
        let mut distinct: Vec<Block> = Vec::new();
        for cell in view.grid.iter().flatten().flatten() {
            if !distinct.contains(cell) {
                distinct.push(*cell);
            }
        }
        assert!(
            distinct.len() >= 2,
            "the grid came out uniform: {distinct:?}"
        );
    }

    #[test]
    fn the_mine_panels_figures_come_from_the_mine() {
        let state = fresh_run();
        let view = projected(&state);
        let mine = state.current_mine();

        assert_eq!(view.mine_panel.size_level, mine.get_size_level());
        assert_eq!(view.mine_panel.richness_level, mine.get_richness_level());
        assert_eq!(view.mine_panel.value_percent, mine.value_weight_percent());
        // The ceiling is the core's, not a `9` this crate remembers.
        assert_eq!(view.mine_panel.richness_max, MAX_RICHNESS_LEVEL);
    }

    #[test]
    fn an_unenchanted_pickaxe_says_so_by_omission() {
        // Level 0 is the *absence* of an enchant, so the panel drops the clause
        // rather than printing `Efficiency 0` — which would name a level the player
        // does not own, on a line four rows tall that has no room for absences.
        assert_eq!(pickaxe_summary(PickaxeTier::Wooden, 0), "Wooden Pickaxe");
        assert_eq!(fortune_line(0, 1), "Fortune —");
        assert_eq!(enchant_roster(&[]), "—");
        assert_eq!(
            enchant_roster(&[(EnchantType::Explosive, 0), (EnchantType::Nuke, 0)]),
            "—",
            "a track at zero is one the player has not bought"
        );
    }

    #[test]
    fn an_enchanted_pickaxe_reads_as_the_wireframe_drew_it() {
        // The counted frame's own pickaxe: `Diamond Pickaxe  Efficiency IV`,
        // `Fortune III   drops ×4`, `Exp II   Jck I   Exc I`.
        assert_eq!(
            pickaxe_summary(PickaxeTier::Diamond, 4),
            "Diamond Pickaxe  Efficiency IV"
        );
        assert_eq!(fortune_line(3, 4), "Fortune III   drops ×4");
        assert_eq!(
            enchant_roster(&[
                (EnchantType::Explosive, 2),
                (EnchantType::Jackhammer, 1),
                (EnchantType::Excavator, 1),
            ]),
            "Exp II   Jck I   Exc I"
        );
    }

    #[test]
    fn the_roster_lists_the_five_specials_and_only_them() {
        // Walks the whole enum, because the split is the point: the five specials
        // are abbreviated and listed, and Efficiency and Fortune are not — they
        // ride in the summary and on the Fortune line, and repeating either would
        // spend a 36-column panel saying it twice.
        //
        // Handing every enchant a level at once is what makes the negative half
        // testable at all: a roster built from only the specials would pass on a
        // table that abbreviated all seven.
        let all = [
            (EnchantType::Efficiency, 4),
            (EnchantType::Fortune, 3),
            (EnchantType::Explosive, 2),
            (EnchantType::Jackhammer, 1),
            (EnchantType::Nuke, 3),
            (EnchantType::Excavator, 1),
            (EnchantType::Haste, 2),
        ];
        // Bound rather than called inline below, because `split` borrows *from* the
        // `String` it is given: called on a temporary, the roster would be dropped at
        // the end of that statement and `tags` would be pointing into freed memory —
        // which the borrow checker refuses rather than letting through.
        let roster = enchant_roster(&all);
        assert_eq!(roster, "Exp II   Jck I   Nuke III   Exc I   Hst II");

        // And the abbreviations are distinct, or two enchants would read as one.
        let tags: Vec<&str> = roster.split("   ").collect();
        let mut unique = tags.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            tags.len(),
            "two specials share a tag: {tags:?}"
        );
    }

    #[test]
    fn a_roman_numeral_is_total_over_every_level_a_cap_allows() {
        // `ROMAN` spans 1..=15, which is Netherite's Efficiency cap — the highest
        // any enchant can reach. The two ends are the interesting ones, and `0` is
        // the guard that keeps `level - 1` from underflowing rather than a level the
        // game ever asks about.
        assert_eq!(roman(1), "I");
        assert_eq!(roman(15), "XV");
        assert_eq!(roman(0), "?");
        assert_eq!(roman(16), "?", "past the table, not a panic");
    }

    #[test]
    fn a_boost_rounds_its_countdown_up_and_measures_against_the_full_duration() {
        // **`div_ceil`, not `/`.** One tick left is a boost the player still holds,
        // and flooring would print `0s` for a twentieth of a second — the gauge
        // announcing an expiry that has not happened.
        let one_tick = boost_view(1, BOOST_MULTIPLIER);
        assert_eq!(one_tick.seconds, 1);

        let full = boost_view(BOOST_DURATION_TICKS, BOOST_MULTIPLIER);
        assert_eq!(
            u64::from(full.seconds),
            u64::from(BOOST_DURATION_TICKS) / TICKS_PER_SECOND
        );
        assert!((full.ratio - 1.0).abs() < f32::EPSILON);

        // Firing a second charge *extends* rather than refreshes, so the ratio can
        // exceed one. It is left that way here and clamped at the gauge, which is
        // the one place that must never be handed an out-of-range value.
        let extended = boost_view(2 * BOOST_DURATION_TICKS, BOOST_MULTIPLIER);
        assert!(extended.ratio > 1.0);
    }

    #[test]
    fn the_haul_carries_one_entry_or_two_according_to_the_mine() {
        // The test is `common != value`, asked of the core. Walked over all twelve
        // so the count is the rules' answer and not a list remembered here: exactly
        // three mines produce two materials, and they are the three whose richness
        // dial is a real choice.
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Quartz), 73);
        inventory.add(Item::Compressed(Material::Netherrack), 2);

        let two = haul_view(MineKind::Quartz, &inventory);
        assert_eq!(two.common.material, "Netherrack");
        assert_eq!(two.common.compressed, 2);
        assert_eq!(two.value.map(|entry| entry.material), Some("Quartz"));
        assert_eq!(two.value.map(|entry| entry.raw), Some(73));

        let one = haul_view(MineKind::Iron, &inventory);
        assert_eq!(one.common.material, "Iron");
        assert!(one.value.is_none());

        let two_material = ALL_MINE_KINDS
            .iter()
            .filter(|kind| haul_view(**kind, &inventory).value.is_some())
            .count();
        assert_eq!(
            two_material, 3,
            "the two-material mines are Quartz, Obsidian and End"
        );
    }

    /// Every [`MineKind`], for the walks that must cover all twelve.
    ///
    /// Test-only and spelled out, for the reason `block`'s `ALL_BLOCKS` is: an enum
    /// cannot enumerate itself, and the core keeps its own list `pub(crate)`.
    const ALL_MINE_KINDS: [MineKind; 12] = [
        MineKind::Stone,
        MineKind::Coal,
        MineKind::Iron,
        MineKind::Gold,
        MineKind::Lapis,
        MineKind::Redstone,
        MineKind::Emerald,
        MineKind::Diamond,
        MineKind::Quartz,
        MineKind::AncientDebris,
        MineKind::Obsidian,
        MineKind::Amethyst,
    ];

    /// Every grid fixture, live or dormant, so the assertions below hold for
    /// whichever one `View::sample` currently names.
    ///
    /// The dormant ones are `#[expect(dead_code)]` for the *renderer*, not for the
    /// tests — a fixture nobody ever compiles is one that has silently rotted by the
    /// time it is wanted, which is the whole failure mode commented-out code has.
    fn every_grid_fixture() -> Vec<GridFixture> {
        vec![
            sample_grid_full_20x10(),
            sample_grid_small_5x5(),
            sample_grid_wireframe_12x7(),
        ]
    }

    #[test]
    fn every_grid_fixture_is_rectangular_and_fits_the_reserve() {
        // 20×10 is the largest mine in the game and the size the Mine screen's panel
        // is built around, so a fixture past it would draw outside the box it was
        // handed — clipped by `MineGrid`, but wrong.
        for (grid, _) in every_grid_fixture() {
            let columns = grid.first().map_or(0, Vec::len);
            assert!(grid.len() <= 10, "{} rows is past the reserve", grid.len());
            assert!(columns <= 20, "{columns} columns is past the reserve");
            for row in &grid {
                assert_eq!(row.len(), columns, "the grid is not rectangular");
            }
        }
    }

    #[test]
    fn the_live_fixture_fills_the_reserve() {
        // The live one is deliberately the *full* 20×10: see
        // `sample_grid_full_20x10`'s own note for why the wireframe's 12×7 is no
        // longer what the screen is developed against.
        let view = View::sample();
        assert_eq!(view.grid.len(), 10);
        assert_eq!(view.grid.first().map_or(0, Vec::len), 20);
    }

    /// Every mine gets the same slider, so the note is what tells them apart.
    ///
    /// Three classes, and the middle one is the reason this is a `match` on the two
    /// materials rather than a list: a mine is "pure gain" because its two cells drop
    /// the same material, which is the core's own two-material test, so a thirteenth
    /// mine would be classified by the rules instead of by whoever remembered to edit
    /// a list. Walking `MineKind::ALL` is what proves no mine falls between the arms.
    #[test]
    fn the_dials_note_says_what_is_at_stake_on_this_particular_mine() {
        for kind in MineKind::ALL {
            let note = mine_note(kind).join(" ");
            let same_material = kind.common_material() == kind.value_material();
            match kind {
                // The one dial a player can set *too high*.
                MineKind::Obsidian => assert!(note.contains("optimum"), "{kind:?}: {note:?}"),
                // No trade at all: the value cell is nine of the ore beside it.
                _ if same_material => assert!(note.contains("Pure gain"), "{kind:?}: {note:?}"),
                // Quartz and the End: the split under the bar already says it in
                // numbers, and a sentence repeating it would be filler.
                _ => assert!(note.is_empty(), "{kind:?} said {note:?}"),
            }
        }
    }

    #[test]
    fn the_three_fixtures_agree_on_the_standing_mine() {
        // The Mine panel, the Mines list and the Upgrades Size track all describe the
        // *same* mine, on three screens two keystrokes apart — and they are three
        // independent fixtures, so nothing but this test holds them together. It is
        // written because they came apart: growing the grid to 20×10 left the list
        // still quoting `12 x 7` and Upgrades still offering a `14x8` step for a mine
        // already at its ceiling, which is three answers to one question.
        let view = View::sample();
        let columns = view.grid.first().map_or(0, Vec::len);
        let rows = view.grid.len();
        let size = format!("{columns} x {rows}");

        // The Mines list, on the row for the mine the player is standing in — found
        // by its own `current` flag, which is the third statement of "this is where
        // the player is" and therefore the third that can drift.
        // Collected rather than `find`-ed, so the count is asserted too: two rows
        // claiming to be the standing mine is as wrong as none, and it is the failure
        // a `find` would silently pick a winner for.
        let standing: Vec<&MineListRow> =
            view.mines.rows.iter().filter(|row| row.current).collect();
        assert_eq!(standing.len(), 1, "exactly one row is the standing mine");
        for listed in standing {
            assert_eq!(listed.kind, view.mine_kind);
            assert_eq!(
                (usize::from(listed.size.0), usize::from(listed.size.1)),
                (columns, rows),
                "the Mines list sizes {} differently from the grid, which is {size}",
                listed.kind.name()
            );
            assert_eq!(
                listed.richness_level, view.mine_panel.richness_level,
                "the Mines list and the Mine panel disagree on the richness ceiling"
            );
        }

        // Upgrades › Mines, on the Size track for that same mine. The grid is the
        // largest mine the game has, so there is no step left to sell — and the row
        // has to say so rather than quote a next size. `maxed` carries `—`, the one
        // glyph in that column `theme::MARKS` deliberately does not own.
        assert_eq!(
            (columns, rows),
            (20, 10),
            "the live grid is no longer the largest mine, so the Size track below \
             should quote a next step instead of `maxed`"
        );
        let prefix = view.mine_kind.name();
        let track = view
            .upgrades
            .mines
            .rows
            .iter()
            .find(|row| row.text.starts_with(prefix) && row.text.contains("Size"));
        assert_eq!(
            track.map(|row| (row.text.contains("maxed"), row.mark.as_str())),
            Some((true, "—")),
            "the Size track still offers {prefix} a step it is already past"
        );
    }

    #[test]
    fn every_fixtures_target_names_a_standing_value_block() {
        // A target pointing at a hole would draw a crack on the terminal's own
        // background, which is a state the rules cannot produce; one pointing at an
        // ore cell would contradict the Break gauge's "Iron Block" label. Checked on
        // all three, because swapping fixtures is meant to be a one-line change and
        // a fixture whose target had drifted would be a one-line bug.
        for (grid, (x, y)) in every_grid_fixture() {
            let cell = grid
                .get(usize::from(y))
                .and_then(|row| row.get(usize::from(x)))
                .copied();

            // The nesting is the assertion: the outer `Some` means the target is
            // inside the grid, the inner one that a block stands there. `None` and
            // `Some(None)` are the two ways this can be wrong.
            assert_eq!(cell, Some(Some(Block::IronBlock)), "target ({x}, {y})");
        }
    }

    #[test]
    fn the_pickaxe_ladder_is_the_whole_roadmap() {
        // 5 × (a tier + Efficiency I..V) + Netherite + its fifteen — the count is
        // the core's `efficiency_cap` talking, not a number written down here. If
        // this moves, `PICKAXE_OFFSET` and the counted frame moved with it.
        let ladder = pickaxe_ladder();
        assert_eq!(ladder.len(), 46);
        assert_eq!(
            ladder.first().map(|row| row.text.as_str()),
            Some("Wooden Pickaxe")
        );
        assert_eq!(
            ladder.last().map(|row| row.text.as_str()),
            Some("Netherite Eff XV")
        );
        assert_eq!(
            ladder.get(PICKAXE_OFFSET).map(|row| row.text.as_str()),
            Some("Diamond Eff III"),
            "the counted window no longer starts where UI-EN.md §5.5 drew it"
        );
    }

    #[test]
    fn the_levels_roadmap_is_the_whole_ladder_and_keeps_its_counted_rows() {
        // The full 1..=LEVEL_CAP, with the wireframe's own rows still verbatim
        // inside it — that pairing is the reason `counted_levels` exists at all.
        let levels = sample_levels();
        assert_eq!(levels.len(), LEVEL_CAP as usize);
        let level_23 = levels.iter().find(|row| row.level == 23);
        assert_eq!(
            level_23.map(|row| row.grants.as_str()),
            Some("+115 Quartz, +80 A. Debris, +34 Obsidian")
        );
        assert_eq!(level_23.map(|row| row.xp), Some(2_300));
    }
}
