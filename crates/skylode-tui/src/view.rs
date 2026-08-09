//! The read model the screens render from.
//!
//! Screens never reach into game state directly — they render a **flat snapshot**.
//! [`View::from_state`] builds the whole of it from a `GameState`, and **nothing under
//! `screen/` had to change** as each phase wired another panel — which is the whole
//! reason UI work could start before the rules existed.
//!
//! **The fixture that stood in for the run while that was happening is now
//! `#[cfg(test)]`.** `from_state` used to end in `..Self::sample()`, Rust's functional
//! update syntax filling the unwired fields from a hand-transcribed wireframe; that one
//! line was the literal list of what the phase still owed, and it went when the Stats
//! panels landed. The compiler is exhaustive over this struct again, so a field added
//! here breaks `from_state` until someone decides where it comes from.
//!
//! Keep it plain data: no methods that decide anything, no `Option`s standing in
//! for rules. A computation that belongs to the game belongs to the core.

use std::{collections::BTreeMap, rc::Rc, time::Instant};

use skylode_core::{
    block::Block,
    economy::{self, Affordability, Cost, CostLine},
    enchant::EnchantType,
    game::GameState,
    inventory::Inventory,
    material::{Item, Material},
    mine::{MAX_RICHNESS_LEVEL, Mine},
    mine_kind::{MineKind, MineLock},
    pickaxe::{Pickaxe, PickaxeTier},
    player::Player,
    prestige::{self, PrestigeLock},
    reward::{self, LevelReward},
    tunables::{
        BOOST_DURATION_TICKS, BOOST_MULTIPLIER, HASTE_PER_LEVEL, LEVEL_CAP, RAW_PER_COMPRESSED,
        TICKS_PER_SECOND,
    },
    upgrade,
    world::World,
};

use crate::{
    announce,
    config::Config,
    cursor::{self, Cursors, MineTrack, UpgradeTab},
    flash::{FlashStage, Flashes},
    format::{MAXED, boost_seconds, duration_hm, grouped, roman, rung_label, shown_rung},
    palette::ColourMode,
    toast::Toasts,
};

/// What a readout with nothing to report prints.
///
/// The same em dash the Mine screen's empty gauges use, and for the same reason: a
/// `Fortune 0` or an `Efficiency 0` states a level the player owns, where the truth
/// is that they own no such enchant at all.
pub const NOTHING: &str = "—";

/// One row of the Levels roadmap (UI.md §5.6).
///
/// `xp` is the per-level requirement counted from zero (`level × 100` on today's
/// curve), which is what the status bar's `1 240 / 2 300` also counts against.
#[derive(Clone, Debug)]
pub struct LevelRow {
    /// The level this row is for.
    pub level: u32,
    /// What reaching it grants, pre-formatted: `+115 Quartz, +80 A. Debris, …`, or
    /// a world line like `The Nether opens, +1 charge`.
    ///
    /// **Pre-formatted, and this is the one place in the read model that stays so on
    /// purpose.** Everywhere else a `String` here was a decision taken too early —
    /// phases 4 and 5 deleted five of them — but a roadmap row is *prose about a
    /// level*, three materials wide, with a world line that is not a material list at
    /// all. There is no layout decision hiding in it: the screen prints the sentence
    /// and justifies the XP after it. It is also the same sentence
    /// [`announce`] puts in the level-up toast, and sharing the
    /// wording is what stops the toast and the row disagreeing about what a level
    /// pays.
    pub grants: String,
    /// The XP that level costs, counted from zero, or [`None`] at the cap.
    ///
    /// [`None`] at [`LEVEL_CAP`] because there is
    /// no level 51 to climb to, so the row has no requirement to state — the same
    /// answer [`Player::xp_for_level`](skylode_core::player::Player) gives, carried
    /// rather than flattened. It draws as `—`, following the Mine screen's rule that
    /// an empty gauge never reads `0`: a `5 000` on the last row would name a price
    /// nothing is for sale at.
    pub xp: Option<u32>,
    /// Whether this level's reward is still waiting to be collected.
    ///
    /// **Distinct from "reached"**, which the screen derives from `player_level`, and
    /// the two really do come apart: a level below the player is reached and may still
    /// be waiting, which is the whole state this screen now exists to show. It is the
    /// core's answer ([`GameState::is_unclaimed`](skylode_core::game::GameState)) and
    /// not a guess from the level, because only the run knows what has been collected.
    pub unclaimed: bool,
}

/// The Levels screen: the whole ladder, where the cursor is, and how much is waiting
/// (UI.md §5.6).
///
/// **Grouped, where the two fields used to sit loose on [`View`]**, and phase 7 is
/// what forced it: a third one was about to join them. The other four screens have
/// carried a `*View` of their own since phase 3, and the reason is the same here —
/// a cursor and the list it points into are one thing, and a screen that has to
/// reach for them separately is one refactor away from them disagreeing.
#[derive(Clone, Debug)]
pub struct LevelsView {
    /// The **whole** roadmap, `1..=LEVEL_CAP`.
    ///
    /// It was the visible window until the screens learned to window their own lists;
    /// carrying the slice meant a taller terminal could not show more of the ladder,
    /// because the extra rows were never in the view to begin with.
    pub rows: Vec<LevelRow>,
    /// The topmost drawn row — scroll position, not selection.
    pub offset: usize,
    /// The level under the cursor, drawn `▸`.
    ///
    /// **A level and not an index**, matching every other cursor in the read model:
    /// the ladder is `1..=LEVEL_CAP` and a level is what the claim is addressed by, so
    /// an index would have to be converted back at the one moment it matters. It opens
    /// on the player's own level and parts company with it the moment `↑` is pressed —
    /// which is what `Home` exists to undo.
    pub selected: u32,
    /// How many rewards are waiting anywhere on the ladder.
    ///
    /// Carried rather than counted off `rows`, because the footer prints it on every
    /// frame and the screen would otherwise walk fifty rows to render one number. It
    /// is also what decides whether the collect-everything key is advertised at all.
    pub waiting: usize,
}

/// One row of an Upgrades sub-tab's list (UI.md §5.4).
///
/// Two mark channels, because the Pickaxe ladder carries both: `cursor`/`current`
/// are where you are and where the selection sits (`▸`/`●`), while `mark` is
/// **cumulative reachability** — `✓`/`~`/`✗`, "reachable buying every rung from
/// here". The marks come from `upgrade::max_affordable` on the Pickaxe ladder and
/// from `economy::affordability` on the other two sub-tabs, and the ladder invariant
/// (the `✓` region is a contiguous prefix) is asserted on the fixture *and* on a run.
#[derive(Clone, Debug)]
pub struct UpgradeRow {
    /// The row's columns, left to right — a rung label alone on the Pickaxe ladder,
    /// three cells on the two sub-tabs that draw a table.
    ///
    /// **Cells, not one laid-out line, and that is phase 6's instance of the recurring
    /// lesson.** The fixture wrote `"Fortune     III → IV  10"`, padding included, so
    /// the column widths were decided in the read model — where nothing knows how wide
    /// the pane is, and where a name one character longer silently breaks the
    /// alignment of every row below it. The screen owns the padding now: it measures
    /// the widest cell per column and lays them out, so the table is right by
    /// construction at any width.
    pub cells: Vec<String>,
    /// What buying this row would cost the player, as a verdict.
    pub mark: Mark,
    /// Whether the selection cursor sits here — drawn `▸`.
    pub cursor: bool,
    /// Whether this is the player's current position — drawn `●` (Pickaxe only).
    pub current: bool,
}

/// What a row's reachability column says (UI.md §5.4).
///
/// **A verdict and not a glyph**, which is what lets [`crate::theme::marked`] keep
/// owning the colour and the ladder-prefix test assert on meaning rather than on a
/// character. It carries one state [`Affordability`] does not: a track with no price
/// to quote at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Already bought — the rungs behind the player. Drawn blank.
    Owned,
    /// The till would take it: `✓`.
    Affordable,
    /// The value is held, the denomination is not: `~`.
    CompressFirst,
    /// The ore is not there: `✗`.
    Refused,
    /// **Nothing to price**, drawn `—`: a maxed track, or one gated by a level rather
    /// than by a purchase (the End's two rows, `Lv 30`). Distinct from
    /// [`Refused`](Mark::Refused) on purpose — a price the player cannot meet and a
    /// price that does not exist are different news, and `docs/UI.md` §5.4.2 draws the
    /// End's rows locked *with the reason* rather than as unaffordable.
    NoPrice,
}

impl Mark {
    /// The three-state verdict a [`Cost`] got, as a mark.
    ///
    /// A free function over the core's own enum rather than a `From` impl, because the
    /// mapping is not total in the other direction: [`NoPrice`](Mark::NoPrice) and
    /// [`Owned`](Mark::Owned) are the front-end's, and an [`Affordability`] can never
    /// mean either.
    pub fn of(verdict: &Affordability) -> Self {
        match verdict {
            Affordability::Affordable => Self::Affordable,
            Affordability::CompressFirst(_) => Self::CompressFirst,
            Affordability::Insufficient(_) => Self::Refused,
        }
    }

    /// The glyph this mark draws in the reachability column.
    ///
    /// Empty for [`Owned`](Mark::Owned): a rung already bought has nothing to say, and
    /// a glyph there would put a fourth symbol in a column the eye reads as three.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Owned => "",
            Self::Affordable => "✓",
            Self::CompressFirst => "~",
            Self::Refused => "✗",
            Self::NoPrice => "—",
        }
    }
}

/// One line of a price: what is owed, in one denomination, against what is held.
///
/// **The whole price used to be verdicted once**, and the detail pane painted every
/// line of it in that one colour — so a two-material cost short of a single ore read
/// entirely red and said nothing about which half was missing. The verdict belongs on
/// the line, because that is the granularity the player acts on: one line sends them
/// mining, the one beside it sends them to the Inventory screen.
///
/// **Not a [`CostLine`], and the reason is an invariant.** `CostLine` guarantees
/// `raw < RAW_PER_COMPRESSED` — it is the *split* of a total, and everything downstream
/// reads it as one. The pickaxe chain's aggregate (see
/// [`chain_price`]) sums forty-five rungs per denomination and must **not** re-split, so
/// it produces totals like `110 raw` that no `CostLine` may legally hold. Its fields are
/// `pub`, so nothing but a separate type would have stopped that.
///
/// It carries `held` for the same reason it carries `mark`: the Mines pane used to spend
/// four lines on a `You hold` block far from the price it was about, and the number is
/// only ever read beside the demand it answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceLine {
    /// The demand, in the denomination it is owed in.
    pub item: Item,
    /// How many of it the price asks for.
    pub needed: u32,
    /// How many of *that item* the player holds.
    pub held: u32,
    /// This line's own verdict.
    pub mark: Mark,
}

/// A price as the panes quote it: one entry per `(material, denomination)`, verdicted
/// line by line against what the player holds.
///
/// **The two passes are [`economy::affordability`]'s, narrowed to one line**, and the
/// asymmetry between them is the rule rather than an oversight: the wealth question is
/// asked per **material** (`raw_value`, blind to denomination) while the shape question
/// is asked per **item**. Asking wealth per item would put a `Compressed` line and a raw
/// line of the same ore in disagreement — one `~`, one `✗` — over a single pile the
/// player can convert either way.
///
/// The `owed` closure re-reads the whole material's demand for that reason: a line is
/// only *repairable* if the material's total value covers the material's total price,
/// not merely this line's share of it.
fn price_lines(inventory: &Inventory, lines: &[CostLine]) -> Vec<PriceLine> {
    let owed = |material: Material| -> u32 {
        lines
            .iter()
            .filter(|line| line.material == material)
            .map(|line| line.compressed * RAW_PER_COMPRESSED + line.raw)
            .sum()
    };
    lines
        .iter()
        .flat_map(CostLine::requirements)
        .map(|(item, needed)| {
            let held = inventory.count(item);
            PriceLine {
                item,
                needed,
                held,
                mark: if held >= needed {
                    Mark::Affordable
                } else if inventory.raw_value(item.material()) >= owed(item.material()) {
                    Mark::CompressFirst
                } else {
                    Mark::Refused
                },
            }
        })
        .collect()
}

/// The same, over a demand already summed past what a [`CostLine`] may hold.
///
/// The pickaxe chain's aggregate is one `(Item, total)` per denomination with no
/// `< RAW_PER_COMPRESSED` bound, so it cannot go through [`price_lines`]. The verdict is
/// the same three states, with the wealth pass reading the material's summed demand out
/// of the same map.
fn aggregate_price_lines(inventory: &Inventory, owed: &BTreeMap<Item, u32>) -> Vec<PriceLine> {
    let per_material = |material: Material| -> u32 {
        owed.iter()
            .filter(|(item, _)| item.material() == material)
            .map(|(item, total)| match item {
                Item::Compressed(_) => total * RAW_PER_COMPRESSED,
                Item::Raw(_) => *total,
            })
            .sum()
    };
    owed.iter()
        .map(|(&item, &needed)| {
            let held = inventory.count(item);
            PriceLine {
                item,
                needed,
                held,
                mark: if held >= needed {
                    Mark::Affordable
                } else if inventory.raw_value(item.material()) >= per_material(item.material()) {
                    Mark::CompressFirst
                } else {
                    Mark::Refused
                },
            }
        })
        .collect()
}

/// One sub-tab of the Upgrades screen: a list on the left, a detail pane on the
/// right (UI.md §5.4).
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
    /// The detail pane for the row under the cursor.
    pub detail: UpgradeDetail,
    /// The screen-local footer for this sub-tab.
    pub footer: String,
}

/// The right-hand pane, typed by sub-tab (UI.md §5.4, §5.4.1, §5.4.2).
///
/// **It used to be `Vec<String>` — the frame's box art transcribed verbatim** — and
/// that was honest while the numbers were placeholders. It cannot survive a real run:
/// the dip box quotes a power the pickaxe actually has, the cost block lists the
/// lines a purchase actually demands, and a pane built from strings would be a second
/// place those could be wrong.
#[derive(Clone, Debug)]
pub enum UpgradeDetail {
    /// The chain up to the cursor: its price, its dip, what it unlocks.
    Pickaxe(Box<PickaxeDetail>),
    /// One enchant track at its frontier.
    Enchant(EnchantDetail),
    /// One mine's size or richness track.
    Mine(MineTrackDetail),
    /// The one consumable the game sells.
    Boost(BoostDetail),
}

/// The Pickaxe sub-tab's pane: what the chain to the cursor costs and does.
///
/// **`costs` is summed per denomination and is never re-split.** It used to be one entry
/// per rung, which is what [`upgrade::chain_affordability`] walks — and at forty-five
/// rungs that block alone was longer than the pane, pushing the dip box, `Unlocks` and
/// `Ceiling` off the bottom. The rule those two carry is about the *re-split*: adding
/// `30 raw` to `80 raw` and quoting `1 Compressed + 10` describes a payment the player is
/// never asked to make. Summing inside a denomination invents nothing —
/// [`economy::pay`](skylode_core::economy) is strict per denomination and converts
/// nothing, and no ore enters the purse between two rungs, so the multiset of demands the
/// walk makes *is* this sum. See [`chain_price`].
///
/// The verdict is still the core's: [`mark`](PickaxeDetail::mark) comes from
/// `chain_affordability`, which is what the list column beside the pane shows. Where the
/// two can differ is *which* refusal they name — the walk stops at the first rung that
/// fails, the lines report every material independently — and the lines are the richer
/// answer.
#[derive(Clone, Debug)]
pub struct PickaxeDetail {
    /// The rung the cursor is on, named.
    pub title: String,
    /// Whether reaching it crosses a tier jump — the pane's `tier jump` tag.
    pub crosses_tier_jump: bool,
    /// Every rung the chain would buy, named, in climbing order.
    ///
    /// **Named and not merely counted**, because §6.7's modal opens on *"Buying Diamond
    /// Efficiency V, then the tier jump"* — the sentence that tells a player what they
    /// are about to lose Efficiency *from*. The pane beside it only wants the length,
    /// and `len()` is that; carrying the count instead would have made the modal ask
    /// the ladder a second time, from the far side of the read model.
    pub chain: Vec<String>,
    /// What the whole chain is verdicted at.
    pub mark: Mark,
    /// What the chain demands, one entry per material and denomination.
    pub costs: Vec<PriceLine>,
    /// What the chain does to the pickaxe's speed, on every rung.
    pub power: PowerDetail,
    /// The dip box, when the chain ends below the power it started at.
    pub dip: Option<DipDetail>,
    /// The mines this tier opens, if the chain reaches a new one.
    pub unlocks: Vec<MineKind>,
    /// The Efficiency cap before and after, when the chain crosses a jump.
    pub ceiling: Option<(u8, u8)>,
    /// What this rung *is*, when the player already owns it — [`None`] on a rung
    /// there is still something to buy on.
    ///
    /// **Exactly the complement of [`chain`](PickaxeDetail::chain) being non-empty**,
    /// and an [`Option`] rather than a flag because the two cases carry different
    /// numbers: a rung ahead is a *transition* (`22.0 → 25.0`), a rung behind is a
    /// *state* (`22.0`). Merging them would mean printing `22.0 → 22.0` on every owned
    /// rung, which is not a smaller answer but a wrong one.
    pub owned: Option<OwnedRung>,
}

/// What a rung the player already holds is worth, as opposed to what buying one
/// would change.
///
/// **The pane used to answer `Owned already — nothing to buy here.` and stop**, which
/// is true and useless: the player scrolling back up the ladder is asking what they
/// have, and the screen knew every number and printed none of them. Every field here
/// is the single-valued twin of a field the buyable rungs already show.
///
/// Every number is asked of the rung and not of the pickaxe, so a rung *below* the
/// player answers for itself — [`Pickaxe::power_with`](skylode_core::pickaxe::Pickaxe)
/// weighs a `(tier, efficiency)` pair without building one, which is the same door
/// §6.7's dip modal uses to weigh a rung the player does not own yet.
#[derive(Clone, Debug)]
pub struct OwnedRung {
    /// Mining power at this rung.
    pub power: f64,
    /// The block the tick count is quoted against — the value cell of the standing
    /// mine, the same reference [`PowerDetail`] uses, so the two panes are comparable.
    pub block: Block,
    /// Ticks that block takes at this rung, or [`None`] if this rung cannot break it.
    pub ticks: Option<u32>,
    /// This rung's Efficiency level, and the tier's cap on it.
    ///
    /// Both numbers, for the reason the Enchants table carries both: `4` alone cannot
    /// be told from a ceiling, and `4 / 5` says at a glance that the tier has one rung
    /// left in it.
    pub efficiency: (u8, u8),
    /// The mines this rung opened — empty unless the rung **is** the tier jump.
    ///
    /// A tier's mines are opened by the purchase that reaches the tier, not by the
    /// Efficiency rungs above it, so listing them on `Iron Eff IV` would credit that
    /// rung with something `Iron Pickaxe` did.
    pub unlocks: Vec<MineKind>,
}

/// What a chain does to the pickaxe's speed — the numbers UI.md §5.4's dip box and
/// §6.7's modal are made of, and the ones every *other* rung is now sold on.
///
/// **Stated in ticks per block as well as in power**, because `34.0 → 9.0` is a number
/// no player has an intuition for. The reference block is the *value* cell of the mine
/// they are standing in — always defined, and stable while the pane is being read, which
/// the aimed cell is not.
///
/// **Present on every rung, not only on a dip**, which is the answer to a plain
/// question the pane could not answer: a player buying Efficiency IV was told what it
/// cost and never what it bought. Only the *framing* is conditional — a dip draws the
/// box art, an ordinary rung a labelled block — because the box is a **warning**, and a
/// warning drawn on all forty-six rungs stops reading as one.
#[derive(Clone, Copy, Debug)]
pub struct PowerDetail {
    /// Mining power now.
    pub before: f64,
    /// Mining power after the chain.
    pub after: f64,
    /// The block the two tick counts are quoted against.
    pub block: Block,
    /// Ticks it takes now, or [`None`] if this pickaxe never breaks that block.
    pub ticks_before: Option<u32>,
    /// Ticks it will take after.
    pub ticks_after: Option<u32>,
}

/// The dip box (UI.md §5.4) and the modal that guards it (§6.7).
///
/// **What is left once the powers moved to [`PowerDetail`]: the repayment alone.** Its
/// presence is what says the chain *is* a dip — [`upgrade::preview`]'s `is_dip()` read
/// once, on the core's side of the boundary, so the box and the modal cannot disagree
/// about whether there is anything to warn about.
#[derive(Clone, Debug)]
pub struct DipDetail {
    /// The rung that earns the power back, when there is one.
    pub repaid_at: Option<Repaid>,
}

/// Where a dip is paid back, and how far away that is.
///
/// **The distance is carried, not recomputed.** §6.7's frame ends on *"five purchases
/// later"*, and that is the number the decision actually turns on: a dip repaid one
/// rung on is a different offer from one repaid five, and both read `35.0` in the
/// power column. Derived here from the two ladder indices, which is the only place
/// both are in hand.
#[derive(Clone, Debug)]
pub struct Repaid {
    /// The rung that earns the power back, named.
    pub rung: String,
    /// The power it restores — above [`PowerDetail::before`] by construction.
    pub power: f64,
    /// How many rungs past the one being bought it sits.
    pub rungs_later: usize,
}

/// One stat an upgrade moves, as the pane states it: a word and a `now → next` pair.
///
/// **The name is separate from the value so the screen can align the column**, which is
/// the same lesson [`UpgradeRow::cells`] records — a read model that pads has decided a
/// width it cannot know. The value is one already-formatted string because `4.0% → 6.0%`
/// is a single unit of meaning and no layout decision falls between its halves.
#[derive(Clone, Debug)]
pub struct StatStep {
    /// The stat's own word, lower-case: `square`, `procs`, `drops`, `speed`.
    pub name: &'static str,
    /// What it is now and what it becomes, as one phrase.
    pub value: String,
}

/// The Enchants sub-tab's pane: one track, its effect and its price.
#[derive(Clone, Debug)]
pub struct EnchantDetail {
    /// Which track.
    pub kind: EnchantType,
    /// The level held.
    pub level: u8,
    /// The cap the highest world reached allows.
    pub cap: u8,
    /// The world that cap comes from — the highest the player has unlocked, not the one
    /// whose mine they are standing in.
    ///
    /// Carried because the pane must name it: `Cap 3` alone says neither *whose* 3 it is
    /// nor that the game's ceiling is 10, which is the whole of UI.md §5.4.1's `Cap`
    /// block and the half the implementation had dropped.
    pub world: World,
    /// What the enchant does, in prose — front-end text, like the Mines screen's note.
    pub effect: Vec<String>,
    /// What the next level moves, in numbers. Empty at the cap.
    pub at_next: Vec<StatStep>,
    /// The one thing the numbers cannot say, when there is one — Explosive's square
    /// standing still because it only grows every third level.
    pub note: Vec<String>,
    /// What one level costs, verdicted line by line. Empty at the cap.
    ///
    /// **No overall verdict beside it.** The pane used to carry one and tint the whole
    /// price with it; every line now states its own, and the list row beside the pane
    /// still carries the price's — a third copy could only ever disagree with one of
    /// them.
    pub cost: Vec<PriceLine>,
}

/// The Mines sub-tab's pane: one paid track of one mine.
#[derive(Clone, Debug)]
pub struct MineTrackDetail {
    /// Which mine.
    pub kind: MineKind,
    /// Which of its two tracks.
    pub track: MineTrack,
    /// The level held, and the one being bought.
    pub level: (u32, u32),
    /// What the next level buys: a grid size, or a value-cell share — both sides of it.
    pub at_next: TrackOutcome,
    /// What it costs, verdicted line by line. Empty when the track is maxed, when the
    /// mine is locked, and when this run has never entered it — see
    /// [`EnchantDetail::cost`] for why no overall verdict sits beside it.
    pub cost: Vec<PriceLine>,
    /// Why this mine cannot be upgraded at all, if it cannot.
    pub blocked: Option<TrackBlock>,
}

/// The Boost sub-tab's pane: the one repeatable purchase in the game.
///
/// **Every field here is a constant of the game except two**, and that is what makes the
/// pane rather than the row the point of the sub-tab. The multiplier and the duration
/// are [`tunables`](skylode_core::tunables) — the same for the first charge and the
/// hundredth, since a boost carries no level — so the only figures that move with the
/// run are what the player holds and what they have banked. A table with three columns
/// of constants would be a table pretending to be a track.
///
/// **`reserve` lives here rather than being read off some shared field**, because the
/// question the shop asks about it is its own: *"how many will this purchase add to"*.
/// The Mine screen asks a different one — *"have I got one to fire"* — and answers it
/// from its own projection, so neither screen's read model depends on the other's.
#[derive(Clone, Debug)]
pub struct BoostDetail {
    /// What one charge costs, verdicted line by line — see [`EnchantDetail::cost`] for
    /// why no overall verdict sits beside it.
    pub price: Vec<PriceLine>,
    /// What the charge multiplies mining power by while it runs.
    pub multiplier: f32,
    /// How long one charge runs, in seconds — converted here because
    /// [`tunables`](skylode_core::tunables) quotes it in ticks, which is the tick
    /// loop's unit and not a player's.
    pub seconds: u32,
    /// How many charges are banked and unfired.
    pub reserve: u32,
}

/// What buying the next level of a mine track hands over — **both sides of the step**.
///
/// It used to carry the *after* alone, which is what UI.md §5.4.2's `At 7` block prints;
/// a player deciding whether to spend on it has no way to know what they are moving
/// *from* unless the pane also says so, and the number is free — both tracks are pure
/// functions of a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackOutcome {
    /// The grid now, and the one the size track would grow to.
    Size {
        /// The grid at the level held.
        before: (u8, u8),
        /// The grid one level up.
        after: (u8, u8),
    },
    /// The value cell's share of the grid, in percent, at the ceiling held and the next.
    Richness {
        /// The share the dial may reach today.
        before: u32,
        /// The share it may reach one ceiling up.
        after: u32,
    },
    /// The track has nothing left to sell.
    Maxed,
}

/// Why a mine's tracks are shut, when they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackBlock {
    /// Progression has not opened this mine yet — the `Lv 30` of §5.4.2.
    Locked(MineLock),
    /// The mine is open but this run has never walked into it, so it has no state to
    /// upgrade. One keypress away, which is why it is not a [`Locked`](TrackBlock::Locked).
    NotEntered,
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
    /// The boost charge, the only thing here that is bought more than once.
    pub boost: UpgradeSubtab,
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
            UpgradeTab::Boost => &self.boost,
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

/// One of a mine's two blocks, and what it costs *this* pickaxe to break.
///
/// **The pair travels together for the reason [`TargetView`] gives for its own**: a
/// bare `[Option<u32>; 2]` beside a screen that re-derives the blocks would let a
/// tick count be printed against the wrong rock, and nothing in either file would
/// notice. One type, both facts, paired at the point they are computed.
///
/// **There is deliberately no hardness field.** [`Block::hardness`] is a pure
/// function of the block, so the pane asks it at the moment of drawing — the same
/// argument [`TargetView`] makes for refusing a stored name. `ticks` cannot be had
/// that way: it depends on the pickaxe, which the screen only sees widened to
/// [`f64`] for display.
#[derive(Clone, Copy, Debug)]
pub struct BlockCostView {
    /// The block itself — the pane's source for both its name and its hardness.
    pub block: Block,
    /// Ticks it takes at the player's **unboosted** power, or [`None`] when the
    /// pickaxe's tier refuses it outright.
    ///
    /// **Unboosted on purpose.** A boost is thirty seconds long, and this screen
    /// exists to compare twelve mines; a number that changes by itself and changes
    /// back cannot be compared. The boosted product has a home already, on the Mine
    /// screen's Pickaxe panel, where it describes the swing being taken.
    ///
    /// **[`None`] is a refusal, not a very large number.** Below a block's
    /// [`min_pickaxe_tier`](skylode_core::block::Block::min_pickaxe_tier) the answer
    /// is *no*, never *slowly* — Skylode has no second regime for the wrong tool —
    /// so the arithmetic `ticks_to_break` would still happily do is a price the rules
    /// would never let the player pay. The pane prints
    /// [`NOTHING`] for it, as the dip modal does.
    pub ticks: Option<u32>,
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
    /// The mine's two blocks with their break costs — **common first, then value**,
    /// the order the pane has always named them in.
    ///
    /// An array and not two named fields, so the pane draws its two rows from one
    /// loop: the alternative is the same `format!` written twice, which is how two
    /// rows of one table drift apart.
    pub blocks: [BlockCostView; 2],
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
/// which one the player is short of.
///
/// **`material` is the [`Material`] and not its name**, which is the same move the
/// Mines list made with [`MineKind`]: a display string decided here is a decision
/// taken on the wrong side of the boundary, and the screen needs the value anyway to
/// tell which row the cursor is on.
#[derive(Clone, Copy, Debug)]
pub struct InvRow {
    /// The material this row is for — the source of its display name.
    pub material: Material,
    /// Compressed units held.
    pub compressed: u32,
    /// Raw units held.
    pub raw: u32,
}

/// What a refused purchase is waiting on, in the material under the cursor
/// (UI.md §5.3, §8.4).
///
/// **The panel's whole reason to exist.** UI.md §5.3's frame is drawn mid-refusal on
/// purpose: Iron reads 680 against a price of 650 and the player still cannot buy it,
/// so a panel that said "you cannot afford this" would be lying. What it says instead
/// is which *denomination* is missing, and this is the pair of facts that sentence is
/// built from.
///
/// Carried as a [`CostLine`] rather than the finished sentence, so the screen
/// composes `6 Compressed + 50` from the two numbers and cannot print a split that
/// disagrees with the price.
#[derive(Clone, Debug)]
pub struct CompressHint {
    /// What was refused, in the front-end's own words: `Efficiency V`.
    pub purchase: String,
    /// What that price wants in this material, in both denominations.
    pub needed: CostLine,
}

/// The Inventory screen: the table, the cursor, and the compress-first context
/// (UI.md §5.3).
#[derive(Clone, Debug)]
pub struct InventoryView {
    /// The fifteen materials, in [`Material::ALL`]'s order.
    pub rows: Vec<InvRow>,
    /// The material under the cursor.
    ///
    /// A [`Material`] rather than an index into `rows`, matching
    /// [`MinesView::selected`] and [`Cursors::material`](crate::cursor::Cursors): the
    /// screen looks the row up, so the cursor cannot survive as a number pointing
    /// somewhere the table does not go.
    pub selected: Material,
    /// The refusal this screen was walked here to clear, or [`None`].
    ///
    /// **[`None`] until something is actually refused, and that is the point.** A
    /// `CompressFirst` verdict comes only from `Enter` on the Upgrades screen, and it
    /// is carried here from [`App::refused`](crate::app::App::refused) **only when it
    /// names the material under the cursor** — a price in Stone printed beside the Coal
    /// row would attach a number to the wrong pile. Printing the frame's
    /// `Efficiency V wants…` regardless would have the screen inventing a refusal that
    /// never happened, which is the same class of lie the panel exists to avoid.
    pub hint: Option<CompressHint>,
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

/// The prestige trade, as the Stats panel and the two prestige modals both read it
/// (UI.md §5.5, §6.8, §6.9).
///
/// **One projection, two renderings**, and that is not tidiness — it is the rule the
/// dip modal already follows. §5.5's Progression panel and §6.8's preview quote the
/// same five figures, and a box that re-derived them from `GameState` could disagree
/// with the panel the player opened it from. So the panel and the box read this, and
/// there is nothing for them to disagree about.
///
/// **Values, not sentences.** The rank is a number and not `"III"`, the verdict is an
/// [`Affordability`] and not a `bool`: the box has to tell *"you are short of
/// Amethyst"* from *"you hold it in the wrong denomination"*, which is the same
/// distinction [`InventoryView`]'s hint is built on, and a `bool` would have thrown it
/// away before either reader could ask.
#[derive(Clone, Debug)]
pub struct PrestigeView {
    /// The rank the player holds; `0` before the first prestige.
    pub rank: u32,
    /// The multiplier that rank grants, in permille — `1000` at rank 0.
    pub multiplier_permille: u32,
    /// What the *next* rank would grant, in permille.
    pub next_multiplier_permille: u32,
    /// The material a prestige is paid in — Amethyst, and only Amethyst.
    pub material: Material,
    /// The next rank's price as a raw total, split for display by
    /// [`denominations`](crate::format::denominations).
    pub cost: u32,
    /// The **value** of the `material` the player holds, counted in raw whatever it is
    /// stored as ([`Inventory::raw_value`]).
    ///
    /// Only ever used as arithmetic — the shortfall the closing line quotes. It must
    /// **not** be printed through [`denominations`](crate::format::denominations): a
    /// total re-split that way reports what a *price* of that size would be owed in,
    /// so a purse of 20 000 raw reads as `200 Compressed`, which the player holds none
    /// of. That is what the pair below is for.
    pub held: u32,
    /// How many Compressed units of `material` are actually in the inventory.
    pub held_compressed: u32,
    /// How many raw items of it are actually in the inventory.
    ///
    /// Carried beside [`held`](PrestigeView::held) rather than derived from it, because
    /// it *cannot* be derived from it: `raw_value` is a sum, and a sum does not remember
    /// its terms. The two denominations are what the till reads and what the player is
    /// refused on, so they are what the box prints.
    pub held_raw: u32,
    /// What the till would say to that price right now.
    ///
    /// Carried whole rather than as a mark, because §6.8's closing line names the
    /// refusal: a player holding the value in the wrong denomination is one conversion
    /// away, and telling them to go mining would send them on a trip that ends in the
    /// same `✗`.
    pub verdict: Affordability,
    /// Which progression gates are still shut, or none.
    pub lock: PrestigeLock,
    /// The pickaxe tier the reset would take back to Wooden.
    pub tier: PickaxeTier,
    /// Efficiency's level, `0` if unowned — the row is dropped at zero.
    pub efficiency: u8,
    /// Fortune's level, same rule.
    pub fortune: u8,
    /// How many *other* enchants stand above level 0.
    ///
    /// Efficiency and Fortune have lines of their own in §6.8's left column, so
    /// counting them again would bill the player twice for the same loss.
    pub other_enchants: usize,
    /// The mining level the reset would take back to 1.
    pub level: u32,
}

/// One line of the Stats history panel (UI.md §5.5).
///
/// **A value and a shared sentence, not a formatted row.** The age is whole seconds
/// and the screen turns it into `2m`; formatting it here would spend an allocation per
/// entry on every reprojection — twenty a second while mining — for rows a terminal
/// mostly cannot show. That is [`PrestigeView`]'s *values, not sentences* rule applied
/// to the one field in the read model that is copied five hundred at a time.
///
/// The text is an [`Rc<str>`](std::rc::Rc) for the same arithmetic: it is cloned out
/// of [`Toasts`] on every reprojection, and cloning a
/// reference-counted pointer bumps a counter where cloning a [`String`] would copy the
/// sentence.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    /// How long ago it was announced, in whole seconds.
    pub age_secs: u64,
    /// What was announced, in [`announce`]'s words and nobody else's.
    pub text: Rc<str>,
}

/// The three panels of the Stats screen (UI.md §5.5).
///
/// **Every field is now the run's**, which is what took the last `..Self::sample()`
/// out of [`View::from_state`]. The prestige figures moved to [`PrestigeView`] when
/// the preview modal needed to quote them without being able to disagree with the
/// panel; the worlds table and the level cap are not here either, because the screen
/// derives them from `World` and `LEVEL_CAP`, which already answer them.
#[derive(Clone, Debug)]
pub struct StatsView {
    /// Blocks the **player** has broken — the auto-miner's are excluded, so the figure
    /// means *swings*. A lifetime total that survives a prestige.
    ///
    /// A [`u64`] because the core counts it in one: a maxed Nuke is two hundred cells a
    /// tick, and a `u32` would not survive a fortnight of them.
    pub blocks_broken: u64,
    /// Lifetime playtime, pre-formatted: `14h 22m`. Survives a prestige.
    ///
    /// Pre-formatted where [`blocks_broken`](StatsView::blocks_broken) is not, and the
    /// asymmetry is deliberate: a count has one rendering and a span has several, so
    /// choosing between `14h 22m` and `1d 2h` is a decision, and it is
    /// [`duration_hm`]'s. Making the screen redo it would
    /// let this row and the one below it disagree about which units they are in.
    pub playtime: String,
    /// Time in the current run, pre-formatted: `3h 07m` — cleared by a prestige.
    pub this_run: String,
    /// The run-progress rows of the "This run" panel.
    pub milestones: Vec<Milestone>,
    /// The announcement log, **newest first** — the toast buffer, verbatim.
    ///
    /// The whole log, not the visible window: the panel shows as much of it as its box
    /// has rows, which on a tall terminal is a good deal more than the ten UI.md §5.5
    /// had room to draw.
    pub history: Vec<HistoryEntry>,
    /// Which entry the player has scrolled to, counted from the newest.
    ///
    /// The panel has **no stored offset** — [`window`](crate::screen::window) derives
    /// the visible slice from this cursor, exactly as the Levels roadmap does. A second
    /// scroll position would be a second thing to keep in step with the first.
    pub selected: usize,
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
    ///
    /// **A fraction of what this boost was granted, not of one charge's duration.**
    /// The range holds by construction rather than by the gauge clamping it — see
    /// [`boost_view`] for why the denominator has to come from the boost itself once
    /// charges can stack.
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
    /// Charges banked and unfired — what `b` has to spend.
    ///
    /// **Beside [`boost`](View::boost) and not inside it**, and the two shapes say two
    /// different things on purpose. `Option<BoostView>` means *a boost either runs or
    /// does not exist*; the reserve is at its most interesting in exactly the case that
    /// `None` covers, since a player with three charges and nothing running is a player
    /// with a key to press. Folding the count into `BoostView` would make it
    /// unreachable precisely when it matters.
    ///
    /// A `u32` and not a formatted string, unlike most of what this screen draws: the
    /// gauge label has three shapes for it — running, idle with charges, idle with none
    /// — and choosing between them is a rendering decision that belongs where the
    /// column budget is known.
    pub boost_charges: u32,
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
    /// Which cells a spatial blast is flashing this frame, and on which beat (UI.md §7).
    ///
    /// **The one field here that is not about the run at all**, and the only reason it
    /// travels with the rest: it is a consequence of an event that has already happened,
    /// resolved against a wall clock the core is forbidden. [`Flashes::resolve`] answers
    /// it from [`App`](crate::app::App)'s own buffer, which is the same route
    /// [`stats`](View::stats)' history takes out of [`Toasts`].
    ///
    /// **Already a beat and not an instant**, so that everything downstream — the
    /// widget, and every test that draws it — is a pure function of what it is handed.
    /// The clock is read exactly once per frame, in [`from_state`](View::from_state).
    ///
    /// Empty on all but a handful of frames a session, which is what makes carrying it
    /// by value affordable: a blast is at most 200 entries, and most reprojections copy
    /// none at all.
    pub flash: BTreeMap<(u8, u8), FlashStage>,
    /// The Levels roadmap, its cursor, and what is waiting on it (UI.md §5.6).
    pub levels: LevelsView,
    /// The three panels of the Stats screen (UI.md §5.5).
    pub stats: StatsView,
    /// The prestige trade, read by the Stats panel and by both prestige modals
    /// (UI.md §5.5, §6.8, §6.9).
    pub prestige: PrestigeView,
    /// The Inventory table and its compress panel (UI.md §5.3).
    pub inventory: InventoryView,
    /// The Mines list and the selected mine's detail pane (UI.md §5.2).
    pub mines: MinesView,
    /// The Upgrades screen's three sub-tabs (UI.md §5.4).
    pub upgrades: UpgradesView,
    /// How many colours to ask the terminal for — a player preference that lives in
    /// the save and is edited on the Settings screen.
    ///
    /// **Copied into the read model rather than read from [`Config`] by the screen that
    /// wants it**, which is the route every preference a *screen* consults will take:
    /// `Screen::render` is handed a `&View` and nothing else, so a screen that reached
    /// for the config would need a second parameter added to all six signatures for the
    /// sake of the one that uses it. The copy costs a `Copy` enum per projection.
    pub colour_mode: ColourMode,
}

impl View {
    /// The whole read model, derived from a real run.
    ///
    /// **This is the wire the whole `View` indirection existed for.** The Mine
    /// screen's every figure now comes from `GameState`, and nothing under
    /// `screen/` changed to make that true — which is what the module header
    /// promised while the rules were still being written.
    ///
    /// **The `..Self::sample()` is gone, and its absence is the guarantee.** It was the
    /// progress marker: Rust's *functional update syntax* — `Self { a, b, ..other }`,
    /// "these fields explicitly, the remainder taken from `other`" — made that one line
    /// the literal list of what the phase still owed, and each phase lifted its own
    /// fields above it. The Stats panels were the last, so the line went with them and
    /// **the compiler is exhaustive over [`View`] again**: a field added here now
    /// breaks this function until someone decides where it comes from, where before it
    /// would have silently taken a wireframe's value into a real run.
    ///
    /// Six parameters and not one, because six of the answers are not in the run:
    ///
    /// - `config` is what the player asked the front-end to look like. It is the one
    ///   argument that *is* in the save — preferences travel inside it — but it is not
    ///   in the [`GameState`], and deliberately: a keybinding is not a game rule, and
    ///   the determinism contract must not see one.
    /// - `cursors` is where the player is *pointing*, which is front-end state by
    ///   definition — a list selection has no business reaching a save.
    /// - `refused` is the last compress-first refusal
    ///   ([`App::refused`](crate::app::App::refused)). A refusal changes no game state,
    ///   which is precisely what makes it the front-end's to remember.
    /// - `toasts` and `now` are the announcement log and the instant to age it against.
    ///   Same argument, twice: the run does not know what was said about it, and the
    ///   core is forbidden a clock.
    /// - `flashes` is the third instance of that same argument, and the purest: a proc
    ///   flash is *nothing but* an ambient clock, which is the one thing the determinism
    ///   contract keeps outside the core. It is resolved against the same `now`, so the
    ///   beat a cell is drawn on and the age the history prints cannot come from two
    ///   readings of the clock that disagree.
    ///
    /// Six positional parameters is more than this crate likes, and the reason it is
    /// tolerable here rather than a `struct` is that no two of them share a type: a
    /// misordered call does not compile. That is precisely the hazard
    /// [`Input`](skylode_core::game::Input) exists to guard against one crate down,
    /// where every field is a `bool`.
    ///
    /// A projection this wide does real work, which is why [`App`](crate::app::App)
    /// caches the result in a field rather than calling this inside its `render`: the
    /// cost is paid when the state changes, not thirty times a second.
    pub fn from_state(
        state: &GameState,
        config: &Config,
        cursors: Cursors,
        refused: Option<&CompressHint>,
        toasts: &Toasts,
        flashes: &Flashes,
        now: Instant,
    ) -> Self {
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
            boost: state.active_boost().map(|boost| {
                boost_view(
                    boost.remaining_ticks(),
                    boost.granted_ticks(),
                    boost.multiplier(),
                )
            }),
            boost_charges: state.boost_charges(),
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
            // Asked for *this* mine, so a blast cannot follow the player into the next
            // one: the coordinates are bare `(u8, u8)` and the buffer is what refuses,
            // rather than every site that changes mine having to remember to clear it.
            flash: flashes.resolve(kind, now),
            mines: mines_view(state, cursors),
            inventory: inventory_view(player.get_inventory(), cursors, refused),
            upgrades: upgrades_view(state, cursors),
            levels: levels_view(state, cursors),
            prestige: prestige_view(player),
            stats: stats_view(state, cursors, toasts, now),
            // The preference, at last, where it used to be a hard-wired default. The
            // field and its one consumer (`screen::mine`'s grid) have existed since the
            // palette landed; what was missing was the wire, and this is it.
            colour_mode: config.colour,
        }
    }

    /// The placeholder save drawn throughout UI.md §5: level 23, Diamond
    /// pickaxe, standing in the Iron Mine.
    ///
    /// **`#[cfg(test)]`, and that is what wiring the last screen bought.** This
    /// fixture used to be *production* code: [`from_state`](View::from_state) ended in
    /// `..Self::sample()`, so every real projection built the whole wireframe and threw
    /// most of it away, and the several hundred lines of it shipped in the binary. With
    /// the last field wired there is no caller left outside the tests, so it compiles
    /// out entirely — the fixture is now what it always claimed to be.
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
    #[cfg(test)]
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
            // Both halves at once, which is the state the label has least room for:
            // a boost running *and* charges banked behind it. The fixture is where the
            // counted frame is measured, so it should carry the widest case.
            boost_charges: 3,
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
            // Empty, and deliberately not a fixture: a flash lasts 200 ms, so a
            // *placeholder* one would put a blast on every screenshot this fixture draws
            // and on none of the frames a player ever sees. The tests that care build
            // their own map, which is also the only way to name an instant.
            flash: BTreeMap::new(),
            levels: sample_levels(),
            stats: sample_stats(),
            prestige: sample_prestige(),
            inventory: sample_inventory(),
            mines: sample_mines(),
            upgrades: sample_upgrades(),
            colour_mode: ColourMode::default(),
        }
    }
}

/// The Pickaxe panel's first line: `Diamond Pickaxe  Efficiency IV`.
///
/// **Takes the tier and the level, not the [`Pickaxe`]**,
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

/// The Boost gauge's figures, from a running boost's three numbers.
///
/// **Takes the numbers and not the [`Boost`](skylode_core::boost::Boost)**, for
/// [`pickaxe_summary`]'s reason
/// exactly: `Boost::new` is `pub(crate)` — minting the game's strongest multiplier
/// is not a front-end's business — and a helper taking one could not be tested from
/// this crate at all, since no legal sequence of public calls reaches a boost from a
/// level-1 run. `from_state` unwraps the boost; this formats it.
///
/// The seconds go through [`boost_seconds`], which is where the `div_ceil` and its
/// argument live: this is a **countdown**, so flooring would print `0s` for a
/// twentieth of a second while the boost was still multiplying. It moved there when the
/// fire toast needed the same conversion — one implementation, two readers, rather than
/// a gauge and a toast free to disagree about when a boost ends.
///
/// **The ratio is against `granted`, the boost's own total — not against
/// [`BOOST_DURATION_TICKS`].** The constant is one charge's worth, and charges
/// *stack by addition*, so it is the right denominator for exactly the first charge
/// and wrong for every boost built out of two or more: a sixty-second boost measured
/// against thirty seconds sits at a clamped 100 % for its whole first half, which is
/// a gauge that stops answering the only question it is asked. Dividing by the total
/// the core now carries makes the bar open full, fall to empty, and step *up*
/// visibly when another charge lands.
///
/// Rounding the constant up to the next multiple was the version that needed no core
/// change, and it is worse than the bug: crossing back under thirty seconds drops the
/// denominator with it, so the bar leaps from half to full **while draining**.
///
/// The result is in `0.0..=1.0` by construction — the core refuses a boost with more
/// left than it was granted. The `ratio` helper in `screen::mine` still clamps, and
/// that is not a duplicate check: it is `LineGauge`'s panic guard, which has to hold
/// whatever arithmetic happens up here.
///
/// A `granted` of zero cannot reach this — the tick sweeps a lapsed boost off the
/// state before anything projects it — but `0 / 0` is `NaN`, and one branch is
/// cheaper than an argument about who else might call it later.
fn boost_view(remaining: u32, granted: u32, multiplier: f32) -> BoostView {
    BoostView {
        seconds: boost_seconds(remaining),
        multiplier: f64::from(multiplier),
        ratio: if granted == 0 {
            0.0
        } else {
            remaining as f32 / granted as f32
        },
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
    // The cost of the selected mine's two blocks, weighed here rather than in the
    // screen for two reasons. The power is the core's own `f32` at this point —
    // [`PickaxeView::power`] is widened to `f64` for the Mine screen's boost product,
    // and narrowing it back would be a cast that buys nothing. And the tier gate is a
    // *rule*, asked of [`Pickaxe::can_mine`], which is the read model's job to resolve
    // rather than the renderer's to re-implement.
    let pickaxe = player.get_pickaxe();
    let cost = |block: Block| BlockCostView {
        block,
        // Flattened, because the two refusals mean the same thing to the screen: a
        // tier too low and a power that buys no progress both leave the pane with no
        // tick count to print, and nesting them would make the renderer unwrap a
        // distinction it has nothing to do with.
        ticks: pickaxe
            .can_mine(block)
            .then(|| block.ticks_to_break(pickaxe.mining_power()))
            .flatten(),
    };
    let detail = MineDetail {
        blocks: [cost(selected.common_block()), cost(selected.value_block())],
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

/// The Inventory screen's read model, projected from what the player carries.
///
/// **Walks [`Material::ALL`] and not the inventory**, which is the opposite of what
/// the data structure invites. An [`Inventory`] is *sparse* — an item absent from its
/// map is held zero times, and `remove` deletes an entry the moment it hits zero — so
/// iterating it would give a table whose rows appeared and vanished as the player
/// spent. The frame is fifteen fixed rows (`docs/UI.md` §5.3), and a row reading `0`
/// is information: it says the player has none of a material that exists, which is
/// exactly what an empty inventory should look like.
///
/// **Takes the [`Inventory`] and not the [`GameState`]**, unlike [`mines_view`]. It
/// needs nothing else — no lock, no lazily-created mine — and the narrower parameter
/// is what lets a test build one directly instead of driving a run to reach it.
///
/// The `hint` is [`None`] here rather than derived: see [`InventoryView::hint`].
fn inventory_view(
    inventory: &Inventory,
    cursors: Cursors,
    refused: Option<&CompressHint>,
) -> InventoryView {
    InventoryView {
        rows: Material::ALL
            .into_iter()
            .map(|material| InvRow {
                material,
                compressed: inventory.count(Item::Compressed(material)),
                raw: inventory.count(Item::Raw(material)),
            })
            .collect(),
        selected: cursors.material,
        // **Shown only on the row it is about.** The hint names a price in one
        // material, so printing it beside a different pile would attach a number to
        // the wrong thing — and the player walked here to look at one row.
        hint: refused
            .filter(|hint| hint.needed.material == cursors.material)
            .cloned(),
    }
}

/// The three Upgrades sub-tabs, projected from the run (UI.md §5.4).
///
/// All three are built on every redraw, not just the one showing: they are cheap —
/// forty-six rungs, six tracks, twenty-four mine rows — and building only the active
/// one would put a `match` on the cursor in front of a projection that has no other
/// reason to know which tab is up. [`App`](crate::app::App) caches the whole `View`
/// anyway, so this runs when the run changes rather than thirty times a second.
/// The Levels roadmap, projected from the run (UI.md §5.6).
///
/// **The whole ladder every frame, rewards included, and it costs nothing to build**:
/// [`reward_for_level`](reward::reward_for_level) is a pure total function of the
/// level, which is precisely why the roadmap can show what level 40 pays to a player
/// on level 3. A run that *stored* its rewards could not — a save holds what has
/// happened, and this list is mostly about what has not.
///
/// The only thing here that the run answers is
/// [`unclaimed`](LevelRow::unclaimed): what a formula cannot know is what the player
/// has already picked up.
///
/// `offset` is `0` and not the cursor's neighbourhood, for the reason every other list
/// in this crate leaves it so: the screen windows its own rows against the height it
/// actually has, and [`window`](crate::screen::window) moves the offset to keep the
/// cursor on screen. A view that pre-scrolled would be guessing at a terminal size it
/// cannot see.
fn levels_view(state: &GameState, cursors: Cursors) -> LevelsView {
    LevelsView {
        rows: (1..=LEVEL_CAP)
            .map(|level| {
                let reward = reward::reward_for_level(level);
                LevelRow {
                    level,
                    grants: grants_line(reward.as_ref()),
                    xp: Player::xp_for_level(level),
                    unclaimed: state.is_unclaimed(level),
                }
            })
            .collect(),
        offset: 0,
        selected: cursors.level,
        waiting: state.unclaimed_count(),
    }
}

/// A roadmap row's `Grants` cell: the payout, then the charge if the level carries one.
///
/// **The charge is appended here and not in [`announce::payout`]**, which is the seam
/// between the roadmap and the toast: §5.6 draws `The Nether opens, +1 charge` and the
/// toast deliberately drops the garnish. Both read the same payout wording, so the two
/// cannot name different materials — they differ only in what each frame has room to
/// care about.
///
/// A level with no reward at all gets an empty cell rather than a dash: the row's own
/// level number and XP are still there, so the line is not empty, and `—` would read
/// as *"a reward that is nothing"* instead of *"no reward"*.
fn grants_line(reward: Option<&LevelReward>) -> String {
    let Some(reward) = reward else {
        return String::new();
    };
    let mut line = announce::payout(&reward.payout);
    if reward.boost_charges > 0 {
        line.push_str(&format!(", +{} charge", reward.boost_charges));
    }
    line
}

fn upgrades_view(state: &GameState, cursors: Cursors) -> UpgradesView {
    UpgradesView {
        active: cursors.upgrade_tab,
        pickaxe: pickaxe_subtab(state, cursors),
        enchants: enchants_subtab(state, cursors),
        mines: mine_tracks_subtab(state, cursors),
        boost: boost_subtab(state),
    }
}

/// The Boost sub-tab: one row, and a pane that carries the rest (UI.md §5.4.4).
///
/// **It takes no [`Cursors`], and that is the shape of the thing rather than a
/// simplification.** The other three sub-tabs need to know which row is pointed at
/// because their panes describe *that* row; a list of one has nothing to point at that
/// is not already the whole list. The `cursor: true` below is therefore a constant, and
/// `App::step_list_cursor` has nothing to move here.
///
/// The price goes through [`economy::boost_cost`] and [`price_lines`] like every other
/// price on this screen, so the `✓ ~ ✗` this row draws is the same verdict the till will
/// reach. The boost is the one purchase with **no cap and no ladder**, which is why
/// there is no [`Mark::NoPrice`] branch: a maxed track has nothing to sell, and this one
/// never runs out.
fn boost_subtab(state: &GameState) -> UpgradeSubtab {
    let inventory = state.player().get_inventory();
    let cost = economy::boost_cost();
    let price = price_lines(inventory, cost.lines());
    let reserve = state.boost_charges();
    // Widened, divided, narrowed — [`boost_view`]'s conversion and for its reason:
    // `TICKS_PER_SECOND` is a `u64` while a tick counter is a `u32`.
    let seconds = u32::try_from(u64::from(BOOST_DURATION_TICKS) / TICKS_PER_SECOND).unwrap_or(0);

    // **Two columns where the other sub-tabs have three, and the list is measured
    // rather than guessed.** The master side is 35 columns; `Redstone boost` alone is
    // 14 of them, so a third column carrying the effect was clipped to `3` by the
    // reachability mark. What the row must say is what it is, how many are banked, and
    // whether one is affordable — the effect and the price are the pane's, on the rule
    // the other three already follow: none of them prints a cost in its list either.
    let rows = vec![UpgradeRow {
        cells: vec![
            "Redstone boost".to_owned(),
            // The reserve on the row as well as in the pane: the list is what a player
            // scanning the four sub-tabs sees first, and *how many you already hold* is
            // the fact that decides whether to buy another one.
            match reserve {
                0 => NOTHING.to_owned(),
                held => format!("{} held", grouped(held)),
            },
        ],
        mark: Mark::of(&economy::affordability(inventory, &cost)),
        cursor: true,
        current: false,
    }];

    UpgradeSubtab {
        header: vec!["Item".to_owned(), "Reserve".to_owned()],
        rows,
        offset: 0,
        detail: UpgradeDetail::Boost(BoostDetail {
            price,
            multiplier: BOOST_MULTIPLIER,
            seconds,
            reserve,
        }),
        footer: BOOST_FOOTER.to_owned(),
    }
}

/// The pickaxe ladder: forty-six rungs, the cumulative `✓` prefix, and the chain the
/// cursor is pointing at.
///
/// **The prefix is read off [`upgrade::max_affordable`] and not asked per row.** One
/// walk answers where the `✓`s stop, and asking each rung on its own would be
/// forty-six simulations of the same climb — and, worse, forty-six chances for the
/// column to come out with a hole in it. The `~` is the *first* refused rung's own
/// verdict, since that is the one the player is being told how to clear; everything
/// past it is `✗`, because a chain that already failed cannot be diagnosed further.
fn pickaxe_subtab(state: &GameState, cursors: Cursors) -> UpgradeSubtab {
    let ladder = upgrade::ladder();
    let pickaxe = state.player().get_pickaxe();
    let inventory = state.player().get_inventory();
    let here = upgrade::position(&ladder, pickaxe);
    let furthest = upgrade::max_affordable(inventory, pickaxe);
    // The verdict on the first rung past the prefix — `~` or `✗` — which is what
    // decides whether that one row invites a conversion or a mine.
    let frontier = Mark::of(&upgrade::chain_affordability(
        inventory,
        pickaxe,
        furthest + 1,
    ));

    let rows = ladder
        .iter()
        .enumerate()
        .map(|(index, rung)| UpgradeRow {
            cells: vec![rung_label(rung.tier, rung.efficiency)],
            mark: match index {
                i if i <= here => Mark::Owned,
                i if i <= furthest => Mark::Affordable,
                i if i == furthest + 1 => frontier,
                _ => Mark::Refused,
            },
            cursor: index == cursors.pickaxe_rung,
            current: index == here,
        })
        .collect();

    UpgradeSubtab {
        header: Vec::new(),
        rows,
        offset: 0,
        detail: UpgradeDetail::Pickaxe(Box::new(pickaxe_detail(state, &ladder, cursors))),
        footer: " ↑↓  select     Enter  buy to here     M  buy max     Tab  next screen".to_owned(),
    }
}

/// The Pickaxe pane: the chain to the cursor, priced, with its dip if it has one.
fn pickaxe_detail(
    state: &GameState,
    ladder: &[upgrade::PickaxeRung],
    cursors: Cursors,
) -> PickaxeDetail {
    let pickaxe = state.player().get_pickaxe();
    let inventory = state.player().get_inventory();
    let here = upgrade::position(ladder, pickaxe);
    let target = cursors.pickaxe_rung.min(ladder.len().saturating_sub(1));
    let preview = upgrade::preview(pickaxe, target);

    // The rungs actually being bought: everything past where the player stands, up to
    // and including the cursor. Empty when the cursor is at or behind them.
    let climbed = ladder.get(here + 1..=target).unwrap_or(&[]);
    let costs = aggregate_price_lines(inventory, &chain_price(climbed));

    let title = ladder
        .get(target)
        .map_or_else(String::new, |rung| rung_label(rung.tier, rung.efficiency));
    let ceiling = climbed
        .iter()
        .any(upgrade::PickaxeRung::is_tier_jump)
        .then(|| {
            (
                pickaxe.get_tier().efficiency_cap(),
                ladder
                    .get(target)
                    .map_or(0, |rung| rung.tier.efficiency_cap()),
            )
        });
    // What the chain opens: the mines gated behind a tier the player does not have
    // yet but will after it. Asked of `MineKind` rather than listed, so a thirteenth
    // mine announces itself.
    let reached = ladder.get(target).map(|rung| rung.tier);
    let unlocks = reached
        .filter(|&tier| tier != pickaxe.get_tier())
        .map(|tier| {
            MineKind::ALL
                .into_iter()
                .filter(|kind| kind.gating_tier() == tier)
                .collect()
        })
        .unwrap_or_default();

    // A rung at or behind the player: there is no chain and so no transition to
    // quote, and what the pane owes instead is what this rung is worth. Gated on
    // `climbed` rather than on a comparison of indices, so the two branches of the
    // pane cannot disagree about which rungs are owned.
    let owned = climbed
        .is_empty()
        .then(|| ladder.get(target))
        .flatten()
        .map(|rung| {
            let power = pickaxe.power_with(rung.tier, rung.efficiency);
            let block = state.current_mine().kind().value_block();
            OwnedRung {
                power: f64::from(power),
                block,
                ticks: block.ticks_to_break(power),
                efficiency: (rung.efficiency, rung.tier.efficiency_cap()),
                unlocks: if rung.is_tier_jump() {
                    MineKind::ALL
                        .into_iter()
                        .filter(|kind| kind.gating_tier() == rung.tier)
                        .collect()
                } else {
                    Vec::new()
                },
            }
        });

    PickaxeDetail {
        title,
        crosses_tier_jump: preview.crosses_tier_jump,
        chain: climbed
            .iter()
            .map(|rung| rung_label(rung.tier, rung.efficiency))
            .collect(),
        mark: if climbed.is_empty() {
            Mark::Owned
        } else {
            Mark::of(&upgrade::chain_affordability(inventory, pickaxe, target))
        },
        costs,
        power: power_detail(state, &preview),
        dip: preview.is_dip().then(|| DipDetail {
            repaid_at: preview.repaid_at.and_then(|index| {
                ladder.get(index).map(|rung: &upgrade::PickaxeRung| Repaid {
                    rung: rung_label(rung.tier, rung.efficiency),
                    power: f64::from(
                        state
                            .player()
                            .get_pickaxe()
                            .power_with(rung.tier, rung.efficiency),
                    ),
                    rungs_later: index.saturating_sub(target),
                })
            }),
        }),
        unlocks,
        ceiling,
        owned,
    }
}

/// The chain's whole demand, summed per material **and denomination**.
///
/// **The sum never crosses a denomination**, which is the entire safety of it: `30 raw`
/// plus `80 raw` is `110 raw` and is never re-quoted as `1 Compressed + 10`. That
/// re-split is what `docs/UI.md` and the core's `chain_affordability` forbid — it names a
/// payment the player is never asked to make, and one they may be unable to make while
/// being able to pay the two steps in order.
///
/// Within a denomination there is nothing to invent. `economy::pay` checks and debits
/// each `(Item, amount)` strictly, converting nothing, and no ore enters the purse
/// between two rungs — so the demands the sequential walk makes are exactly this
/// multiset, and holding it is the same fact as being able to pay every rung in order.
///
/// A [`BTreeMap`] and not a `HashMap`, for the reason every map in `save` is one: the
/// same chain must lay its lines out in the same order on every redraw.
fn chain_price(climbed: &[upgrade::PickaxeRung]) -> BTreeMap<Item, u32> {
    let mut owed = BTreeMap::new();
    for (item, amount) in climbed
        .iter()
        .filter_map(|rung| rung.cost.as_ref())
        .flat_map(|cost| cost.lines().iter().flat_map(CostLine::requirements))
    {
        *owed.entry(item).or_insert(0) += amount;
    }
    owed
}

/// What the chain does to the swing, quoted against the block the player is actually
/// mining.
///
/// **The *value* cell of the standing mine**, per Enoal's call for phase 6: it is
/// always defined, unlike the aimed cell on a fresh grid, and it does not change
/// while the pane is being read. It is also the dearer of the two, which is the honest
/// half of a warning about losing speed — and the useful half of a promise about
/// gaining it.
///
/// **Computed on every rung**, not only under `is_dip()`. It used to sit inside the dip
/// branch, which meant the one number a speed upgrade is bought for was printed only
/// when it went the wrong way.
fn power_detail(state: &GameState, preview: &upgrade::UpgradePreview) -> PowerDetail {
    let block = state.current_mine().kind().value_block();
    PowerDetail {
        before: f64::from(preview.power_before),
        after: f64::from(preview.power_after),
        block,
        ticks_before: block.ticks_to_break(preview.power_before),
        ticks_after: block.ticks_to_break(preview.power_after),
    }
}

/// The six enchant tracks, each at its frontier (UI.md §5.4.1).
///
/// **The marks here are *independent*, unlike the pickaxe ladder's**, and that is a
/// property of the tracks rather than a rendering choice: each is paid in its own
/// materials, so a cheaper track really can be unaffordable while a dearer one is
/// not. Same three glyphs, two meanings; the sub-tab is what keeps them apart.
fn enchants_subtab(state: &GameState, cursors: Cursors) -> UpgradeSubtab {
    let player = state.player();
    let enchants = player.get_pickaxe().enchants();
    let world = player.highest_unlocked_world();
    let inventory = player.get_inventory();

    let rows = cursor::enchant_tracks()
        .into_iter()
        .map(|kind| {
            let level = enchants.get_level(kind);
            let cap = kind.max_level(player.get_pickaxe().get_tier(), world);
            let cost = economy::enchant_cost(kind, level);
            UpgradeRow {
                cells: vec![
                    kind.name().to_owned(),
                    if level >= cap {
                        MAXED.to_owned()
                    } else {
                        format!("{} → {}", level_word(level), roman(level + 1))
                    },
                    // **Both numbers, because the column showed the world's as if it
                    // were the game's.** A player reading `3` beside a track has no way
                    // to tell a ceiling they have hit from one the Overworld is holding
                    // down, and the two call for opposite decisions — stop buying, or go
                    // open the Nether.
                    format!("{cap}/{}", World::End.enchant_cap()),
                ],
                // A capped track has no price to quote, which is the `—` case and not a
                // refusal: the player is not short of anything. The same arm catches
                // the un-priced enchant — [`economy::enchant_cost`] answers [`None`]
                // for Efficiency alone, which `enchant_tracks` has already filtered
                // out — so there is no third branch here that no row can reach.
                mark: match cost {
                    Some(cost) if level < cap => {
                        Mark::of(&economy::affordability(inventory, &cost))
                    }
                    _ => Mark::NoPrice,
                },
                cursor: kind == cursors.enchant,
                current: false,
            }
        })
        .collect();

    let kind = cursors.enchant;
    let level = enchants.get_level(kind);
    let cap = kind.max_level(player.get_pickaxe().get_tier(), world);
    let cost = (level < cap)
        .then(|| economy::enchant_cost(kind, level))
        .flatten();
    let detail = EnchantDetail {
        kind,
        level,
        cap,
        world,
        effect: enchant_effect(kind, level),
        at_next: enchant_at_next(
            kind,
            level,
            cap,
            state.current_mine().get_size().0,
            player.get_pickaxe(),
        ),
        note: enchant_note(kind, level, cap),
        cost: cost
            .as_ref()
            .map(|cost| price_lines(inventory, cost.lines()))
            .unwrap_or_default(),
    };

    UpgradeSubtab {
        header: vec!["Enchant".to_owned(), "Level".to_owned(), "Cap".to_owned()],
        rows,
        offset: 0,
        detail: UpgradeDetail::Enchant(detail),
        footer: SELECT_FOOTER.to_owned(),
    }
}

/// A level as the roadmap prints it: a Roman numeral, or `0` at the bottom.
///
/// [`roman`] answers `?` for zero — deliberately, since no *bought* level is zero —
/// but a track the player has never touched reads `0 → I` in the frame, so the zero
/// case is spelled here rather than by loosening the numeral table.
pub fn level_word(level: u8) -> String {
    if level == 0 {
        "0".to_owned()
    } else {
        roman(level).to_owned()
    }
}

/// The twelve mines' two paid tracks each (UI.md §5.4.2).
///
/// **The mine's name repeats on both of its rows**, because a scroll window can open
/// on a richness row whose mine name has gone off the top. Every row must be readable
/// alone, which is the only state a scrolled row is ever in.
fn mine_tracks_subtab(state: &GameState, cursors: Cursors) -> UpgradeSubtab {
    let player = state.player();
    let inventory = player.get_inventory();

    let rows = cursor::mine_tracks()
        .into_iter()
        .map(|(kind, track)| {
            let (next, mark) = track_row(state, kind, track);
            UpgradeRow {
                cells: vec![kind.name().to_owned(), track_word(track).to_owned(), next],
                mark,
                cursor: (kind, track) == cursors.mine_track,
                current: kind == state.current_mine().kind(),
            }
        })
        .collect();

    let (kind, track) = cursors.mine_track;
    let lock = player.mine_lock(kind);
    let mine = state.mine(kind);
    let level = mine.map_or(0, |mine| match track {
        MineTrack::Size => mine.get_size_level(),
        MineTrack::Richness => mine.get_richness_level(),
    });
    let cost = track_cost(kind, track, level, mine.is_some());
    let detail = MineTrackDetail {
        kind,
        track,
        level: (level, level + 1),
        at_next: track_outcome(track, level),
        cost: cost
            .as_ref()
            .map(|cost| price_lines(inventory, cost.lines()))
            .unwrap_or_default(),
        blocked: if lock.is_open() {
            // Open but never walked into: the one refusal a keypress clears.
            mine.is_none().then_some(TrackBlock::NotEntered)
        } else {
            Some(TrackBlock::Locked(lock))
        },
    };

    UpgradeSubtab {
        header: vec!["Mine".to_owned(), "Track".to_owned(), "Next".to_owned()],
        rows,
        offset: 0,
        detail: UpgradeDetail::Mine(detail),
        footer: SELECT_FOOTER.to_owned(),
    }
}

/// One mine-track row's `Next` cell and its mark.
///
/// **A locked mine and an unvisited one both read [`Mark::NoPrice`]**, for the same
/// reason with two different fixes: neither has a price the player could meet today,
/// so an affordability glyph would be answering a question nobody asked. The `Next`
/// cell is what tells them apart — a level gate prints `Lv 30`, an unvisited mine
/// prints what it *would* cost to grow, since that much is knowable without a grid.
fn track_row(state: &GameState, kind: MineKind, track: MineTrack) -> (String, Mark) {
    let player = state.player();
    let lock = player.mine_lock(kind);
    if let Some(level) = lock.missing_level() {
        return (format!("Lv {level}"), Mark::NoPrice);
    }

    let mine = state.mine(kind);
    let level = mine.map_or(0, |mine| match track {
        MineTrack::Size => mine.get_size_level(),
        MineTrack::Richness => mine.get_richness_level(),
    });
    let next = match track_outcome(track, level) {
        TrackOutcome::Maxed => MAXED.to_owned(),
        TrackOutcome::Size { after: (w, h), .. } => format!("{w}x{h}"),
        // The rung this buy *arrives at*, in the numbering the detail pane's
        // `level 4 → 5` prints — so `level + 1` for the step, then the display shift.
        TrackOutcome::Richness { .. } => shown_rung(level + 1).to_string(),
    };
    let mark = match track_cost(kind, track, level, mine.is_some()) {
        Some(cost) => Mark::of(&economy::affordability(player.get_inventory(), &cost)),
        None => Mark::NoPrice,
    };
    (next, mark)
}

/// What the next level of a track hands over, or that there is no next level.
fn track_outcome(track: MineTrack, level: u32) -> TrackOutcome {
    match track {
        MineTrack::Size => {
            let (before, after) = (Mine::size_for_level(level), Mine::size_for_level(level + 1));
            if before == after {
                TrackOutcome::Maxed
            } else {
                TrackOutcome::Size { before, after }
            }
        }
        MineTrack::Richness if level >= MAX_RICHNESS_LEVEL => TrackOutcome::Maxed,
        // The share the *dial* would then be free to reach — the ceiling being bought
        // is permission, and this is what the permission is worth. Asked of the same
        // pure function the Mines screen draws its bar from, so the two agree.
        MineTrack::Richness => TrackOutcome::Richness {
            before: Mine::value_weight_percent_for(level),
            after: Mine::value_weight_percent_for(level + 1),
        },
    }
}

/// What the next level of a track costs, or [`None`] when there is nothing to sell.
///
/// `entered` is passed rather than looked up so this stays a pure function of the
/// numbers: an unvisited mine is still *priced* — the curve is keyed by the level,
/// which is 0 — and what it is not is *buyable*, which is the pane's business and not
/// the price's.
fn track_cost(kind: MineKind, track: MineTrack, level: u32, entered: bool) -> Option<Cost> {
    if !entered {
        return None;
    }
    match track_outcome(track, level) {
        TrackOutcome::Maxed => None,
        _ => Some(match track {
            MineTrack::Size => economy::mine_size_cost(kind, level),
            MineTrack::Richness => economy::mine_richness_cost(kind, level),
        }),
    }
}

/// The word a track goes by in its own column.
fn track_word(track: MineTrack) -> &'static str {
    match track {
        MineTrack::Size => "Size",
        MineTrack::Richness => "Richness",
    }
}

/// What an enchant does at the level it is held, in prose.
///
/// **Front-end text, like the Mines screen's dial note**, and for the same reason: it
/// is an explanation of a rule rather than the rule, and the core has no business
/// carrying a sentence. The shapes it describes are the core's, though — the bands
/// come from `blast_cells`' own arithmetic, and §5.4.1 is explicit that a pane
/// promising `5x5` at a level that still blasts `3x3` is promising a reward the rules
/// do not pay.
///
/// **Which is why the square is now *asked for*.** This function and `enchant_at_next`
/// each carried their own copy of `1 + 2 * (1 + (level - 1) / 3).min(3)` — the very
/// transcription §5.4.1 warns about, twice. `EnchantType::explosive_side` is that
/// formula's one home; a band that moves in the core now moves here.
fn enchant_effect(kind: EnchantType, level: u8) -> Vec<String> {
    let square = |level: u8| {
        let side = EnchantType::Explosive.explosive_side(level);
        format!("{side}x{side}")
    };
    let lines: Vec<String> = match kind {
        EnchantType::Fortune => vec![format!("multiplies every drop by {}", 1 + u32::from(level))],
        EnchantType::Explosive => vec![
            format!("clears a {} square on a", square(level.max(1))),
            "proc, centred on the cell".to_owned(),
        ],
        EnchantType::Jackhammer => vec![
            "clears one full-width row on a".to_owned(),
            "proc — the mine's own width".to_owned(),
        ],
        EnchantType::Nuke => vec![
            "clears the whole grid on a proc.".to_owned(),
            "The level buys frequency alone.".to_owned(),
        ],
        EnchantType::Excavator => vec![
            "substitutes one Compressed unit".to_owned(),
            "for a block's whole raw drop".to_owned(),
        ],
        EnchantType::Haste => vec![
            "multiplies mining speed, and".to_owned(),
            "the Efficiency bonus with it".to_owned(),
        ],
        EnchantType::Efficiency => Vec::new(),
    };
    lines
}

/// What the next level moves, in numbers.
///
/// **"Procs more often" is not the universal answer**, even though §5.4.1's frame prints
/// it on the row it happened to draw. Three of these six enchants never roll a die:
/// [`EnchantType::proc_permille`] returns `0` for Fortune and Haste (and for Efficiency,
/// which the shop does not price), because they are permanent multipliers rather than
/// chances. So each track names the number it actually moves, and the ones that do roll
/// name both — the shape *and* the frequency, since for Jackhammer and Nuke the shape
/// never changes and the frequency is the whole purchase.
///
/// **Numbers replacing the prose this used to return.** A sentence saying the square
/// "grows every third level" leaves the player to work out whether *this* level is the
/// third; `square 3x3 → 3x3` says it. The one thing a pair cannot say — *why* it stands
/// still — survives in [`enchant_note`].
///
/// **Rates are printed as a percentage, not in permille.** Permille is the core's unit
/// because the proc roll is an integer comparison a save resumes and must not depend on
/// how a division rounded; it is not a unit anyone reads.
///
/// **Takes the two facts it needs, not the [`GameState`] they come from**, which is the
/// same rule `pickaxe_summary` and `boost_view` already follow. Only two of the six
/// tracks reach outside themselves at all — Jackhammer's stripe is the mine's width,
/// Haste's multiplier is worth whatever *this* pickaxe's tier and Efficiency make it —
/// and a signature naming the run would make the test fixture unable to call this
/// without building one.
fn enchant_at_next(
    kind: EnchantType,
    level: u8,
    cap: u8,
    mine_width: u8,
    pickaxe: &Pickaxe,
) -> Vec<StatStep> {
    if level >= cap {
        return Vec::new();
    }
    let next = level + 1;
    let step = |name: &'static str, value: String| StatStep { name, value };
    let procs = || {
        step(
            "procs",
            format!(
                "{} → {}",
                percent(kind.proc_permille(level)),
                percent(kind.proc_permille(next))
            ),
        )
    };

    match kind {
        EnchantType::Fortune => vec![step(
            "drops",
            format!("x{} → x{}", 1 + u32::from(level), 1 + u32::from(next)),
        )],
        EnchantType::Explosive => {
            let side = |level: u8| {
                let side = kind.explosive_side(level);
                format!("{side}x{side}")
            };
            vec![
                step("square", format!("{} → {}", side(level.max(1)), side(next))),
                procs(),
            ]
        }
        // The stripe spans the mine's whole width, so what scales its reach is the *mine*
        // and never the level — the one enchant whose shape is bought on another screen.
        EnchantType::Jackhammer => vec![
            step("row", format!("the mine's width ({mine_width})")),
            procs(),
        ],
        EnchantType::Nuke => vec![step("blast", "the whole grid".to_owned()), procs()],
        EnchantType::Excavator => vec![step("yield", "1 Compressed per proc".to_owned()), procs()],
        // Both rows, because neither alone decides the purchase: the multiplier is what
        // the level literally buys, and the power is what it is worth on *this* pickaxe.
        EnchantType::Haste => {
            vec![
                step(
                    "speed",
                    format!(
                        "x{:.2} → x{:.2}",
                        1.0 + HASTE_PER_LEVEL * f32::from(level),
                        1.0 + HASTE_PER_LEVEL * f32::from(next)
                    ),
                ),
                step(
                    "power",
                    format!(
                        "{:.1} → {:.1}",
                        pickaxe.power_at_haste(level),
                        pickaxe.power_at_haste(next)
                    ),
                ),
            ]
        }
        // Never reached: `cursor::enchant_tracks` filters Efficiency out, since it is a
        // pickaxe upgrade on the ladder rather than an enchant the shop prices.
        EnchantType::Efficiency => Vec::new(),
    }
}

/// A proc rate, as a percentage with one decimal: `0.1%`, `4.0%`, `20.0%`.
///
/// Integer arithmetic on the way in and a float only to print, so the halving is the
/// display's and never the rule's — [`EnchantType::proc_permille`] stays the number the
/// die is actually compared against.
fn percent(permille: u32) -> String {
    format!("{:.1}%", f64::from(permille) / 10.0)
}

/// The one thing [`enchant_at_next`]'s numbers cannot say, when there is one.
///
/// Only Explosive has it: its square stands still on two levels out of three, and
/// `square 3x3 → 3x3` states the fact without stating that it is a *rhythm* rather than
/// a ceiling. Empty everywhere else, and empty on the level where the square does grow —
/// there the pair speaks for itself.
fn enchant_note(kind: EnchantType, level: u8, cap: u8) -> Vec<String> {
    if kind != EnchantType::Explosive || level >= cap {
        return Vec::new();
    }
    if kind.explosive_side(level.max(1)) == kind.explosive_side(level + 1) {
        vec![
            "The square grows every third".to_owned(),
            "level.".to_owned(),
        ]
    } else {
        Vec::new()
    }
}

/// The footer both table sub-tabs share.
const SELECT_FOOTER: &str =
    " ↑↓  select     Enter  buy one level     M  buy to cap     Tab  next screen";

/// The Boost sub-tab's own, which differs in both halves rather than in wording.
///
/// `buy one charge` and not `buy one level`, because a charge has no level; `buy max`
/// and not `buy to cap`, because there is no cap — `M` here stops at an empty purse.
/// Naming what the key actually does is the whole reason this is not [`SELECT_FOOTER`].
const BOOST_FOOTER: &str =
    " ↑↓  select     Enter  buy one charge     M  buy max     Tab  next screen";

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

#[cfg(test)]
/// The rung the fixture's player stands on — dotted `●` in the list.
const CURRENT_RUNG: &str = "Diamond Eff IV";

#[cfg(test)]
/// The rung the fixture's cursor sits on — the tier jump the detail pane warns about.
const SELECTED_RUNG: &str = "Netherite Pickaxe";

#[cfg(test)]
/// The topmost drawn rung at 80×24, which is what makes the counted frame the
/// counted frame: `window(46, 30, 27, 19)` is `27..46`, and row 27 is
/// `Diamond Eff III`, exactly as UI-EN.md §5.5 drew it.
const PICKAXE_OFFSET: usize = 27;

#[cfg(test)]
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
        labels.push(rung_label(current, 0));
        for level in 1..=current.efficiency_cap() {
            labels.push(rung_label(current, level));
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
                i if i <= current => Mark::Owned,
                // Reachable buying every rung from here — the cumulative sense.
                i if i <= selected => Mark::Affordable,
                // The third state: the ore is held, the denomination is not.
                i if i == selected + 1 => Mark::CompressFirst,
                _ => Mark::Refused,
            },
            cursor: index == selected,
            current: index == current,
            cells: vec![text],
        })
        .collect()
}

#[cfg(test)]
/// The three Upgrades sub-tabs as `docs/UI.md` §5.4 draws them, for the frame tests.
///
/// **A fixture and no longer the screen's data**: [`upgrades_view`] projects all three
/// from the run. What survives here is the *rich* save the wireframes were counted
/// against — a level-23 player on a Diamond pickaxe at Efficiency IV, standing one
/// purchase away from a tier jump — which no fresh run can reach and which the layout
/// assertions need in order to have a full ladder, a scrolling table and a dip box to
/// measure.
fn sample_upgrades() -> UpgradesView {
    let pickaxe = UpgradeSubtab {
        header: Vec::new(),
        rows: pickaxe_ladder(),
        offset: PICKAXE_OFFSET,
        detail: UpgradeDetail::Pickaxe(Box::new(PickaxeDetail {
            title: SELECTED_RUNG.to_owned(),
            crosses_tier_jump: true,
            chain: vec!["Diamond Eff V".to_owned(), "Netherite Pickaxe".to_owned()],
            mark: Mark::Affordable,
            // Diamond Efficiency V, then the jump out of Diamond — the two rungs the
            // frame's `Chain  Diamond Eff V + the jump` names, already summed per
            // denomination the way `chain_price` sums a real one.
            costs: sample_price(&[
                (Item::Compressed(Material::Diamond), 2, 2),
                (Item::Compressed(Material::AncientDebris), 4, 4),
                (Item::Raw(Material::AncientDebris), 60, 60),
            ]),
            power: PowerDetail {
                before: 34.0,
                after: 9.0,
                block: Block::AncientDebris,
                ticks_before: Some(27),
                ticks_after: Some(100),
            },
            dip: Some(DipDetail {
                repaid_at: Some(Repaid {
                    rung: "Netherite Eff V".to_owned(),
                    power: 35.0,
                    rungs_later: 5,
                }),
            }),
            unlocks: vec![MineKind::Amethyst],
            ceiling: Some((5, 15)),
            // The fixture's cursor sits two rungs *ahead* of the player, which is what
            // gives it a chain, a price and a dip to draw. An owned rung is the
            // complement of that, so it is `None` here by the same rule `from_state`
            // applies — and `an_owned_rung_and_a_chain_are_never_both_drawn` is what
            // stops the fixture drifting into a state a run cannot reach.
            owned: None,
        })),
        footer: " ↑↓  select     Enter  buy to here     M  buy max     Tab  next screen".to_owned(),
    };

    let enchants = UpgradeSubtab {
        header: vec!["Enchant".to_owned(), "Level".to_owned(), "Cap".to_owned()],
        rows: sample_enchant_rows(),
        // Six tracks, and no terminal this crate will draw into is shorter than the
        // nineteen rows they fit in — so this sub-tab never scrolls and its offset is
        // structurally zero rather than merely happening to be.
        offset: 0,
        detail: UpgradeDetail::Enchant(EnchantDetail {
            kind: EnchantType::Explosive,
            level: 2,
            cap: 6,
            world: World::Nether,
            effect: enchant_effect(EnchantType::Explosive, 2),
            at_next: enchant_at_next(
                EnchantType::Explosive,
                2,
                6,
                SAMPLE_MINE_WIDTH,
                &Pickaxe::default(),
            ),
            note: enchant_note(EnchantType::Explosive, 2, 6),
            cost: sample_price(&[
                (Item::Compressed(Material::Quartz), 3, 3),
                (Item::Raw(Material::Redstone), 40, 40),
            ]),
        }),
        footer: SELECT_FOOTER.to_owned(),
    };

    let mines = UpgradeSubtab {
        header: vec!["Mine".to_owned(), "Track".to_owned(), "Next".to_owned()],
        rows: sample_mine_rows(),
        // Row 6 (`Gold Size`) at the top, which is where the counted frame starts.
        offset: 6,
        detail: UpgradeDetail::Mine(MineTrackDetail {
            kind: MineKind::Obsidian,
            track: MineTrack::Richness,
            level: (6, 7),
            at_next: TrackOutcome::Richness {
                before: 66,
                after: 73,
            },
            cost: sample_price(&[
                (Item::Compressed(Material::Obsidian), 2, 0),
                (Item::Raw(Material::CryingObsidian), 40, 2),
            ]),
            blocked: None,
        }),
        footer: SELECT_FOOTER.to_owned(),
    };

    // The one sub-tab whose fixture is *almost* the real thing: every figure in it but
    // the reserve and the holding is a tunable, so the sample can only differ from a
    // run in what the player has banked.
    let boost = UpgradeSubtab {
        header: vec!["Item".to_owned(), "Reserve".to_owned()],
        rows: vec![UpgradeRow {
            cells: vec!["Redstone boost".to_owned(), "3 held".to_owned()],
            mark: Mark::Affordable,
            cursor: true,
            current: false,
        }],
        offset: 0,
        detail: UpgradeDetail::Boost(BoostDetail {
            // Quoted in Compressed units, which is what `Cost::single` normalises a
            // 300-raw price into — the fixture would otherwise show a denomination the
            // till never asks for.
            price: sample_price(&[(Item::Compressed(Material::Redstone), 3, 12)]),
            multiplier: BOOST_MULTIPLIER,
            seconds: 30,
            reserve: 3,
        }),
        footer: BOOST_FOOTER.to_owned(),
    };

    UpgradesView {
        active: UpgradeTab::Pickaxe,
        pickaxe,
        enchants,
        mines,
        boost,
    }
}

#[cfg(test)]
/// The mine width the fixture's Jackhammer row is quoted against.
///
/// The §5.4 frame's player stands in a `12x7` mine, which is the number
/// [`enchant_at_next`] would read off a real run — and the reason that function takes
/// the width rather than the run it comes from.
const SAMPLE_MINE_WIDTH: u8 = 12;

#[cfg(test)]
/// A fixture price: `(item, needed, held)` per line, verdicted the way
/// [`price_lines`] verdicts a real one.
///
/// **The mark is derived, never written down**, so a fixture cannot state a green line
/// the numbers beside it contradict — the one failure mode a hand-built price has that a
/// projected one does not.
fn sample_price(lines: &[(Item, u32, u32)]) -> Vec<PriceLine> {
    lines
        .iter()
        .map(|&(item, needed, held)| PriceLine {
            item,
            needed,
            held,
            mark: if held >= needed {
                Mark::Affordable
            } else {
                Mark::Refused
            },
        })
        .collect()
}

#[cfg(test)]
/// The six enchant rows the §5.4.1 frame draws, at the levels it draws them.
fn sample_enchant_rows() -> Vec<UpgradeRow> {
    let rows: &[(EnchantType, u8, u8, Mark)] = &[
        (EnchantType::Fortune, 3, 10, Mark::Affordable),
        (EnchantType::Explosive, 2, 6, Mark::Affordable),
        (EnchantType::Jackhammer, 1, 6, Mark::CompressFirst),
        (EnchantType::Nuke, 0, 6, Mark::Refused),
        (EnchantType::Excavator, 1, 6, Mark::Refused),
        (EnchantType::Haste, 0, 6, Mark::Refused),
    ];
    rows.iter()
        .map(|&(kind, level, cap, mark)| UpgradeRow {
            cells: vec![
                kind.name().to_owned(),
                format!("{} → {}", level_word(level), roman(level + 1)),
                format!("{cap}/{}", World::End.enchant_cap()),
            ],
            mark,
            cursor: kind == EnchantType::Explosive,
            current: false,
        })
        .collect()
}

#[cfg(test)]
/// The twenty-four mine-track rows of the §5.4.2 frame.
fn sample_mine_rows() -> Vec<UpgradeRow> {
    // `(mine, size next, size mark, richness next, richness mark)` — **two marks per
    // mine, not one**: a maxed size track has no price to quote while the richness
    // track beside it may be a purchase away, and one shared mark would have the
    // fixture asserting a state the rules cannot produce.
    let tracks: &[(MineKind, &str, Mark, &str, Mark)] = &[
        (MineKind::Stone, MAXED, Mark::NoPrice, MAXED, Mark::NoPrice),
        (
            MineKind::Coal,
            "20x10",
            Mark::CompressFirst,
            "8",
            Mark::CompressFirst,
        ),
        (MineKind::Iron, MAXED, Mark::NoPrice, "1", Mark::Affordable),
        (
            MineKind::Gold,
            "12x7",
            Mark::CompressFirst,
            "3",
            Mark::CompressFirst,
        ),
        (MineKind::Lapis, "10x6", Mark::Refused, "2", Mark::Refused),
        (
            MineKind::Redstone,
            "8x5",
            Mark::Affordable,
            "1",
            Mark::Affordable,
        ),
        (MineKind::Emerald, "8x5", Mark::Refused, "1", Mark::Refused),
        (MineKind::Diamond, "10x6", Mark::Refused, "2", Mark::Refused),
        (MineKind::Quartz, "10x6", Mark::Refused, "4", Mark::Refused),
        (
            MineKind::AncientDebris,
            "8x5",
            Mark::Affordable,
            "1",
            Mark::Affordable,
        ),
        (
            MineKind::Obsidian,
            "10x6",
            Mark::Refused,
            "7",
            Mark::Refused,
        ),
        (
            MineKind::Amethyst,
            "Lv 30",
            Mark::NoPrice,
            "Lv 30",
            Mark::NoPrice,
        ),
    ];
    tracks
        .iter()
        .flat_map(|&(kind, size, size_mark, richness, richness_mark)| {
            [
                (MineTrack::Size, size, size_mark),
                (MineTrack::Richness, richness, richness_mark),
            ]
            .map(|(track, next, mark)| UpgradeRow {
                cells: vec![
                    kind.name().to_owned(),
                    track_word(track).to_owned(),
                    next.to_owned(),
                ],
                mark,
                cursor: kind == MineKind::Obsidian && track == MineTrack::Richness,
                current: false,
            })
        })
        .collect()
}

#[cfg(test)]
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
    /// The power the same save mines at — [`View::sample`]'s own `25.0`, a Diamond
    /// with Efficiency IV.
    ///
    /// Written down rather than read off a [`Pickaxe`], because this fixture builds a
    /// *read model* and never a run: there is no pickaxe here to ask. Keeping it
    /// beside [`TIER`] is what stops the pane from quoting a break time some other
    /// pickaxe would take.
    const POWER: f32 = 25.0;

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

    // The tier gate spelled out rather than asked of `Pickaxe::can_mine`, for the
    // reason `POWER` is a constant: the fixture has no pickaxe. It is the same
    // comparison, against the same tier every lock above is derived from.
    let cost = |block: Block| BlockCostView {
        block,
        ticks: (TIER >= block.min_pickaxe_tier())
            .then(|| block.ticks_to_break(POWER))
            .flatten(),
    };

    MinesView {
        rows,
        selected: MineKind::Obsidian,
        detail: MineDetail {
            blocks: [
                cost(MineKind::Obsidian.common_block()),
                cost(MineKind::Obsidian.value_block()),
            ],
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

#[cfg(test)]
/// The Inventory table and compress panel drawn in UI.md §5.3, from the frame.
///
/// The counts are fixture data; the compress panel's derived numbers (value,
/// compressible-now) are computed in the screen, not stored here.
///
/// **The one thing a real run cannot produce, and the reason this fixture outlived
/// the wiring**: the frame is drawn **mid-refusal**. Iron is selected, worth 680 and
/// short the Compressed denomination an upgrade wants — a state only `Enter` on the
/// Upgrades screen can reach, which is phase 6. [`inventory_view`] therefore answers
/// [`None`] for the hint on every run, and this fixture is where the panel's refusal
/// half stays testable until then.
fn sample_inventory() -> InventoryView {
    // `(material, compressed, raw)`. The order is `Material::ALL`'s — the same order
    // the frame lists and the table prints — so this is a column of counts against
    // that table rather than a second copy of it.
    let counts = [
        (12, 4_508),
        (3, 871),
        (2, 480),
        (0, 312),
        (1, 44),
        (0, 128),
        (0, 17),
        (0, 9),
        (2, 340),
        (0, 73),
        (4, 60),
        (0, 21),
        (0, 2),
        (0, 0),
        (0, 38),
    ];
    let rows = Material::ALL
        .into_iter()
        .zip(counts)
        .map(|(material, (compressed, raw))| InvRow {
            material,
            compressed,
            raw,
        })
        .collect();

    InventoryView {
        rows,
        // Iron: the row the frame highlights and the compress panel details.
        selected: Material::Iron,
        hint: Some(CompressHint {
            purchase: "Efficiency V".to_owned(),
            // 650 raw, which `CostLine` splits into the frame's `6 Compressed + 50`.
            needed: CostLine::from_raw_total(Material::Iron, 650),
        }),
    }
}

#[cfg(test)]
/// The Stats panels at the run UI.md §5.5 is drawn at: level 23, mid-climb, rank II.
///
/// **The history is no longer transcribed from the frame, and that is a correction
/// rather than a liberty.** §5.5 draws invented, abbreviated lines (`+80 A. Debris`,
/// `Explosive — 9 blocks cleared`) that nothing in the code produces; the sentences
/// below are the ones [`announce::of`](crate::announce::of) and the purchase paths
/// actually word. Two of the frame's lines are gone entirely — entering a mine and
/// moving the richness dial raise no announcement at all — and the rest are longer
/// than the frame made them look, which is what the panel now has to survive.
/// `docs/UI.md` §5.5.2.
fn sample_stats() -> StatsView {
    // `(done, current, text, detail)` for each "This run" row, at level 23 with a
    // Diamond pickaxe: the three cleared goals, the one in progress, and four ahead.
    let milestones = [
        (true, false, "Break your first block", ""),
        (true, false, "Reach the Nether", "Lv 15"),
        (true, false, "Diamond pickaxe", ""),
        (false, true, "Reach the End", "Lv 30    23/30"),
        (false, false, "Netherite pickaxe", ""),
        (false, false, "Instamine Obsidian", "25 / 50"),
        // **No `✓` in the detail**, against the frame, which draws one on a row it
        // leaves un-ticked — and `20x10 R9` *is* a maxed mine, so the two contradict
        // each other. The row's own mark is the only one that answers.
        (false, false, "Max out a mine", "Iron 12x7 R3"),
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

    // `(age in seconds, the sentence)`, newest first — the shape `Toasts::log` hands
    // over. The ages climb so the column shows all three of its units.
    let history = [
        (95, "Excavator!  +1 Compressed Iron"),
        (170, "Explosive — 9 blocks"),
        (200, "Mine refilled"),
        (
            860,
            "Level 23 — +115 Quartz, +80 Ancient Debris — claim on 6",
        ),
        (1_020, "Not enough Iron: 6 Compressed short"),
        (1_400, "Jackhammer — 8 blocks"),
        (2_300, "Bought Diamond Pickaxe Efficiency IV"),
        (3_100, "Redstone boost ended"),
        (4_700, "Claimed Lv 22 — +110 Quartz, +77 Ancient Debris"),
        (5_200, "Mine refilled"),
        // Past the ten rows the counted frame had room for. They change nothing at
        // 80×24 — the window still starts at zero and still ends after eleven — and
        // are what a taller Stats panel now has to show.
        (6_000, "Nuke — 21 blocks"),
        (6_400, "Mine refilled"),
        (
            7_100,
            "Level 22 — +110 Quartz, +77 Ancient Debris — claim on 6",
        ),
        (7_900, "Bought Fortune III"),
        (8_800, "Excavator!  +1 Compressed Obsidian"),
        (9_600, "Explosive — 9 blocks"),
        (10_400, "Mine refilled"),
        (11_900, "Bought Netherite Pickaxe"),
        (13_100, "Claimed Lv 21 — +105 Quartz, +73 Ancient Debris"),
        (14_000, "Jackhammer — 8 blocks"),
        (15_500, "Mine refilled"),
        (
            61_000,
            "Level 21 — +105 Quartz, +73 Ancient Debris — claim on 6",
        ),
        (95_000, "Mine refilled"),
    ]
    .into_iter()
    .map(|(age_secs, text)| HistoryEntry {
        age_secs,
        text: Rc::from(text),
    })
    .collect();

    StatsView {
        blocks_broken: 418_297,
        playtime: "14h 22m".to_owned(),
        this_run: "3h 07m".to_owned(),
        milestones,
        history,
        selected: 0,
    }
}

#[cfg(test)]
/// The prestige trade the §6.8 frame is drawn at: rank II, unaffordable, mid-climb.
///
/// Transcribed rather than invented, and it agrees with the §5.5 frame beside it
/// because both are now one struct — which is the property
/// [`prestige_view`] exists to make structural. Rank II is deliberately *not* rank 0:
/// the fixture's job is to draw the box with something in every line, and a run that
/// has never prestiged has no `Rank … → …` to show.
fn sample_prestige() -> PrestigeView {
    let cost = 6_540;
    PrestigeView {
        rank: 2,
        multiplier_permille: 1_200,
        next_multiplier_permille: 1_300,
        material: Material::Amethyst,
        cost,
        held: 0,
        held_compressed: 0,
        held_raw: 0,
        // The frame's own `✗`: the player holds nothing, so the ore is missing
        // outright rather than being held in the wrong denomination.
        verdict: Affordability::Insufficient(vec![skylode_core::economy::Shortfall {
            item: Item::Raw(Material::Amethyst),
            needed: cost,
            held: 0,
        }]),
        lock: prestige::lock(23, PickaxeTier::Diamond),
        tier: PickaxeTier::Diamond,
        efficiency: 4,
        fortune: 3,
        // The frame's `All 5 enchants → 0`, the five that are neither of the two above.
        other_enchants: 5,
        level: 23,
    }
}

/// The Stats screen's three panels, drawn from the run and the announcement log.
///
/// **`now` and the log are parameters rather than something read off `state`**, for
/// the reason [`View::from_state`]'s `refused` is one: the run does not know what was
/// announced. An announcement changes no game state — that is exactly what makes it
/// the front-end's to keep — and an age needs a clock the core is forbidden.
fn stats_view(state: &GameState, cursors: Cursors, toasts: &Toasts, now: Instant) -> StatsView {
    StatsView {
        blocks_broken: state.blocks_broken(),
        playtime: duration_hm(state.playtime_ticks()),
        this_run: duration_hm(state.run_playtime_ticks()),
        milestones: milestones(state),
        history: toasts
            .log(now)
            .map(|(age_secs, text)| HistoryEntry { age_secs, text })
            .collect(),
        selected: cursors.history,
    }
}

/// The eight rows of the `This run` panel (UI.md §5.5).
///
/// **Run progress, not achievements, and every row is a pure predicate.** Nothing here
/// is stored: the save carries no "ever achieved" bitset, so the panel resets with a
/// prestige — which is honest, because that is what it claims to be. A panel called
/// *Milestones* that un-ticks would be broken; one called *This run* that un-ticks is
/// working.
///
/// **`current` is the first row that is not done**, in list order and not in order of
/// difficulty. The rows are not monotone — a Netherite pickaxe is reachable before the
/// End is — so "the next one you will clear" would need an ordering nothing in the
/// game defines, where "the first one still open" needs only the list the panel draws.
fn milestones(state: &GameState) -> Vec<Milestone> {
    let player = state.player();
    let level = player.get_level();
    let tier = player.get_pickaxe().get_tier();
    let power = player.get_pickaxe().mining_power();
    let obsidian = Block::Obsidian.hardness();
    let maxed = most_advanced_mine(state);

    // `(done, text, detail)`. The detail is the row's evidence, and it is written for
    // the row rather than derived from a rule: the frame shows a distance on the two
    // numeric goals, a threshold on the two world ones, and the frontrunner on the mine
    // one — three different kinds of answer to three different kinds of question.
    let rows: [(bool, String, String); 8] = [
        // **The one row with no counter behind it.** `blocks_broken` is a *lifetime*
        // total, so it would stay ticked across a prestige and make a panel headed
        // `This run` state a fact about a run that ended. Experience answers it
        // instead, and answers it exactly: the auto-miner grants none — a contract the
        // core states — so any XP at all means this run has swung at something, and a
        // prestige clears it. The `level > 1` half is for the instant after a level-up,
        // where the counter is back at zero and a great deal has been broken.
        (
            level > 1 || player.get_experience() > 0,
            "Break your first block".to_owned(),
            String::new(),
        ),
        world_row(World::Nether, level),
        (
            tier >= PickaxeTier::Diamond,
            "Diamond pickaxe".to_owned(),
            String::new(),
        ),
        world_row(World::End, level),
        (
            tier >= PickaxeTier::Netherite,
            "Netherite pickaxe".to_owned(),
            String::new(),
        ),
        // **Base power, with no boost and no prestige on it.** An instamine that lasts
        // as long as a ten-minute charge is not a threshold the run has crossed, and a
        // row that ticked and un-ticked with a timer would be reporting the boost.
        (
            power >= obsidian,
            "Instamine Obsidian".to_owned(),
            format!("{power:.0} / {obsidian:.0}"),
        ),
        (
            maxed.as_ref().is_some_and(|&(_, _, both)| both),
            "Max out a mine".to_owned(),
            maxed.map(|(label, _, _)| label).unwrap_or_default(),
        ),
        (
            level >= LEVEL_CAP,
            format!("Reach mining level {LEVEL_CAP}"),
            format!("{level}/{LEVEL_CAP}"),
        ),
    ];

    // The `▸` goes on the first row still open. `position` answers `None` when every
    // row is done, and then no row is current — which is the right reading of a
    // finished panel rather than a mark stranded on the last line.
    let current = rows.iter().position(|(done, _, _)| !done);
    rows.into_iter()
        .enumerate()
        .map(|(index, (done, text, detail))| Milestone {
            done,
            current: current == Some(index),
            text,
            // A done row keeps its threshold and drops its distance: `23/30` on a goal
            // already cleared is a number counting towards something that has happened.
            detail: if done && detail.contains('/') {
                String::new()
            } else {
                detail
            },
        })
        .collect()
}

/// One of the two world rows: the threshold it opens at, and how far off it is.
fn world_row(world: World, level: u32) -> (bool, String, String) {
    let unlock = world.unlock_level();
    (
        world.is_unlocked_at(level),
        format!("Reach the {}", world.name()),
        format!("Lv {unlock}    {level}/{unlock}"),
    )
}

/// The mine this run has taken furthest, its label, and whether it is maxed on **both**
/// tracks.
///
/// **An unvisited mine has no state and counts as level 0**, which falls out of the
/// lazy creation rather than being special-cased: [`GameState::mine`] answers [`None`]
/// for a mine never entered, and a mine never entered has bought nothing.
///
/// "Furthest" is the sum of the two levels, ties going to whichever
/// [`MineKind::ALL`] lists first. A sum rather than a lexicographic order because the
/// two tracks are bought against each other — a player who put everything into size is
/// as far along as one who split it — and because the row is evidence, not a ranking.
fn most_advanced_mine(state: &GameState) -> Option<(String, u32, bool)> {
    MineKind::ALL
        .into_iter()
        .filter_map(|kind| state.mine(kind).map(|mine| (kind, mine)))
        .map(|(kind, mine)| {
            let (width, height) = mine.get_size();
            let richness = mine.get_richness_level();
            (
                format!("{} {width}x{height} R{richness}", kind.name()),
                mine.get_size_level() + richness,
                mine.is_size_maxed() && mine.is_richness_maxed(),
            )
        })
        .max_by_key(|&(_, reach, _)| reach)
}

/// Everything §6.8 and §5.5 say about the prestige trade, read off the run.
///
/// **Takes the [`Player`] and not the [`GameState`]**, because that is the whole of
/// what the trade is about: the rank, the two gates, the pickaxe, the enchants, the
/// level and the inventory that pays. Nothing here asks about a mine, and a projection
/// that took the run would be able to.
///
/// The verdict comes from [`economy::affordability`] rather than from `held >= cost`,
/// for the reason the Upgrades pane reads the same function: a prestige is paid as a
/// [`Cost`] in two denominations, so a player holding 6 540 raw Amethyst is genuinely
/// refused, and a comparison of totals would print a `✓` the till then contradicts.
fn prestige_view(player: &Player) -> PrestigeView {
    let rank = player.get_prestige();
    let cost = prestige::cost(rank);
    // A prestige is quoted in exactly one material, so the total is the one line's.
    // `map_or` rather than an index: the lints here forbid the panic a `[0]` would be.
    let total = cost
        .lines()
        .first()
        .map_or(0, |line| line.compressed * RAW_PER_COMPRESSED + line.raw);
    let material = cost
        .lines()
        .first()
        .map_or(Material::Amethyst, |line| line.material);

    let pickaxe = player.get_pickaxe();
    let enchants = pickaxe.enchants();
    PrestigeView {
        rank,
        multiplier_permille: prestige::multiplier_permille(rank),
        // `rank + 1` cannot overflow in play and saturates if it ever could: the rank
        // is unbounded by design, so the arithmetic has to be total rather than
        // relying on nobody reaching the top of a `u32`.
        next_multiplier_permille: prestige::multiplier_permille(rank.saturating_add(1)),
        material,
        cost: total,
        held: player.get_inventory().raw_value(material),
        held_compressed: player.get_inventory().count(Item::Compressed(material)),
        held_raw: player.get_inventory().count(Item::Raw(material)),
        verdict: economy::affordability(player.get_inventory(), &cost),
        lock: player.prestige_lock(),
        tier: pickaxe.get_tier(),
        efficiency: enchants.get_level(EnchantType::Efficiency),
        fortune: enchants.get_level(EnchantType::Fortune),
        other_enchants: enchants
            .iter()
            .filter(|(kind, level)| {
                *level > 0 && !matches!(kind, EnchantType::Efficiency | EnchantType::Fortune)
            })
            .count(),
        level: player.get_level(),
    }
}

#[cfg(test)]
/// The topmost drawn level at 80×24 — index 12, which is level 13.
///
/// That is the row UI.md §5.6 starts its window on, and `window(50, 22, 12, 19)` is
/// `12..31`: levels 13..=31, the counted frame exactly.
const LEVELS_OFFSET: usize = 12;

#[cfg(test)]
/// The **whole** Levels roadmap, `1..=LEVEL_CAP`.
///
/// It used to be the window UI.md §5.6 drew — levels 13..=31 — because the view
/// decided what fit. It no longer does, so this is the full ladder and the screen
/// windows it; [`LEVELS_OFFSET`] is what keeps 13 at the top at 80×24.
///
/// **Two sources, deliberately not merged.** The nineteen levels the wireframe
/// counted stay *verbatim* below, so the frame can still be compared row for row
/// against the document; every other level gets a generated filler line, because the
/// wireframe never drew them and generating the *real* bundle here would make the
/// fixture agree with `from_state` by construction and so test nothing. `xp` is
/// `level × 100` throughout, which is the curve the counted rows already follow.
///
/// **Three of the fixture's levels have a reward waiting**, chosen to be the three
/// states the mark column has to tell apart at once: one behind the player (`~` where
/// a plain row would read `✓`), one *on* the player's level (`▸●` must still win), and
/// one at the very top of the drawn window. A fixture with nothing waiting would draw
/// a screen no player reaches after their first level-up.
fn sample_levels() -> LevelsView {
    let counted = counted_levels();
    let rows: Vec<LevelRow> = (1..=LEVEL_CAP)
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
                xp: (level < LEVEL_CAP).then(|| row.map_or(level * 100, |(_, _, xp)| *xp)),
                unclaimed: SAMPLE_WAITING.contains(&level),
            }
        })
        .collect();
    LevelsView {
        rows,
        offset: LEVELS_OFFSET,
        // The fixture's player is on level 23, and a cursor opens where the player is.
        selected: 23,
        waiting: SAMPLE_WAITING.len(),
    }
}

#[cfg(test)]
/// The fixture's uncollected levels — see [`sample_levels`] for why these three.
const SAMPLE_WAITING: [u32; 3] = [13, 21, 23];

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

    use skylode_core::{save, tunables::BOOST_MULTIPLIER};

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
        View::from_state(
            state,
            &Config::default(),
            Cursors::new(
                state.current_mine().kind(),
                upgrade::position(&upgrade::ladder(), state.player().get_pickaxe()),
                state.player().get_level(),
            ),
            None,
            &Toasts::new(),
            &Flashes::new(),
            Instant::now(),
        )
    }

    /// A run further along than a test could plausibly *play* to, built by writing a
    /// save and reading it back with a few fields rewritten.
    ///
    /// **This is a door, not a back door.** A front-end cannot mint a pickaxe tier or
    /// a pile of ore — `Player::inventory_mut` and `Enchants::upgrade` are
    /// `pub(crate)` precisely so it cannot — and reaching Netherite by mining would
    /// take a test the length of the game. What it *can* do is what it will do every
    /// launch from phase 7 on: hand [`save::from_json`] a document. That path
    /// [validates](GameState) before it returns, so a patch describing a run the rules
    /// could not produce is refused here rather than silently projected. The config is
    /// `()` so this crate needs no serde dependency of its own.
    ///
    /// Patches are applied in order and each must match, so a rename in the save format
    /// fails these tests loudly instead of quietly leaving the field alone. The standing
    /// mine's grid is then **re-cut to whatever `size_level` now says**, because that
    /// pairing is an invariant `Mine::validate` enforces: a patch that grows the mine
    /// would otherwise be refused for a reason that has nothing to do with the test.
    fn veteran(patches: &[(&str, &str)]) -> GameState {
        let mut text = match save::to_json(&fresh_run(), &()) {
            Ok(text) => text,
            Err(error) => unreachable!("a fresh run must serialise: {error:?}"),
        };
        for (from, to) in patches {
            assert!(text.contains(from), "the save no longer contains {from:?}");
            text = text.replacen(from, to, 1);
        }
        text = recut_grid(&text);
        match save::from_json::<()>(&text) {
            Ok(save) => save.state,
            Err(error) => unreachable!("a patched save must still be legal: {error:?}"),
        }
    }

    /// Replaces the standing mine's `grid` with a rectangle of the size its
    /// `size_level` implies, filled with the mine's common block.
    ///
    /// Cut out by its two delimiters rather than matched literally: the grid a fresh
    /// run draws depends on the seed, so a literal would pin the RNG in a test that is
    /// not about it.
    fn recut_grid(text: &str) -> String {
        let level = text
            .split_once(r#""size_level":"#)
            .and_then(|(_, rest)| rest.split_once(','))
            .and_then(|(level, _)| level.parse::<u32>().ok())
            .unwrap_or_default();
        let (head, tail) = match (text.find(r#""grid":"#), text.find(r#","break_progress""#)) {
            (Some(start), Some(end)) => (&text[..start], &text[end..]),
            _ => return text.to_owned(),
        };
        let (width, height) = Mine::size_for_level(level);
        let row = format!("[{}]", vec![r#""Stone""#; usize::from(width)].join(","));
        format!(
            r#"{head}"grid":[{}]{tail}"#,
            vec![row; usize::from(height)].join(",")
        )
    }

    /// The cursors a test needs when it is *about* the Upgrades screen.
    fn upgrading(state: &GameState, tab: UpgradeTab) -> Cursors {
        let mut cursors = Cursors::new(
            state.current_mine().kind(),
            upgrade::position(&upgrade::ladder(), state.player().get_pickaxe()),
            state.player().get_level(),
        );
        cursors.upgrade_tab = tab;
        cursors
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

    /// **The one field of the read model that comes from the preferences.**
    ///
    /// Asserted on *both* answers rather than on the non-default one alone: a
    /// projection that had simply swapped one hard-wired constant for the other would
    /// pass a single assertion and still be ignoring what it was handed.
    #[test]
    fn the_colour_preference_is_what_the_read_model_carries() {
        let state = fresh_run();
        for colour in [ColourMode::Ansi256, ColourMode::Ansi16] {
            let config = Config {
                colour,
                ..Config::default()
            };
            let view = View::from_state(
                &state,
                &config,
                Cursors::new(MineKind::Stone, 0, 1),
                None,
                &Toasts::new(),
                &Flashes::new(),
                Instant::now(),
            );
            assert_eq!(view.colour_mode, colour);
        }
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
    fn a_mines_two_blocks_are_costed_against_the_pickaxe_that_would_dig_them() {
        // A fresh run carries a bare Wooden pickaxe: power 2.0, and the tier that
        // opens Stone and nothing beyond it. That is what makes one run answer both
        // halves — the Stone mine is costed, the Iron mine is refused.
        let state = fresh_run();
        let looking_at = |kind| Cursors::new(kind, 0, state.player().get_level());

        let stone = mines_view(&state, looking_at(MineKind::Stone)).detail;
        assert_eq!(
            stone.blocks[0].block,
            Block::Stone,
            "the common block first"
        );
        assert_eq!(
            stone.blocks[1].block,
            Block::Cobblestone,
            "then the value one"
        );
        // `ceil(30 × 1.5 / 2.0)` and `ceil(30 × 2.0 / 2.0)` — computed through
        // `Block::ticks_to_break`, so the pane and `Mine::dig` cannot disagree about
        // how long a swing takes.
        assert_eq!(stone.blocks[0].ticks, Some(23));
        assert_eq!(stone.blocks[1].ticks, Some(30));

        let iron = mines_view(&state, looking_at(MineKind::Iron)).detail;
        assert_eq!(iron.blocks[0].block, Block::IronOre);
        // Iron Ore's `min_pickaxe_tier` is Stone, so this is the first rock a fresh
        // run may not touch. A refusal is total, never a slowdown — which is why the
        // answer is `None` and not the 45 ticks the arithmetic would happily give.
        assert_eq!(iron.blocks[0].ticks, None);
        assert_eq!(iron.blocks[1].ticks, None);
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
    fn a_stacked_boost_measures_against_its_own_granted_time() {
        // **`div_ceil`, not `/`.** One tick left is a boost the player still holds,
        // and flooring would print `0s` for a twentieth of a second — the gauge
        // announcing an expiry that has not happened.
        let one_tick = boost_view(1, BOOST_DURATION_TICKS, BOOST_MULTIPLIER);
        assert_eq!(one_tick.seconds, 1);

        let full = boost_view(BOOST_DURATION_TICKS, BOOST_DURATION_TICKS, BOOST_MULTIPLIER);
        assert_eq!(
            u64::from(full.seconds),
            u64::from(BOOST_DURATION_TICKS) / TICKS_PER_SECOND
        );
        assert!((full.ratio - 1.0).abs() < f32::EPSILON);

        // Two charges fired at once: sixty seconds, and the bar opens full rather
        // than at a clamped 200 %.
        let stacked = boost_view(
            2 * BOOST_DURATION_TICKS,
            2 * BOOST_DURATION_TICKS,
            BOOST_MULTIPLIER,
        );
        assert_eq!(u64::from(stacked.seconds), 60);
        assert!((stacked.ratio - 1.0).abs() < f32::EPSILON);

        // The case the old constant denominator got wrong, and the reason this test
        // was renamed: halfway through a two-charge boost the bar must read half.
        // Against `BOOST_DURATION_TICKS` it read 1.0, so the gauge sat pinned at full
        // for the entire first charge and only started moving in the second half.
        let halfway = boost_view(
            BOOST_DURATION_TICKS,
            2 * BOOST_DURATION_TICKS,
            BOOST_MULTIPLIER,
        );
        assert_eq!(u64::from(halfway.seconds), 30);
        assert!(
            (halfway.ratio - 0.5).abs() < f32::EPSILON,
            "a 60 s boost with 30 s left read {}",
            halfway.ratio,
        );

        // Never out of range, so `LineGauge`'s clamp is a guard and not a repair.
        for remaining in [0, 1, BOOST_DURATION_TICKS, 3 * BOOST_DURATION_TICKS] {
            let ratio = boost_view(remaining, 3 * BOOST_DURATION_TICKS, BOOST_MULTIPLIER).ratio;
            assert!(
                (0.0..=1.0).contains(&ratio),
                "{remaining} ticks gave {ratio}"
            );
        }
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

    /// **Both halves of the boost are projected, and from the run rather than each
    /// other.** The reserve is the number `b` spends and the shop adds to, and it is
    /// the one that survives a boost lapsing — so a projection that read it off
    /// `active_boost` would report nothing in exactly the state where the player has
    /// charges waiting and none running.
    #[test]
    fn the_reserve_is_projected_whether_or_not_a_boost_is_running() {
        let mut state = GameState::new(0x5B1_0DE, std::time::UNIX_EPOCH);
        state.dev_grant_boost_charges(3);

        let idle = projected(&state);
        assert_eq!(idle.boost_charges, 3);
        assert!(idle.boost.is_none(), "nothing was fired");

        if let Err(refusal) = state.fire_boost() {
            unreachable!("a granted charge must be spendable: {refusal:?}");
        }
        let running = projected(&state);
        assert_eq!(
            running.boost_charges, 2,
            "the fired charge is still counted"
        );
        assert!(running.boost.is_some());
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
        let track = view.upgrades.mines.rows.iter().find(|row| {
            row.cells.first().is_some_and(|name| name == prefix)
                && row.cells.get(1).is_some_and(|track| track == "Size")
        });
        assert_eq!(
            track.map(|row| (row.cells.get(2).map(String::as_str), row.mark)),
            Some((Some(MAXED), Mark::NoPrice)),
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
            ladder
                .first()
                .and_then(|row| row.cells.first())
                .map(String::as_str),
            Some("Wooden Pickaxe")
        );
        assert_eq!(
            ladder
                .last()
                .and_then(|row| row.cells.first())
                .map(String::as_str),
            Some("Netherite Eff XV")
        );
        assert_eq!(
            ladder
                .get(PICKAXE_OFFSET)
                .and_then(|row| row.cells.first())
                .map(String::as_str),
            Some("Diamond Eff III"),
            "the counted window no longer starts where UI-EN.md §5.5 drew it"
        );
    }

    #[test]
    fn the_levels_roadmap_is_the_whole_ladder_and_keeps_its_counted_rows() {
        // The full 1..=LEVEL_CAP, with the wireframe's own rows still verbatim
        // inside it — that pairing is the reason `counted_levels` exists at all.
        let levels = sample_levels();
        assert_eq!(levels.rows.len(), LEVEL_CAP as usize);
        let level_23 = levels.rows.iter().find(|row| row.level == 23);
        assert_eq!(
            level_23.map(|row| row.grants.as_str()),
            Some("+115 Quartz, +80 A. Debris, +34 Obsidian")
        );
        assert_eq!(level_23.map(|row| row.xp), Some(Some(2_300)));
        // The last rung has no next level to price, so its requirement is absent
        // rather than a number nothing is for sale at.
        assert_eq!(
            levels.rows.last().map(|row| row.xp),
            Some(None),
            "the capped row quoted an XP requirement"
        );
    }

    #[test]
    fn a_fresh_runs_ladder_opens_on_the_rung_the_player_stands_on() {
        let state = fresh_run();
        let view = projected(&state);
        let pickaxe = &view.upgrades.pickaxe;

        // Rung 0 is the bare Wooden pickaxe every run starts on, and it is `current`
        // rather than for sale — `Mark::Owned` is what "nothing to buy" looks like.
        assert_eq!(pickaxe.rows.len(), upgrade::ladder().len());
        assert!(pickaxe.rows.first().is_some_and(|row| row.current));
        match &view.upgrades.active_subtab().detail {
            UpgradeDetail::Pickaxe(detail) => {
                assert_eq!(detail.title, "Wooden Pickaxe");
                assert!(detail.chain.is_empty());
                assert_eq!(detail.mark, Mark::Owned);
                assert!(detail.dip.is_none());
            }
            other => unreachable!("the Pickaxe sub-tab must project a pickaxe: {other:?}"),
        }
    }

    #[test]
    fn the_reachability_marks_of_a_real_ladder_form_a_contiguous_prefix() {
        // The invariant the whole chain-simulation design exists to earn, carried from
        // the fixture to a run: paying for a rung cannot make the next one *more*
        // affordable, so once the column leaves `✓` it never comes back. A hole here
        // would be a bug in `chain_affordability`, not a rendering oddity.
        let state = veteran(&[(
            r#""inventory":{}"#,
            r#""inventory":{"compressed_stone":40,"stone":30}"#,
        )]);
        let view = projected(&state);

        let mut left_ticks = false;
        for row in &view.upgrades.pickaxe.rows {
            match row.mark {
                Mark::Affordable => assert!(!left_ticks, "a ✓ came back: {:?}", row.cells),
                Mark::CompressFirst | Mark::Refused => left_ticks = true,
                Mark::Owned | Mark::NoPrice => {}
            }
        }
        assert!(left_ticks, "a 40-Compressed purse must run out somewhere");
    }

    #[test]
    fn a_chain_through_a_tier_jump_names_the_ceiling_it_raises_and_the_mines_it_opens() {
        // A Diamond pickaxe at its Efficiency cap, aimed at the Netherite rung: the one
        // purchase in the game that is worth *less* power than what it replaces, and so
        // the one that has to say what it buys instead.
        let state = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Diamond""#),
            (r#""enchants":{}"#, r#""enchants":{"Efficiency":5}"#),
        ]);
        let ladder = upgrade::ladder();
        let here = upgrade::position(&ladder, state.player().get_pickaxe());
        let mut cursors = upgrading(&state, UpgradeTab::Pickaxe);
        cursors.pickaxe_rung = here + 1;

        let view = View::from_state(
            &state,
            &Config::default(),
            cursors,
            None,
            &Toasts::new(),
            &Flashes::new(),
            Instant::now(),
        );
        match &view.upgrades.active_subtab().detail {
            UpgradeDetail::Pickaxe(detail) => {
                assert_eq!(detail.title, "Netherite Pickaxe");
                assert!(detail.crosses_tier_jump);
                // Diamond caps Efficiency at 5, Netherite at 15 — the sentence the
                // §5.4 frame prints as `Ceiling  Efficiency 5 → 15`.
                assert_eq!(detail.ceiling, Some((5, 15)));
                // Asked of `MineKind::gating_tier`, so a thirteenth mine gated on
                // Netherite would appear here without this projection changing.
                assert!(!detail.unlocks.is_empty());
                assert!(
                    detail
                        .unlocks
                        .iter()
                        .all(|kind| kind.gating_tier() == PickaxeTier::Netherite)
                );
            }
            other => unreachable!("the Pickaxe sub-tab must project a pickaxe: {other:?}"),
        }
    }

    #[test]
    fn the_tier_jumps_dip_is_quoted_in_the_standing_mines_own_block() {
        // The dip box exists because the numbers are counter-intuitive, so the test is
        // the numbers: a maxed Diamond pickaxe is 34.0, a bare Netherite one 9.0, and
        // the ticks are the same fall told in the unit the player watches.
        let state = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Diamond""#),
            (r#""enchants":{}"#, r#""enchants":{"Efficiency":5}"#),
        ]);
        let ladder = upgrade::ladder();
        let here = upgrade::position(&ladder, state.player().get_pickaxe());
        let mut cursors = upgrading(&state, UpgradeTab::Pickaxe);
        cursors.pickaxe_rung = here + 1;

        let view = View::from_state(
            &state,
            &Config::default(),
            cursors,
            None,
            &Toasts::new(),
            &Flashes::new(),
            Instant::now(),
        );
        match &view.upgrades.active_subtab().detail {
            UpgradeDetail::Pickaxe(detail) => {
                let dip = match detail.dip.as_ref() {
                    Some(dip) => dip,
                    None => unreachable!("Diamond Eff V → Netherite is the dip"),
                };
                let power = &detail.power;
                assert!((power.before - 34.0).abs() < f64::EPSILON);
                assert!((power.after - 9.0).abs() < f64::EPSILON);
                // The *value* block of the mine the player is standing in — Stone here,
                // since a fresh run has never left it.
                assert_eq!(power.block, MineKind::Stone.value_block());
                assert!(power.ticks_after > power.ticks_before);
                // The power is earned back, and the pane says exactly where.
                let repaid = match dip.repaid_at.as_ref() {
                    Some(repaid) => repaid,
                    None => unreachable!("Netherite Efficiency repays the jump"),
                };
                assert!(repaid.rung.starts_with("Netherite Eff"));
                assert!(repaid.power > power.before);
                // Five purchases later, which is the §6.7 modal's closing sentence.
                assert_eq!(repaid.rungs_later, 5);
            }
            other => unreachable!("the Pickaxe sub-tab must project a pickaxe: {other:?}"),
        }
    }

    /// A rung behind the player answers for **itself**, and that is the whole reason
    /// [`OwnedRung`] exists rather than a flag on [`PowerDetail`].
    ///
    /// [`upgrade::preview`] clamps its target up to where the player stands, so a
    /// rung below them previews `power_before == power_after` — the current pickaxe,
    /// twice. Reading the pane off that would have printed the *Diamond* power under a
    /// title reading `Iron Eff II`. The projection therefore asks
    /// [`Pickaxe::power_with`](skylode_core::pickaxe::Pickaxe) for the rung's own pair,
    /// which is a strictly smaller number here — the assertion that would fail if the
    /// clamped preview ever crept back in.
    #[test]
    fn a_rung_behind_the_player_is_projected_at_its_own_power() {
        let state = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Diamond""#),
            (r#""enchants":{}"#, r#""enchants":{"Efficiency":5}"#),
        ]);
        let ladder = upgrade::ladder();
        let here = upgrade::position(&ladder, state.player().get_pickaxe());
        let mut cursors = upgrading(&state, UpgradeTab::Pickaxe);
        // The Iron tier's jump: several rungs behind a maxed Diamond pickaxe, and a
        // tier jump, so it is also the rung that has mines to name.
        let iron_jump = match ladder
            .iter()
            .position(|rung| rung.tier == PickaxeTier::Iron && rung.is_tier_jump())
        {
            Some(index) => index,
            None => unreachable!("the ladder climbs through Iron"),
        };
        assert!(
            iron_jump < here,
            "the Iron jump must be behind a Diamond player"
        );
        cursors.pickaxe_rung = iron_jump;

        let view = View::from_state(
            &state,
            &Config::default(),
            cursors,
            None,
            &Toasts::new(),
            &Flashes::new(),
            Instant::now(),
        );
        match &view.upgrades.active_subtab().detail {
            UpgradeDetail::Pickaxe(detail) => {
                assert!(detail.chain.is_empty(), "an owned rung buys nothing");
                let owned = match detail.owned.as_ref() {
                    Some(owned) => owned,
                    None => unreachable!("a rung behind the player is owned"),
                };
                let current = f64::from(state.player().get_pickaxe().mining_power());
                assert!(
                    owned.power < current,
                    "the Iron jump reads {:.1}, which is not below the Diamond \
                     pickaxe's {current:.1} — the clamped preview is being read again",
                    owned.power
                );
                // A tier jump is Efficiency 0 by definition, against the tier's cap.
                assert_eq!(owned.efficiency, (0, PickaxeTier::Iron.efficiency_cap()));
                assert!(!owned.unlocks.is_empty());
                assert!(
                    owned
                        .unlocks
                        .iter()
                        .all(|kind| kind.gating_tier() == PickaxeTier::Iron)
                );
                // Quoted against the same reference as every other rung's pane.
                assert_eq!(owned.block, MineKind::Stone.value_block());
            }
            other => unreachable!("the Pickaxe sub-tab must project a pickaxe: {other:?}"),
        }
    }

    /// The two halves of the pane are complements, never both and never neither: a
    /// rung is owned exactly when there is no chain to buy it with.
    ///
    /// Walked over the **whole ladder** rather than at a boundary, because the rule is
    /// what lets [`pickaxe_pane`](crate::screen::upgrades) branch on `owned` alone and
    /// still be sure the buyable path is reached.
    #[test]
    fn an_owned_rung_and_a_chain_are_never_both_drawn() {
        let state = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Iron""#),
            (r#""enchants":{}"#, r#""enchants":{"Efficiency":2}"#),
        ]);
        for rung in 0..upgrade::ladder().len() {
            let mut cursors = upgrading(&state, UpgradeTab::Pickaxe);
            cursors.pickaxe_rung = rung;
            let view = View::from_state(
                &state,
                &Config::default(),
                cursors,
                None,
                &Toasts::new(),
                &Flashes::new(),
                Instant::now(),
            );
            match &view.upgrades.active_subtab().detail {
                UpgradeDetail::Pickaxe(detail) => assert_eq!(
                    detail.owned.is_some(),
                    detail.chain.is_empty(),
                    "rung {rung} is both owned and buyable, or neither"
                ),
                other => unreachable!("the Pickaxe sub-tab must project a pickaxe: {other:?}"),
            }
        }
    }

    /// An Efficiency rung opened no mine, and must not claim its tier's.
    ///
    /// The rung that opens a tier's mines is the jump; the five Efficiency rungs above
    /// it are bought inside a tier the player already had. Crediting them would put
    /// `Unlocks  the Iron mine` under four rungs that unlocked nothing.
    #[test]
    fn an_owned_efficiency_rung_claims_no_unlock() {
        let state = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Diamond""#),
            (r#""enchants":{}"#, r#""enchants":{"Efficiency":5}"#),
        ]);
        let ladder = upgrade::ladder();
        let rung = match ladder
            .iter()
            .position(|rung| rung.tier == PickaxeTier::Iron && rung.efficiency == 2)
        {
            Some(index) => index,
            None => unreachable!("Iron carries an Efficiency ladder"),
        };
        let mut cursors = upgrading(&state, UpgradeTab::Pickaxe);
        cursors.pickaxe_rung = rung;

        let view = View::from_state(
            &state,
            &Config::default(),
            cursors,
            None,
            &Toasts::new(),
            &Flashes::new(),
            Instant::now(),
        );
        match &view.upgrades.active_subtab().detail {
            UpgradeDetail::Pickaxe(detail) => {
                let owned = match detail.owned.as_ref() {
                    Some(owned) => owned,
                    None => unreachable!("Iron Eff II is behind a Diamond player"),
                };
                assert!(owned.unlocks.is_empty());
                assert_eq!(owned.efficiency, (2, PickaxeTier::Iron.efficiency_cap()));
            }
            other => unreachable!("the Pickaxe sub-tab must project a pickaxe: {other:?}"),
        }
    }

    /// **The defect the price block was rewritten for**, stated where it is decided: a
    /// price short of one material out of two must not verdict the other one with it.
    ///
    /// Before, the pane asked [`economy::affordability`] about the *whole* price and
    /// painted every line in that one answer, so `Insufficient` — which wins a mixed
    /// shortage by design — made both lines read `✗`.
    #[test]
    fn a_price_is_verdicted_one_material_at_a_time() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Compressed(Material::Quartz), 5);
        inventory.add(Item::Raw(Material::Redstone), 2);

        let lines = price_lines(
            &inventory,
            &[
                CostLine::from_raw_total(Material::Quartz, 300),
                CostLine::from_raw_total(Material::Redstone, 40),
            ],
        );

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].item, Item::Compressed(Material::Quartz));
        assert_eq!(lines[0].mark, Mark::Affordable);
        assert_eq!(lines[1].item, Item::Raw(Material::Redstone));
        assert_eq!(lines[1].mark, Mark::Refused);
        assert_eq!((lines[1].needed, lines[1].held), (40, 2));
    }

    /// The middle state, and the reason the wealth pass is asked per **material** while
    /// the shape pass is asked per **item**.
    ///
    /// `1 Compressed + 50` is owed and `200 raw` is held. The Compressed line holds
    /// *none* of what it asks for and is still one conversion away, because the pile
    /// behind it covers the whole price — asking wealth per **item** would have read
    /// `held 0 of 1` and called it `✗`, sending the player to a mine they do not need.
    #[test]
    fn a_line_the_pile_behind_it_covers_invites_a_conversion_rather_than_a_mine() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::Iron), 200);

        let lines = price_lines(&inventory, &[CostLine::from_raw_total(Material::Iron, 150)]);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].item, Item::Compressed(Material::Iron));
        assert_eq!((lines[0].needed, lines[0].held), (1, 0));
        assert_eq!(lines[0].mark, Mark::CompressFirst);
        // The raw half is over-served by the same pile, and says so on its own.
        assert_eq!(lines[1].mark, Mark::Affordable);

        // Halve the pile and the *same* line becomes a mine's problem instead.
        let mut poorer = Inventory::new();
        poorer.add(Item::Raw(Material::Iron), 40);
        let lines = price_lines(&poorer, &[CostLine::from_raw_total(Material::Iron, 150)]);
        assert!(
            lines.iter().all(|line| line.mark == Mark::Refused),
            "{lines:?}"
        );
    }

    /// **The rule the aggregate exists to respect.** Two rungs of `30` and `80` raw owe
    /// `110 raw` — one hundred and ten loose items — and never `1 Compressed + 10`,
    /// which is a payment `economy::pay` would refuse and the player was never offered.
    #[test]
    fn a_summed_chain_never_re_splits_into_a_compressed_unit() {
        let rungs = [
            upgrade::PickaxeRung {
                tier: PickaxeTier::Wooden,
                efficiency: 1,
                cost: Some(Cost::single(Material::Stone, 30)),
            },
            upgrade::PickaxeRung {
                tier: PickaxeTier::Wooden,
                efficiency: 2,
                cost: Some(Cost::single(Material::Stone, 80)),
            },
        ];

        let owed = chain_price(&rungs);
        assert_eq!(owed.get(&Item::Raw(Material::Stone)), Some(&110));
        assert_eq!(owed.get(&Item::Compressed(Material::Stone)), None);
    }

    /// The aggregate and the walk must agree on the only thing the player acts on.
    ///
    /// [`upgrade::chain_affordability`] simulates rung by rung through a cloned purse;
    /// the pane sums per denomination. They may name *different* refusals — the walk
    /// stops at the first rung that fails, the lines report every material — but
    /// "payable or not" is one fact, and this walks the whole ladder to say so.
    #[test]
    fn the_summed_price_is_payable_exactly_when_the_walk_is() {
        let state = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Iron""#),
            (
                r#""inventory":{}"#,
                r#""inventory":{"compressed_iron":30,"iron":80,"compressed_gold":12}"#,
            ),
        ]);
        let ladder = upgrade::ladder();
        let pickaxe = state.player().get_pickaxe();
        let inventory = state.player().get_inventory();
        let here = upgrade::position(&ladder, pickaxe);

        let mut verdicts = Vec::new();
        for target in here..ladder.len() {
            let climbed = ladder.get(here + 1..=target).unwrap_or(&[]);
            let summed = aggregate_price_lines(inventory, &chain_price(climbed))
                .iter()
                .all(|line| line.mark == Mark::Affordable);
            let walked = upgrade::chain_affordability(inventory, pickaxe, target)
                == Affordability::Affordable;
            assert_eq!(summed, walked, "the two disagree at rung {target}");
            verdicts.push(walked);
        }
        // Both answers must occur, or the agreement above is agreement about nothing.
        assert!(
            verdicts.contains(&true) && verdicts.contains(&false),
            "{verdicts:?}"
        );
    }

    #[test]
    fn every_enchant_track_says_what_it_does_and_what_the_next_level_changes() {
        // Six tracks, six pairs of sentences, and none of them may be empty: the pane
        // is the only place the game explains what an enchant *is*. Walked in one test
        // rather than six because the assertion is the same one six times.
        let state = fresh_run();
        for kind in cursor::enchant_tracks() {
            let mut cursors = upgrading(&state, UpgradeTab::Enchants);
            cursors.enchant = kind;
            let view = View::from_state(
                &state,
                &Config::default(),
                cursors,
                None,
                &Toasts::new(),
                &Flashes::new(),
                Instant::now(),
            );
            match &view.upgrades.active_subtab().detail {
                UpgradeDetail::Enchant(detail) => {
                    assert_eq!(detail.kind, kind);
                    assert!(!detail.effect.is_empty(), "{kind:?} explains nothing");
                    assert!(!detail.at_next.is_empty(), "{kind:?} promises nothing");
                }
                other => unreachable!("the Enchants sub-tab must project an enchant: {other:?}"),
            }
        }
    }

    #[test]
    fn only_the_enchants_that_roll_a_die_are_promised_more_procs() {
        // `proc_permille` is `0` for Fortune and Haste — they are permanent
        // multipliers — so telling a player their next level procs more often would
        // describe a mechanic the enchant does not have. Now that the pane states
        // numbers rather than sentences, that reads as a `procs` row the two passive
        // tracks do not have at all.
        let cap = 6;
        let steps = |kind, level| {
            enchant_at_next(kind, level, cap, 12, &Pickaxe::default())
                .into_iter()
                .map(|step| (step.name, step.value))
                .collect::<Vec<_>>()
        };
        let names = |kind, level: u8| -> Vec<&'static str> {
            steps(kind, level)
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        };

        assert_eq!(names(EnchantType::Fortune, 0), vec!["drops"]);
        assert_eq!(names(EnchantType::Haste, 0), vec!["speed", "power"]);
        for kind in [
            EnchantType::Explosive,
            EnchantType::Jackhammer,
            EnchantType::Nuke,
            EnchantType::Excavator,
        ] {
            assert!(
                names(kind, 0).contains(&"procs"),
                "{kind:?} rolls a die and must quote its rate"
            );
        }

        // The numbers themselves, on the two the frame draws.
        assert!(
            steps(EnchantType::Fortune, 0)
                .iter()
                .any(|(_, value)| value == "x1 → x2")
        );
        assert!(
            steps(EnchantType::Nuke, 0)
                .iter()
                .any(|(name, value)| *name == "procs" && value == "0.0% → 0.1%")
        );
    }

    /// The square is the one stat that stands still on two levels out of three, and the
    /// pane has to be honest about which — a `5x5` printed at II → III would promise a
    /// reward `blast_cells` does not pay (UI.md §5.4.1).
    #[test]
    fn the_explosive_square_is_quoted_band_by_band() {
        let cap = 6;
        let square = |level: u8| {
            enchant_at_next(EnchantType::Explosive, level, cap, 12, &Pickaxe::default())
                .into_iter()
                .find(|step| step.name == "square")
                .map(|step| step.value)
        };

        // Bands of three: I-III blast 3x3, IV-VI 5x5. So two steps out of three stand
        // still, and the third is the one that pays.
        assert_eq!(square(1), Some("3x3 → 3x3".to_owned()));
        assert_eq!(square(2), Some("3x3 → 3x3".to_owned()));
        assert_eq!(square(3), Some("3x3 → 5x5".to_owned()));
        assert_eq!(square(4), Some("5x5 → 5x5".to_owned()));

        // The sentence the numbers cannot say, and only where they cannot say it.
        assert!(!enchant_note(EnchantType::Explosive, 1, cap).is_empty());
        assert!(enchant_note(EnchantType::Explosive, 3, cap).is_empty());
        assert!(enchant_note(EnchantType::Nuke, 1, cap).is_empty());
    }

    /// A capped track sells nothing, and says so by having nothing to say: no steps, no
    /// note, no price. The pane prints its own `Maxed` line off that emptiness.
    #[test]
    fn a_capped_track_moves_no_stat_at_all() {
        let cap = 6;
        for kind in cursor::enchant_tracks() {
            assert!(
                enchant_at_next(kind, cap, cap, 12, &Pickaxe::default()).is_empty(),
                "{kind:?} is at its cap and must promise nothing"
            );
            assert!(enchant_note(kind, cap, cap).is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn efficiency_has_no_sentence_here_because_the_shop_does_not_sell_it() {
        // `cursor::enchant_tracks` drops it on the price — `economy::enchant_cost`
        // answers `None` for Efficiency and only Efficiency — so this arm exists to
        // keep the match exhaustive, not because a pane can reach it. Asserted rather
        // than assumed: if the filter ever changed, the pane would print a blank
        // Effect block instead of failing here.
        assert!(enchant_effect(EnchantType::Efficiency, 5).is_empty());
        assert!(enchant_at_next(EnchantType::Efficiency, 0, 5, 12, &Pickaxe::default()).is_empty());
        assert!(!cursor::enchant_tracks().contains(&EnchantType::Efficiency));
    }

    #[test]
    fn a_capped_enchant_is_priceless_rather_than_unaffordable() {
        // `—`, not `✗`: the player is not short of anything, there is simply nothing
        // left to sell them. Efficiency 3 is the Overworld cap a fresh run lives under.
        let state = veteran(&[(r#""enchants":{}"#, r#""enchants":{"Fortune":3}"#)]);
        let cursors = upgrading(&state, UpgradeTab::Enchants);
        let view = View::from_state(
            &state,
            &Config::default(),
            cursors,
            None,
            &Toasts::new(),
            &Flashes::new(),
            Instant::now(),
        );

        let fortune = view
            .upgrades
            .enchants
            .rows
            .iter()
            .find(|row| row.cells.first().is_some_and(|cell| cell == "Fortune"));
        assert_eq!(fortune.map(|row| row.mark), Some(Mark::NoPrice));
        assert!(fortune.is_some_and(|row| row.cells.get(1).is_some_and(|cell| cell == MAXED)));
        match &view.upgrades.active_subtab().detail {
            UpgradeDetail::Enchant(detail) => assert!(detail.cost.is_empty()),
            other => unreachable!("the Enchants sub-tab must project an enchant: {other:?}"),
        }
    }

    #[test]
    fn a_locked_mine_prints_its_level_gate_and_an_unvisited_one_prints_a_size() {
        // Two rows that both read `—`, told apart by the `Next` cell alone: the End is
        // shut behind a level, Coal is merely unopened, and only one of those is
        // something the player can do anything about today.
        let state = fresh_run();
        let view = projected(&state);
        let rows = &view.upgrades.mines.rows;
        let cell = |kind: MineKind, track: MineTrack| {
            rows.iter()
                .find(|row| {
                    row.cells.first().is_some_and(|name| name == kind.name())
                        && row
                            .cells
                            .get(1)
                            .is_some_and(|word| word == track_word(track))
                })
                .cloned()
        };

        let end = cell(MineKind::Amethyst, MineTrack::Size);
        assert_eq!(end.as_ref().map(|row| row.mark), Some(Mark::NoPrice));
        assert!(end.is_some_and(|row| row.cells.get(2).is_some_and(|next| next.starts_with("Lv"))));

        let coal = cell(MineKind::Coal, MineTrack::Richness);
        assert_eq!(coal.as_ref().map(|row| row.mark), Some(Mark::NoPrice));
        assert!(coal.is_some_and(|row| row.cells.get(2).is_some_and(|next| next == "2")));

        // Two rows per mine, and every one of the twelve gets both.
        assert_eq!(rows.len(), MineKind::ALL.len() * MineTrack::ALL.len());
    }

    #[test]
    fn an_unvisited_mines_pane_sends_the_player_there_rather_than_pricing_it() {
        let state = fresh_run();
        let mut cursors = upgrading(&state, UpgradeTab::Mines);
        cursors.mine_track = (MineKind::Coal, MineTrack::Size);

        let view = View::from_state(
            &state,
            &Config::default(),
            cursors,
            None,
            &Toasts::new(),
            &Flashes::new(),
            Instant::now(),
        );
        match &view.upgrades.active_subtab().detail {
            UpgradeDetail::Mine(detail) => {
                assert_eq!(detail.blocked, Some(TrackBlock::NotEntered));
                assert!(detail.cost.is_empty());
            }
            other => unreachable!("the Mines sub-tab must project a mine track: {other:?}"),
        }
    }

    #[test]
    fn a_maxed_track_has_no_next_level_to_quote() {
        // Both ends of both tracks: a size at the top of `MINE_SIZES` and a richness at
        // `MAX_RICHNESS_LEVEL` are `—` rather than a price, and the `Next` cell says so
        // in words instead of a number that would be a lie.
        let state = veteran(&[(
            r#""size_level":0,"richness_level":0,"richness_setting":0"#,
            &format!(
                r#""size_level":9,"richness_level":{MAX_RICHNESS_LEVEL},"richness_setting":0"#
            ),
        )]);

        for track in MineTrack::ALL {
            let mut cursors = upgrading(&state, UpgradeTab::Mines);
            cursors.mine_track = (MineKind::Stone, track);
            let view = View::from_state(
                &state,
                &Config::default(),
                cursors,
                None,
                &Toasts::new(),
                &Flashes::new(),
                Instant::now(),
            );
            match &view.upgrades.active_subtab().detail {
                UpgradeDetail::Mine(detail) => {
                    assert_eq!(detail.at_next, TrackOutcome::Maxed, "{track:?}");
                    // No price at all, which is what the pane reads to print `Maxed`
                    // rather than a number: the mark used to say it, and every line of
                    // a price now carries its own.
                    assert!(detail.cost.is_empty(), "{track:?}");
                }
                other => unreachable!("the Mines sub-tab must project a mine track: {other:?}"),
            }
        }
    }
}
